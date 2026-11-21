use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Rect, Webview, WebviewBuilder,
    WebviewUrl,
};
use tokio::sync::oneshot;

pub const BROWSER_LABEL: &str = "jarvis-browser";
const DEFAULT_URL: &str = "https://www.google.com/";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserElement {
    pub id: u32,
    pub tag: String,
    pub role: String,
    pub input_type: String,
    pub label: String,
    pub placeholder: String,
    pub value: String,
    pub href: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserInspectResult {
    pub url: String,
    pub title: String,
    pub scroll_y: f64,
    pub max_scroll_y: f64,
    pub viewport_width: f64,
    pub viewport_height: f64,
    pub text: String,
    pub elements: Vec<BrowserElement>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserStatus {
    pub url: String,
    pub title: String,
    pub element_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSnapshot {
    pub url: String,
    pub title: String,
    pub data_base64: String,
    pub mime_type: String,
    pub viewport_width: f64,
    pub viewport_height: f64,
    pub scroll_y: f64,
    pub max_scroll_y: f64,
    pub elements: Vec<BrowserElement>,
    pub text: String,
}

pub fn normalize_browser_url(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return DEFAULT_URL.to_string();
    }
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return trimmed.to_string();
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("localhost")
        || lower.starts_with("127.0.0.1")
        || lower.starts_with("0.0.0.0")
    {
        return format!("http://{trimmed}");
    }
    if !trimmed.contains(' ') && trimmed.contains('.') {
        return format!("https://{trimmed}");
    }
    let encoded: String = url::form_urlencoded::byte_serialize(trimmed.as_bytes()).collect();
    format!("https://www.google.com/search?q={encoded}")
}

fn emit_browser_updated(app: &AppHandle, action: &str, url: &str, title: &str) {
    let _ = app.emit(
        "jarvis:browser-updated",
        json!({
            "action": action,
            "url": url,
            "title": title,
        }),
    );
}

pub async fn ensure_browser_webview(
    app: &AppHandle,
    initial_url: Option<&str>,
    bounds: Option<(f64, f64, f64, f64)>,
    visible: Option<bool>,
) -> Result<Webview, String> {
    if let Some(existing) = app.get_webview(BROWSER_LABEL) {
        if let Some((x, y, w, h)) = bounds {
            if w >= 40.0 && h >= 40.0 {
                let _ = existing.set_bounds(Rect {
                    position: LogicalPosition::new(x, y).into(),
                    size: LogicalSize::new(w, h).into(),
                });
            }
        }
        if let Some(vis) = visible {
            if vis {
                let _ = existing.show();
            } else {
                let _ = existing.hide();
            }
        }
        return Ok(existing);
    }

    let main_win = app
        .get_window("main")
        .ok_or_else(|| "Janela principal do J.A.R.V.I.S. não encontrada.".to_string())?;

    let url_str = normalize_browser_url(initial_url.unwrap_or(DEFAULT_URL));
    let parsed =
        url::Url::parse(&url_str).map_err(|e| format!("URL inválida para o navegador: {e}"))?;

    let (x, y, w, h) = bounds.unwrap_or((40.0, 150.0, 860.0, 560.0));
    let wv = main_win
        .add_child(
            WebviewBuilder::new(BROWSER_LABEL, WebviewUrl::External(parsed))
                .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/133.0.0.0 Safari/537.36"),
            LogicalPosition::new(x, y),
            LogicalSize::new(w.max(200.0), h.max(200.0)),
        )
        .map_err(|e| format!("Falha ao incorporar navegador WebView2: {e}"))?;

    if visible == Some(false) {
        let _ = wv.hide();
    }

    for _ in 0..25 {
        if probe_cdp_ready(&wv).await {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let current_href = cdp_call_on_webview(
        &wv,
        "Runtime.evaluate",
        json!({ "expression": "location.href", "returnByValue": true }),
    )
    .await
    .ok()
    .and_then(|v| v["result"]["value"].as_str().map(str::to_string))
    .unwrap_or_default();

    if current_href.is_empty() || current_href == "about:blank" {
        let _ = cdp_call_on_webview(&wv, "Page.navigate", json!({ "url": url_str })).await;
        for _ in 0..25 {
            tokio::time::sleep(Duration::from_millis(140)).await;
            let state = cdp_call_on_webview(
                &wv,
                "Runtime.evaluate",
                json!({
                    "expression": "({ href: location.href, ready: document.readyState })",
                    "returnByValue": true
                }),
            )
            .await
            .ok();
            let href = state
                .as_ref()
                .and_then(|v| v["result"]["value"]["href"].as_str())
                .unwrap_or("about:blank");
            let ready = state
                .as_ref()
                .and_then(|v| v["result"]["value"]["ready"].as_str())
                .unwrap_or("");
            if href != "about:blank" && ready == "complete" {
                break;
            }
        }
    }

    Ok(wv)
}

#[cfg(windows)]
async fn probe_cdp_ready(wv: &Webview) -> bool {
    cdp_call_on_webview(wv, "Runtime.evaluate", json!({ "expression": "1+1" }))
        .await
        .is_ok()
}

#[cfg(not(windows))]
async fn probe_cdp_ready(_wv: &Webview) -> bool {
    false
}

#[cfg(windows)]
async fn cdp_call_on_webview(wv: &Webview, method: &str, params: Value) -> Result<Value, String> {
    use webview2_com::CallDevToolsProtocolMethodCompletedHandler;
    use windows::core::HSTRING;

    let (tx, rx) = oneshot::channel::<Result<String, String>>();
    let tx_cell = Arc::new(Mutex::new(Some(tx)));
    let method_owned = method.to_string();
    let params_json = params.to_string();

    let tx_for_err = Arc::clone(&tx_cell);
    let dispatch_res = wv.with_webview(move |platform_wv| {
        let controller = platform_wv.controller();
        let core = match unsafe { controller.CoreWebView2() } {
            Ok(c) => c,
            Err(e) => {
                if let Some(tx) = tx_cell.lock().ok().and_then(|mut g| g.take()) {
                    let _ = tx.send(Err(format!("CoreWebView2 indisponível: {e}")));
                }
                return;
            }
        };

        let method_w = HSTRING::from(&method_owned);
        let params_w = HSTRING::from(&params_json);
        let method_err_label = method_owned.clone();
        let tx_cb = Arc::clone(&tx_cell);

        let handler =
            CallDevToolsProtocolMethodCompletedHandler::create(Box::new(move |hr, result| {
                if let Some(tx) = tx_cb.lock().ok().and_then(|mut g| g.take()) {
                    if hr.is_ok() {
                        let _ = tx.send(Ok(result));
                    } else {
                        let _ = tx.send(Err(format!("Erro CDP em {method_err_label}: {:?}", hr)));
                    }
                }
                Ok(())
            }));

        if let Err(e) = unsafe { core.CallDevToolsProtocolMethod(&method_w, &params_w, &handler) } {
            if let Some(tx) = tx_cell.lock().ok().and_then(|mut g| g.take()) {
                let _ = tx.send(Err(format!("Falha ao invocar CDP {method_owned}: {e}")));
            }
        }
    });

    if let Err(e) = dispatch_res {
        if let Some(tx) = tx_for_err.lock().ok().and_then(|mut g| g.take()) {
            let _ = tx.send(Err(format!("Falha ao acessar WebView2: {e}")));
        }
    }

    let raw = tokio::time::timeout(Duration::from_secs(16), rx)
        .await
        .map_err(|_| format!("Tempo esgotado no comando CDP '{method}'."))?
        .map_err(|_| format!("Canal CDP '{method}' encerrado inesperadamente."))??;

    if raw.trim().is_empty() {
        Ok(json!({}))
    } else {
        serde_json::from_str::<Value>(&raw)
            .map_err(|e| format!("Resposta CDP inválida em '{method}': {e}"))
    }
}

#[cfg(not(windows))]
async fn cdp_call_on_webview(
    _wv: &Webview,
    _method: &str,
    _params: Value,
) -> Result<Value, String> {
    Err("Controle CDP do navegador disponível apenas no Windows (WebView2).".to_string())
}

pub async fn cdp_call(app: &AppHandle, method: &str, params: Value) -> Result<Value, String> {
    let wv = ensure_browser_webview(app, None, None, None).await?;
    cdp_call_on_webview(&wv, method, params).await
}

pub async fn eval_js(app: &AppHandle, expression: &str) -> Result<Value, String> {
    let res = cdp_call(
        app,
        "Runtime.evaluate",
        json!({
            "expression": expression,
            "returnByValue": true,
            "awaitPromise": true
        }),
    )
    .await?;

    if let Some(exc) = res.get("exceptionDetails") {
        let msg = exc["exception"]["description"]
            .as_str()
            .or_else(|| exc["text"].as_str())
            .unwrap_or("Erro ao executar script na página");
        return Err(msg.to_string());
    }

    Ok(res["result"]["value"].clone())
}

async fn wait_for_document_ready(app: &AppHandle, max_wait_ms: u64) {
    let steps = (max_wait_ms / 150).max(1);
    for _ in 0..steps {
        if let Ok(val) = eval_js(app, "document.readyState").await {
            if val.as_str() == Some("complete") {
                tokio::time::sleep(Duration::from_millis(180)).await;
                return;
            }
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }
}

pub async fn sync_bounds(
    app: &AppHandle,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    visible: bool,
    initial_url: Option<&str>,
) -> Result<BrowserStatus, String> {
    if !visible {
        if let Some(existing) = app.get_webview(BROWSER_LABEL) {
            let _ = existing.hide();
        }
        return Ok(BrowserStatus {
            url: String::new(),
            title: String::new(),
            element_count: 0,
        });
    }

    let _wv =
        ensure_browser_webview(app, initial_url, Some((x, y, width, height)), Some(true)).await?;
    get_status(app).await
}

pub async fn set_visible(app: &AppHandle, visible: bool) -> Result<(), String> {
    if let Some(existing) = app.get_webview(BROWSER_LABEL) {
        if visible {
            let _ = existing.show();
        } else {
            let _ = existing.hide();
        }
    }
    Ok(())
}

pub async fn get_status(app: &AppHandle) -> Result<BrowserStatus, String> {
    let Some(wv) = app.get_webview(BROWSER_LABEL) else {
        return Ok(BrowserStatus {
            url: DEFAULT_URL.to_string(),
            title: "Google".to_string(),
            element_count: 0,
        });
    };

    let val = cdp_call_on_webview(
        &wv,
        "Runtime.evaluate",
        json!({
            "expression": "({ url: location.href, title: document.title || location.hostname || '' })",
            "returnByValue": true
        }),
    )
    .await
    .ok();

    let url = val
        .as_ref()
        .and_then(|v| v["result"]["value"]["url"].as_str())
        .map(str::to_string)
        .or_else(|| wv.url().ok().map(|u| u.to_string()))
        .unwrap_or_else(|| DEFAULT_URL.to_string());
    let title = val
        .as_ref()
        .and_then(|v| v["result"]["value"]["title"].as_str())
        .map(str::to_string)
        .unwrap_or_default();

    Ok(BrowserStatus {
        url,
        title,
        element_count: 0,
    })
}

pub async fn navigate(app: &AppHandle, raw_url: &str) -> Result<BrowserInspectResult, String> {
    let url_str = normalize_browser_url(raw_url);
    let parsed = url::Url::parse(&url_str).map_err(|e| format!("URL inválida '{raw_url}': {e}"))?;
    match parsed.scheme() {
        "http" | "https" => {}
        _ => return Err("Apenas endereços http:// ou https:// são permitidos.".to_string()),
    }

    let existed = app.get_webview(BROWSER_LABEL).is_some();
    let wv = ensure_browser_webview(app, Some(parsed.as_str()), None, None).await?;
    if existed {
        let _ = cdp_call_on_webview(&wv, "Page.navigate", json!({ "url": parsed.as_str() })).await;
    }

    tokio::time::sleep(Duration::from_millis(240)).await;
    wait_for_document_ready(app, 4200).await;

    let info = inspect_page(app, false).await?;
    emit_browser_updated(app, "navigate", &info.url, &info.title);
    Ok(info)
}

pub async fn go_back(app: &AppHandle) -> Result<BrowserStatus, String> {
    let _ = eval_js(app, "window.history.back(); true").await;
    tokio::time::sleep(Duration::from_millis(420)).await;
    wait_for_document_ready(app, 2500).await;
    let st = get_status(app).await?;
    emit_browser_updated(app, "back", &st.url, &st.title);
    Ok(st)
}

pub async fn go_forward(app: &AppHandle) -> Result<BrowserStatus, String> {
    let _ = eval_js(app, "window.history.forward(); true").await;
    tokio::time::sleep(Duration::from_millis(420)).await;
    wait_for_document_ready(app, 2500).await;
    let st = get_status(app).await?;
    emit_browser_updated(app, "forward", &st.url, &st.title);
    Ok(st)
}

pub async fn reload(app: &AppHandle) -> Result<BrowserStatus, String> {
    let _ = cdp_call(app, "Page.reload", json!({ "ignoreCache": false })).await;
    tokio::time::sleep(Duration::from_millis(360)).await;
    wait_for_document_ready(app, 3200).await;
    let st = get_status(app).await?;
    emit_browser_updated(app, "reload", &st.url, &st.title);
    Ok(st)
}

const INSPECT_JS: &str = r#"(() => {
  const showOverlay = __SHOW_OVERLAY__;
  const old = document.getElementById('__jarvis_hud_overlay');
  if (old) old.remove();

  const selectors = [
    'a[href]',
    'button',
    'input:not([type="hidden"])',
    'textarea',
    'select',
    '[role="button"]',
    '[role="link"]',
    '[role="textbox"]',
    '[role="searchbox"]',
    '[role="combobox"]',
    '[role="tab"]',
    '[role="menuitem"]',
    '[contenteditable="true"]',
    '[onclick]'
  ].join(',');

  const vw = window.innerWidth || 1000;
  const vh = window.innerHeight || 700;
  const seen = new Set();
  const nodes = Array.from(document.querySelectorAll(selectors));
  const elements = [];

  let overlay = null;
  if (showOverlay && document.body) {
    overlay = document.createElement('div');
    overlay.id = '__jarvis_hud_overlay';
    overlay.style.cssText = 'position:fixed;inset:0;pointer-events:none;z-index:2147483647;overflow:hidden;';
    document.body.appendChild(overlay);
  }

  for (const el of nodes) {
    if (elements.length >= 80) break;
    if (seen.has(el)) continue;
    seen.add(el);
    if (el.closest('#__jarvis_hud_overlay')) continue;

    const style = window.getComputedStyle(el);
    if (style.display === 'none' || style.visibility === 'hidden' || parseFloat(style.opacity || '1') < 0.05) {
      continue;
    }
    const r = el.getBoundingClientRect();
    if (r.width < 8 || r.height < 8) continue;
    if (r.bottom < -40 || r.top > vh + 40 || r.right < -40 || r.left > vw + 40) continue;

    const tag = el.tagName.toLowerCase();
    const role = (el.getAttribute('role') || '').trim();
    const inputType = (el.getAttribute('type') || '').trim();
    const placeholder = (el.getAttribute('placeholder') || '').trim();
    const aria = (el.getAttribute('aria-label') || el.getAttribute('title') || '').trim();
    const inner = (el.innerText || el.textContent || '').replace(/\s+/g, ' ').trim();
    const label = (aria || inner || placeholder || el.getAttribute('name') || '').slice(0, 90);
    const val = typeof el.value === 'string' ? el.value.slice(0, 80) : '';
    const href = tag === 'a' ? (el.href || '').slice(0, 140) : '';

    if (!label && !placeholder && tag !== 'input' && tag !== 'textarea' && tag !== 'select') {
      continue;
    }

    const id = elements.length + 1;
    el.setAttribute('data-jarvis-id', String(id));

    const cx = Math.round(r.left + r.width / 2);
    const cy = Math.round(r.top + r.height / 2);

    elements.push({
      id,
      tag,
      role,
      inputType,
      label,
      placeholder,
      value: val,
      href,
      x: cx,
      y: cy,
      width: Math.round(r.width),
      height: Math.round(r.height)
    });

    if (overlay && r.top >= 0 && r.left >= 0 && r.top < vh - 12 && r.left < vw - 12) {
      const box = document.createElement('div');
      box.style.cssText = `position:fixed;left:${Math.round(r.left)}px;top:${Math.round(r.top)}px;width:${Math.round(r.width)}px;height:${Math.round(r.height)}px;border:1px solid rgba(108,232,255,0.68);border-radius:4px;box-sizing:border-box;`;
      const badge = document.createElement('span');
      badge.textContent = String(id);
      badge.style.cssText = 'position:absolute;top:-10px;left:-2px;background:#041422;color:#6ce8ff;border:1px solid #6ce8ff;border-radius:4px;padding:0 4px;font:700 10px/14px Consolas,monospace;box-shadow:0 0 8px rgba(108,232,255,0.55);';
      box.appendChild(badge);
      overlay.appendChild(box);
    }
  }

  const rawText = (document.body ? document.body.innerText : '') || '';
  const text = rawText
    .split(/\r?\n/)
    .map(l => l.trim())
    .filter(l => l.length > 0)
    .join('\n')
    .slice(0, 4500);

  return {
    url: location.href,
    title: document.title || location.hostname || 'Sem título',
    scrollY: Math.round(window.scrollY || 0),
    maxScrollY: Math.max(0, Math.round((document.documentElement.scrollHeight || 0) - vh)),
    viewportWidth: vw,
    viewportHeight: vh,
    text,
    elements
  };
})()"#;

pub async fn inspect_page(
    app: &AppHandle,
    show_overlay: bool,
) -> Result<BrowserInspectResult, String> {
    let script = INSPECT_JS.replace(
        "__SHOW_OVERLAY__",
        if show_overlay { "true" } else { "false" },
    );
    let val = eval_js(app, &script).await?;
    serde_json::from_value::<BrowserInspectResult>(val)
        .map_err(|e| format!("Falha ao interpretar elementos da página: {e}"))
}

pub async fn capture_snapshot(
    app: &AppHandle,
    show_overlay: bool,
) -> Result<BrowserSnapshot, String> {
    let wv = ensure_browser_webview(app, None, None, None).await?;
    let inspected = inspect_page(app, show_overlay).await?;

    let shot = cdp_call_on_webview(
        &wv,
        "Page.captureScreenshot",
        json!({ "format": "jpeg", "quality": 76 }),
    )
    .await?;

    let data_base64 = shot["data"]
        .as_str()
        .ok_or_else(|| "WebView2 não retornou imagem da página.".to_string())?
        .to_string();

    Ok(BrowserSnapshot {
        url: inspected.url,
        title: inspected.title,
        data_base64,
        mime_type: "image/jpeg".to_string(),
        viewport_width: inspected.viewport_width,
        viewport_height: inspected.viewport_height,
        scroll_y: inspected.scroll_y,
        max_scroll_y: inspected.max_scroll_y,
        elements: inspected.elements,
        text: inspected.text,
    })
}

async fn flash_click_indicator(app: &AppHandle, x: f64, y: f64) {
    let js = format!(
        r#"(() => {{
          if (!document.body) return;
          const dot = document.createElement('div');
          dot.style.cssText = 'position:fixed;left:{x}px;top:{y}px;width:22px;height:22px;margin:-11px 0 0 -11px;border-radius:50%;border:2px solid #6ce8ff;background:rgba(108,232,255,0.28);box-shadow:0 0 18px #6ce8ff;pointer-events:none;z-index:2147483647;transition:transform 0.35s ease,opacity 0.35s ease;';
          document.body.appendChild(dot);
          requestAnimationFrame(() => {{ dot.style.transform = 'scale(2.1)'; dot.style.opacity = '0'; }});
          setTimeout(() => dot.remove(), 380);
        }})()"#
    );
    let _ = eval_js(app, &js).await;
}

pub async fn dispatch_mouse_click(app: &AppHandle, x: f64, y: f64) -> Result<(), String> {
    flash_click_indicator(app, x, y).await;
    cdp_call(
        app,
        "Input.dispatchMouseEvent",
        json!({ "type": "mouseMoved", "x": x, "y": y }),
    )
    .await?;
    cdp_call(
        app,
        "Input.dispatchMouseEvent",
        json!({ "type": "mousePressed", "x": x, "y": y, "button": "left", "clickCount": 1 }),
    )
    .await?;
    cdp_call(
        app,
        "Input.dispatchMouseEvent",
        json!({ "type": "mouseReleased", "x": x, "y": y, "button": "left", "clickCount": 1 }),
    )
    .await?;
    Ok(())
}

async fn resolve_element_point(
    app: &AppHandle,
    element_id: Option<u32>,
    text_query: Option<&str>,
    prefer_input: bool,
) -> Result<(f64, f64, String), String> {
    let id_json = element_id
        .map(|v| v.to_string())
        .unwrap_or_else(|| "null".to_string());
    let query_json =
        serde_json::to_string(&text_query.unwrap_or("")).unwrap_or_else(|_| "\"\"".to_string());
    let prefer_js = if prefer_input { "true" } else { "false" };

    let js = format!(
        r#"(() => {{
          const targetId = {id_json};
          const query = ({query_json}).trim().toLowerCase();
          const preferInput = {prefer_js};
          let el = null;

          if (targetId !== null) {{
            el = document.querySelector(`[data-jarvis-id="${{targetId}}"]`);
          }}

          if (!el && query) {{
            const selector = preferInput
              ? 'input:not([type="hidden"]),textarea,[role="textbox"],[role="searchbox"],[contenteditable="true"],[role="combobox"]'
              : 'a[href],button,input:not([type="hidden"]),textarea,select,[role="button"],[role="link"],[role="textbox"],[role="searchbox"],[role="tab"],[contenteditable="true"],[onclick]';
            const list = Array.from(document.querySelectorAll(selector));
            let bestScore = 0;
            for (const cand of list) {{
              const st = window.getComputedStyle(cand);
              if (st.display === 'none' || st.visibility === 'hidden') continue;
              const r = cand.getBoundingClientRect();
              if (r.width < 6 || r.height < 6) continue;
              const hay = [
                cand.getAttribute('aria-label') || '',
                cand.getAttribute('placeholder') || '',
                cand.getAttribute('title') || '',
                cand.getAttribute('name') || '',
                (cand.innerText || cand.textContent || '').replace(/\s+/g, ' ').trim()
              ].join(' ').toLowerCase();
              if (!hay) continue;
              let score = 0;
              if (hay.trim() === query) score = 100;
              else if (hay.includes(query)) score = 60 - Math.min(40, Math.abs(hay.length - query.length) / 4);
              if (score > bestScore) {{
                bestScore = score;
                el = cand;
              }}
            }}
          }}

          if (!el && preferInput) {{
            const inputs = Array.from(document.querySelectorAll('input[type="search"],input[name="q"],input[name="search_query"],input[type="text"],textarea,[role="searchbox"],[role="textbox"],[contenteditable="true"]'));
            for (const cand of inputs) {{
              const st = window.getComputedStyle(cand);
              if (st.display === 'none' || st.visibility === 'hidden') continue;
              const r = cand.getBoundingClientRect();
              if (r.width >= 40 && r.height >= 16 && r.top >= 0 && r.top < window.innerHeight) {{
                el = cand;
                break;
              }}
            }}
          }}

          if (!el) return {{ found: false }};

          el.scrollIntoView({{ behavior: 'instant', block: 'center', inline: 'center' }});
          const rect = el.getBoundingClientRect();
          const cx = Math.max(2, Math.min(window.innerWidth - 2, rect.left + rect.width / 2));
          const cy = Math.max(2, Math.min(window.innerHeight - 2, rect.top + rect.height / 2));
          const desc = (el.getAttribute('aria-label') || el.getAttribute('placeholder') || el.innerText || el.tagName || '').replace(/\s+/g, ' ').trim().slice(0, 70);
          return {{ found: true, x: cx, y: cy, desc }};
        }})()"#
    );

    let val = eval_js(app, &js).await?;
    if val["found"].as_bool() == Some(true) {
        let x = val["x"].as_f64().unwrap_or(0.0);
        let y = val["y"].as_f64().unwrap_or(0.0);
        let desc = val["desc"].as_str().unwrap_or("elemento").to_string();
        Ok((x, y, desc))
    } else {
        Err("Elemento não encontrado na página atual. Execute browser_inspect para ver os IDs atualizados.".to_string())
    }
}

pub async fn click(
    app: &AppHandle,
    element_id: Option<u32>,
    text: Option<&str>,
    x: Option<f64>,
    y: Option<f64>,
) -> Result<String, String> {
    let (cx, cy, label) = if element_id.is_some() || text.is_some_and(|t| !t.trim().is_empty()) {
        let resolved = resolve_element_point(app, element_id, text, false).await;
        match resolved {
            Ok(pt) => pt,
            Err(_) if element_id.is_some() => {
                let _ = inspect_page(app, false).await;
                resolve_element_point(app, element_id, text, false).await?
            }
            Err(e) => return Err(e),
        }
    } else if let (Some(px), Some(py)) = (x, y) {
        (px, py, format!("coordenada ({px:.0}, {py:.0})"))
    } else {
        return Err("Informe element_id, text ou coordenadas (x, y) para clicar.".to_string());
    };

    tokio::time::sleep(Duration::from_millis(70)).await;
    dispatch_mouse_click(app, cx, cy).await?;
    tokio::time::sleep(Duration::from_millis(420)).await;
    wait_for_document_ready(app, 2500).await;

    let st = get_status(app).await.ok();
    let url = st.as_ref().map(|i| i.url.as_str()).unwrap_or("");
    let title = st.as_ref().map(|i| i.title.as_str()).unwrap_or("");
    emit_browser_updated(app, "click", url, title);

    Ok(format!(
        "Clique realizado em '{label}' ({cx:.0}, {cy:.0}). Página atual: {title} ({url})"
    ))
}

pub fn map_key_spec(key: &str) -> (&'static str, &'static str, i32, Option<&'static str>) {
    match key.trim().to_ascii_lowercase().as_str() {
        "enter" | "return" => ("Enter", "Enter", 13, Some("\r")),
        "escape" | "esc" => ("Escape", "Escape", 27, None),
        "tab" => ("Tab", "Tab", 9, None),
        "backspace" => ("Backspace", "Backspace", 8, None),
        "delete" | "del" => ("Delete", "Delete", 46, None),
        "space" | " " => (" ", "Space", 32, Some(" ")),
        "arrowup" | "up" => ("ArrowUp", "ArrowUp", 38, None),
        "arrowdown" | "down" => ("ArrowDown", "ArrowDown", 40, None),
        "arrowleft" | "left" => ("ArrowLeft", "ArrowLeft", 37, None),
        "arrowright" | "right" => ("ArrowRight", "ArrowRight", 39, None),
        "pageup" => ("PageUp", "PageUp", 33, None),
        "pagedown" => ("PageDown", "PageDown", 34, None),
        "home" => ("Home", "Home", 36, None),
        "end" => ("End", "End", 35, None),
        _ => ("", "", 0, None),
    }
}

pub async fn press_key(app: &AppHandle, key: &str) -> Result<String, String> {
    let (k, code, vk, text_opt) = map_key_spec(key);
    if !k.is_empty() {
        let mut down = json!({
            "type": "keyDown",
            "key": k,
            "code": code,
            "windowsVirtualKeyCode": vk,
            "nativeVirtualKeyCode": vk
        });
        if let Some(txt) = text_opt {
            down["text"] = json!(txt);
            down["unmodifiedText"] = json!(txt);
        }
        cdp_call(app, "Input.dispatchKeyEvent", down).await?;
        cdp_call(
            app,
            "Input.dispatchKeyEvent",
            json!({
                "type": "keyUp",
                "key": k,
                "code": code,
                "windowsVirtualKeyCode": vk,
                "nativeVirtualKeyCode": vk
            }),
        )
        .await?;
    } else if !key.is_empty() {
        cdp_call(app, "Input.insertText", json!({ "text": key })).await?;
    }

    if k == "Enter" {
        tokio::time::sleep(Duration::from_millis(600)).await;
        wait_for_document_ready(app, 3000).await;
    } else {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let st = get_status(app).await.ok();
    emit_browser_updated(
        app,
        "key",
        st.as_ref().map(|i| i.url.as_str()).unwrap_or(""),
        st.as_ref().map(|i| i.title.as_str()).unwrap_or(""),
    );
    Ok(format!("Tecla '{key}' enviada."))
}

pub async fn type_text(
    app: &AppHandle,
    text: &str,
    element_id: Option<u32>,
    target_label: Option<&str>,
    clear_first: bool,
    press_enter: bool,
) -> Result<String, String> {
    let mut target_desc = "campo em foco".to_string();

    if element_id.is_some() || target_label.is_some_and(|t| !t.trim().is_empty()) {
        let resolved = resolve_element_point(app, element_id, target_label, true).await;
        let (cx, cy, desc) = match resolved {
            Ok(pt) => pt,
            Err(_) => {
                let _ = inspect_page(app, false).await;
                resolve_element_point(app, element_id, target_label, true).await?
            }
        };
        target_desc = desc;
        dispatch_mouse_click(app, cx, cy).await?;
        tokio::time::sleep(Duration::from_millis(100)).await;
    } else {
        let active_is_editable = eval_js(
            app,
            "(() => { const a = document.activeElement; return !!(a && (a.tagName === 'INPUT' || a.tagName === 'TEXTAREA' || a.isContentEditable)); })()",
        )
        .await
        .ok()
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

        if !active_is_editable {
            if let Ok((cx, cy, desc)) = resolve_element_point(app, None, None, true).await {
                target_desc = desc;
                dispatch_mouse_click(app, cx, cy).await?;
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }

    if clear_first {
        let _ = cdp_call(
            app,
            "Input.dispatchKeyEvent",
            json!({
                "type": "keyDown",
                "modifiers": 2,
                "key": "a",
                "code": "KeyA",
                "windowsVirtualKeyCode": 65
            }),
        )
        .await;
        let _ = cdp_call(
            app,
            "Input.dispatchKeyEvent",
            json!({
                "type": "keyUp",
                "modifiers": 2,
                "key": "a",
                "code": "KeyA",
                "windowsVirtualKeyCode": 65
            }),
        )
        .await;
        let _ = press_key(app, "Backspace").await;
    }

    if !text.is_empty() {
        cdp_call(app, "Input.insertText", json!({ "text": text })).await?;
    }

    if press_enter {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let _ = press_key(app, "Enter").await;
    } else {
        tokio::time::sleep(Duration::from_millis(140)).await;
    }

    let st = get_status(app).await.ok();
    let url = st.as_ref().map(|i| i.url.as_str()).unwrap_or("");
    let title = st.as_ref().map(|i| i.title.as_str()).unwrap_or("");
    emit_browser_updated(app, "type", url, title);

    Ok(format!(
        "Texto '{}' digitado em '{}'{} · Página atual: {title} ({url})",
        text,
        target_desc,
        if press_enter {
            " e Enter pressionado"
        } else {
            ""
        }
    ))
}

pub async fn scroll_page(
    app: &AppHandle,
    direction: &str,
    amount: Option<i32>,
) -> Result<String, String> {
    let px = amount.unwrap_or(560).clamp(100, 3000);
    let expr = match direction.trim().to_ascii_lowercase().as_str() {
        "up" | "cima" => format!(
            "window.scrollBy({{ top: -{px}, behavior: 'smooth' }}); Math.round(window.scrollY)"
        ),
        "top" | "topo" => "window.scrollTo({ top: 0, behavior: 'smooth' }); 0".to_string(),
        "bottom" | "fim" | "baixo_fim" => {
            "window.scrollTo({ top: document.documentElement.scrollHeight, behavior: 'smooth' }); Math.round(window.scrollY)"
                .to_string()
        }
        _ => format!(
            "window.scrollBy({{ top: {px}, behavior: 'smooth' }}); Math.round(window.scrollY)"
        ),
    };

    let y = eval_js(app, &expr).await?.as_f64().unwrap_or(0.0);
    tokio::time::sleep(Duration::from_millis(160)).await;

    let st = get_status(app).await.ok();
    emit_browser_updated(
        app,
        "scroll",
        st.as_ref().map(|i| i.url.as_str()).unwrap_or(""),
        st.as_ref().map(|i| i.title.as_str()).unwrap_or(""),
    );

    Ok(format!(
        "Rolagem '{direction}' aplicada. Posição Y atual: {y:.0}px."
    ))
}

pub fn format_inspect_for_llm(info: &BrowserInspectResult) -> String {
    let mut out = format!(
        "URL: {}\nTítulo: {}\nRolagem: {:.0}px / {:.0}px (Viewport {:.0}x{:.0})\n\nElementos interativos visíveis ({}):\n",
        info.url,
        info.title,
        info.scroll_y,
        info.max_scroll_y,
        info.viewport_width,
        info.viewport_height,
        info.elements.len()
    );

    for el in info.elements.iter().take(65) {
        let mut desc = format!("[{}] <{}", el.id, el.tag);
        if !el.input_type.is_empty() {
            desc.push_str(&format!(" type=\"{}\"", el.input_type));
        }
        if !el.role.is_empty() {
            desc.push_str(&format!(" role=\"{}\"", el.role));
        }
        desc.push('>');
        if !el.label.is_empty() {
            desc.push_str(&format!(" \"{}\"", el.label));
        }
        if !el.placeholder.is_empty() && el.placeholder != el.label {
            desc.push_str(&format!(" placeholder=\"{}\"", el.placeholder));
        }
        if !el.value.is_empty() {
            desc.push_str(&format!(" valor=\"{}\"", el.value));
        }
        desc.push_str(&format!(" ({:.0},{:.0})\n", el.x, el.y));
        out.push_str(&desc);
    }

    if !info.text.trim().is_empty() {
        out.push_str("\nTexto visível da página:\n");
        let snippet: String = info.text.chars().take(2800).collect();
        out.push_str(&snippet);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_urls_and_queries() {
        assert_eq!(normalize_browser_url("youtube.com"), "https://youtube.com");
        assert_eq!(
            normalize_browser_url("https://github.com/trending"),
            "https://github.com/trending"
        );
        assert_eq!(
            normalize_browser_url("localhost:5173"),
            "http://localhost:5173"
        );
        assert!(
            normalize_browser_url("preço rtx 4060").starts_with("https://www.google.com/search?q=")
        );
    }

    #[test]
    fn maps_special_keys() {
        assert_eq!(map_key_spec("Enter").2, 13);
        assert_eq!(map_key_spec("Escape").2, 27);
        assert_eq!(map_key_spec("Backspace").2, 8);
    }
}
