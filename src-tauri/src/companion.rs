use crate::routines;
use crate::state::AppState;
use crate::storage;
use crate::tools;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::UdpSocket;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicUsize, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Listener};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, Mutex, RwLock};
use uuid::Uuid;

const COMPANION_HTML: &str = include_str!("companion_ui.html");
const MAX_BODY_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionInfo {
    pub enabled: bool,
    pub port: u16,
    pub ip: String,
    pub url: String,
    pub token: String,
    pub connected_clients: usize,
}

pub struct CompanionState {
    pub enabled: AtomicBool,
    pub token: RwLock<String>,
    pub port: AtomicU16,
    pub connected_clients: AtomicUsize,
    pub events_tx: broadcast::Sender<String>,
    pub pending_approval: Mutex<Option<Value>>,
}

impl CompanionState {
    pub fn new() -> Self {
        let (events_tx, _) = broadcast::channel(256);
        let token = Uuid::new_v4().simple().to_string();
        Self {
            enabled: AtomicBool::new(true),
            token: RwLock::new(token),
            port: AtomicU16::new(47821),
            connected_clients: AtomicUsize::new(0),
            events_tx,
            pending_approval: Mutex::new(None),
        }
    }

    pub async fn regenerate_token(&self) -> String {
        let next = Uuid::new_v4().simple().to_string();
        let mut guard = self.token.write().await;
        *guard = next.clone();
        next
    }

    pub async fn info(&self) -> CompanionInfo {
        let enabled = self.enabled.load(Ordering::SeqCst);
        let port = self.port.load(Ordering::SeqCst);
        let ip = detect_lan_ip();
        let token = self.token.read().await.clone();
        let url = format!("http://{ip}:{port}/?token={token}");
        CompanionInfo {
            enabled,
            port,
            ip,
            url,
            token,
            connected_clients: self.connected_clients.load(Ordering::SeqCst),
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
            Ok(format!("Porta TCP {port} liberada no Firewall do Windows (redes privadas)."))
        } else {
            Err("Autorização do Firewall cancelada pelo usuário.".to_string())
        }
    }
    #[cfg(not(windows))]
    {
        Ok(format!("Firewall não aplicável nesta plataforma (porta {port})."))
    }
}

struct SseClientGuard {
    counter: Arc<CompanionState>,
}

impl Drop for SseClientGuard {
    fn drop(&mut self) {
        self.counter.connected_clients.fetch_sub(1, Ordering::SeqCst);
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

    let header_end = loop {
        if let Some(pos) = find_header_end(&buf) {
            break pos;
        }
        if buf.len() > 32 * 1024 {
            return Err("Cabeçalho HTTP muito grande".to_string());
        }
        let n = stream
            .read(&mut tmp)
            .await
            .map_err(|e| format!("Erro de leitura: {e}"))?;
        if n == 0 {
            return Err("Conexão encerrada".to_string());
        }
        buf.extend_from_slice(&tmp[..n]);
    };

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

fn is_authorized_token(expected: &str, req: &ParsedHttpRequest) -> bool {
    if let Some(qtok) = req.query.get("token") {
        if qtok.trim() == expected {
            return true;
        }
    }
    if let Some(auth) = req.headers.get("authorization") {
        if let Some(rest) = auth.strip_prefix("Bearer ").or_else(|| auth.strip_prefix("bearer ")) {
            if rest.trim() == expected {
                return true;
            }
        }
    }
    false
}

async fn write_http_response(
    stream: &mut TcpStream,
    status: &str,
    content_type: &str,
    body: &[u8],
) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(head.as_bytes()).await?;
    stream.write_all(body).await?;
    stream.flush().await
}

async fn read_hardware_metrics_blocking() -> (String, f32, f64, f64, f64) {
    tokio::task::spawn_blocking(|| {
        use sysinfo::System;
        let mut sys = System::new_all();
        std::thread::sleep(std::time::Duration::from_millis(90));
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
        let created = storage::create_conversation(&app_state.data_dir, Some("Celular"), None, None)?;
        Ok(created.id)
    }
}

async fn handle_client(
    mut stream: TcpStream,
    app: AppHandle,
    app_state: Arc<AppState>,
    companion: Arc<CompanionState>,
) {
    let Ok(req) = read_http_request(&mut stream).await else {
        return;
    };

    if !companion.enabled.load(Ordering::SeqCst) {
        let _ = write_http_response(
            &mut stream,
            "503 Service Unavailable",
            "text/plain; charset=utf-8",
            b"Jarvis Companion desativado no PC.",
        )
        .await;
        return;
    }

    if req.method == "OPTIONS" {
        let _ = write_http_response(&mut stream, "204 No Content", "text/plain", b"").await;
        return;
    }

    // Public UI + PWA manifest routes
    if req.method == "GET" && (req.path == "/" || req.path == "/index.html") {
        let _ = write_http_response(
            &mut stream,
            "200 OK",
            "text/html; charset=utf-8",
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
            "background_color": "#050811",
            "theme_color": "#050811",
            "orientation": "portrait"
        })
        .to_string();
        let _ = write_http_response(
            &mut stream,
            "200 OK",
            "application/manifest+json; charset=utf-8",
            manifest.as_bytes(),
        )
        .await;
        return;
    }

    let expected_token = companion.token.read().await.clone();
    if !is_authorized_token(&expected_token, &req) {
        let _ = write_http_response(
            &mut stream,
            "401 Unauthorized",
            "application/json; charset=utf-8",
            br#"{"error":"Token invalido ou ausente"}"#,
        )
        .await;
        return;
    }

    // SSE live event stream
    if req.method == "GET" && req.path == "/api/events" {
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
        let mut ping = tokio::time::interval(std::time::Duration::from_secs(20));
        ping.tick().await;

        loop {
            tokio::select! {
                _ = ping.tick() => {
                    if stream.write_all(b": ping\n\n").await.is_err() {
                        break;
                    }
                    let _ = stream.flush().await;
                }
                msg = rx.recv() => {
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
        let pending_approval = companion.pending_approval.lock().await.clone();

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

        let excluded = tools::set_jarvis_capture_exclusion(&app, true);
        if !excluded.is_empty() {
            tokio::time::sleep(std::time::Duration::from_millis(85)).await;
        }

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

        let _ = tools::set_jarvis_capture_exclusion(&app, false);

        match res {
            Ok(body_val) => {
                let s = body_val.to_string();
                let _ = write_http_response(
                    &mut stream,
                    "200 OK",
                    "application/json; charset=utf-8",
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
                    s.as_bytes(),
                )
                .await;
            }
        }
        return;
    }

    if req.method == "POST" && (req.path == "/api/chat" || req.path == "/api/skill") {
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
            if raw.starts_with('/') {
                let mut parts = raw[1..].trim().splitn(2, char::is_whitespace);
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
                br#"{"error":"Mensagem vazia"}"#,
            )
            .await;
            return;
        }

        match resolve_target_conversation_id(&app_state, req_conv_id).await {
            Ok(conv_id) => {
                match crate::dispatch_chat_turn(
                    app.clone(),
                    Arc::clone(&app_state),
                    conv_id.clone(),
                    prompt_text,
                    None,
                )
                .await
                {
                    Ok(conv) => {
                        let _ = app.emit(
                            "jarvis:companion-sync",
                            json!({ "conversationId": conv.id }),
                        );
                        let resp = json!({ "ok": true, "conversationId": conv.id }).to_string();
                        let _ = write_http_response(
                            &mut stream,
                            "200 OK",
                            "application/json; charset=utf-8",
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
                tokio::task::spawn_blocking(move || tools::set_windows_volume_blocking(level, muted))
                    .await
                    .map_err(|e| format!("Erro volume: {e}"))
                    .and_then(|r| r)
            }
            "media" => {
                let action = body["action"].as_str().unwrap_or("play_pause");
                tools::send_media_key(action)
            }
            "timer" => {
                let secs = body["seconds"].as_u64().unwrap_or(300).clamp(1, 86400);
                let label = body["label"].as_str().unwrap_or("Timer do Celular");
                tools::run_set_timer(&app, &app_state, secs, label).await
            }
            "approve" => {
                let request_id = body["requestId"].as_str().unwrap_or("");
                let approved = body["approved"].as_bool().unwrap_or(false);
                {
                    let mut map = app_state.pending_approvals.lock().await;
                    if let Some(tx) = map.remove(request_id) {
                        let _ = tx.send(approved);
                    }
                }
                {
                    let mut pending = companion.pending_approval.lock().await;
                    *pending = None;
                }
                let _ = app.emit(
                    "jarvis:approval-resolved",
                    json!({ "approvalId": request_id, "approved": approved }),
                );
                let _ = companion.events_tx.send(
                    json!({ "event": "approval_resolved", "data": { "approvalId": request_id, "approved": approved } }).to_string(),
                );
                Ok("Aprovação processada.".to_string())
            }
            "cancel" => {
                let mut active = app_state.active_cancel.lock().await;
                if let Some(tx) = active.take() {
                    let _ = tx.send(true);
                }
                app_state.cancel_all_pending_approvals().await;
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
        br#"{"error":"Rota nao encontrada"}"#,
    )
    .await;
}

pub fn start_companion_server(
    app: AppHandle,
    app_state: Arc<AppState>,
    companion: Arc<CompanionState>,
) {
    // Forward Tauri desktop events to all connected mobile SSE clients
    let tx_chat = companion.events_tx.clone();
    app.listen("jarvis:chat-event", move |ev| {
        if let Ok(val) = serde_json::from_str::<Value>(ev.payload()) {
            let _ = tx_chat.send(json!({ "event": "chat", "data": val }).to_string());
        }
    });

    let tx_app = companion.events_tx.clone();
    let comp_app = Arc::clone(&companion);
    app.listen("jarvis:tool-approval", move |ev| {
        if let Ok(val) = serde_json::from_str::<Value>(ev.payload()) {
            let comp_clone = Arc::clone(&comp_app);
            let val_clone = val.clone();
            tauri::async_runtime::spawn(async move {
                let mut g = comp_clone.pending_approval.lock().await;
                *g = Some(val_clone);
            });
            let _ = tx_app.send(json!({ "event": "approval", "data": val }).to_string());
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

    tauri::async_runtime::spawn(async move {
        let mut bound = None;
        for port in 47821..=47835u16 {
            if let Ok(listener) = TcpListener::bind(("0.0.0.0", port)).await {
                companion.port.store(port, Ordering::SeqCst);
                bound = Some(listener);
                break;
            }
        }
        if bound.is_none() {
            if let Ok(listener) = TcpListener::bind(("0.0.0.0", 0)).await {
                if let Ok(addr) = listener.local_addr() {
                    companion.port.store(addr.port(), Ordering::SeqCst);
                }
                bound = Some(listener);
            }
        }
        let Some(listener) = bound else {
            eprintln!("Aviso: não foi possível iniciar servidor Jarvis Companion.");
            return;
        };

        loop {
            let Ok((stream, _)) = listener.accept().await else {
                continue;
            };
            let app_c = app.clone();
            let state_c = Arc::clone(&app_state);
            let comp_c = Arc::clone(&companion);
            tauri::async_runtime::spawn(async move {
                handle_client(stream, app_c, state_c, comp_c).await;
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn companion_token_and_query_validation_works() {
        let st = CompanionState::new();
        let tok1 = st.token.read().await.clone();
        assert_eq!(tok1.len(), 32);

        let tok2 = st.regenerate_token().await;
        assert_eq!(tok2.len(), 32);
        assert_ne!(tok1, tok2);

        let query = parse_query_params(&format!("token={tok2}&window=firefox"));
        assert_eq!(query.get("window").map(String::as_str), Some("firefox"));

        let req_ok_query = ParsedHttpRequest {
            method: "GET".to_string(),
            path: "/api/status".to_string(),
            query,
            headers: HashMap::new(),
            body: Vec::new(),
        };
        assert!(is_authorized_token(&tok2, &req_ok_query));
        assert!(!is_authorized_token(&tok1, &req_ok_query));

        let mut hdrs = HashMap::new();
        hdrs.insert("authorization".to_string(), format!("Bearer {tok2}"));
        let req_ok_bearer = ParsedHttpRequest {
            method: "POST".to_string(),
            path: "/api/chat".to_string(),
            query: HashMap::new(),
            headers: hdrs,
            body: Vec::new(),
        };
        assert!(is_authorized_token(&tok2, &req_ok_bearer));
    }
}
