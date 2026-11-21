use crate::config::{DEFAULT_ELEVENLABS_MODEL_ID, DEFAULT_ELEVENLABS_VOICE_ID};
use base64::Engine as _;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ElevenLabsVoice {
    pub voice_id: String,
    pub name: String,
    pub category: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeechOutput {
    pub mime_type: String,
    pub audio_base64: String,
}

pub fn clean_text_for_speech(text: &str) -> String {
    static RE_CODE_BLOCK: OnceLock<Regex> = OnceLock::new();
    static RE_MD_LINK: OnceLock<Regex> = OnceLock::new();
    static RE_URL: OnceLock<Regex> = OnceLock::new();
    static RE_LINE_PREFIX: OnceLock<Regex> = OnceLock::new();
    static RE_WS: OnceLock<Regex> = OnceLock::new();

    let re_code_block =
        RE_CODE_BLOCK.get_or_init(|| Regex::new(r"(?s)```.*?```").expect("valid code regex"));
    let re_md_link =
        RE_MD_LINK.get_or_init(|| Regex::new(r"\[([^\]]+)\]\([^)]+\)").expect("valid link regex"));
    let re_url = RE_URL.get_or_init(|| Regex::new(r"https?://\S+").expect("valid url regex"));
    let re_line_prefix = RE_LINE_PREFIX.get_or_init(|| {
        Regex::new(r"(?m)^\s*(?:#{1,6}\s+|[-*+]\s+|\d+\.\s+|>\s+)").expect("valid prefix regex")
    });
    let re_ws = RE_WS.get_or_init(|| Regex::new(r"\s+").expect("valid ws regex"));

    let step1 = re_code_block.replace_all(text, " código exibido na tela ");
    let step2 = re_md_link.replace_all(&step1, "$1");
    let step3 = re_url.replace_all(&step2, "");
    let step4 = re_line_prefix.replace_all(&step3, "");
    let step5 = step4.replace(['`', '*', '~', '|'], "");
    re_ws.replace_all(step5.trim(), " ").trim().to_string()
}

fn cache_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("elevenlabs_cache")
}

fn cache_file_path(
    data_dir: &Path,
    voice_id: &str,
    model_id: &str,
    rate: f64,
    clean_text: &str,
) -> PathBuf {
    let mut hasher = Sha256::new();
    hasher.update(
        format!(
            "{}|{}|{:.2}|{}",
            voice_id.trim(),
            model_id.trim(),
            rate,
            clean_text.to_lowercase()
        )
        .as_bytes(),
    );
    let hex = format!("{:x}", hasher.finalize());
    cache_dir(data_dir).join(format!("{hex}.mp3"))
}

fn read_cached_mp3(path: &Path) -> Option<Vec<u8>> {
    let bytes = fs::read(path).ok()?;
    if bytes.len() > 64 {
        Some(bytes)
    } else {
        None
    }
}

fn write_cached_mp3(data_dir: &Path, path: &Path, bytes: &[u8]) {
    if bytes.len() <= 64 {
        return;
    }
    let dir = cache_dir(data_dir);
    if fs::create_dir_all(&dir).is_err() {
        return;
    }
    let tmp = path.with_extension(format!("tmp-{}", Uuid::new_v4()));
    if fs::write(&tmp, bytes).is_ok() {
        let _ = fs::rename(&tmp, path);
    } else {
        let _ = fs::remove_file(&tmp);
    }

    if let Ok(entries) = fs::read_dir(&dir) {
        let mut files: Vec<(PathBuf, std::time::SystemTime)> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("mp3"))
            .filter_map(|e| {
                let mtime = e.metadata().ok()?.modified().ok()?;
                Some((e.path(), mtime))
            })
            .collect();
        if files.len() > 200 {
            files.sort_by_key(|(_, t)| *t);
            for (old_path, _) in files.into_iter().take(30) {
                let _ = fs::remove_file(old_path);
            }
        }
    }
}

pub async fn list_elevenlabs_voices(
    client: &reqwest::Client,
    api_key: &str,
) -> Result<Vec<ElevenLabsVoice>, String> {
    let resp = client
        .get("https://api.elevenlabs.io/v1/voices")
        .header("xi-api-key", api_key.trim())
        .send()
        .await
        .map_err(|e| format!("Erro de rede ao consultar vozes ElevenLabs: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let detail = resp.text().await.unwrap_or_default();
        return Err(format!("ElevenLabs recusou ({status}): {detail}"));
    }

    let parsed: Value = resp
        .json()
        .await
        .map_err(|e| format!("Resposta inválida da ElevenLabs: {e}"))?;

    let mut out = Vec::new();
    if let Some(arr) = parsed["voices"].as_array() {
        for item in arr {
            let voice_id = item["voice_id"].as_str().unwrap_or("").trim().to_string();
            let name = item["name"].as_str().unwrap_or("").trim().to_string();
            let category = item["category"]
                .as_str()
                .unwrap_or("premade")
                .trim()
                .to_string();
            if !voice_id.is_empty() && !name.is_empty() {
                out.push(ElevenLabsVoice {
                    voice_id,
                    name,
                    category,
                });
            }
        }
    }

    out.sort_by(|a, b| {
        let rank_a = if a.category == "premade" { 1 } else { 0 };
        let rank_b = if b.category == "premade" { 1 } else { 0 };
        (rank_a, &a.name).cmp(&(rank_b, &b.name))
    });

    Ok(out)
}

pub async fn synthesize_elevenlabs_mp3(
    client: &reqwest::Client,
    data_dir: &Path,
    api_key: &str,
    text: &str,
    voice_id: &str,
    model_id: &str,
    rate: f64,
) -> Result<Vec<u8>, String> {
    static RE_JARVIS: OnceLock<Regex> = OnceLock::new();
    let raw_clean = clean_text_for_speech(text);
    if raw_clean.is_empty() {
        return Err("Texto vazio para síntese de voz".to_string());
    }
    let re_jarvis = RE_JARVIS
        .get_or_init(|| Regex::new(r"(?i)\bj\.a\.r\.v\.i\.s\.?").expect("valid jarvis regex"));
    let clean = re_jarvis.replace_all(&raw_clean, "Jarvis").to_string();

    let v_id = if voice_id.trim().is_empty() {
        DEFAULT_ELEVENLABS_VOICE_ID
    } else {
        voice_id.trim()
    };
    let m_id = if model_id.trim().is_empty() {
        DEFAULT_ELEVENLABS_MODEL_ID
    } else {
        model_id.trim()
    };
    let speed = rate.clamp(0.7, 1.2);

    let cached_path = cache_file_path(data_dir, v_id, m_id, speed, &clean);
    if let Some(hit) = read_cached_mp3(&cached_path) {
        return Ok(hit);
    }

    let url =
        format!("https://api.elevenlabs.io/v1/text-to-speech/{v_id}?output_format=mp3_44100_128");
    let body = json!({
        "text": clean,
        "model_id": m_id,
        "voice_settings": {
            "stability": 0.55,
            "similarity_boost": 0.80,
            "style": 0.0,
            "use_speaker_boost": true,
            "speed": speed
        }
    });

    let resp = client
        .post(&url)
        .header("xi-api-key", api_key.trim())
        .header("Accept", "audio/mpeg")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Erro de rede na ElevenLabs: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let detail = resp.text().await.unwrap_or_default();
        return Err(format!("ElevenLabs falhou ({status}): {detail}"));
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| format!("Falha ao ler áudio da ElevenLabs: {e}"))?
        .to_vec();

    if bytes.len() <= 64 {
        return Err("Áudio retornado pela ElevenLabs está vazio.".to_string());
    }

    write_cached_mp3(data_dir, &cached_path, &bytes);
    Ok(bytes)
}

pub fn encode_speech_output(mime_type: &str, bytes: &[u8]) -> SpeechOutput {
    SpeechOutput {
        mime_type: mime_type.to_string(),
        audio_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_text_strips_markdown_and_urls() {
        let input = "## **Status**\n- CPU em `24%`\nVeja [link](https://example.com) e https://foo.bar\n```rust\nlet x = 1;\n```";
        let cleaned = clean_text_for_speech(input);
        assert_eq!(
            cleaned,
            "Status CPU em 24% Veja link e código exibido na tela"
        );
    }

    #[test]
    fn elevenlabs_cache_roundtrip_saves_credits() {
        let dir = std::env::temp_dir().join(format!("jarvis-11labs-cache-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();

        let path = cache_file_path(
            &dir,
            DEFAULT_ELEVENLABS_VOICE_ID,
            DEFAULT_ELEVENLABS_MODEL_ID,
            1.0,
            "Olá, senhor.",
        );
        assert!(read_cached_mp3(&path).is_none());

        let fake_mp3 = vec![0xFFu8; 128];
        write_cached_mp3(&dir, &path, &fake_mp3);

        let hit = read_cached_mp3(&path).expect("should read cached mp3");
        assert_eq!(hit.len(), 128);

        let _ = fs::remove_dir_all(&dir);
    }
}
