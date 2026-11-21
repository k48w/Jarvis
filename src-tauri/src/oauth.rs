use crate::config::{atomic_write, mask_key};
use base64::Engine as _;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use uuid::Uuid;

const AUTH_URL: &str = "https://auth.openai.com/api/accounts/authorize";
const TOKEN_URL: &str = "https://auth.openai.com/api/accounts/oauth/token";
const USERINFO_URL: &str = "https://auth.openai.com/api/accounts/oauth/userinfo";
const REVOKE_URL: &str = "https://auth.openai.com/api/accounts/oauth/revoke";
const RESOURCE_URL: &str = "https://api.openai.com/v1";
const OAUTH_SCOPES: &str =
    "openid profile email offline_access resource.invoke chatgpt.tokens.use.direct";
const DYNAMIC_CLIENT_ID: &str = "dynamic_agent_client";
const AGENT_NAME: &str = "J.A.R.V.I.S.";

const KEYRING_SERVICE: &str = "studio.jarvis.cinematic";
const KEYRING_SESSION_KEY: &str = "chatgpt_oauth_session";

static REFRESH_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn refresh_mutex() -> &'static Mutex<()> {
    REFRESH_LOCK.get_or_init(|| Mutex::new(()))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HostRegistration {
    host_id: String,
    issued_client_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatGptOAuthSession {
    pub client_id: String,
    pub host_id: String,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: i64,
    pub scope: String,
    pub email: Option<String>,
    pub name: Option<String>,
    pub plan_usage_granted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ChatGptAuthStatus {
    pub connected: bool,
    pub email: Option<String>,
    pub name: Option<String>,
    pub plan_usage_granted: bool,
    pub client_id_preview: Option<String>,
    pub expires_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatGptLoginResult {
    pub status: ChatGptAuthStatus,
    pub models: Vec<String>,
    pub selected_model: String,
}

fn host_reg_path(data_dir: &Path) -> PathBuf {
    data_dir.join("chatgpt_host.json")
}

fn fallback_session_path(data_dir: &Path) -> PathBuf {
    data_dir.join("oauth_chatgpt.json")
}

fn load_or_init_host_registration(data_dir: &Path) -> Result<HostRegistration, String> {
    let path = host_reg_path(data_dir);
    if let Ok(raw) = fs::read_to_string(&path) {
        if let Ok(reg) = serde_json::from_str::<HostRegistration>(&raw) {
            if !reg.host_id.trim().is_empty() {
                return Ok(reg);
            }
        }
    }
    let reg = HostRegistration {
        host_id: format!("urn:uuid:{}", Uuid::new_v4()),
        issued_client_id: None,
    };
    save_host_registration(data_dir, &reg)?;
    Ok(reg)
}

fn save_host_registration(data_dir: &Path, reg: &HostRegistration) -> Result<(), String> {
    let json = serde_json::to_vec_pretty(reg)
        .map_err(|e| format!("Falha ao serializar registro OAuth: {e}"))?;
    atomic_write(&host_reg_path(data_dir), &json)
}

pub fn load_session(data_dir: &Path) -> Option<ChatGptOAuthSession> {
    if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, KEYRING_SESSION_KEY) {
        if let Ok(raw) = entry.get_password() {
            if let Ok(session) = serde_json::from_str::<ChatGptOAuthSession>(&raw) {
                return Some(session);
            }
        }
    }
    crate::config::read_encrypted_fallback(&fallback_session_path(data_dir))
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .and_then(|raw| serde_json::from_str::<ChatGptOAuthSession>(&raw).ok())
}

fn save_session(data_dir: &Path, session: &ChatGptOAuthSession) -> Result<(), String> {
    let raw = serde_json::to_string(session)
        .map_err(|e| format!("Falha ao serializar sessão OAuth: {e}"))?;
    let stored_in_keyring = keyring::Entry::new(KEYRING_SERVICE, KEYRING_SESSION_KEY)
        .and_then(|e| e.set_password(&raw))
        .is_ok();

    let fallback = fallback_session_path(data_dir);
    if stored_in_keyring {
        if fallback.exists() {
            let _ = fs::remove_file(fallback);
        }
        Ok(())
    } else {
        eprintln!("Aviso de segurança: sessão ChatGPT salva via armazenamento alternativo cifrado (DPAPI).");
        crate::config::write_encrypted_fallback(&fallback, raw.as_bytes())
    }
}

fn delete_session(data_dir: &Path) {
    if let Ok(entry) = keyring::Entry::new(KEYRING_SERVICE, KEYRING_SESSION_KEY) {
        let _ = entry.delete_credential();
    }
    let fallback = fallback_session_path(data_dir);
    if fallback.exists() {
        let _ = fs::remove_file(fallback);
    }
}

pub fn get_auth_status(data_dir: &Path) -> ChatGptAuthStatus {
    match load_session(data_dir) {
        Some(s) => ChatGptAuthStatus {
            connected: true,
            email: s.email,
            name: s.name,
            plan_usage_granted: s.plan_usage_granted,
            client_id_preview: Some(mask_key(&s.client_id)),
            expires_at: Some(s.expires_at),
        },
        None => ChatGptAuthStatus::default(),
    }
}

pub(crate) fn random_base64url(len: usize) -> Result<String, String> {
    let mut bytes = vec![0u8; len];
    getrandom::getrandom(&mut bytes)
        .map_err(|e| format!("Falha ao gerar aleatoriedade segura: {e}"))?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes))
}

pub fn pkce_s256_challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
}

pub fn build_authorize_url(
    redirect_uri: &str,
    host_id: &str,
    issued_client_id: Option<&str>,
    code_challenge: &str,
    state: &str,
    nonce: &str,
) -> Result<String, String> {
    let mut url = url::Url::parse(AUTH_URL).map_err(|e| format!("URL OAuth inválida: {e}"))?;
    {
        let mut q = url.query_pairs_mut();
        q.append_pair("response_type", "code");
        q.append_pair("redirect_uri", redirect_uri);
        q.append_pair("scope", OAUTH_SCOPES);
        q.append_pair("resource", RESOURCE_URL);
        q.append_pair("code_challenge", code_challenge);
        q.append_pair("code_challenge_method", "S256");
        q.append_pair("state", state);
        q.append_pair("nonce", nonce);
        q.append_pair("ext_agent_host_id", host_id);
        match issued_client_id.map(str::trim).filter(|s| !s.is_empty()) {
            Some(cid) => {
                q.append_pair("client_id", cid);
            }
            None => {
                q.append_pair("client_id", DYNAMIC_CLIENT_ID);
                q.append_pair("agent_name_hint", AGENT_NAME);
            }
        }
    }
    Ok(url.to_string())
}

#[derive(Debug, PartialEq, Eq)]
pub struct CallbackParams {
    pub code: String,
    pub state: String,
    pub client_id: Option<String>,
    pub scope: Option<String>,
}

pub fn parse_callback_query(query: &str) -> Result<CallbackParams, String> {
    let params: HashMap<String, String> = url::form_urlencoded::parse(query.as_bytes())
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();

    if let Some(err) = params.get("error") {
        let desc = params
            .get("error_description")
            .map(String::as_str)
            .unwrap_or(err);
        return Err(format!("Autorização ChatGPT recusada: {desc}"));
    }

    let code = params
        .get("code")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "Callback OAuth sem código de autorização.".to_string())?;
    let state = params
        .get("state")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "Callback OAuth sem parâmetro state.".to_string())?;

    Ok(CallbackParams {
        code,
        state,
        client_id: params
            .get("client_id")
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
        scope: params
            .get("scope")
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty()),
    })
}

pub fn decode_jwt_claims(jwt: &str) -> Option<Value> {
    let part = jwt.split('.').nth(1)?;
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(part)
        .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(part))
        .ok()?;
    serde_json::from_slice::<Value>(&decoded).ok()
}

fn has_plan_usage_scope(scope_str: &str, access_token: &str) -> bool {
    if scope_str
        .split_whitespace()
        .any(|s| s == "chatgpt.tokens.use.direct")
    {
        return true;
    }
    if let Some(claims) = decode_jwt_claims(access_token) {
        if let Some(scp) = claims["scp"].as_array() {
            if scp
                .iter()
                .any(|v| v.as_str() == Some("chatgpt.tokens.use.direct"))
            {
                return true;
            }
        }
        if let Some(scope) = claims["scope"].as_str() {
            if scope
                .split_whitespace()
                .any(|s| s == "chatgpt.tokens.use.direct")
            {
                return true;
            }
        }
    }
    false
}

pub(crate) fn html_page(title: &str, heading: &str, body: &str, ok: bool) -> String {
    let accent = if ok { "#a3edf7" } else { "#ffc58a" };
    format!(
        "<!doctype html><html lang=\"pt-BR\"><head><meta charset=\"utf-8\"><title>{title}</title>\
        <style>body{{margin:0;min-height:100vh;display:grid;place-items:center;background:#02040a;\
        color:#e7f4fc;font-family:system-ui,sans-serif}}.card{{max-width:440px;padding:28px;\
        border-radius:16px;border:1px solid #4c788b66;background:#07131b;text-align:center}}\
        h1{{margin:0 0 10px;font-size:20px;color:{accent}}}p{{margin:0;line-height:1.5;color:#c3d9e2;font-size:14px}}</style>\
        </head><body><div class=\"card\"><h1>{heading}</h1><p>{body}</p></div></body></html>"
    )
}

pub(crate) async fn write_http_response(
    stream: &mut tokio::net::TcpStream,
    status_line: &str,
    content_type: &str,
    body: &str,
) {
    let resp = format!(
        "HTTP/1.1 {status_line}\r\nContent-Type: {content_type}; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(resp.as_bytes()).await;
    let _ = stream.flush().await;
}

pub(crate) async fn read_http_request_target(stream: &mut tokio::net::TcpStream) -> Option<String> {
    let mut buf = Vec::with_capacity(4096);
    let mut chunk = [0u8; 1024];
    loop {
        let n = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut chunk))
            .await
            .ok()?
            .ok()?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") || buf.len() >= 16384 {
            break;
        }
    }
    let req_str = String::from_utf8_lossy(&buf);
    let first_line = req_str.lines().next()?;
    let mut parts = first_line.split_whitespace();
    let _method = parts.next()?;
    let target = parts.next()?;
    Some(target.to_string())
}

async fn fetch_userinfo(
    http: &reqwest::Client,
    access_token: &str,
) -> (Option<String>, Option<String>) {
    let Ok(resp) = http
        .get(USERINFO_URL)
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
    let name = json["name"]
        .as_str()
        .or_else(|| json["preferred_username"].as_str())
        .map(str::to_string);
    (email, name)
}

pub async fn start_chatgpt_login(
    http: &reqwest::Client,
    data_dir: &Path,
    force_register: bool,
) -> Result<(ChatGptAuthStatus, Vec<String>), String> {
    let mut reg = load_or_init_host_registration(data_dir)?;
    if force_register {
        reg.issued_client_id = None;
    }

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("Falha ao abrir porta local para callback OAuth: {e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("Falha ao ler porta local: {e}"))?
        .port();
    let redirect_uri = format!("http://127.0.0.1:{port}/auth/callback");

    let verifier = random_base64url(32)?;
    let challenge = pkce_s256_challenge(&verifier);
    let expected_state = random_base64url(24)?;
    let nonce = random_base64url(24)?;

    let auth_url = build_authorize_url(
        &redirect_uri,
        &reg.host_id,
        reg.issued_client_id.as_deref(),
        &challenge,
        &expected_state,
        &nonce,
    )?;

    open::that(&auth_url)
        .map_err(|e| format!("Não foi possível abrir o navegador para login no ChatGPT: {e}"))?;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(180);

    let (mut callback_stream, callback) = loop {
        let now = tokio::time::Instant::now();
        if now >= deadline {
            return Err("Tempo esgotado aguardando login no ChatGPT pelo navegador.".to_string());
        }
        let remaining = deadline - now;
        let accept_res = tokio::time::timeout(remaining, listener.accept()).await;
        let (mut stream, _) = match accept_res {
            Ok(Ok(pair)) => pair,
            Ok(Err(e)) => return Err(format!("Erro ao receber retorno OAuth: {e}")),
            Err(_) => {
                return Err("Tempo esgotado aguardando login no ChatGPT pelo navegador.".to_string())
            }
        };

        let Some(target) = read_http_request_target(&mut stream).await else {
            continue;
        };
        let (path, query) = target.split_once('?').unwrap_or((target.as_str(), ""));
        if path != "/auth/callback" {
            write_http_response(&mut stream, "204 No Content", "text/plain", "").await;
            continue;
        }

        match parse_callback_query(query) {
            Ok(cb) if cb.state == expected_state => break (stream, cb),
            Ok(_) => {
                let page = html_page(
                    "J.A.R.V.I.S. — Erro OAuth",
                    "Sessão OAuth inválida",
                    "O parâmetro de segurança state não confere. Feche esta aba e tente novamente no J.A.R.V.I.S.",
                    false,
                );
                write_http_response(&mut stream, "400 Bad Request", "text/html", &page).await;
                return Err("Validação de segurança state do OAuth falhou.".to_string());
            }
            Err(err) => {
                let page = html_page(
                    "J.A.R.V.I.S. — Login cancelado",
                    "Login não concluído",
                    &err,
                    false,
                );
                write_http_response(&mut stream, "400 Bad Request", "text/html", &page).await;
                return Err(err);
            }
        }
    };

    let client_id = callback
        .client_id
        .or_else(|| reg.issued_client_id.clone())
        .ok_or_else(|| {
            "A resposta OAuth do ChatGPT não retornou um client_id emitido.".to_string()
        })?;

    let token_res = http
        .post(TOKEN_URL)
        .header("Accept", "application/json")
        .form(&[
            ("grant_type", "authorization_code"),
            ("client_id", client_id.as_str()),
            ("code", callback.code.as_str()),
            ("code_verifier", verifier.as_str()),
            ("redirect_uri", redirect_uri.as_str()),
            ("resource", RESOURCE_URL),
        ])
        .send()
        .await;

    let token_resp = match token_res {
        Ok(r) => r,
        Err(e) => {
            let msg = format!("Erro de rede ao trocar código OAuth: {e}");
            let page = html_page("J.A.R.V.I.S. — Erro OAuth", "Falha de conexão", &msg, false);
            write_http_response(&mut callback_stream, "502 Bad Gateway", "text/html", &page).await;
            return Err(msg);
        }
    };

    if !token_resp.status().is_success() {
        let status = token_resp.status();
        let err_body = token_resp.text().await.unwrap_or_default();
        let msg = format!("OpenAI recusou o token OAuth ({status}): {err_body}");
        let page = html_page(
            "J.A.R.V.I.S. — Erro OAuth",
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
        .map_err(|e| format!("Resposta inválida de token OAuth: {e}"))?;

    let access_token = token_json["access_token"]
        .as_str()
        .map(str::to_string)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "Resposta OAuth não incluiu access_token.".to_string())?;
    let refresh_token = token_json["refresh_token"]
        .as_str()
        .map(str::to_string)
        .filter(|s| !s.is_empty());
    let expires_in = token_json["expires_in"].as_i64().unwrap_or(3600);
    let scope = token_json["scope"]
        .as_str()
        .map(str::to_string)
        .or(callback.scope)
        .unwrap_or_else(|| OAUTH_SCOPES.to_string());

    let mut email = None;
    let mut name = None;
    if let Some(id_tok) = token_json["id_token"].as_str() {
        if let Some(claims) = decode_jwt_claims(id_tok) {
            email = claims["email"].as_str().map(str::to_string);
            name = claims["name"]
                .as_str()
                .or_else(|| claims["preferred_username"].as_str())
                .map(str::to_string);
        }
    }
    if email.is_none() && name.is_none() {
        let (u_email, u_name) = fetch_userinfo(http, &access_token).await;
        email = u_email;
        name = u_name;
    }

    let plan_usage_granted = has_plan_usage_scope(&scope, &access_token);
    reg.issued_client_id = Some(client_id.clone());
    save_host_registration(data_dir, &reg)?;

    let session = ChatGptOAuthSession {
        client_id,
        host_id: reg.host_id,
        access_token: access_token.clone(),
        refresh_token,
        expires_at: Utc::now().timestamp() + expires_in,
        scope,
        email,
        name,
        plan_usage_granted,
    };
    save_session(data_dir, &session)?;

    let page = html_page(
        "J.A.R.V.I.S. — Conectado",
        "J.A.R.V.I.S. conectado ao ChatGPT",
        "Autenticação concluída com sucesso. Você já pode fechar esta aba e voltar ao J.A.R.V.I.S.",
        true,
    );
    write_http_response(&mut callback_stream, "200 OK", "text/html", &page).await;

    let models = list_chatgpt_models_with_token(http, &access_token)
        .await
        .unwrap_or_default();

    Ok((get_auth_status(data_dir), models))
}

pub async fn get_valid_access_token(
    http: &reqwest::Client,
    data_dir: &Path,
) -> Result<String, String> {
    let _guard = refresh_mutex().lock().await;
    let mut session = load_session(data_dir).ok_or_else(|| {
        "Conta ChatGPT não conectada. Clique em Ajustes → IA → Entrar com ChatGPT (OAuth)."
            .to_string()
    })?;

    let now = Utc::now().timestamp();
    if session.expires_at - 60 > now {
        return Ok(session.access_token);
    }

    let Some(refresh_token) = session.refresh_token.clone() else {
        return Err("Sessão ChatGPT expirada. Faça login novamente em Ajustes → IA.".to_string());
    };

    let resp = http
        .post(TOKEN_URL)
        .header("Accept", "application/json")
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", session.client_id.as_str()),
            ("refresh_token", refresh_token.as_str()),
            ("resource", RESOURCE_URL),
        ])
        .send()
        .await
        .map_err(|e| format!("Falha ao renovar sessão ChatGPT: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let detail = resp.text().await.unwrap_or_default();
        if status.as_u16() == 400 || status.as_u16() == 401 {
            delete_session(data_dir);
        }
        return Err(format!(
            "Sessão ChatGPT expirada ou revogada ({status}). Entre novamente em Ajustes → IA. {detail}"
        ));
    }

    let json: Value = resp
        .json()
        .await
        .map_err(|e| format!("Resposta inválida ao renovar token ChatGPT: {e}"))?;

    let new_access = json["access_token"]
        .as_str()
        .map(str::to_string)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "Renovação OAuth não retornou access_token.".to_string())?;
    let new_refresh = json["refresh_token"]
        .as_str()
        .map(str::to_string)
        .filter(|s| !s.is_empty());
    let expires_in = json["expires_in"].as_i64().unwrap_or(3600);
    if let Some(scope) = json["scope"].as_str() {
        session.scope = scope.to_string();
        session.plan_usage_granted = has_plan_usage_scope(&session.scope, &new_access);
    }
    session.access_token = new_access.clone();
    if new_refresh.is_some() {
        session.refresh_token = new_refresh;
    }
    session.expires_at = Utc::now().timestamp() + expires_in;
    save_session(data_dir, &session)?;

    Ok(new_access)
}

pub async fn logout_chatgpt(
    http: &reqwest::Client,
    data_dir: &Path,
    reset_registration: bool,
) -> Result<ChatGptAuthStatus, String> {
    if let Some(session) = load_session(data_dir) {
        if let Some(refresh_tok) = session.refresh_token {
            let _ = http
                .post(REVOKE_URL)
                .form(&[
                    ("token", refresh_tok.as_str()),
                    ("client_id", session.client_id.as_str()),
                ])
                .send()
                .await;
        }
    }
    delete_session(data_dir);
    if reset_registration {
        if let Ok(mut reg) = load_or_init_host_registration(data_dir) {
            reg.issued_client_id = None;
            let _ = save_host_registration(data_dir, &reg);
        }
    }
    Ok(ChatGptAuthStatus::default())
}

fn rank_chatgpt_model(id: &str) -> (u8, String) {
    let l = id.to_ascii_lowercase();
    let tier = match l.as_str() {
        "gpt-6.2-sol" => 0,
        "gpt-6.2-astra" => 1,
        "gpt-6.1-sol" => 2,
        "gpt-6-luna" => 3,
        _ if l.starts_with("gpt-6") => 4,
        _ if l.starts_with("gpt-5") => 5,
        _ if l.starts_with("o4") || l.starts_with("o3") => 6,
        _ if l.starts_with("gpt-4") => 7,
        _ => 8,
    };
    (tier, l)
}

pub fn pick_best_chatgpt_model(models: &[String], rejected: &[String]) -> String {
    let is_rejected = |candidate: &str| {
        rejected
            .iter()
            .any(|r| r.eq_ignore_ascii_case(candidate.trim()))
    };
    let preferred = ["gpt-6.2-sol", "gpt-6.2-astra", "gpt-6.1-sol", "gpt-6-luna"];
    for p in preferred {
        if !is_rejected(p) && models.iter().any(|m| m.eq_ignore_ascii_case(p)) {
            return p.to_string();
        }
    }
    if let Some(first_ok) = models.iter().find(|m| !is_rejected(m)) {
        return first_ok.clone();
    }
    for p in ["gpt-6.2-sol", "gpt-6.1-sol", "gpt-6-luna"] {
        if !is_rejected(p) {
            return p.to_string();
        }
    }
    "gpt-6.2-sol".to_string()
}

pub async fn list_chatgpt_models_with_token(
    http: &reqwest::Client,
    access_token: &str,
) -> Result<Vec<String>, String> {
    let resp = http
        .get("https://api.openai.com/v1/models")
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| format!("Erro ao buscar modelos do ChatGPT: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let detail = resp.text().await.unwrap_or_default();
        return Err(format!("Catálogo de modelos recusado ({status}): {detail}"));
    }

    let json: Value = resp
        .json()
        .await
        .map_err(|e| format!("Resposta inválida do catálogo de modelos: {e}"))?;

    let mut all: Vec<String> = json["data"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|m| {
                    m["id"]
                        .as_str()
                        .or_else(|| m["slug"].as_str())
                        .map(str::to_string)
                })
                .collect()
        })
        .unwrap_or_default();

    if all.is_empty() {
        all = json["models"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| {
                        m["slug"]
                            .as_str()
                            .or_else(|| m["id"].as_str())
                            .map(str::to_string)
                    })
                    .collect()
            })
            .unwrap_or_default();
    }

    let mut filtered: Vec<String> = all
        .iter()
        .filter(|id| {
            let l = id.to_ascii_lowercase();
            (l.starts_with("gpt-")
                || l.starts_with("o3")
                || l.starts_with("o4")
                || l.starts_with("o1"))
                && !l.contains("audio")
                && !l.contains("realtime")
                && !l.contains("transcribe")
                && !l.contains("tts")
                && !l.contains("image")
                && !l.contains("search")
        })
        .cloned()
        .collect();

    if filtered.is_empty() {
        filtered = all;
    }

    filtered.sort_by_key(|id| rank_chatgpt_model(id));
    filtered.dedup();
    Ok(filtered)
}

pub async fn list_chatgpt_models(
    http: &reqwest::Client,
    data_dir: &Path,
) -> Result<Vec<String>, String> {
    let token = get_valid_access_token(http, data_dir).await?;
    list_chatgpt_models_with_token(http, &token).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_s256_matches_rfc7636_vector() {
        // RFC 7636 Appendix B test vector.
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        assert_eq!(
            pkce_s256_challenge(verifier),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn authorize_url_handles_dynamic_and_issued_clients() {
        let first = build_authorize_url(
            "http://127.0.0.1:43210/auth/callback",
            "urn:uuid:11111111-2222-4333-8444-555555555555",
            None,
            "CHALLENGE",
            "STATE1",
            "NONCE1",
        )
        .unwrap();
        assert!(first.contains("client_id=dynamic_agent_client"));
        assert!(first.contains("agent_name_hint=J.A.R.V.I.S."));
        assert!(first.contains("chatgpt.tokens.use.direct"));
        assert!(first.contains("resource=https%3A%2F%2Fapi.openai.com%2Fv1"));
        assert!(first.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A43210%2Fauth%2Fcallback"));

        let returning = build_authorize_url(
            "http://127.0.0.1:43210/auth/callback",
            "urn:uuid:11111111-2222-4333-8444-555555555555",
            Some("oaiapp_abc123"),
            "CHALLENGE",
            "STATE2",
            "NONCE2",
        )
        .unwrap();
        assert!(returning.contains("client_id=oaiapp_abc123"));
        assert!(!returning.contains("agent_name_hint"));
    }

    #[test]
    fn callback_parser_and_jwt_decoder_work() {
        let parsed = parse_callback_query(
            "code=AUTH_CODE_1&scope=openid+chatgpt.tokens.use.direct&state=ST_1&client_id=oaiapp_999",
        )
        .unwrap();
        assert_eq!(parsed.code, "AUTH_CODE_1");
        assert_eq!(parsed.state, "ST_1");
        assert_eq!(parsed.client_id.as_deref(), Some("oaiapp_999"));

        let err = parse_callback_query("error=access_denied&error_description=User+cancelled");
        assert!(err.is_err());

        let header = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(r#"{"alg":"RS256"}"#);
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(r#"{"email":"user@example.com","name":"Tony"}"#);
        let jwt = format!("{header}.{payload}.sig");
        let claims = decode_jwt_claims(&jwt).unwrap();
        assert_eq!(claims["email"].as_str(), Some("user@example.com"));
        assert_eq!(claims["name"].as_str(), Some("Tony"));
    }

    #[test]
    fn model_picker_prefers_gpt6_and_skips_rejected() {
        let catalog = vec![
            "gpt-5.2".to_string(),
            "gpt-6-luna".to_string(),
            "gpt-6.2-sol".to_string(),
        ];
        assert_eq!(
            pick_best_chatgpt_model(&catalog, &["gpt-5.2".to_string()]),
            "gpt-6.2-sol"
        );
        assert_eq!(
            pick_best_chatgpt_model(
                &catalog,
                &["gpt-5.2".to_string(), "gpt-6.2-sol".to_string()]
            ),
            "gpt-6-luna"
        );
    }
}
