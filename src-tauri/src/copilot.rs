//! # Modo Co-Pilot de Tela (Observação Inteligente) — `copilot.rs`
//!
//! Monitora periodicamente uma janela escolhida (ou o desktop inteiro) em
//! segundo plano usando `ScreenCaptureGuard` e calcula localmente em Rust a
//! diferença visual entre quadros (`compute_jpeg_visual_delta_pct`).
//!
//! Enquanto a tela permanecer estática, **nenhum token ou chamada de IA é gasto**.
//! Quando uma mudança real acima do limiar configurado ocorre (ex.: nova fase
//! em um jogo ou erro aparecendo em logs/IDE), emite `jarvis:copilot-trigger`
//! com o quadro atualizado para dicas curtas proativas.

use base64::Engine as _;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::sync::{watch, Mutex, RwLock};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenCopilotStatus {
    pub active: bool,
    pub target_window: Option<String>,
    pub interval_secs: u64,
    pub threshold_pct: f32,
    pub frames_checked: u64,
    pub triggers_sent: u64,
    pub last_delta_pct: f32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenCopilotTriggerEvent {
    pub target_window: Option<String>,
    pub delta_pct: f32,
    pub image_base64: String,
    pub mime_type: String,
    pub prompt: String,
}

pub struct ScreenCopilotController {
    active: AtomicBool,
    target_window: RwLock<Option<String>>,
    interval_secs: AtomicU64,
    threshold_tenths: AtomicU64,
    frames_checked: AtomicU64,
    triggers_sent: AtomicU64,
    last_delta_tenths: AtomicU64,
    stop_tx: Mutex<Option<watch::Sender<bool>>>,
}

impl Default for ScreenCopilotController {
    fn default() -> Self {
        Self {
            active: AtomicBool::new(false),
            target_window: RwLock::new(None),
            interval_secs: AtomicU64::new(8),
            threshold_tenths: AtomicU64::new(80), // 8.0%
            frames_checked: AtomicU64::new(0),
            triggers_sent: AtomicU64::new(0),
            last_delta_tenths: AtomicU64::new(0),
            stop_tx: Mutex::new(None),
        }
    }
}

impl ScreenCopilotController {
    pub async fn status(&self) -> ScreenCopilotStatus {
        ScreenCopilotStatus {
            active: self.active.load(Ordering::SeqCst),
            target_window: self.target_window.read().await.clone(),
            interval_secs: self.interval_secs.load(Ordering::SeqCst),
            threshold_pct: self.threshold_tenths.load(Ordering::SeqCst) as f32 / 10.0,
            frames_checked: self.frames_checked.load(Ordering::SeqCst),
            triggers_sent: self.triggers_sent.load(Ordering::SeqCst),
            last_delta_pct: self.last_delta_tenths.load(Ordering::SeqCst) as f32 / 10.0,
        }
    }

    pub async fn stop(&self) -> ScreenCopilotStatus {
        self.active.store(false, Ordering::SeqCst);
        if let Some(tx) = self.stop_tx.lock().await.take() {
            let _ = tx.send(true);
        }
        self.status().await
    }

    pub async fn start(
        self: &Arc<Self>,
        app: AppHandle,
        target_window: Option<String>,
        interval_secs: u64,
        threshold_pct: f32,
        focus_goal: Option<String>,
    ) -> ScreenCopilotStatus {
        let _ = self.stop().await;

        let clean_target = target_window
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let clamped_interval = interval_secs.clamp(4, 60);
        let clamped_threshold = threshold_pct.clamp(2.0, 50.0);
        let goal_text = focus_goal
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                "Observe a mudança relevante nesta tela e dê uma dica prática e curta em 1 frase."
                    .to_string()
            });

        *self.target_window.write().await = clean_target.clone();
        self.interval_secs.store(clamped_interval, Ordering::SeqCst);
        self.threshold_tenths
            .store((clamped_threshold * 10.0).round() as u64, Ordering::SeqCst);
        self.frames_checked.store(0, Ordering::SeqCst);
        self.triggers_sent.store(0, Ordering::SeqCst);
        self.last_delta_tenths.store(0, Ordering::SeqCst);
        self.active.store(true, Ordering::SeqCst);

        let (tx, mut rx) = watch::channel(false);
        *self.stop_tx.lock().await = Some(tx);

        let ctrl = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            let mut prev_luma: Option<Vec<u8>> = None;
            loop {
                tokio::select! {
                    _ = rx.changed() => {
                        if *rx.borrow() {
                            break;
                        }
                    }
                    _ = tokio::time::sleep(Duration::from_secs(clamped_interval)) => {
                        if !ctrl.active.load(Ordering::SeqCst) {
                            break;
                        }
                        let _guard = crate::tools::acquire_screen_capture_guard(&app).await;
                        let target_clone = clean_target.clone();
                        let cap_res = tokio::task::spawn_blocking(move || {
                            if let Some(q) = target_clone {
                                let wins = crate::tools::list_open_windows_info_blocking();
                                crate::tools::capture_target_window_blocking(&q, &wins)
                                    .map(|((_, _, _, _, b64), _, _)| b64)
                            } else {
                                crate::tools::capture_desktop_screen_blocking()
                                    .map(|(_, _, _, _, b64)| b64)
                            }
                        })
                        .await;
                        drop(_guard);

                        let Ok(Ok(b64)) = cap_res else {
                            continue;
                        };
                        ctrl.frames_checked.fetch_add(1, Ordering::SeqCst);

                        let Ok(curr_luma) = decode_jpeg_luma_thumb(&b64) else {
                            continue;
                        };
                        let delta_pct = match prev_luma.as_deref() {
                            Some(prev) => compute_luma_delta_pct(prev, &curr_luma),
                            None => 100.0, // Primeiro quadro estabelece baseline; não dispara IA ainda
                        };
                        let is_baseline = prev_luma.is_none();
                        prev_luma = Some(curr_luma);

                        if is_baseline {
                            ctrl.last_delta_tenths.store(0, Ordering::SeqCst);
                            continue;
                        }
                        ctrl.last_delta_tenths
                            .store((delta_pct * 10.0).round() as u64, Ordering::SeqCst);

                        if delta_pct >= clamped_threshold {
                            ctrl.triggers_sent.fetch_add(1, Ordering::SeqCst);
                            let prompt = format!(
                                "[Co-Pilot de Tela — mudança visual de {:.1}%{}] {}",
                                delta_pct,
                                clean_target
                                    .as_deref()
                                    .map(|w| format!(" em '{w}'"))
                                    .unwrap_or_default(),
                                goal_text
                            );
                            let _ = app.emit(
                                "jarvis:copilot-trigger",
                                ScreenCopilotTriggerEvent {
                                    target_window: clean_target.clone(),
                                    delta_pct,
                                    image_base64: b64,
                                    mime_type: "image/jpeg".to_string(),
                                    prompt,
                                },
                            );
                        }
                    }
                }
            }
            ctrl.active.store(false, Ordering::SeqCst);
        });

        self.status().await
    }
}

/// Decodifica um JPEG base64 em uma miniatura `32x18` em tons de cinza para comparação ultrarrápida.
pub fn decode_jpeg_luma_thumb(b64: &str) -> Result<Vec<u8>, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| format!("Base64 JPEG inválido: {e}"))?;
    let img = image::load_from_memory_with_format(&bytes, image::ImageFormat::Jpeg)
        .map_err(|e| format!("JPEG inválido: {e}"))?;
    let small = img.resize_exact(32, 18, image::imageops::FilterType::Triangle);
    Ok(small.to_luma8().into_raw())
}

/// Calcula a porcentagem média de diferença visual (`0.0..=100.0`) entre duas miniaturas luma.
pub fn compute_luma_delta_pct(a: &[u8], b: &[u8]) -> f32 {
    if a.is_empty() || a.len() != b.len() {
        return 0.0;
    }
    let total_diff: u64 = a
        .iter()
        .zip(b.iter())
        .map(|(&p1, &p2)| u64::from(p1.abs_diff(p2)))
        .sum();
    let max_diff = (a.len() as f64) * 255.0;
    ((total_diff as f64 / max_diff) * 100.0) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visual_delta_detector_distinguishes_static_and_changed_frames() {
        let frame_a = vec![40u8; 32 * 18];
        let frame_same = vec![40u8; 32 * 18];
        let frame_bright = vec![220u8; 32 * 18];

        assert_eq!(compute_luma_delta_pct(&frame_a, &frame_same), 0.0);
        let delta = compute_luma_delta_pct(&frame_a, &frame_bright);
        assert!(
            delta > 65.0,
            "Esperava delta alto entre telas distintas, obteve {delta}"
        );
    }
}
