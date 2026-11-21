use crate::llm;
use crate::state::AppState;
use base64::Engine as _;
use serde::Serialize;
use serde_json::json;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

const SAMPLE_RATE: u32 = 16_000;
const CHUNK_SAMPLES: usize = 1_280; // 80ms at 16kHz

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MicDeviceInfo {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordedAudio {
    pub audio_base64: String,
    pub mime_type: String,
    pub had_audio: bool,
}

pub struct MicController {
    rec_stop: Mutex<Option<Arc<AtomicBool>>>,
    rec_samples: Arc<Mutex<Vec<i16>>>,
    test_stop: Mutex<Option<Arc<AtomicBool>>>,
    wake_stop: Mutex<Option<Arc<AtomicBool>>>,
    wake_paused: Arc<AtomicBool>,
}

impl Default for MicController {
    fn default() -> Self {
        Self {
            rec_stop: Mutex::new(None),
            rec_samples: Arc::new(Mutex::new(Vec::new())),
            test_stop: Mutex::new(None),
            wake_stop: Mutex::new(None),
            wake_paused: Arc::new(AtomicBool::new(false)),
        }
    }
}

pub fn list_microphones() -> Vec<MicDeviceInfo> {
    #[cfg(windows)]
    {
        use windows::Win32::Media::Audio::{waveInGetDevCapsW, waveInGetNumDevs, WAVEINCAPSW};
        let count = unsafe { waveInGetNumDevs() };
        let mut out = Vec::new();
        for idx in 0..count {
            let mut caps = WAVEINCAPSW::default();
            let rc = unsafe {
                waveInGetDevCapsW(
                    idx as usize,
                    &mut caps,
                    std::mem::size_of::<WAVEINCAPSW>() as u32,
                )
            };
            if rc == 0 {
                let pname = caps.szPname;
                let end = pname.iter().position(|&c| c == 0).unwrap_or(pname.len());
                let label = String::from_utf16_lossy(&pname[..end]).trim().to_string();
                out.push(MicDeviceInfo {
                    id: idx.to_string(),
                    label: if label.is_empty() {
                        format!("Microfone {}", idx + 1)
                    } else {
                        label
                    },
                });
            }
        }
        out
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

pub fn encode_pcm16_wav(samples: &[i16]) -> Vec<u8> {
    let data_bytes = (samples.len() * 2) as u32;
    let mut buf = Vec::with_capacity(44 + data_bytes as usize);
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&(36 + data_bytes).to_le_bytes());
    buf.extend_from_slice(b"WAVE");
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes());
    buf.extend_from_slice(&1u16.to_le_bytes()); // PCM
    buf.extend_from_slice(&1u16.to_le_bytes()); // mono
    buf.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    buf.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    buf.extend_from_slice(&2u16.to_le_bytes()); // block align
    buf.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_bytes.to_le_bytes());
    for &s in samples {
        buf.extend_from_slice(&s.to_le_bytes());
    }
    buf
}

fn compute_rms(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let mut sum = 0.0f64;
    for &s in samples {
        let v = (s as f64) / 32768.0;
        sum += v * v;
    }
    (sum / samples.len() as f64).sqrt() as f32
}

pub fn parse_wake_command_rust(transcript: &str) -> Option<String> {
    let clean = transcript.trim();
    if clean.is_empty() {
        return None;
    }
    // Lowercase ASCII-ish normalization for Portuguese wake phrases
    let lowered: String = clean
        .chars()
        .map(|c| match c {
            'Á' | 'À' | 'Â' | 'Ã' | 'á' | 'à' | 'â' | 'ã' => 'a',
            'É' | 'È' | 'Ê' | 'é' | 'è' | 'ê' => 'e',
            'Í' | 'Ì' | 'Î' | 'í' | 'ì' | 'î' => 'i',
            'Ó' | 'Ò' | 'Ô' | 'Õ' | 'ó' | 'ò' | 'ô' | 'õ' => 'o',
            'Ú' | 'Ù' | 'Û' | 'ú' | 'ù' | 'û' => 'u',
            'Ç' | 'ç' => 'c',
            other => other.to_ascii_lowercase(),
        })
        .collect();

    let re = regex::Regex::new(
        r"(?i)(?:^|\b)(?:(?:ei|hey|oi|ok|fala|e\s+ai|ola)\s+)?(?:jarvis|jarves|javes|jarviz|charvis|yarvis|diarvis)\b[\s,.:;!?-]*(.*)$",
    )
    .ok()?;
    let caps = re.captures(&lowered)?;
    let norm_tail = caps.get(1).map(|m| m.as_str().trim()).unwrap_or("");
    if norm_tail.is_empty() {
        return Some(String::new());
    }
    // Try extracting the original-cased tail from clean string
    let re_raw = regex::Regex::new(
        r"(?i)(?:jarvis|jarves|javes|jarviz|charvis|yarvis|diarvis)\b[\s,.:;!?-]*(.*)$",
    )
    .ok()?;
    let raw_tail = re_raw
        .captures(clean)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().trim().to_string())
        .unwrap_or_else(|| norm_tail.to_string());
    Some(raw_tail)
}

#[cfg(windows)]
fn resolve_wave_device_id(device_id: Option<&str>) -> u32 {
    use windows::Win32::Media::Audio::{waveInGetNumDevs, WAVE_MAPPER};
    let count = unsafe { waveInGetNumDevs() };
    if let Some(raw) = device_id.map(str::trim).filter(|s| !s.is_empty()) {
        if let Ok(idx) = raw.parse::<u32>() {
            if idx < count {
                return idx;
            }
        }
    }
    WAVE_MAPPER
}

#[cfg(windows)]
fn run_wavein_loop<F>(
    device_id: Option<String>,
    stop_flag: Arc<AtomicBool>,
    mut on_chunk: F,
) -> Result<(), String>
where
    F: FnMut(&[i16], f32),
{
    use windows::Win32::Media::Audio::{
        waveInAddBuffer, waveInClose, waveInGetNumDevs, waveInOpen, waveInPrepareHeader,
        waveInReset, waveInStart, waveInStop, waveInUnprepareHeader, CALLBACK_NULL, HWAVEIN,
        WAVEFORMATEX, WAVEHDR, WAVE_FORMAT_PCM, WHDR_DONE,
    };

    let num_devs = unsafe { waveInGetNumDevs() };
    if num_devs == 0 {
        return Err("Nenhum microfone conectado ou habilitado no Windows.".to_string());
    }

    let dev_id = resolve_wave_device_id(device_id.as_deref());
    let wfx = WAVEFORMATEX {
        wFormatTag: WAVE_FORMAT_PCM as u16,
        nChannels: 1,
        nSamplesPerSec: SAMPLE_RATE,
        nAvgBytesPerSec: SAMPLE_RATE * 2,
        nBlockAlign: 2,
        wBitsPerSample: 16,
        cbSize: 0,
    };

    let mut hwi = HWAVEIN::default();
    let open_rc = unsafe {
        waveInOpen(
            Some(&mut hwi),
            dev_id,
            &wfx,
            None,
            None,
            CALLBACK_NULL,
        )
    };
    if open_rc != 0 {
        return Err(format!(
            "Não foi possível abrir o microfone do Windows (erro WinMM {open_rc}). Verifique se o microfone está conectado e permitido nas configurações de privacidade."
        ));
    }

    const NUM_BUFS: usize = 4;
    let buf_bytes = (CHUNK_SAMPLES * 2) as u32;
    let mut buffers: Vec<Vec<i16>> = (0..NUM_BUFS).map(|_| vec![0i16; CHUNK_SAMPLES]).collect();
    let mut headers: Vec<WAVEHDR> = (0..NUM_BUFS)
        .map(|i| WAVEHDR {
            lpData: windows::core::PSTR(buffers[i].as_mut_ptr() as *mut u8),
            dwBufferLength: buf_bytes,
            ..Default::default()
        })
        .collect();

    unsafe {
        for hdr in &mut headers {
            let p_rc = waveInPrepareHeader(hwi, hdr, std::mem::size_of::<WAVEHDR>() as u32);
            if p_rc != 0 {
                let _ = waveInClose(hwi);
                return Err(format!("Falha ao preparar buffer de áudio (erro {p_rc})."));
            }
            let a_rc = waveInAddBuffer(hwi, hdr, std::mem::size_of::<WAVEHDR>() as u32);
            if a_rc != 0 {
                let _ = waveInClose(hwi);
                return Err(format!("Falha ao enfileirar buffer de áudio (erro {a_rc})."));
            }
        }
        let s_rc = waveInStart(hwi);
        if s_rc != 0 {
            let _ = waveInReset(hwi);
            let _ = waveInClose(hwi);
            return Err(format!("Falha ao iniciar captura do microfone (erro {s_rc})."));
        }
    }

    while !stop_flag.load(Ordering::Relaxed) {
        let mut processed_any = false;
        for i in 0..NUM_BUFS {
            if (headers[i].dwFlags & WHDR_DONE) != 0 {
                processed_any = true;
                let recorded_samples = (headers[i].dwBytesRecorded as usize) / 2;
                let count = recorded_samples.min(CHUNK_SAMPLES);
                if count > 0 {
                    let slice = &buffers[i][..count];
                    let rms = compute_rms(slice);
                    on_chunk(slice, rms);
                }
                if !stop_flag.load(Ordering::Relaxed) {
                    headers[i].dwFlags &= !WHDR_DONE;
                    headers[i].dwBytesRecorded = 0;
                    unsafe {
                        let _ = waveInAddBuffer(
                            hwi,
                            &mut headers[i],
                            std::mem::size_of::<WAVEHDR>() as u32,
                        );
                    }
                }
            }
        }
        if !processed_any {
            std::thread::sleep(Duration::from_millis(12));
        }
    }

    unsafe {
        let _ = waveInStop(hwi);
        let _ = waveInReset(hwi);
        for hdr in &mut headers {
            let _ = waveInUnprepareHeader(hwi, hdr, std::mem::size_of::<WAVEHDR>() as u32);
        }
        let _ = waveInClose(hwi);
    }

    Ok(())
}

#[cfg(not(windows))]
fn run_wavein_loop<F>(
    _device_id: Option<String>,
    _stop_flag: Arc<AtomicBool>,
    _on_chunk: F,
) -> Result<(), String>
where
    F: FnMut(&[i16], f32),
{
    Err("Captura nativa de microfone disponível apenas no Windows.".to_string())
}

impl MicController {
    pub fn start_recording(
        &self,
        app: AppHandle,
        vad_enabled: bool,
        device_id: Option<String>,
    ) -> Result<(), String> {
        self.stop_recording_flag_only();
        self.wake_paused.store(true, Ordering::SeqCst);

        {
            let mut buf = self.rec_samples.lock().unwrap();
            buf.clear();
        }

        let stop = Arc::new(AtomicBool::new(false));
        {
            let mut guard = self.rec_stop.lock().unwrap();
            *guard = Some(Arc::clone(&stop));
        }

        let samples_arc = Arc::clone(&self.rec_samples);
        let stop_clone = Arc::clone(&stop);

        // Quick preflight check on Windows so immediate errors return synchronously
        #[cfg(windows)]
        {
            use windows::Win32::Media::Audio::waveInGetNumDevs;
            if unsafe { waveInGetNumDevs() } == 0 {
                self.wake_paused.store(false, Ordering::SeqCst);
                return Err("Nenhum microfone detectado no Windows.".to_string());
            }
        }

        std::thread::spawn(move || {
            let started = Instant::now();
            let mut heard_speech = false;
            let mut silence_since: Option<Instant> = None;
            let mut auto_stopped = false;

            let res = run_wavein_loop(device_id, Arc::clone(&stop_clone), |chunk, rms| {
                let norm = (rms * 9.0).min(1.0);
                let _ = app.emit("jarvis:mic-level", json!({ "level": norm }));

                if let Ok(mut buf) = samples_arc.lock() {
                    buf.extend_from_slice(chunk);
                }

                if vad_enabled && !auto_stopped {
                    if rms > 0.012 {
                        heard_speech = true;
                        silence_since = None;
                    } else if heard_speech {
                        let s = silence_since.get_or_insert_with(Instant::now);
                        if s.elapsed() >= Duration::from_millis(1350) {
                            auto_stopped = true;
                            stop_clone.store(true, Ordering::SeqCst);
                            let _ = app.emit("jarvis:mic-auto-stop", json!({}));
                        }
                    }
                    if started.elapsed() >= Duration::from_secs(30) {
                        auto_stopped = true;
                        stop_clone.store(true, Ordering::SeqCst);
                        let _ = app.emit("jarvis:mic-auto-stop", json!({}));
                    }
                }
            });

            let _ = app.emit("jarvis:mic-level", json!({ "level": 0.0 }));
            if let Err(err) = res {
                let _ = app.emit("jarvis:mic-error", json!({ "error": err }));
            }
        });

        Ok(())
    }

    fn stop_recording_flag_only(&self) {
        if let Ok(mut guard) = self.rec_stop.lock() {
            if let Some(flag) = guard.take() {
                flag.store(true, Ordering::SeqCst);
            }
        }
    }

    pub fn stop_recording(&self) -> RecordedAudio {
        self.stop_recording_flag_only();
        // Give the waveIn thread a moment to flush its final buffer
        std::thread::sleep(Duration::from_millis(60));
        let samples = {
            let mut buf = self.rec_samples.lock().unwrap();
            std::mem::take(&mut *buf)
        };
        let had_audio = samples.len() > (SAMPLE_RATE as usize) / 4;
        let wav = encode_pcm16_wav(&samples);
        let b64 = base64::engine::general_purpose::STANDARD.encode(&wav);
        RecordedAudio {
            audio_base64: b64,
            mime_type: "audio/wav".to_string(),
            had_audio,
        }
    }

    pub fn start_mic_test(&self, app: AppHandle, device_id: Option<String>) -> Result<(), String> {
        self.stop_mic_test();
        let stop = Arc::new(AtomicBool::new(false));
        {
            let mut guard = self.test_stop.lock().unwrap();
            *guard = Some(Arc::clone(&stop));
        }

        std::thread::spawn(move || {
            let res = run_wavein_loop(device_id, stop, |_chunk, rms| {
                let norm = (rms * 10.0).min(1.0);
                let _ = app.emit("jarvis:mic-test-level", json!({ "level": norm }));
            });
            let _ = app.emit("jarvis:mic-test-level", json!({ "level": 0.0 }));
            if let Err(err) = res {
                let _ = app.emit("jarvis:mic-test-error", json!({ "error": err }));
            }
        });

        Ok(())
    }

    pub fn stop_mic_test(&self) {
        if let Ok(mut guard) = self.test_stop.lock() {
            if let Some(flag) = guard.take() {
                flag.store(true, Ordering::SeqCst);
            }
        }
    }

    pub fn set_wake_paused(&self, paused: bool) {
        self.wake_paused.store(paused, Ordering::SeqCst);
    }

    pub fn set_wake_word(
        &self,
        app: AppHandle,
        state: Arc<AppState>,
        enabled: bool,
        device_id: Option<String>,
    ) -> Result<(), String> {
        // Stop any existing wake word thread first
        if let Ok(mut guard) = self.wake_stop.lock() {
            if let Some(flag) = guard.take() {
                flag.store(true, Ordering::SeqCst);
            }
        }
        let _ = app.emit("jarvis:wake-level", json!({ "level": 0.0 }));

        if !enabled {
            return Ok(());
        }

        let stop = Arc::new(AtomicBool::new(false));
        {
            let mut guard = self.wake_stop.lock().unwrap();
            *guard = Some(Arc::clone(&stop));
        }

        let paused_flag = Arc::clone(&self.wake_paused);
        let busy_transcribing = Arc::new(AtomicBool::new(false));
        let last_wake_ms = Arc::new(Mutex::new(Instant::now() - Duration::from_secs(10)));

        std::thread::spawn(move || {
            let mut pre_roll: VecDeque<Vec<i16>> = VecDeque::with_capacity(8);
            let mut speech_buf: Vec<i16> = Vec::new();
            let mut capturing = false;
            let mut speech_start = Instant::now();
            let mut silence_since: Option<Instant> = None;

            let res = run_wavein_loop(device_id, Arc::clone(&stop), |chunk, rms| {
                if paused_flag.load(Ordering::Relaxed) {
                    capturing = false;
                    speech_buf.clear();
                    silence_since = None;
                    pre_roll.clear();
                    return;
                }

                let norm = (rms * 10.0).min(1.0);
                let _ = app.emit("jarvis:wake-level", json!({ "level": norm }));

                if busy_transcribing.load(Ordering::Relaxed) {
                    return;
                }

                // Local VAD threshold (0.010 works reliably on quiet Windows HD Audio mics too)
                if rms > 0.010 {
                    if !capturing {
                        capturing = true;
                        speech_start = Instant::now();
                        speech_buf.clear();
                        for prev in &pre_roll {
                            speech_buf.extend_from_slice(prev);
                        }
                    }
                    speech_buf.extend_from_slice(chunk);
                    silence_since = None;

                    if speech_start.elapsed() >= Duration::from_millis(3600) {
                        Self::dispatch_wake_clip(
                            &app,
                            &state,
                            &mut speech_buf,
                            &paused_flag,
                            &busy_transcribing,
                            &last_wake_ms,
                        );
                        capturing = false;
                        silence_since = None;
                    }
                } else if capturing {
                    speech_buf.extend_from_slice(chunk);
                    let s = silence_since.get_or_insert_with(Instant::now);
                    if s.elapsed() >= Duration::from_millis(620) {
                        Self::dispatch_wake_clip(
                            &app,
                            &state,
                            &mut speech_buf,
                            &paused_flag,
                            &busy_transcribing,
                            &last_wake_ms,
                        );
                        capturing = false;
                        silence_since = None;
                    }
                } else {
                    pre_roll.push_back(chunk.to_vec());
                    if pre_roll.len() > 6 {
                        pre_roll.pop_front();
                    }
                }
            });

            let _ = app.emit("jarvis:wake-level", json!({ "level": 0.0 }));
            if let Err(err) = res {
                let _ = app.emit("jarvis:wake-error", json!({ "error": err }));
            }
        });

        Ok(())
    }

    fn dispatch_wake_clip(
        app: &AppHandle,
        state: &Arc<AppState>,
        speech_buf: &mut Vec<i16>,
        paused_flag: &Arc<AtomicBool>,
        busy_transcribing: &Arc<AtomicBool>,
        last_wake_ms: &Arc<Mutex<Instant>>,
    ) {
        let samples = std::mem::take(speech_buf);
        // Ignore tiny accidental bumps (< 0.30s)
        if samples.len() < (SAMPLE_RATE as usize) * 3 / 10 {
            return;
        }
        busy_transcribing.store(true, Ordering::SeqCst);

        let wav = encode_pcm16_wav(&samples);
        let b64 = base64::engine::general_purpose::STANDARD.encode(&wav);

        let app_clone = app.clone();
        let state_clone = Arc::clone(state);
        let paused_clone = Arc::clone(paused_flag);
        let busy_clone = Arc::clone(busy_transcribing);
        let last_wake_clone = Arc::clone(last_wake_ms);

        tauri::async_runtime::spawn(async move {
            let res = llm::transcribe_audio(&state_clone, &b64, "audio/wav").await;
            match res {
                Ok(text) => {
                    if !paused_clone.load(Ordering::Relaxed) {
                        if let Some(cmd) = parse_wake_command_rust(&text) {
                            let allow = {
                                let mut last = last_wake_clone.lock().unwrap();
                                if last.elapsed() >= Duration::from_millis(1600) {
                                    *last = Instant::now();
                                    true
                                } else {
                                    false
                                }
                            };
                            if allow {
                                let _ = app_clone.emit(
                                    "jarvis:wake-triggered",
                                    json!({ "command": cmd, "raw": text }),
                                );
                            }
                        }
                    }
                }
                Err(err) => {
                    // Surface real configuration/auth errors so the user is never left wondering!
                    let _ = app_clone.emit("jarvis:wake-error", json!({ "error": err }));
                }
            }
            busy_clone.store(false, Ordering::SeqCst);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wav_encoder_produces_valid_16khz_pcm_header() {
        let wav = encode_pcm16_wav(&[0, 1000, -1000, 32767]);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[36..40], b"data");
        assert_eq!(wav.len(), 44 + 8);
    }

    #[test]
    fn rust_wake_parser_detects_ptbr_variants() {
        assert_eq!(parse_wake_command_rust("Ei, Jarvis"), Some(String::new()));
        assert_eq!(
            parse_wake_command_rust("Ei Jarvis, que horas são?"),
            Some("que horas são?".to_string())
        );
        assert_eq!(
            parse_wake_command_rust("Jarvis abre o YouTube"),
            Some("abre o YouTube".to_string())
        );
        assert_eq!(parse_wake_command_rust("bom dia pessoal"), None);
    }

    #[test]
    fn rms_computation_and_phonetic_wake_variants_work() {
        assert_eq!(compute_rms(&[]), 0.0);
        let silence = vec![0i16; 640];
        assert!(compute_rms(&silence) < 0.0001);
        let tone = vec![16384i16; 640];
        assert!((compute_rms(&tone) - 0.5).abs() < 0.01);

        // Common Portuguese ASR phonetic variants for "Jarvis"
        assert_eq!(
            parse_wake_command_rust("Oi Jarves, abre o navegador"),
            Some("abre o navegador".to_string())
        );
        assert_eq!(
            parse_wake_command_rust("Fala Javes!"),
            Some(String::new())
        );
    }
}
