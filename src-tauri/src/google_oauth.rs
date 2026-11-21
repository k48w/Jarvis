use crate::config::{get_api_key, KeySlot};
use crate::oauth::{
    decode_jwt_claims, html_page, parse_callback_query, pkce_s256_challenge, random_base64url,
    read_http_request_target, write_http_response,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::Mutex;

const GOOGLE_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const GOOGLE_USERINFO_URL: &str = "https://openidconnect.googleapis.com/v1/userinfo";
const GOOGLE_REVOKE_URL: &str = "https://oauth2.googleapis.com/revoke";
const GOOGLE_SCOPES: &str = "openid email profile https://www.googleapis.com/auth/cloud-platform";

const KEYRING_SERVICE: &str = "studio.jarvis.cinematic";
const KEYRING_GOOGLE_SESSION: &str = "google_oauth_session";

static GOOGLE_REFRESH_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn refresh_mutex() -> &'static Mutex<()> {
    GOOGLE_REFRESH_LOCK.get_or_init(|| Mutex::new(()))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoogleOAuthSession {
    pub client_id: String,
    pub project_id: Option<String>,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: i64,
    pub scope: String,
    pub email: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GoogleAuthStatus {
    pub connected: bool,
    pub email: Option<String>,
    pub name: Option<String>,
    pub project_id: Option<String>,
    pub expires_at: Option<i64>,
}

fn fallback_session_path(data_dir: &Path) -> PathBuf {
    data_dir.join("oauth_google.json")
}

pub fn load_google_session(data_dir: &Path) -> Option<GoogleOAuthSession> {
    if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, KEYRING_GOOGLE_SESSION) {
        if let Ok(raw) = entry.get_password() {
            if let Ok(session) = serde_json::from_str::<GoogleOAuthSession>(&raw) {
                return Some(session);
            }
        }
    }
    crate::config::read_encrypted_fallback(&fallback_session_path(data_dir))
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .and_then(|raw| serde_json::from_str::<GoogleOAuthSession>(&raw).ok())
}

fn save_google_session(data_dir: &Path, session: &GoogleOAuthSession) -> Result<(), String> {
    let raw = serde_json::to_string(session)
        .map_err(|e| format!("Falha ao serializar sessão Google OAuth: {e}"))?;
    let stored_in_keyring = keyring::Entry::new(KEYRING_SERVICE, KEYRING_GOOGLE_SESSION)
        .and_then(|e| e.set_password(&raw))
        .is_ok();

    let fallback = fallback_session_path(data_dir);
    if stored_in_keyring {
        if fallback.exists() {
            let _ = fs::remove_file(fallback);
        }
        Ok(())
    } else {
        eprintln!("Aviso de segurança: sessão Google salva via armazenamento alternativo cifrado (DPAPI).");
        crate::config::write_encrypted_fallback(&fallback, raw.as_bytes())
    }
}

fn delete_google_session(data_dir: &Path) {
    if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, KEYRING_GOOGLE_SESSION) {
        let _ = entry.delete_credential();
    }
    let fallback = fallback_session_path(data_dir);
    if fallback.exists() {
        let _ = fs::remove_file(fallback);
    }
}

pub fn get_google_auth_status(data_dir: &Path) -> GoogleAuthStatus {
    match load_google_session(data_dir) {
        Some(s) => GoogleAuthStatus {
            connected: true,
            email: s.email,
            name: s.name,
            project_id: s.project_id,
            expires_at: Some(s.expires_at),
        },
        None => GoogleAuthStatus::default(),
    }
}

pub fn parse_google_credentials_json(
    raw: &str,
) -> Result<(String, String, Option<String>), String> {
    let parsed: Value = serde_json::from_str(raw.trim())
        .map_err(|_| "Arquivo JSON inválido do Google Cloud Console.".to_string())?;

    let root = parsed
        .get("installed")
        .or_else(|| parsed.get("web"))
        .unwrap_or(&parsed);

    let client_id = root["client_id"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "JSON não contém o campo 'client_id'.".to_string())?
        .to_string();

    let client_secret = root["client_secret"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            "JSON não contém 'client_secret'. Use credenciais OAuth do tipo Aplicativo para Computador (Desktop)."
                .to_string()
        })?
        .to_string();

    let project_id = root["project_id"]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    Ok((client_id, client_secret, project_id))
}

pub fn build_google_authorize_url(
    client_id: &str,
    redirect_uri: &str,
    code_challenge: &str,
    state: &str,
) -> Result<String, String> {
    let mut url =
        url::Url::parse(GOOGLE_AUTH_URL).map_err(|e| format!("URL Google OAuth inválida: {e}"))?;
    {
        let mut q = url.query_pairs_mut();
        q.append_pair("client_id", client_id.trim());
        q.append_pair("redirect_uri", redirect_uri);
        q.append_pair("response_type", "code");
        q.append_pair("scope", GOOGLE_SCOPES);
        q.append_pair("code_challenge", code_challenge);
        q.append_pair("code_challenge_method", "S256");
        q.append_pair("state", state);
        q.append_pair("access_type", "offline");
        q.append_pair("prompt", "consent");
    }
    Ok(url.to_string())
}

async fn fetch_google_userinfo(
    http: &reqwest::Client,
    access_token: &str,
) -> (Option<String>, Option<String>) {
    let Ok(resp) = http
        .get(GOOGLE_USERINFO_URL)
        .bearer_auth(access_token)
        .send()
        .await
    else {
        return (None, None);
    };
    if !resp.status().is_success() {
        return (None, None);
    }
    let Ok(json) = resp.json::<Value>().await else {
        return (None, None);
    };
    let email = json["email"].as_str().map(str::to_string);
    let name = json["name"].as_str().map(str::to_string);
    (email, name)
}

pub async fn start_google_login(
    http: &reqwest::Client,
    data_dir: &Path,
    client_id: &str,
    client_secret: &str,
    project_id: Option<&str>,
) -> Result<GoogleAuthStatus, String> {
    let cid = client_id.trim();
    let secret = client_secret.trim();
    if cid.is_empty() || secret.is_empty() {
        return Err(
            "Configure o Client ID e o Client Secret (ou importe o JSON do Google Cloud) antes de entrar com o Google."
                .to_string(),
        );
    }

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("Falha ao abrir porta local para callback Google OAuth: {e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("Falha ao ler porta local: {e}"))?
        .port();
    let redirect_uri = format!("http://127.0.0.1:{port}/google/callback");

    let verifier = random_base64url(32)?;
    let challenge = pkce_s256_challenge(&verifier);
    let expected_state = random_base64url(24)?;

    let auth_url = build_google_authorize_url(cid, &redirect_uri, &challenge, &expected_state)?;

    open::that(&auth_url)
        .map_err(|e| format!("Não foi possível abrir o navegador para login Google: {e}"))?;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(180);

    let (mut callback_stream, callback) = loop {
        let now = tokio::time::Instant::now();
        if now >= deadline {
            return Err("Tempo esgotado aguardando login com a conta Google.".to_string());
        }
        let remaining = deadline - now;
        let accept_res = tokio::time::timeout(remaining, listener.accept()).await;
        let (mut stream, _) = match accept_res {
            Ok(Ok(pair)) => pair,
            Ok(Err(e)) => return Err(format!("Erro ao receber retorno do Google: {e}")),
            Err(_) => return Err("Tempo esgotado aguardando login com a conta Google.".to_string()),
        };

        let Some(target) = read_http_request_target(&mut stream).await else {
            continue;
        };
        let (path, query) = target.split_once('?').unwrap_or((target.as_str(), ""));
        if path != "/google/callback" {
            write_http_response(&mut stream, "204 No Content", "text/plain", "").await;
            continue;
        }

        match parse_callback_query(query) {
            Ok(cb) if cb.state == expected_state => break (stream, cb),
            Ok(_) => {
                let page = html_page(
                    "J.A.R.V.I.S. — Erro Google OAuth",
                    "Sessão OAuth inválida",
                    "O parâmetro state não confere. Feche esta aba e tente novamente no J.A.R.V.I.S.",
                    false,
                );
                write_http_response(&mut stream, "400 Bad Request", "text/html", &page).await;
                return Err("Validação de segurança state do Google OAuth falhou.".to_string());
            }
            Err(err) => {
                let page = html_page(
                    "J.A.R.V.I.S. — Login cancelado",
                    "Login Google não concluído",
                    &err,
                    false,
                );
                write_http_response(&mut stream, "400 Bad Request", "text/html", &page).await;
                return Err(err);
            }
        }
    };

    let token_res = http
        .post(GOOGLE_TOKEN_URL)
        .header("Accept", "application/json")
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", callback.code.as_str()),
            ("client_id", cid),
            ("client_secret", secret),
            ("redirect_uri", redirect_uri.as_str()),
            ("code_verifier", verifier.as_str()),
        ])
        .send()
        .await;

    let token_resp = match token_res {
        Ok(r) => r,
        Err(e) => {
            let msg = format!("Erro de rede ao trocar código Google OAuth: {e}");
            let page = html_page(
                "J.A.R.V.I.S. — Erro Google",
                "Falha de conexão",
                &msg,
                false,
            );
            write_http_response(&mut callback_stream, "502 Bad Gateway", "text/html", &page).await;
            return Err(msg);
        }
    };

    if !token_resp.status().is_success() {
        let status = token_resp.status();
        let err_body = token_resp.text().await.unwrap_or_default();
        let msg = format!("Google recusou o token OAuth ({status}): {err_body}");
        let page = html_page(
            "J.A.R.V.I.S. — Erro Google",
            "Autenticação recusada",
            &msg,
            false,
        );
        write_http_response(&mut callback_stream, "400 Bad Request", "text/html", &page).await;
        return Err(msg);
    }

    let token_json: Value = token_resp
        .json()
        .await
        .map_err(|e| format!("Resposta inválida de token Google OAuth: {e}"))?;

    let access_token = token_json["access_token"]
        .as_str()
        .map(str::to_string)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "Resposta do Google não incluiu access_token.".to_string())?;
    let refresh_token = token_json["refresh_token"]
        .as_str()
        .map(str::to_string)
        .filter(|s| !s.is_empty());
    let expires_in = token_json["expires_in"].as_i64().unwrap_or(3600);
    let scope = token_json["scope"]
        .as_str()
        .map(str::to_string)
        .or(callback.scope)
        .unwrap_or_else(|| GOOGLE_SCOPES.to_string());

    let mut email = None;
    let mut name = None;
    if let Some(id_tok) = token_json["id_token"].as_str() {
        if let Some(claims) = decode_jwt_claims(id_tok) {
            email = claims["email"].as_str().map(str::to_string);
            name = claims["name"].as_str().map(str::to_string);
        }
    }
    if email.is_none() && name.is_none() {
        let (u_email, u_name) = fetch_google_userinfo(http, &access_token).await;
        email = u_email;
        name = u_name;
    }

    let clean_project = project_id
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let session = GoogleOAuthSession {
        client_id: cid.to_string(),
        project_id: clean_project,
        access_token,
        refresh_token,
        expires_at: Utc::now().timestamp() + expires_in,
        scope,
        email,
        name,
    };
    save_google_session(data_dir, &session)?;

    let page = html_page(
        "J.A.R.V.I.S. — Google conectado",
        "J.A.R.V.I.S. conectado à conta Google",
        "Autenticação concluída com sucesso. Você já pode fechar esta aba e voltar ao J.A.R.V.I.S.",
        true,
    );
    write_http_response(&mut callback_stream, "200 OK", "text/html", &page).await;

    Ok(get_google_auth_status(data_dir))
}

pub async fn get_valid_google_token(
    http: &reqwest::Client,
    data_dir: &Path,
) -> Result<(String, Option<String>), String> {
    let _guard = refresh_mutex().lock().await;
    let mut session = load_google_session(data_dir).ok_or_else(|| {
        "Conta Google não conectada. Abra Ajustes → IA → Conta Google (OAuth) e clique em Entrar com Google."
            .to_string()
    })?;

    let now = Utc::now().timestamp();
    if session.expires_at - 60 > now {
        return Ok((session.access_token, session.project_id));
    }

    let Some(refresh_token) = session.refresh_token.clone() else {
        return Err(
            "Sessão Google expirada. Clique em Entrar com Google (OAuth) novamente em Ajustes → IA."
                .to_string(),
        );
    };
    let secret = get_api_key(data_dir, KeySlot::GoogleClientSecret)
        .ok_or_else(|| "Client Secret do Google OAuth ausente para renovar o token.".to_string())?;

    let resp = http
        .post(GOOGLE_TOKEN_URL)
        .header("Accept", "application/json")
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", session.client_id.as_str()),
            ("client_secret", secret.as_str()),
            ("refresh_token", refresh_token.as_str()),
        ])
        .send()
        .await
        .map_err(|e| format!("Falha ao renovar sessão Google: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let detail = resp.text().await.unwrap_or_default();
        if status.as_u16() == 400 || status.as_u16() == 401 {
            delete_google_session(data_dir);
        }
        return Err(format!(
            "Sessão Google expirada ou revogada ({status}). Entre novamente em Ajustes → IA. {detail}"
        ));
    }

    let json: Value = resp
        .json()
        .await
        .map_err(|e| format!("Resposta inválida ao renovar token Google: {e}"))?;

    let new_access = json["access_token"]
        .as_str()
        .map(str::to_string)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "Renovação Google OAuth não retornou access_token.".to_string())?;
    let new_refresh = json["refresh_token"]
        .as_str()
        .map(str::to_string)
        .filter(|s| !s.is_empty());
    let expires_in = json["expires_in"].as_i64().unwrap_or(3600);
    if let Some(scope) = json["scope"].as_str() {
        session.scope = scope.to_string();
    }
    session.access_token = new_access.clone();
    if new_refresh.is_some() {
        session.refresh_token = new_refresh;
    }
    session.expires_at = Utc::now().timestamp() + expires_in;
    save_google_session(data_dir, &session)?;

    Ok((new_access, session.project_id))
}

pub async fn logout_google(
    http: &reqwest::Client,
    data_dir: &Path,
) -> Result<GoogleAuthStatus, String> {
    if let Some(session) = load_google_session(data_dir) {
        let token_to_revoke = session
            .refresh_token
            .as_deref()
            .unwrap_or(session.access_token.as_str());
        let _ = http
            .post(GOOGLE_REVOKE_URL)
            .form(&[("token", token_to_revoke)])
            .send()
            .await;
    }
    delete_google_session(data_dir);
    Ok(GoogleAuthStatus::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn google_authorize_url_includes_pkce_and_offline_params() {
        let url = build_google_authorize_url(
            "123456-abc.apps.googleusercontent.com",
            "http://127.0.0.1:54321/google/callback",
            "CHALLENGE_VALUE",
            "STATE_VALUE",
        )
        .unwrap();

        assert!(url.starts_with("https://accounts.google.com/o/oauth2/v2/auth?"));
        assert!(url.contains("127.0.0.1%3A54321%2Fgoogle%2Fcallback"));
        assert!(url.contains("code_challenge_method=S256"));
        assert!(url.contains("access_type=offline"));
        assert!(url.contains("prompt=consent"));
        assert!(url.contains("cloud-platform"));
    }

    #[test]
    fn google_credentials_json_parses_installed_and_web_and_rejects_invalid() {
        let installed = r#"{
            "installed": {
                "client_id": "abc.apps.googleusercontent.com",
                "project_id": "my-jarvis-proj",
                "client_secret": "GOCSPX-secret123"
            }
        }"#;
        let (cid, sec, proj) = parse_google_credentials_json(installed).unwrap();
        assert_eq!(cid, "abc.apps.googleusercontent.com");
        assert_eq!(sec, "GOCSPX-secret123");
        assert_eq!(proj.as_deref(), Some("my-jarvis-proj"));

        let web = r#"{
            "web": {
                "client_id": "web.apps.googleusercontent.com",
                "client_secret": "GOCSPX-websecret"
            }
        }"#;
        let (wcid, wsec, wproj) = parse_google_credentials_json(web).unwrap();
        assert_eq!(wcid, "web.apps.googleusercontent.com");
        assert_eq!(wsec, "GOCSPX-websecret");
        assert_eq!(wproj, None);

        assert!(parse_google_credentials_json("{not json").is_err());
        assert!(parse_google_credentials_json(r#"{"installed":{"client_id":"only"}}"#).is_err());
    }
}
