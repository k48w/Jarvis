//! # Histórico Inteligente da Área de Transferência (`Ctrl+C`) — `clipboard_history.rs`
//!
//! Monitora de forma leve as alterações na área de transferência do Windows via
//! `GetClipboardSequenceNumber()` (zero leitura quando nada mudou), mantém os
//! últimos 20 trechos de texto/código apenas em memória RAM (nunca gravados em
//! disco) e descarta automaticamente prováveis senhas, tokens e chaves privadas.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

const MAX_HISTORY_ITEMS: usize = 20;
const MAX_SNIPPET_CHARS: usize = 6000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardHistoryEntry {
    pub id: String,
    pub text: String,
    pub preview: String,
    pub kind: String,
    pub chars: usize,
    pub copied_at: String,
}

#[derive(Default)]
pub struct ClipboardHistoryState {
    entries: Mutex<VecDeque<ClipboardHistoryEntry>>,
    paused: AtomicBool,
}

impl ClipboardHistoryState {
    pub fn list(&self) -> Vec<ClipboardHistoryEntry> {
        self.entries
            .lock()
            .map(|q| q.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn clear(&self) {
        if let Ok(mut q) = self.entries.lock() {
            q.clear();
        }
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    pub fn set_paused(&self, paused: bool) -> bool {
        self.paused.store(paused, Ordering::SeqCst);
        paused
    }

    /// Registra um novo texto copiado caso não seja segredo nem duplicado do item no topo.
    pub fn push_text(&self, raw: &str) -> Option<ClipboardHistoryEntry> {
        if self.is_paused() {
            return None;
        }
        let trimmed = raw.trim();
        if trimmed.len() < 2 || looks_like_secret(trimmed) {
            return None;
        }
        let clipped: String = trimmed.chars().take(MAX_SNIPPET_CHARS).collect();
        let mut q = self.entries.lock().ok()?;
        if q.front().is_some_and(|top| top.text == clipped) {
            return None;
        }
        // Remove duplicata antiga para promover o item ao topo
        q.retain(|item| item.text != clipped);
        let preview = build_preview(&clipped);
        let entry = ClipboardHistoryEntry {
            id: format!("clip-{}", Uuid::new_v4().simple()),
            kind: classify_snippet(&clipped).to_string(),
            chars: clipped.chars().count(),
            preview,
            text: clipped,
            copied_at: Utc::now().to_rfc3339(),
        };
        q.push_front(entry.clone());
        while q.len() > MAX_HISTORY_ITEMS {
            q.pop_back();
        }
        Some(entry)
    }
}

/// Filtra chaves de API, tokens Bearer/JWT, chaves privadas PEM e prováveis senhas de 1 linha.
pub fn looks_like_secret(text: &str) -> bool {
    let t = text.trim();
    if t.contains("-----BEGIN ")
        || t.contains("PRIVATE KEY")
        || t.starts_with("sk-")
        || t.starts_with("ghp_")
        || t.starts_with("gho_")
        || t.starts_with("github_pat_")
        || t.starts_with("AIza")
        || t.starts_with("AKIA")
        || t.starts_with("xoxb-")
        || t.starts_with("xoxp-")
        || t.starts_with("Bearer ")
        || t.starts_with("eyJ")
    {
        return true;
    }
    // Linha única sem espaços, entre 16 e 128 chars, misturando letras, números e símbolos (cara de senha/token)
    if !t.contains('\n') && !t.contains(' ') && (16..=128).contains(&t.len()) {
        let has_upper = t.chars().any(|c| c.is_ascii_uppercase());
        let has_lower = t.chars().any(|c| c.is_ascii_lowercase());
        let has_digit = t.chars().any(|c| c.is_ascii_digit());
        let has_symbol = t.chars().any(|c| !c.is_ascii_alphanumeric());
        if has_upper && has_lower && has_digit && has_symbol && !t.starts_with("http") {
            return true;
        }
    }
    false
}

pub fn classify_snippet(text: &str) -> &'static str {
    let lower = text.to_ascii_lowercase();
    if !text.contains('\n') && (lower.starts_with("http://") || lower.starts_with("https://")) {
        return "url";
    }
    if lower.contains("error:")
        || lower.contains("error[e")
        || lower.contains("traceback")
        || lower.contains("exception")
        || lower.contains("panicked at")
        || lower.contains("uncaught ")
    {
        return "error";
    }
    if text.contains("fn ")
        || text.contains("function ")
        || text.contains("const ")
        || text.contains("import ")
        || text.contains("=>")
        || (text.contains('{') && text.contains('}') && text.contains(';'))
    {
        return "code";
    }
    "text"
}

fn build_preview(text: &str) -> String {
    let one_line = text
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let mut out: String = one_line.chars().take(120).collect();
    if one_line.chars().count() > 120 {
        out.push('…');
    }
    out
}

#[cfg(windows)]
fn current_clipboard_seq() -> u32 {
    use windows::Win32::System::DataExchange::GetClipboardSequenceNumber;
    unsafe { GetClipboardSequenceNumber() }
}

#[cfg(not(windows))]
fn current_clipboard_seq() -> u32 {
    0
}

/// Inicia o monitor leve de área de transferência em background.
pub fn start_clipboard_watcher(app: AppHandle, state: Arc<ClipboardHistoryState>) {
    tauri::async_runtime::spawn(async move {
        let mut last_seq = current_clipboard_seq();
        loop {
            tokio::time::sleep(Duration::from_millis(750)).await;
            if state.is_paused() {
                last_seq = current_clipboard_seq();
                continue;
            }
            let seq = current_clipboard_seq();
            if seq == 0 || seq == last_seq {
                continue;
            }
            last_seq = seq;

            if let Ok(Ok(content)) =
                tokio::task::spawn_blocking(crate::tools::read_clipboard_blocking).await
            {
                if content.kind == "text" {
                    if let Some(txt) = content.text {
                        if let Some(entry) = state.push_text(&txt) {
                            let _ = app.emit("jarvis:clipboard-history-updated", entry);
                        }
                    }
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_history_filters_secrets_classifies_and_deduplicates() {
        let state = ClipboardHistoryState::default();

        // Segredos nunca devem entrar no histórico
        assert!(state.push_text("sk-proj-1234567890abcdefABCDEF").is_none());
        assert!(state
            .push_text("ghp_1234567890abcdefghij1234567890")
            .is_none());
        assert!(state.push_text("MyS3cret!Passw0rd#2026").is_none());
        assert!(state.list().is_empty());

        // Classificação e deduplicação
        let e1 = state.push_text("https://github.com/k48w/Jarvis").unwrap();
        assert_eq!(e1.kind, "url");
        let e2 = state
            .push_text("error[E0425]: cannot find value `x` in this scope")
            .unwrap();
        assert_eq!(e2.kind, "error");
        let e3 = state
            .push_text("pub fn soma(a: i32, b: i32) -> i32 { a + b }")
            .unwrap();
        assert_eq!(e3.kind, "code");

        // Repetir item já existente no topo não duplica
        assert!(state
            .push_text("pub fn soma(a: i32, b: i32) -> i32 { a + b }")
            .is_none());
        assert_eq!(state.list().len(), 3);

        // Pausar impede novas capturas
        state.set_paused(true);
        assert!(state.push_text("texto ignorado enquanto pausado").is_none());
        assert_eq!(state.list().len(), 3);
    }
}
