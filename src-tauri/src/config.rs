use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const KEYRING_SERVICE: &str = "studio.jarvis.cinematic";
pub const DEFAULT_ELEVENLABS_VOICE_ID: &str = "JBFqnCBsd6RMkjVDRZzb";
pub const DEFAULT_ELEVENLABS_MODEL_ID: &str = "eleven_flash_v2_5";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    #[default]
    Chatgpt,
    Gemini,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PermissionMode {
    #[default]
    Auto,
    Review,
    Safe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TtsEngine {
    #[default]
    Elevenlabs,
    Off,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TtsReplyMode {
    #[default]
    Brief,
    Miconly,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SttEngine {
    #[default]
    Auto,
    Elevenlabs,
    Gemini,
    Browser,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum GeminiAuthMode {
    #[default]
    Apikey,
    Oauth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeySlot {
    Gemini,
    GoogleClientSecret,
    Elevenlabs,
}

impl KeySlot {
    pub fn as_str(self) -> &'static str {
        match self {
            KeySlot::Gemini => "gemini",
            KeySlot::GoogleClientSecret => "google_client_secret",
            KeySlot::Elevenlabs => "elevenlabs",
        }
    }
}

pub fn default_system_prompt() -> String {
    "Você é J.A.R.V.I.S., um assistente pessoal de desktop para Windows, inteligente, preciso e leal.\n\
Responda sempre em português do Brasil. Seja extremamente conciso e direto: em conversas rápidas e comandos do sistema, responda em apenas 1 frase curta de 5 a 14 palavras (ex.: 'Sim, senhor. Tudo operando normalmente.' ou 'Pronto, senhor. Volume ajustado.').\n\
Quando o usuário pedir explicações detalhadas, código ou pesquisas longas, comece sempre com uma primeira frase curta de até 14 palavras que resuma a resposta (pois apenas essa primeira frase será falada em voz alta) e coloque os detalhes nos parágrafos seguintes.\n\
Use as ferramentas disponíveis proativamente quando precisar de status do sistema, abrir programas, ajustar volume, controlar mídia, buscar na web ou criar timers."
        .to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub provider: Provider,
    pub gemini_auth_mode: GeminiAuthMode,
    pub google_oauth_client_id: String,
    pub google_oauth_project_id: String,
    pub gemini_model: String,
    pub chatgpt_model: String,
    pub permission_mode: PermissionMode,
    pub system_prompt: String,
    pub tts_engine: TtsEngine,
    pub tts_reply_mode: TtsReplyMode,
    pub tts_rate: f64,
    pub elevenlabs_voice_id: String,
    pub elevenlabs_model_id: String,
    pub stt_engine: SttEngine,
    pub mic_device_id: String,
    pub auto_listen_vad: bool,
    pub wake_word_enabled: bool,
    pub hands_free_mode: bool,
    pub global_shortcut: String,
    pub minimize_to_tray: bool,
    pub web_search_enabled: bool,
    pub companion_enabled: bool,
    pub companion_lan_enabled: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            provider: Provider::Chatgpt,
            gemini_auth_mode: GeminiAuthMode::Apikey,
            google_oauth_client_id: String::new(),
            google_oauth_project_id: String::new(),
            gemini_model: "gemini-2.5-flash".to_string(),
            chatgpt_model: "gpt-6.2-sol".to_string(),
            permission_mode: PermissionMode::Auto,
            system_prompt: default_system_prompt(),
            tts_engine: TtsEngine::Elevenlabs,
            tts_reply_mode: TtsReplyMode::Brief,
            tts_rate: 1.0,
            elevenlabs_voice_id: DEFAULT_ELEVENLABS_VOICE_ID.to_string(),
            elevenlabs_model_id: DEFAULT_ELEVENLABS_MODEL_ID.to_string(),
            stt_engine: SttEngine::Auto,
            mic_device_id: String::new(),
            auto_listen_vad: true,
            wake_word_enabled: false,
            hands_free_mode: false,
            global_shortcut: "Alt+Space".to_string(),
            minimize_to_tray: true,
            web_search_enabled: true,
            companion_enabled: false,
            companion_lan_enabled: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyStatus {
    pub configured: bool,
    pub preview: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AllKeyStatus {
    pub gemini: KeyStatus,
    pub google_client_secret: KeyStatus,
    pub elevenlabs: KeyStatus,
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Falha ao criar pasta: {e}"))?;
    }
    let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
    fs::write(&tmp, bytes).map_err(|e| format!("Falha ao gravar arquivo temporário: {e}"))?;
    if let Err(err) = fs::rename(&tmp, path) {
        if path.exists() {
            let _ = fs::remove_file(path);
            fs::rename(&tmp, path).map_err(|e| {
                let _ = fs::remove_file(&tmp);
                format!("Falha ao substituir arquivo: {e}")
            })?;
        } else {
            let _ = fs::remove_file(&tmp);
            return Err(format!("Falha ao renomear arquivo: {err}"));
        }
    }
    Ok(())
}

fn config_path(data_dir: &Path) -> PathBuf {
    data_dir.join("config.json")
}

fn secrets_path(data_dir: &Path) -> PathBuf {
    data_dir.join("secrets.json")
}

pub fn load_config(data_dir: &Path) -> AppConfig {
    let path = config_path(data_dir);
    match fs::read_to_string(&path) {
        Ok(raw) => serde_json::from_str::<AppConfig>(&raw).unwrap_or_default(),
        Err(_) => AppConfig::default(),
    }
}

pub fn save_config_file(data_dir: &Path, cfg: &AppConfig) -> Result<(), String> {
    let mut normalized = cfg.clone();
    normalized.tts_rate = normalized.tts_rate.clamp(0.7, 1.2);
    if normalized.elevenlabs_voice_id.trim().is_empty() {
        normalized.elevenlabs_voice_id = DEFAULT_ELEVENLABS_VOICE_ID.to_string();
    }
    if normalized.elevenlabs_model_id.trim().is_empty() {
        normalized.elevenlabs_model_id = DEFAULT_ELEVENLABS_MODEL_ID.to_string();
    }
    if normalized.system_prompt.trim().is_empty() {
        normalized.system_prompt = default_system_prompt();
    }
    let json = serde_json::to_vec_pretty(&normalized)
        .map_err(|e| format!("Falha ao serializar configuração: {e}"))?;
    atomic_write(&config_path(data_dir), &json)
}

const DPAPI_PREFIX: &str = "DPAPI1:";

#[cfg(windows)]
pub(crate) fn protect_bytes_dpapi(plain: &[u8]) -> Result<Vec<u8>, String> {
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{
        CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    let mut in_blob = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(plain.len())
            .map_err(|_| "Segredo excede tamanho DPAPI".to_string())?,
        pbData: plain.as_ptr() as *mut u8,
    };
    let mut out_blob = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptProtectData(
            &mut in_blob,
            None,
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out_blob,
        )
        .map_err(|e| format!("Falha ao cifrar credencial com Windows DPAPI: {e}"))?;
        let slice = std::slice::from_raw_parts(out_blob.pbData, out_blob.cbData as usize);
        let result = slice.to_vec();
        let _ = LocalFree(Some(HLOCAL(out_blob.pbData.cast())));
        Ok(result)
    }
}

#[cfg(not(windows))]
pub(crate) fn protect_bytes_dpapi(_plain: &[u8]) -> Result<Vec<u8>, String> {
    Err("Armazenamento seguro indisponível fora do Windows sem keyring do sistema.".to_string())
}

#[cfg(windows)]
pub(crate) fn unprotect_bytes_dpapi(cipher: &[u8]) -> Result<Vec<u8>, String> {
    use windows::Win32::Foundation::{LocalFree, HLOCAL};
    use windows::Win32::Security::Cryptography::{
        CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    let mut in_blob = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(cipher.len()).map_err(|_| "Cipher DPAPI inválido".to_string())?,
        pbData: cipher.as_ptr() as *mut u8,
    };
    let mut out_blob = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptUnprotectData(
            &mut in_blob,
            None,
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out_blob,
        )
        .map_err(|e| format!("Falha ao decifrar credencial DPAPI: {e}"))?;
        let slice = std::slice::from_raw_parts(out_blob.pbData, out_blob.cbData as usize);
        let result = slice.to_vec();
        let _ = LocalFree(Some(HLOCAL(out_blob.pbData.cast())));
        Ok(result)
    }
}

#[cfg(not(windows))]
pub(crate) fn unprotect_bytes_dpapi(_cipher: &[u8]) -> Result<Vec<u8>, String> {
    Err("Decifração DPAPI disponível apenas no Windows.".to_string())
}

pub(crate) fn write_encrypted_fallback(path: &Path, plaintext: &[u8]) -> Result<(), String> {
    use base64::Engine as _;
    let cipher = protect_bytes_dpapi(plaintext)?;
    let encoded = format!(
        "{DPAPI_PREFIX}{}",
        base64::engine::general_purpose::STANDARD.encode(&cipher)
    );
    atomic_write(path, encoded.as_bytes())
}

pub(crate) fn read_encrypted_fallback(path: &Path) -> Option<Vec<u8>> {
    use base64::Engine as _;
    let raw = fs::read_to_string(path).ok()?;
    let trimmed = raw.trim();
    let payload = trimmed.strip_prefix(DPAPI_PREFIX)?;
    let cipher = base64::engine::general_purpose::STANDARD
        .decode(payload)
        .ok()?;
    unprotect_bytes_dpapi(&cipher).ok()
}

fn read_fallback_secrets(data_dir: &Path) -> BTreeMap<String, String> {
    let path = secrets_path(data_dir);
    read_encrypted_fallback(&path)
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn write_fallback_secrets(data_dir: &Path, map: &BTreeMap<String, String>) -> Result<(), String> {
    if map.is_empty() {
        let p = secrets_path(data_dir);
        if p.exists() {
            let _ = fs::remove_file(p);
        }
        return Ok(());
    }
    let json =
        serde_json::to_vec_pretty(map).map_err(|e| format!("Falha ao serializar chaves: {e}"))?;
    eprintln!("Aviso de segurança: usando armazenamento alternativo criptografado (DPAPI) porque o Gerenciador de Credenciais não aceitou gravação.");
    write_encrypted_fallback(&secrets_path(data_dir), &json)
}

pub fn get_api_key(data_dir: &Path, slot: KeySlot) -> Option<String> {
    let name = slot.as_str();
    if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, name) {
        if let Ok(secret) = entry.get_password() {
            let trimmed = secret.trim().to_string();
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
        }
    }
    read_fallback_secrets(data_dir)
        .get(name)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

pub fn save_api_key(data_dir: &Path, slot: KeySlot, key: &str) -> Result<(), String> {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return clear_api_key(data_dir, slot);
    }
    let name = slot.as_str();
    let stored_in_keyring = keyring::Entry::new(KEYRING_SERVICE, name)
        .and_then(|entry| entry.set_password(trimmed))
        .is_ok();

    let mut fallback = read_fallback_secrets(data_dir);
    if stored_in_keyring {
        if fallback.remove(name).is_some() {
            let _ = write_fallback_secrets(data_dir, &fallback);
        }
        Ok(())
    } else {
        fallback.insert(name.to_string(), trimmed.to_string());
        write_fallback_secrets(data_dir, &fallback)
    }
}

pub fn clear_api_key(data_dir: &Path, slot: KeySlot) -> Result<(), String> {
    let name = slot.as_str();
    if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, name) {
        let _ = entry.delete_credential();
    }
    let mut fallback = read_fallback_secrets(data_dir);
    if fallback.remove(name).is_some() {
        write_fallback_secrets(data_dir, &fallback)?;
    }
    Ok(())
}

pub fn mask_key(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    if chars.len() >= 10 {
        let head: String = chars[..4].iter().collect();
        let tail: String = chars[chars.len() - 4..].iter().collect();
        format!("{head}...{tail}")
    } else {
        "••••".to_string()
    }
}

pub fn key_status_for(data_dir: &Path, slot: KeySlot) -> KeyStatus {
    match get_api_key(data_dir, slot) {
        Some(key) => KeyStatus {
            configured: true,
            preview: Some(mask_key(&key)),
        },
        None => KeyStatus {
            configured: false,
            preview: None,
        },
    }
}

pub fn get_all_key_status(data_dir: &Path) -> AllKeyStatus {
    AllKeyStatus {
        gemini: key_status_for(data_dir, KeySlot::Gemini),
        google_client_secret: key_status_for(data_dir, KeySlot::GoogleClientSecret),
        elevenlabs: key_status_for(data_dir, KeySlot::Elevenlabs),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_secret_storage_never_writes_plaintext() {
        let dir =
            std::env::temp_dir().join(format!("jarvis-secrets-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let secret_val = "sk-super-secret-value-do-not-store-plain-12345";
        let mut map = BTreeMap::new();
        map.insert("gemini".to_string(), secret_val.to_string());

        let res = write_fallback_secrets(&dir, &map);
        let disk_path = secrets_path(&dir);
        if cfg!(windows) {
            res.expect("DPAPI fallback should succeed on Windows");
            let on_disk = fs::read_to_string(&disk_path).expect("encrypted file should exist");
            assert!(on_disk.starts_with(DPAPI_PREFIX));
            assert!(!on_disk.contains(secret_val));
            let decoded = read_fallback_secrets(&dir);
            assert_eq!(decoded.get("gemini").map(String::as_str), Some(secret_val));
        } else {
            assert!(res.is_err(), "Non-Windows without keyring must fail closed");
            assert!(!disk_path.exists());
        }

        // Plaintext legacy file must never be accepted as secret material
        let plain_path = dir.join("plain_probe.json");
        fs::write(&plain_path, secret_val.as_bytes()).unwrap();
        assert!(read_encrypted_fallback(&plain_path).is_none());

        let _ = fs::remove_dir_all(&dir);
    }
}
