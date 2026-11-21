//! # Auto-Updater via GitHub Releases — `updater.rs`
//!
//! Verifica novas versões publicadas em `https://github.com/k48w/Jarvis/releases`,
//! baixa o instalador `.exe` oficial com validação estrita de origem HTTPS e
//! verificação de cabeçalho executável (`MZ`), e inicia a atualização em 1 clique.

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::cmp::Ordering;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use url::Url;

const GITHUB_RELEASE_API: &str = "https://api.github.com/repos/k48w/Jarvis/releases/latest";
const ALLOWED_DOWNLOAD_HOSTS: &[&str] = &[
    "github.com",
    "objects.githubusercontent.com",
    "release-assets.githubusercontent.com",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCheckResult {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub release_title: String,
    pub release_notes: String,
    pub published_at: String,
    pub html_url: String,
    pub installer_name: Option<String>,
    pub installer_url: Option<String>,
    pub installer_size: u64,
    pub installer_updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgressEvent {
    pub stage: String,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub percent: u8,
}

/// Normaliza e compara duas strings de versão (`v8.0.0`, `8.1.0`, etc.).
pub fn compare_semver(current: &str, latest: &str) -> Ordering {
    fn parse_parts(v: &str) -> Vec<u64> {
        v.trim()
            .trim_start_matches(['v', 'V'])
            .split(['.', '-', '+'])
            .take(3)
            .map(|part| part.parse::<u64>().unwrap_or(0))
            .collect()
    }
    let a = parse_parts(current);
    let b = parse_parts(latest);
    for idx in 0..3 {
        let av = *a.get(idx).unwrap_or(&0);
        let bv = *b.get(idx).unwrap_or(&0);
        match av.cmp(&bv) {
            Ordering::Equal => continue,
            non_eq => return non_eq,
        }
    }
    Ordering::Equal
}

/// Garante que o download do instalador só ocorra a partir do repositório oficial no GitHub via HTTPS.
pub fn validate_release_asset_url(raw_url: &str) -> Result<Url, String> {
    let parsed =
        Url::parse(raw_url.trim()).map_err(|e| format!("URL de atualização inválida: {e}"))?;
    if parsed.scheme() != "https" {
        return Err("O download de atualização exige HTTPS.".to_string());
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| "URL de atualização sem host válido.".to_string())?
        .to_ascii_lowercase();
    if !ALLOWED_DOWNLOAD_HOSTS.contains(&host.as_str()) {
        return Err(format!(
            "Host '{}' não autorizado para download de atualização.",
            host
        ));
    }
    if host == "github.com" && !parsed.path().starts_with("/k48w/Jarvis/releases/download/") {
        return Err(
            "O instalador deve pertencer exclusivamente a github.com/k48w/Jarvis/releases."
                .to_string(),
        );
    }
    Ok(parsed)
}

pub fn sanitize_installer_filename(raw_name: &str) -> String {
    let base: String = raw_name
        .trim()
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("JARVIS-setup.exe")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        .collect();
    if base.to_ascii_lowercase().ends_with(".exe") && base.len() >= 5 {
        base
    } else {
        "JARVIS-setup.exe".to_string()
    }
}

/// Consulta a release mais recente no GitHub e retorna metadados + link do instalador `.exe`.
pub async fn check_for_update() -> Result<UpdateCheckResult, String> {
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(12))
        .user_agent(format!("Jarvis-Desktop-Updater/{current_version}"))
        .build()
        .map_err(|e| format!("Falha ao iniciar cliente HTTP do updater: {e}"))?;

    let resp = client
        .get(GITHUB_RELEASE_API)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("Não foi possível consultar o GitHub Releases: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!(
            "GitHub Releases respondeu com status HTTP {}.",
            resp.status()
        ));
    }

    let body: Value = resp
        .json()
        .await
        .map_err(|e| format!("Resposta inválida do GitHub Releases: {e}"))?;

    let tag_name = body["tag_name"]
        .as_str()
        .unwrap_or(&current_version)
        .trim_start_matches(['v', 'V'])
        .to_string();
    let release_title = body["name"]
        .as_str()
        .unwrap_or("Jarvis para Windows")
        .to_string();
    let release_notes = body["body"].as_str().unwrap_or("").to_string();
    let published_at = body["published_at"].as_str().unwrap_or("").to_string();
    let html_url = body["html_url"]
        .as_str()
        .unwrap_or("https://github.com/k48w/Jarvis/releases")
        .to_string();

    let assets = body["assets"].as_array().cloned().unwrap_or_default();
    // Prioriza o instalador NSIS (*-setup.exe), com fallback para JARVIS.exe
    let chosen_asset = assets
        .iter()
        .find(|a| {
            a["name"]
                .as_str()
                .is_some_and(|n| n.to_ascii_lowercase().ends_with("setup.exe"))
        })
        .or_else(|| {
            assets.iter().find(|a| {
                a["name"]
                    .as_str()
                    .is_some_and(|n| n.to_ascii_lowercase().ends_with(".exe"))
            })
        });

    let (installer_name, installer_url, installer_size, installer_updated_at) = match chosen_asset {
        Some(asset) => (
            asset["name"].as_str().map(|s| s.to_string()),
            asset["browser_download_url"]
                .as_str()
                .map(|s| s.to_string()),
            asset["size"].as_u64().unwrap_or(0),
            asset["updated_at"].as_str().map(|s| s.to_string()),
        ),
        None => (None, None, 0, None),
    };

    let update_available = compare_semver(&current_version, &tag_name) == Ordering::Less;

    Ok(UpdateCheckResult {
        current_version,
        latest_version: tag_name,
        update_available,
        release_title,
        release_notes,
        published_at,
        html_url,
        installer_name,
        installer_url,
        installer_size,
        installer_updated_at,
    })
}

/// Baixa o instalador oficial do GitHub Releases com progresso em tempo real e inicia a instalação.
pub async fn download_and_launch_installer(
    app: &AppHandle,
    download_url: &str,
    file_name: &str,
) -> Result<String, String> {
    let valid_url = validate_release_asset_url(download_url)?;
    let safe_name = sanitize_installer_filename(file_name);

    let update_dir: PathBuf = std::env::temp_dir().join("jarvis-updater");
    fs::create_dir_all(&update_dir)
        .map_err(|e| format!("Falha ao preparar pasta temporária de atualização: {e}"))?;
    let dest_path = update_dir.join(&safe_name);

    let _ = app.emit(
        "jarvis:update-progress",
        UpdateProgressEvent {
            stage: "downloading".to_string(),
            downloaded_bytes: 0,
            total_bytes: 0,
            percent: 0,
        },
    );

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(180))
        .user_agent(format!(
            "Jarvis-Desktop-Updater/{}",
            env!("CARGO_PKG_VERSION")
        ))
        .build()
        .map_err(|e| format!("Falha ao iniciar cliente de download: {e}"))?;

    let resp = client
        .get(valid_url)
        .send()
        .await
        .map_err(|e| format!("Falha ao baixar instalador: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!(
            "Download do instalador falhou com HTTP {}.",
            resp.status()
        ));
    }

    // Valida o host final após redirecionamento do GitHub para o CDN de assets
    validate_release_asset_url(resp.url().as_str())?;

    let total_bytes = resp.content_length().unwrap_or(0);
    let mut downloaded_bytes = 0u64;
    let mut file = fs::File::create(&dest_path)
        .map_err(|e| format!("Falha ao criar arquivo do instalador: {e}"))?;
    let mut first_two_bytes = Vec::with_capacity(2);

    let mut stream = resp.bytes_stream();
    while let Some(chunk_res) = stream.next().await {
        let chunk = chunk_res.map_err(|e| format!("Erro durante download da atualização: {e}"))?;
        if first_two_bytes.len() < 2 {
            for &b in chunk.iter() {
                if first_two_bytes.len() < 2 {
                    first_two_bytes.push(b);
                }
            }
        }
        file.write_all(&chunk)
            .map_err(|e| format!("Erro ao gravar instalador em disco: {e}"))?;
        downloaded_bytes += chunk.len() as u64;
        let percent = downloaded_bytes
            .saturating_mul(100)
            .checked_div(total_bytes)
            .map_or(50, |p| p.min(100) as u8);
        let _ = app.emit(
            "jarvis:update-progress",
            UpdateProgressEvent {
                stage: "downloading".to_string(),
                downloaded_bytes,
                total_bytes,
                percent,
            },
        );
    }
    file.flush()
        .map_err(|e| format!("Falha ao finalizar arquivo do instalador: {e}"))?;
    drop(file);

    // Verifica assinatura PE ("MZ") para garantir que um binário Windows real foi baixado
    if first_two_bytes != b"MZ" || downloaded_bytes < 512 * 1024 {
        let _ = fs::remove_file(&dest_path);
        return Err(
            "O arquivo baixado não é um executável Windows válido (cabeçalho PE ausente)."
                .to_string(),
        );
    }

    let _ = app.emit(
        "jarvis:update-progress",
        UpdateProgressEvent {
            stage: "launching".to_string(),
            downloaded_bytes,
            total_bytes: downloaded_bytes,
            percent: 100,
        },
    );

    std::process::Command::new(&dest_path)
        .spawn()
        .map_err(|e| format!("Falha ao iniciar o instalador '{}': {e}", safe_name))?;

    Ok(format!(
        "Instalador '{}' baixado ({:.2} MB) e iniciado.",
        safe_name,
        downloaded_bytes as f64 / (1024.0 * 1024.0)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semver_comparison_handles_prefixes_and_patches() {
        assert_eq!(compare_semver("8.0.0", "v8.0.0"), Ordering::Equal);
        assert_eq!(compare_semver("v8.0.0", "v8.0.1"), Ordering::Less);
        assert_eq!(compare_semver("8.1.0", "8.0.9"), Ordering::Greater);
    }

    #[test]
    fn release_asset_url_validator_enforces_github_https_origin() {
        assert!(validate_release_asset_url(
            "https://github.com/k48w/Jarvis/releases/download/v8.0.0/JARVIS_8.0.0_x64-setup.exe"
        )
        .is_ok());
        assert!(validate_release_asset_url(
            "https://objects.githubusercontent.com/github-production-release-asset-2e65be/12345"
        )
        .is_ok());
        // HTTP puro ou outro repositório/domínio devem ser recusados
        assert!(validate_release_asset_url(
            "http://github.com/k48w/Jarvis/releases/download/v8.0.0/JARVIS.exe"
        )
        .is_err());
        assert!(validate_release_asset_url(
            "https://github.com/evil/repo/releases/download/v1.0/bad.exe"
        )
        .is_err());
        assert!(validate_release_asset_url("https://127.0.0.1/JARVIS.exe").is_err());
    }

    #[test]
    fn installer_filename_sanitizer_strips_traversal() {
        assert_eq!(
            sanitize_installer_filename("../../JARVIS_8.0.0_x64-setup.exe"),
            "JARVIS_8.0.0_x64-setup.exe"
        );
        assert_eq!(sanitize_installer_filename("invalid"), "JARVIS-setup.exe");
    }
}
