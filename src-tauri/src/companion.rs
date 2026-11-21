//! # Jarvis Companion (Servidor Mobile & Pareamento Remoto) — `companion.rs`
//!
//! Servidor HTTP/SSE assíncrono embutido no desktop para controle remoto via
//! celular (PWA em `companion_ui.html`).
//!
//! ## Arquitetura de Segurança e Rede
//! - **Desligado e local (`127.0.0.1`) por padrão**: Só faz bind em `0.0.0.0`
//!   quando `enabled` e `lan_enabled` estão ativos.
//! - **Suporte a Wi-Fi Local, Tailscale VPN (`100.64.0.0/10`) e Túnel HTTPS**:
//!   Detecta automaticamente o IP da interface Tailscale para acesso no 4G/5G
//!   fora de casa e aceita URL externa HTTPS (Tailscale Serve/Funnel ou Cloudflare Tunnel).
//! - **Pareamento de Uso Único + Sessão `HttpOnly` (TTL 8h)**: O QR Code contém um
//!   código descartável trocado em `POST /api/pair` por um cookie `HttpOnly; SameSite=Strict`
//!   (acrescentando `; Secure` automaticamente quando acessado via túnel HTTPS).

use crate::routines;
use crate::state::AppState;
use crate::storage;
use crate::tools;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::{IpAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Listener};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, watch, Mutex, RwLock, Semaphore};
use url::Url;
use uuid::Uuid;

const COMPANION_HTML: &str = include_str!("companion_ui.html");
const MAX_HEADER_BYTES: usize = 16 * 1024;
const MAX_BODY_BYTES: usize = 256 * 1024;
const MAX_CONCURRENT_CONNS: usize = 32;
const MAX_SSE_CLIENTS: usize = 6;
const HEADER_TIMEOUT: Duration = Duration::from_secs(5);
const BODY_TIMEOUT: Duration = Duration::from_secs(10);
const SESSION_TTL: Duration = Duration::from_secs(8 * 60 * 60);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionInfo {
    pub enabled: bool,
    pub lan_enabled: bool,
    pub port: u16,
    pub ip: String,
    pub bind_host: String,
    pub pair_url: String,
    pub pairing_code: String,
    pub connected_clients: usize,
    pub tailscale_ip: Option<String>,
    pub tailscale_pair_url: Option<String>,
    pub external_url: Option<String>,
    pub external_pair_url: Option<String>,
}

pub struct CompanionState {
    pub enabled: AtomicBool,
    pub lan_enabled: AtomicBool,
    pub pairing_code: RwLock<String>,
    pub sessions: RwLock<HashMap<String, Instant>>,
    pub external_url: RwLock<Option<String>>,
    pub port: AtomicU16,
    pub connected_clients: AtomicUsize,
    pub events_tx: broadcast::Sender<String>,
    pub conn_semaphore: Arc<Semaphore>,
    pub sse_semaphore: Arc<Semaphore>,
    pub rebind_tx: Mutex<Option<watch::Sender<(bool, bool)>>>,
}

fn generate_pairing_code() -> String {
    let hex = Uuid::new_v4().simple().to_string().to_ascii_uppercase();
    hex.chars().take(8).collect()
}

fn normalize_code(raw: &str) -> String {
    raw.trim()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_uppercase()
}

/// Valida e normaliza uma URL externa de túnel (ex.: Tailscale Funnel/Serve ou Cloudflare Tunnel).
pub fn normalize_external_url(raw: &str) -> Result<Option<String>, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let parsed = Url::parse(trimmed).map_err(|e| format!("URL externa inválida: {e}"))?;
    if !matches!(parsed.scheme(), "https" | "http") {
        return Err("A URL do túnel deve começar com https:// ou http://".to_string());
    }
    let Some(host) = parsed.host_str() else {
        return Err("A URL do túnel precisa conter um domínio ou IP válido.".to_string());
    };
    let mut base = format!("{}://{}", parsed.scheme(), host);
    if let Some(port) = parsed.port() {
        base.push_str(&format!(":{port}"));
    }
    let path = parsed.path().trim_end_matches('/');
    if !path.is_empty() {
        base.push_str(path);
    }
    Ok(Some(base))
}

impl CompanionState {
    pub fn new() -> Self {
        let (events_tx, _) = broadcast::channel(256);
        Self {
            // Secure defaults: disabled on startup and localhost-only unless user opts into LAN
            enabled: AtomicBool::new(false),
            lan_enabled: AtomicBool::new(false),
            pairing_code: RwLock::new(generate_pairing_code()),
            sessions: RwLock::new(HashMap::new()),
            external_url: RwLock::new(None),
            port: AtomicU16::new(47821),
            connected_clients: AtomicUsize::new(0),
            events_tx,
            conn_semaphore: Arc::new(Semaphore::new(MAX_CONCURRENT_CONNS)),
            sse_semaphore: Arc::new(Semaphore::new(MAX_SSE_CLIENTS)),
            rebind_tx: Mutex::new(None),
        }
    }

    pub async fn notify_listener_config(&self) {
        let cfg = (
            self.enabled.load(Ordering::SeqCst),
            self.lan_enabled.load(Ordering::SeqCst),
        );
        if let Some(tx) = self.rebind_tx.lock().await.as_ref() {
            let _ = tx.send(cfg);
        }
    }

    pub async fn revoke_sessions_and_rotate_code(&self) -> String {
        {
            let mut guard = self.sessions.write().await;
            guard.clear();
        }
        let next = generate_pairing_code();
        {
            let mut code_guard = self.pairing_code.write().await;
            *code_guard = next.clone();
        }
        let _ = self
            .events_tx
            .send(json!({ "event": "session_revoked" }).to_string());
        next
    }

    pub async fn info(&self) -> CompanionInfo {
        let enabled = self.enabled.load(Ordering::SeqCst);
        let lan_enabled = self.lan_enabled.load(Ordering::SeqCst);
        let port = self.port.load(Ordering::SeqCst);
        let ip = if enabled && lan_enabled {
            detect_lan_ip()
        } else {
            "127.0.0.1".to_string()
        };
        let bind_host = if enabled && lan_enabled {
            "0.0.0.0".to_string()
        } else {
            "127.0.0.1".to_string()
        };
        let pairing_code = self.pairing_code.read().await.clone();
        let pair_url = format!("http://{ip}:{port}/?pair={pairing_code}");
        let tailscale_ip = if enabled && lan_enabled {
            detect_tailscale_ip()
        } else {
            None
        };
        let tailscale_pair_url = tailscale_ip
            .as_ref()
            .map(|ts_ip| format!("http://{ts_ip}:{port}/?pair={pairing_code}"));
        let external_url = self.external_url.read().await.clone();
        let external_pair_url = external_url
            .as_ref()
            .map(|ext| format!("{ext}/?pair={pairing_code}"));
        CompanionInfo {
            enabled,
            lan_enabled,
            port,
            ip,
            bind_host,
            pair_url,
            pairing_code,
            connected_clients: self.connected_clients.load(Ordering::SeqCst),
            tailscale_ip,
            tailscale_pair_url,
            external_url,
            external_pair_url,
        }
    }
}

pub fn detect_lan_ip() -> String {
    if let Ok(sock) = UdpSocket::bind("0.0.0.0:0") {
        if sock.connect("8.8.8.8:80").is_ok() {
            if let Ok(addr) = sock.local_addr() {
                let ip = addr.ip().to_string();
                if !ip.starts_with("127.") && ip != "0.0.0.0" {
                    return ip;
                }
            }
        }
    }
    "127.0.0.1".to_string()
}

/// Detecta o IPv4 da interface Tailscale (`100.64.0.0/10` CGNAT) consultando a tabela de rotas para o MagicDNS (`100.100.100.100`).
pub fn detect_tailscale_ip() -> Option<String> {
    let sock = UdpSocket::bind("0.0.0.0:0").ok()?;
    sock.connect("100.100.100.100:80").ok()?;
    let addr = sock.local_addr().ok()?;
    if let IpAddr::V4(v4) = addr.ip() {
        let oct = v4.octets();
        if oct[0] == 100 && (64..=127).contains(&oct[1]) {
            let s = v4.to_string();
            if s != detect_lan_ip() {
                return Some(s);
            }
        }
    }
    None
}

pub fn allow_companion_firewall_port(port: u16) -> Result<String, String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let ps = format!(
            "Start-Process -FilePath 'netsh' -ArgumentList 'advfirewall firewall delete rule name=\"Jarvis Companion\"' -Verb RunAs -WindowStyle Hidden -Wait -ErrorAction SilentlyContinue; \
             Start-Process -FilePath 'netsh' -ArgumentList 'advfirewall firewall add rule name=\"Jarvis Companion\" dir=in action=allow protocol=TCP localport={port} profile=private,domain' -Verb RunAs -WindowStyle Hidden -Wait"
        );
        let status = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", &ps])
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .map_err(|e| format!("Falha ao solicitar permissão de Firewall: {e}"))?;
        if status.success() {
            Ok(format!(
                "Porta TCP {port} liberada no Firewall do Windows (redes privadas)."
            ))
        } else {
            Err("Autorização do Firewall cancelada pelo usuário.".to_string())
        }
    }
    #[cfg(not(windows))]
    {
        Ok(format!(
            "Firewall não aplicável nesta plataforma (porta {port})."
        ))
    }
}

struct SseClientGuard {
    counter: Arc<CompanionState>,
}

impl Drop for SseClientGuard {
    fn drop(&mut self) {
        self.counter
            .connected_clients
            .fetch_sub(1, Ordering::SeqCst);
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ParsedHttpRequest {
    method: String,
    path: String,
    query: HashMap<String, String>,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

fn parse_query_params(raw_query: &str) -> HashMap<String, String> {
    url::form_urlencoded::parse(raw_query.as_bytes())
        .into_owned()
        .collect()
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

async fn read_http_request(stream: &mut TcpStream) -> Result<ParsedHttpRequest, String> {
    let mut buf = Vec::with_capacity(4096);
    let mut tmp = [0u8; 2048];

    let header_end = tokio::time::timeout(HEADER_TIMEOUT, async {
        loop {
            if let Some(pos) = find_header_end(&buf) {
                return Ok(pos);
            }
            if buf.len() > MAX_HEADER_BYTES {
                return Err("Cabeçalho HTTP excede o limite permitido".to_string());
            }
            let n = stream
                .read(&mut tmp)
                .await
                .map_err(|e| format!("Erro de leitura: {e}"))?;
            if n == 0 {
                return Err("Conexão encerrada".to_string());
            }
            buf.extend_from_slice(&tmp[..n]);
        }
    })
    .await
    .map_err(|_| "Timeout ao aguardar cabeçalho HTTP".to_string())??;

    let header_bytes = &buf[..header_end];
    let header_str = String::from_utf8_lossy(header_bytes);
    let mut lines = header_str.split("\r\n");
    let req_line = lines.next().ok_or("Requisição HTTP vazia")?;
    let mut req_parts = req_line.split_whitespace();
    let method = req_parts
        .next()
        .ok_or("Método HTTP ausente")?
        .to_ascii_uppercase();
    let target = req_parts.next().unwrap_or("/");

    let (path, raw_query) = match target.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (target.to_string(), String::new()),
    };
    let query = parse_query_params(&raw_query);

    let mut headers = HashMap::new();
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
        }
    }

    let content_length: usize = headers
        .get("content-length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    if content_length > MAX_BODY_BYTES {
        return Err("Corpo da requisição excede 256 KB".to_string());
    }

    let body_start = header_end + 4;
    if buf.len() - body_start < content_length {
        tokio::time::timeout(BODY_TIMEOUT, async {
            while buf.len() - body_start < content_length {
                let n = stream
                    .read(&mut tmp)
                    .await
                    .map_err(|e| format!("Erro ao ler corpo: {e}"))?;
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&tmp[..n]);
            }
            Ok::<_, String>(())
        })
        .await
        .map_err(|_| "Timeout ao ler corpo da requisição HTTP".to_string())??;
    }
    let body_end = (body_start + content_length).min(buf.len());
    let body = buf[body_start..body_end].to_vec();

    Ok(ParsedHttpRequest {
        method,
        path,
        query,
        headers,
        body,
    })
}

fn extract_session_id(req: &ParsedHttpRequest) -> Option<String> {
    if let Some(cookie_hdr) = req.headers.get("cookie") {
        for part in cookie_hdr.split(';') {
            let trimmed = part.trim();
            if let Some(val) = trimmed.strip_prefix("jarvis_session=") {
                let s = val.trim();
                if !s.is_empty() {
                    return Some(s.to_string());
                }
            }
        }
    }
    if let Some(auth) = req.headers.get("authorization") {
        if let Some(rest) = auth
            .strip_prefix("Bearer ")
            .or_else(|| auth.strip_prefix("bearer "))
        {
            let s = rest.trim();
            if !s.is_empty() {
                return Some(s.to_string());
            }
        }
    }
    None
}

async fn validate_session(companion: &CompanionState, req: &ParsedHttpRequest) -> Option<String> {
    let sid = extract_session_id(req)?;
    let mut sessions = companion.sessions.write().await;
    let valid = sessions
        .get(&sid)
        .is_some_and(|issued| issued.elapsed() < SESSION_TTL);
    if valid {
        Some(sid)
    } else {
        sessions.remove(&sid);
        None
    }
}

async fn session_is_active(companion: &CompanionState, sid: &str) -> bool {
    let mut sessions = companion.sessions.write().await;
    let valid = sessions
        .get(sid)
        .is_some_and(|issued| issued.elapsed() < SESSION_TTL);
    if !valid {
        sessions.remove(sid);
    }
    valid
}

async fn write_http_response(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    extra_headers: &[(&str, &str)],
    body: &[u8],
) -> std::io::Result<()> {
    let mut head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n",
        body.len()
    );
    for (k, v) in extra_headers {
        head.push_str(&format!("{k}: {v}\r\n"));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes()).await?;
    stream.write_all(body).await?;
    stream.flush().await
}

async fn read_hardware_metrics_blocking() -> (String, f32, f64, f64, f64) {
    tokio::task::spawn_blocking(|| {
        use sysinfo::System;
        let mut sys = System::new_all();
        std::thread::sleep(Duration::from_millis(80));
        sys.refresh_cpu_usage();
        sys.refresh_memory();
        let cpu = sys.global_cpu_usage();
        let total_gb = sys.total_memory() as f64 / 1_073_741_824.0;
        let used_gb = sys.used_memory() as f64 / 1_073_741_824.0;
        let ram_pct = if sys.total_memory() > 0 {
            (sys.used_memory() as f64 / sys.total_memory() as f64) * 100.0
        } else {
            0.0
        };
        let host = System::host_name().unwrap_or_else(|| "Windows".to_string());
        (host, cpu, used_gb, total_gb, ram_pct)
    })
    .await
    .unwrap_or_else(|_| ("Windows".to_string(), 0.0, 0.0, 0.0, 0.0))
}

async fn resolve_target_conversation_id(
    app_state: &Arc<AppState>,
    requested_id: Option<&str>,
) -> Result<String, String> {
    if let Some(cid) = requested_id.map(str::trim).filter(|s| !s.is_empty()) {
        if storage::get_conversation(&app_state.data_dir, cid).is_ok() {
            return Ok(cid.to_string());
        }
    }
    let list = storage::list_conversations(&app_state.data_dir)?;
    if let Some(first) = list.first() {
        Ok(first.id.clone())
    } else {
        let created =
            storage::create_conversation(&app_state.data_dir, Some("Celular"), None, None)?;
        Ok(created.id)
    }
}

pub(crate) async fn handle_client_core(
    mut stream: TcpStream,
    app: Option<AppHandle>,
    app_state: Arc<AppState>,
    companion: Arc<CompanionState>,
    _conn_permit: tokio::sync::OwnedSemaphorePermit,
) {
    let Ok(req) = read_http_request(&mut stream).await else {
        return;
    };

    if !companion.enabled.load(Ordering::SeqCst) {
        let _ = write_http_response(
            &mut stream,
            "503 Service Unavailable",
            "application/json; charset=utf-8",
            &[],
            br#"{"error":"Jarvis Companion desativado no PC"}"#,
        )
        .await;
        return;
    }

    if req.method == "GET" && (req.path == "/" || req.path == "/index.html") {
        let _ = write_http_response(
            &mut stream,
            "200 OK",
            "text/html; charset=utf-8",
            &[],
            COMPANION_HTML.as_bytes(),
        )
        .await;
        return;
    }

    if req.method == "GET" && req.path == "/manifest.webmanifest" {
        let manifest = json!({
            "name": "Jarvis",
            "short_name": "Jarvis",
            "start_url": "/",
            "display": "standalone",
            "background_color": "#03060c",
            "theme_color": "#03060c",
            "orientation": "portrait"
        })
        .to_string();
        let _ = write_http_response(
            &mut stream,
            "200 OK",
            "application/manifest+json; charset=utf-8",
            &[],
            manifest.as_bytes(),
        )
        .await;
        return;
    }

    // One-time pairing exchange: pairing_code -> HttpOnly session cookie (code is immediately rotated)
    if req.method == "POST" && req.path == "/api/pair" {
        let body: Value = serde_json::from_slice(&req.body).unwrap_or(Value::Null);
        let submitted = normalize_code(body["code"].as_str().unwrap_or(""));
        let paired = {
            let mut code_guard = companion.pairing_code.write().await;
            if !submitted.is_empty() && submitted == *code_guard {
                *code_guard = generate_pairing_code();
                true
            } else {
                false
            }
        };
        if !paired {
            let _ = write_http_response(
                &mut stream,
                "401 Unauthorized",
                "application/json; charset=utf-8",
                &[],
                br#"{"error":"Codigo de pareamento invalido ou expirado"}"#,
            )
            .await;
            return;
        }
        let session_id = Uuid::new_v4().simple().to_string();
        {
            let mut sessions = companion.sessions.write().await;
            sessions.insert(session_id.clone(), Instant::now());
        }
        // Quando o pareamento ocorre via túnel HTTPS (Tailscale Serve/Funnel, Cloudflare Tunnel),
        // adiciona automaticamente a flag Secure ao cookie HttpOnly.
        let is_https = req
            .headers
            .get("x-forwarded-proto")
            .is_some_and(|p| p.eq_ignore_ascii_case("https"))
            || req
                .headers
                .get("origin")
                .is_some_and(|o| o.starts_with("https://"))
            || req
                .headers
                .get("referer")
                .is_some_and(|r| r.starts_with("https://"));
        let cookie_val = if is_https {
            format!(
                "jarvis_session={session_id}; HttpOnly; SameSite=Strict; Path=/; Max-Age=28800; Secure"
            )
        } else {
            format!("jarvis_session={session_id}; HttpOnly; SameSite=Strict; Path=/; Max-Age=28800")
        };
        let _ = write_http_response(
            &mut stream,
            "200 OK",
            "application/json; charset=utf-8",
            &[("Set-Cookie", cookie_val.as_str())],
            br#"{"ok":true}"#,
        )
        .await;
        return;
    }

    // Reject all other endpoints unless authenticated via HttpOnly Cookie or Bearer header
    let Some(session_id) = validate_session(&companion, &req).await else {
        let _ = write_http_response(
            &mut stream,
            "401 Unauthorized",
            "application/json; charset=utf-8",
            &[],
            br#"{"error":"Sessao invalida ou nao pareada"}"#,
        )
        .await;
        return;
    };

    if req.method == "GET" && req.path == "/api/events" {
        let Ok(_sse_permit) = Arc::clone(&companion.sse_semaphore).try_acquire_owned() else {
            let _ = write_http_response(
                &mut stream,
                "429 Too Many Requests",
                "application/json; charset=utf-8",
                &[],
                br#"{"error":"Limite de conexoes SSE atingido"}"#,
            )
            .await;
            return;
        };

        let headers = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream; charset=utf-8\r\nCache-Control: no-cache\r\nConnection: keep-alive\r\nX-Accel-Buffering: no\r\n\r\n";
        if stream.write_all(headers.as_bytes()).await.is_err() {
            return;
        }
        let _ = stream.write_all(b": connected\n\n").await;
        let _ = stream.flush().await;

        companion.connected_clients.fetch_add(1, Ordering::SeqCst);
        let _guard = SseClientGuard {
            counter: Arc::clone(&companion),
        };
        let mut rx = companion.events_tx.subscribe();
        let mut ping = tokio::time::interval(Duration::from_secs(20));
        ping.tick().await;

        loop {
            tokio::select! {
                _ = ping.tick() => {
                    if !companion.enabled.load(Ordering::SeqCst)
                        || !session_is_active(&companion, &session_id).await
                    {
                        break;
                    }
                    if stream.write_all(b": ping\n\n").await.is_err() {
                        break;
                    }
                    let _ = stream.flush().await;
                }
                msg = rx.recv() => {
                    if !companion.enabled.load(Ordering::SeqCst)
                        || !session_is_active(&companion, &session_id).await
                    {
                        break;
                    }
                    match msg {
                        Ok(json_line) => {
                            let packet = format!("data: {json_line}\n\n");
                            if stream.write_all(packet.as_bytes()).await.is_err() {
                                break;
                            }
                            let _ = stream.flush().await;
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(broadcast::error::RecvError::Closed) => break,
                    }
                }
            }
        }
        return;
    }

    if req.method == "GET" && req.path == "/api/status" {
        let (host, cpu, ram_used, ram_total, ram_pct) = read_hardware_metrics_blocking().await;
        let skills = routines::list_skills(&app_state.data_dir).unwrap_or_default();
        let agents = app_state.agent_runs.lock().await.clone();
        let timers = app_state.list_active_timers().await;
        let pending_approval = app_state.first_pending_approval().await;

        let (active_conv_id, recent_messages) =
            match storage::list_conversations(&app_state.data_dir) {
                Ok(convs) => {
                    if let Some(first) = convs.first() {
                        let msgs = storage::get_conversation(&app_state.data_dir, &first.id)
                            .map(|c| {
                                let len = c.messages.len();
                                c.messages[len.saturating_sub(24)..].to_vec()
                            })
                            .unwrap_or_default();
                        (Some(first.id.clone()), msgs)
                    } else {
                        (None, Vec::new())
                    }
                }
                Err(_) => (None, Vec::new()),
            };

        let payload = json!({
            "hostName": host,
            "cpuPct": cpu,
            "ramUsedGb": ram_used,
            "ramTotalGb": ram_total,
            "ramPct": ram_pct,
            "skills": skills,
            "agents": agents,
            "timers": timers,
            "pendingApproval": pending_approval,
            "activeConversationId": active_conv_id,
            "recentMessages": recent_messages,
        })
        .to_string();

        let _ = write_http_response(
            &mut stream,
            "200 OK",
            "application/json; charset=utf-8",
            &[],
            payload.as_bytes(),
        )
        .await;
        return;
    }

    if req.method == "GET" && req.path == "/api/screen" {
        let target = req
            .query
            .get("window")
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let _cap_guard = match app.as_ref() {
            Some(a) => Some(tools::acquire_screen_capture_guard(a).await),
            None => None,
        };

        let res = tokio::task::spawn_blocking(move || {
            let open_wins = tools::list_open_windows_info_blocking();
            let win_list: Vec<Value> = open_wins
                .iter()
                .take(24)
                .map(|w| {
                    json!({
                        "process": w.process_name,
                        "title": w.title,
                        "minimized": w.minimized,
                    })
                })
                .collect();

            let (cw, ch, _, _, b64) = if let Some(ref q) = target {
                let (shot, _, _) = tools::capture_target_window_blocking(q, &open_wins)?;
                shot
            } else {
                tools::capture_desktop_screen_blocking()?
            };

            Ok::<_, String>(json!({
                "imageBase64": b64,
                "mime": "image/jpeg",
                "width": cw,
                "height": ch,
                "windows": win_list,
            }))
        })
        .await
        .map_err(|e| format!("Erro captura: {e}"))
        .and_then(|r| r);

        drop(_cap_guard);

        match res {
            Ok(body_val) => {
                let s = body_val.to_string();
                let _ = write_http_response(
                    &mut stream,
                    "200 OK",
                    "application/json; charset=utf-8",
                    &[],
                    s.as_bytes(),
                )
                .await;
            }
            Err(err) => {
                let s = json!({ "error": err }).to_string();
                let _ = write_http_response(
                    &mut stream,
                    "500 Internal Server Error",
                    "application/json; charset=utf-8",
                    &[],
                    s.as_bytes(),
                )
                .await;
            }
        }
        return;
    }

    if req.method == "POST" && (req.path == "/api/chat" || req.path == "/api/skill") {
        let Some(ref app_handle) = app else {
            let _ = write_http_response(
                &mut stream,
                "503 Service Unavailable",
                "application/json; charset=utf-8",
                &[],
                br#"{"error":"Chat indisponivel em modo headless"}"#,
            )
            .await;
            return;
        };

        let body: Value = serde_json::from_slice(&req.body).unwrap_or(Value::Null);
        let req_conv_id = body["conversationId"].as_str();

        let prompt_text = if req.path == "/api/skill" {
            let slug = body["slug"].as_str().unwrap_or("");
            let norm = routines::normalize_slug(slug);
            let skills = routines::list_skills(&app_state.data_dir).unwrap_or_default();
            skills
                .into_iter()
                .find(|s| s.slug == norm)
                .map(|s| s.prompt)
                .unwrap_or_else(|| slug.to_string())
        } else {
            let raw = body["text"].as_str().unwrap_or("").trim().to_string();
            if let Some(stripped) = raw.strip_prefix('/') {
                let mut parts = stripped.trim().splitn(2, char::is_whitespace);
                let slug = routines::normalize_slug(parts.next().unwrap_or(""));
                let extra = parts.next().unwrap_or("").trim();
                let skills = routines::list_skills(&app_state.data_dir).unwrap_or_default();
                if let Some(matched) = skills.into_iter().find(|s| s.slug == slug) {
                    if extra.is_empty() {
                        matched.prompt
                    } else {
                        format!("{}\n\nContexto adicional: {extra}", matched.prompt)
                    }
                } else {
                    raw
                }
            } else {
                raw
            }
        };

        if prompt_text.trim().is_empty() {
            let _ = write_http_response(
                &mut stream,
                "400 Bad Request",
                "application/json; charset=utf-8",
                &[],
                br#"{"error":"Mensagem vazia"}"#,
            )
            .await;
            return;
        }

        match resolve_target_conversation_id(&app_state, req_conv_id).await {
            Ok(conv_id) => {
                match crate::dispatch_chat_turn(
                    app_handle.clone(),
                    Arc::clone(&app_state),
                    conv_id.clone(),
                    prompt_text,
                    None,
                )
                .await
                {
                    Ok(conv) => {
                        let _ = app_handle.emit(
                            "jarvis:companion-sync",
                            json!({ "conversationId": conv.id }),
                        );
                        let resp = json!({ "ok": true, "conversationId": conv.id }).to_string();
                        let _ = write_http_response(
                            &mut stream,
                            "200 OK",
                            "application/json; charset=utf-8",
                            &[],
                            resp.as_bytes(),
                        )
                        .await;
                    }
                    Err(err) => {
                        let resp = json!({ "error": err }).to_string();
                        let _ = write_http_response(
                            &mut stream,
                            "400 Bad Request",
                            "application/json; charset=utf-8",
                            &[],
                            resp.as_bytes(),
                        )
                        .await;
                    }
                }
            }
            Err(err) => {
                let resp = json!({ "error": err }).to_string();
                let _ = write_http_response(
                    &mut stream,
                    "500 Internal Server Error",
                    "application/json; charset=utf-8",
                    &[],
                    resp.as_bytes(),
                )
                .await;
            }
        }
        return;
    }

    if req.method == "POST" && req.path == "/api/action" {
        let body: Value = serde_json::from_slice(&req.body).unwrap_or(Value::Null);
        let action_type = body["type"].as_str().unwrap_or("");
        let result: Result<String, String> = match action_type {
            "volume" => {
                let level = body["level"].as_u64().map(|v| v.min(100) as u8);
                let muted = body["muted"].as_bool();
                tokio::task::spawn_blocking(move || {
                    tools::set_windows_volume_blocking(level, muted)
                })
                .await
                .map_err(|e| format!("Erro volume: {e}"))
                .and_then(|r| r)
            }
            "media" => {
                let action = body["action"].as_str().unwrap_or("play_pause");
                tools::send_media_key(action)
            }
            "timer" => {
                if let Some(ref a) = app {
                    let secs = body["seconds"].as_u64().unwrap_or(300).clamp(1, 86400);
                    let label = body["label"].as_str().unwrap_or("Timer do Celular");
                    tools::run_set_timer(a, &app_state, secs, label).await
                } else {
                    Err("Timers indisponíveis em modo headless.".to_string())
                }
            }
            "approve" => {
                let request_id = body["requestId"].as_str().unwrap_or("");
                let approved = body["approved"].as_bool().unwrap_or(false);
                if app_state.resolve_approval(request_id, approved).await {
                    if let Some(ref a) = app {
                        let _ = a.emit(
                            "jarvis:approval-resolved",
                            json!({ "approvalId": request_id, "approved": approved }),
                        );
                    }
                    let _ = companion.events_tx.send(
                        json!({
                            "event": "approval_resolved",
                            "data": { "approvalId": request_id, "approved": approved }
                        })
                        .to_string(),
                    );
                    Ok("Aprovação processada.".to_string())
                } else {
                    Err("Aprovação inexistente ou já processada.".to_string())
                }
            }
            "cancel" => {
                app_state.cancel_active_chat().await;
                let cleared = app_state.cancel_all_pending_approvals().await;
                for cid in cleared {
                    if let Some(ref a) = app {
                        let _ = a.emit(
                            "jarvis:approval-resolved",
                            json!({ "approvalId": cid, "approved": false }),
                        );
                    }
                    let _ = companion.events_tx.send(
                        json!({
                            "event": "approval_resolved",
                            "data": { "approvalId": cid, "approved": false }
                        })
                        .to_string(),
                    );
                }
                Ok("Resposta interrompida.".to_string())
            }
            _ => Err("Ação desconhecida.".to_string()),
        };

        match result {
            Ok(msg) => {
                let resp = json!({ "ok": true, "message": msg }).to_string();
                let _ = write_http_response(
                    &mut stream,
                    "200 OK",
                    "application/json; charset=utf-8",
                    &[],
                    resp.as_bytes(),
                )
                .await;
            }
            Err(err) => {
                let resp = json!({ "error": err }).to_string();
                let _ = write_http_response(
                    &mut stream,
                    "400 Bad Request",
                    "application/json; charset=utf-8",
                    &[],
                    resp.as_bytes(),
                )
                .await;
            }
        }
        return;
    }

    let _ = write_http_response(
        &mut stream,
        "404 Not Found",
        "application/json; charset=utf-8",
        &[],
        br#"{"error":"Rota nao encontrada"}"#,
    )
    .await;
}

pub fn start_companion_server(
    app: AppHandle,
    app_state: Arc<AppState>,
    companion: Arc<CompanionState>,
) {
    let tx_chat = companion.events_tx.clone();
    app.listen("jarvis:chat-event", move |ev| {
        if let Ok(val) = serde_json::from_str::<Value>(ev.payload()) {
            let _ = tx_chat.send(json!({ "event": "chat", "data": val }).to_string());
        }
    });

    // Unified approval event name: jarvis:tool-approval-request
    let tx_app = companion.events_tx.clone();
    app.listen("jarvis:tool-approval-request", move |ev| {
        if let Ok(val) = serde_json::from_str::<Value>(ev.payload()) {
            let _ = tx_app.send(json!({ "event": "approval", "data": val }).to_string());
        }
    });

    let tx_app_res = companion.events_tx.clone();
    app.listen("jarvis:approval-resolved", move |ev| {
        if let Ok(val) = serde_json::from_str::<Value>(ev.payload()) {
            let _ =
                tx_app_res.send(json!({ "event": "approval_resolved", "data": val }).to_string());
        }
    });

    let tx_sum = companion.events_tx.clone();
    app.listen("jarvis:agent-completed-summary", move |ev| {
        if let Ok(val) = serde_json::from_str::<Value>(ev.payload()) {
            let _ = tx_sum.send(json!({ "event": "agent_summary", "data": val }).to_string());
        }
    });

    let tx_sp = companion.events_tx.clone();
    app.listen("jarvis:agent-spawned", move |ev| {
        if let Ok(val) = serde_json::from_str::<Value>(ev.payload()) {
            let _ = tx_sp.send(json!({ "event": "agent_spawned", "data": val }).to_string());
        }
    });

    let tx_tm = companion.events_tx.clone();
    app.listen("jarvis:timer-fired", move |ev| {
        if let Ok(val) = serde_json::from_str::<Value>(ev.payload()) {
            let _ = tx_tm.send(json!({ "event": "timer_fired", "data": val }).to_string());
        }
    });

    let (rebind_tx, mut rebind_rx) = watch::channel((
        companion.enabled.load(Ordering::SeqCst),
        companion.lan_enabled.load(Ordering::SeqCst),
    ));
    if let Ok(mut guard) = companion.rebind_tx.try_lock() {
        *guard = Some(rebind_tx);
    }

    tauri::async_runtime::spawn(async move {
        loop {
            let (enabled, lan_enabled) = *rebind_rx.borrow();
            let bind_host = if enabled && lan_enabled {
                "0.0.0.0"
            } else {
                "127.0.0.1"
            };

            let mut bound = None;
            for port in 47821..=47835u16 {
                if let Ok(listener) = TcpListener::bind((bind_host, port)).await {
                    companion.port.store(port, Ordering::SeqCst);
                    bound = Some(listener);
                    break;
                }
            }
            if bound.is_none() {
                if let Ok(listener) = TcpListener::bind((bind_host, 0)).await {
                    if let Ok(addr) = listener.local_addr() {
                        companion.port.store(addr.port(), Ordering::SeqCst);
                    }
                    bound = Some(listener);
                }
            }

            let Some(listener) = bound else {
                eprintln!("Aviso: não foi possível iniciar listener Jarvis Companion.");
                if rebind_rx.changed().await.is_err() {
                    break;
                }
                continue;
            };

            loop {
                tokio::select! {
                    changed = rebind_rx.changed() => {
                        if changed.is_err() {
                            return;
                        }
                        break; // Rebind listener for updated host/exposure
                    }
                    accepted = listener.accept() => {
                        let Ok((mut stream, _)) = accepted else {
                            continue;
                        };
                        let Ok(permit) = Arc::clone(&companion.conn_semaphore).try_acquire_owned() else {
                            let _ = write_http_response(
                                &mut stream,
                                "429 Too Many Requests",
                                "application/json; charset=utf-8",
                                &[],
                                br#"{"error":"Muitas conexoes simultaneas"}"#,
                            ).await;
                            continue;
                        };
                        let app_c = Some(app.clone());
                        let state_c = Arc::clone(&app_state);
                        let comp_c = Arc::clone(&companion);
                        tauri::async_runtime::spawn(async move {
                            handle_client_core(stream, app_c, state_c, comp_c, permit).await;
                        });
                    }
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppConfig;
    use tokio::sync::oneshot;

    async fn spawn_test_server(
        app_state: Arc<AppState>,
        companion: Arc<CompanionState>,
    ) -> std::net::SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                let Ok(permit) = Arc::clone(&companion.conn_semaphore).try_acquire_owned() else {
                    let _ = write_http_response(
                        &mut stream,
                        "429 Too Many Requests",
                        "application/json",
                        &[],
                        b"{}",
                    )
                    .await;
                    continue;
                };
                let st = Arc::clone(&app_state);
                let cmp = Arc::clone(&companion);
                tokio::spawn(async move {
                    handle_client_core(stream, None, st, cmp, permit).await;
                });
            }
        });
        addr
    }

    async fn send_raw_http(addr: std::net::SocketAddr, req: &str) -> String {
        let mut s = TcpStream::connect(addr).await.unwrap();
        s.write_all(req.as_bytes()).await.unwrap();
        let mut out = Vec::new();
        let mut buf = [0u8; 4096];
        while let Ok(n) = tokio::time::timeout(Duration::from_secs(5), s.read(&mut buf))
            .await
            .unwrap_or(Ok(0))
        {
            if n == 0 {
                break;
            }
            out.extend_from_slice(&buf[..n]);
        }
        String::from_utf8_lossy(&out).to_string()
    }

    #[tokio::test]
    async fn companion_full_protocol_security_approval_and_limits() {
        let dir = std::env::temp_dir().join(format!("jarvis-comp-test-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let app_state = Arc::new(AppState::new(dir.clone(), AppConfig::default()).unwrap());
        let companion = Arc::new(CompanionState::new());
        let addr = spawn_test_server(Arc::clone(&app_state), Arc::clone(&companion)).await;

        // 1. Disabled by default -> 503 everywhere
        assert!(!companion.enabled.load(Ordering::SeqCst));
        assert!(!companion.lan_enabled.load(Ordering::SeqCst));
        let r_disabled =
            send_raw_http(addr, "GET /api/status HTTP/1.1\r\nHost: localhost\r\n\r\n").await;
        assert!(r_disabled.starts_with("HTTP/1.1 503"));

        // 2. Enabled -> unauthenticated & query ?token= rejected with 401
        companion.enabled.store(true, Ordering::SeqCst);
        let code = companion.pairing_code.read().await.clone();
        let r_unauth = send_raw_http(
            addr,
            &format!("GET /api/status?token={code} HTTP/1.1\r\nHost: localhost\r\n\r\n"),
        )
        .await;
        assert!(r_unauth.starts_with("HTTP/1.1 401"));

        // 3. One-time pairing returns HttpOnly cookie and invalidates code reuse
        let pair_body = json!({ "code": code }).to_string();
        let pair_req = format!(
            "POST /api/pair HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{pair_body}",
            pair_body.len()
        );
        let r_pair = send_raw_http(addr, &pair_req).await;
        assert!(r_pair.starts_with("HTTP/1.1 200"));
        assert!(r_pair.contains("Set-Cookie: jarvis_session="));
        assert!(r_pair.contains("HttpOnly"));

        // Reusing old pairing code must fail
        let r_replay = send_raw_http(addr, &pair_req).await;
        assert!(r_replay.starts_with("HTTP/1.1 401"));

        // Extract session ID from Set-Cookie
        let cookie_line = r_pair
            .lines()
            .find(|l| l.starts_with("Set-Cookie: jarvis_session="))
            .unwrap();
        let sid = cookie_line
            .trim_start_matches("Set-Cookie: jarvis_session=")
            .split(';')
            .next()
            .unwrap()
            .trim()
            .to_string();

        // 4. Register pending approval and verify /api/status shows it, SSE streams, and remote approve + deny + duplicate work
        let (tx1, rx1) = oneshot::channel::<bool>();
        app_state
            .register_approval(
                "app-1".into(),
                json!({"approvalId": "app-1", "title": "Test", "argsSummary": "ls"}),
                tx1,
            )
            .await;

        let r_status = send_raw_http(
            addr,
            &format!("GET /api/status HTTP/1.1\r\nHost: localhost\r\nCookie: jarvis_session={sid}\r\n\r\n"),
        )
        .await;
        assert!(r_status.starts_with("HTTP/1.1 200"));
        assert!(r_status.contains("app-1"));

        // Connect real SSE stream and verify approval_resolved event arrives
        let mut sse_stream = TcpStream::connect(addr).await.unwrap();
        let sse_req = format!(
            "GET /api/events HTTP/1.1\r\nHost: localhost\r\nCookie: jarvis_session={sid}\r\n\r\n"
        );
        sse_stream.write_all(sse_req.as_bytes()).await.unwrap();
        let mut sse_buf = [0u8; 1024];
        let _ = sse_stream.read(&mut sse_buf).await.unwrap();
        assert_eq!(companion.connected_clients.load(Ordering::SeqCst), 1);

        // Approve remotely -> must resolve oneshot immediately (< 1s)
        let app_body =
            json!({ "type": "approve", "requestId": "app-1", "approved": true }).to_string();
        let app_req = format!(
            "POST /api/action HTTP/1.1\r\nHost: localhost\r\nCookie: jarvis_session={sid}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{app_body}",
            app_body.len()
        );
        let r_app = send_raw_http(addr, &app_req).await;
        assert!(r_app.starts_with("HTTP/1.1 200"));
        assert!(tokio::time::timeout(Duration::from_millis(500), rx1)
            .await
            .unwrap()
            .unwrap());

        // SSE must receive approval_resolved
        let n_ev = tokio::time::timeout(Duration::from_millis(500), sse_stream.read(&mut sse_buf))
            .await
            .unwrap()
            .unwrap();
        let sse_ev = String::from_utf8_lossy(&sse_buf[..n_ev]);
        assert!(sse_ev.contains("approval_resolved"));

        // Duplicate approve must be rejected and /api/status must show null pendingApproval
        let r_dup = send_raw_http(addr, &app_req).await;
        assert!(r_dup.starts_with("HTTP/1.1 400"));
        let r_status_after = send_raw_http(
            addr,
            &format!("GET /api/status HTTP/1.1\r\nHost: localhost\r\nCookie: jarvis_session={sid}\r\n\r\n"),
        )
        .await;
        assert!(r_status_after.contains("\"pendingApproval\":null"));

        // Deny test
        let (tx2, rx2) = oneshot::channel::<bool>();
        app_state
            .register_approval("app-2".into(), json!({"approvalId": "app-2"}), tx2)
            .await;
        let deny_body =
            json!({ "type": "approve", "requestId": "app-2", "approved": false }).to_string();
        let deny_req = format!(
            "POST /api/action HTTP/1.1\r\nHost: localhost\r\nCookie: jarvis_session={sid}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{deny_body}",
            deny_body.len()
        );
        let r_deny = send_raw_http(addr, &deny_req).await;
        assert!(r_deny.starts_with("HTTP/1.1 200"));
        assert!(!tokio::time::timeout(Duration::from_millis(500), rx2)
            .await
            .unwrap()
            .unwrap());

        // 5. SSE limit returns 429 when semaphore exhausted (sse_stream already holds 1 permit)
        let held_sse = Arc::clone(&companion.sse_semaphore)
            .acquire_many_owned((MAX_SSE_CLIENTS - 1) as u32)
            .await
            .unwrap();
        let r_sse_429 = send_raw_http(
            addr,
            &format!("GET /api/events HTTP/1.1\r\nHost: localhost\r\nCookie: jarvis_session={sid}\r\n\r\n"),
        )
        .await;
        assert!(r_sse_429.starts_with("HTTP/1.1 429"));
        drop(held_sse);
        drop(sse_stream);

        // 6. Session revocation / token rotation invalidates existing cookie
        companion.revoke_sessions_and_rotate_code().await;
        let r_revoked = send_raw_http(
            addr,
            &format!("GET /api/status HTTP/1.1\r\nHost: localhost\r\nCookie: jarvis_session={sid}\r\n\r\n"),
        )
        .await;
        assert!(r_revoked.starts_with("HTTP/1.1 401"));

        // 7. Pairing over an HTTPS tunnel (X-Forwarded-Proto: https) sets the Secure cookie flag
        let https_code = companion.pairing_code.read().await.clone();
        let https_body = json!({ "code": https_code }).to_string();
        let https_pair_req = format!(
            "POST /api/pair HTTP/1.1\r\nHost: jarvis.tail.ts.net\r\nX-Forwarded-Proto: https\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{https_body}",
            https_body.len()
        );
        let r_https_pair = send_raw_http(addr, &https_pair_req).await;
        assert!(r_https_pair.starts_with("HTTP/1.1 200"));
        assert!(r_https_pair.contains("; Secure"));

        assert_eq!(
            normalize_external_url("https://meu-pc.tail1234.ts.net/").unwrap(),
            Some("https://meu-pc.tail1234.ts.net".to_string())
        );
        assert!(normalize_external_url("ftp://invalido").is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
