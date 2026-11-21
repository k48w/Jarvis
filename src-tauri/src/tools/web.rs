//! # Ferramentas Web & Proteção contra SSRF — `tools/web.rs`
//!
//! Valida URLs públicas após resolução DNS (bloqueando loopback, RFC1918,
//! link-local/metadata, CGNAT e redirecionamentos internos), realiza buscas
//! no DuckDuckGo e extrai texto limpo de páginas web.

use crate::state::AppState;
use regex::Regex;
use std::sync::OnceLock;
use std::time::Duration;
pub fn validate_http_url(raw: &str) -> Result<url::Url, String> {
    let parsed = url::Url::parse(raw.trim()).map_err(|_| "URL inválida".to_string())?;
    match parsed.scheme() {
        "http" | "https" => Ok(parsed),
        _ => Err("Apenas URLs http:// ou https:// são permitidas".to_string()),
    }
}

pub fn is_blocked_ip(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => {
            let o = v4.octets();
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_multicast()
                || v4.is_broadcast()
                || v4.is_unspecified()
                // CGNAT 100.64.0.0/10
                || (o[0] == 100 && (o[1] & 0b1100_0000) == 64)
                // Documentation / Benchmark / Reserved
                || (o[0] == 192 && o[1] == 0 && o[2] == 2)
                || (o[0] == 198 && (o[1] == 18 || o[1] == 19))
                || (o[0] == 198 && o[1] == 51 && o[2] == 100)
                || (o[0] == 203 && o[1] == 0 && o[2] == 113)
                || o[0] >= 240
        }
        std::net::IpAddr::V6(v6) => {
            if let Some(mapped) = v6.to_ipv4_mapped() {
                return is_blocked_ip(std::net::IpAddr::V4(mapped));
            }
            let seg = v6.segments();
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                // Unique local fc00::/7
                || (seg[0] & 0xfe00) == 0xfc00
                // Link-local fe80::/10
                || (seg[0] & 0xffc0) == 0xfe80
        }
    }
}

pub async fn validate_public_http_url(raw: &str) -> Result<url::Url, String> {
    let parsed = validate_http_url(raw)?;
    let host = parsed
        .host_str()
        .ok_or_else(|| "URL sem host válido".to_string())?;
    let host_clean = host
        .trim_matches(|c| c == '[' || c == ']')
        .to_ascii_lowercase();

    if host_clean == "localhost"
        || host_clean.ends_with(".localhost")
        || host_clean.ends_with(".local")
        || host_clean.ends_with(".internal")
        || host_clean == "metadata.google.internal"
    {
        return Err("Acesso a hosts internos/locais bloqueado (proteção SSRF).".to_string());
    }

    if let Ok(ip) = host_clean.parse::<std::net::IpAddr>() {
        if is_blocked_ip(ip) {
            return Err(
                "Acesso a endereços IP privados/locais bloqueado (proteção SSRF).".to_string(),
            );
        }
        return Ok(parsed);
    }

    let port = parsed.port_or_known_default().unwrap_or(80);
    let addrs = tokio::net::lookup_host((host_clean.as_str(), port))
        .await
        .map_err(|_| "Não foi possível resolver o host da URL.".to_string())?;
    let resolved: Vec<std::net::SocketAddr> = addrs.collect();
    if resolved.is_empty() {
        return Err("Host sem endereços IP válidos.".to_string());
    }
    for addr in resolved {
        if is_blocked_ip(addr.ip()) {
            return Err(
                "Host resolve para IP interno/privado bloqueado (proteção SSRF).".to_string(),
            );
        }
    }
    Ok(parsed)
}

pub async fn safe_fetch_url(
    raw_url: &str,
    user_agent: &str,
    timeout: Duration,
) -> Result<(url::Url, reqwest::Response), String> {
    let client = reqwest::Client::builder()
        .timeout(timeout)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| format!("Falha ao preparar cliente HTTP seguro: {e}"))?;

    let mut current = validate_public_http_url(raw_url).await?;
    for _hop in 0..5 {
        let resp = client
            .get(current.as_str())
            .header("User-Agent", user_agent)
            .send()
            .await
            .map_err(|e| format!("Erro ao abrir página: {e}"))?;

        if resp.status().is_redirection() {
            let loc = resp
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| "Redirecionamento HTTP sem destino.".to_string())?;
            let next = current
                .join(loc.trim())
                .map_err(|_| "URL de redirecionamento inválida.".to_string())?;
            current = validate_public_http_url(next.as_str()).await?;
            continue;
        }
        return Ok((current, resp));
    }
    Err("Muitos redirecionamentos HTTP.".to_string())
}

fn decode_html_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
}

fn strip_html_tags(s: &str) -> String {
    static RE_TAG: OnceLock<Regex> = OnceLock::new();
    static RE_WS: OnceLock<Regex> = OnceLock::new();
    let re_tag = RE_TAG.get_or_init(|| Regex::new(r"(?s)<[^>]+>").expect("valid tag regex"));
    let re_ws = RE_WS.get_or_init(|| Regex::new(r"\s+").expect("valid ws regex"));
    let no_tags = re_tag.replace_all(s, " ");
    let decoded = decode_html_entities(&no_tags);
    re_ws.replace_all(decoded.trim(), " ").trim().to_string()
}

pub fn parse_duckduckgo_html(html: &str) -> Vec<(String, String, String)> {
    static RE_LINK: OnceLock<Regex> = OnceLock::new();
    static RE_SNIPPET: OnceLock<Regex> = OnceLock::new();
    let re_link = RE_LINK.get_or_init(|| {
        Regex::new(r#"(?s)<a[^>]*class="[^"]*result__a[^"]*"[^>]*href="([^"]+)"[^>]*>(.*?)</a>"#)
            .expect("valid ddg link regex")
    });
    let re_snippet = RE_SNIPPET.get_or_init(|| {
        Regex::new(r#"(?s)<(?:a|div)[^>]*class="[^"]*result__snippet[^"]*"[^>]*>(.*?)</(?:a|div)>"#)
            .expect("valid ddg snippet regex")
    });

    let links: Vec<_> = re_link.captures_iter(html).take(6).collect();
    let snippets: Vec<_> = re_snippet.captures_iter(html).take(6).collect();
    let mut results = Vec::new();

    for (idx, cap) in links.iter().enumerate() {
        let raw_href = decode_html_entities(&cap[1]);
        let clean_url = if let Some(pos) = raw_href.find("uddg=") {
            let encoded = &raw_href[pos + 5..];
            let slice = encoded.split('&').next().unwrap_or(encoded);
            url::form_urlencoded::parse(format!("u={slice}").as_bytes())
                .find(|(k, _)| k == "u")
                .map(|(_, v)| v.into_owned())
                .unwrap_or(raw_href)
        } else if raw_href.starts_with("//") {
            format!("https:{raw_href}")
        } else {
            raw_href
        };
        let title = strip_html_tags(&cap[2]);
        let snippet = snippets
            .get(idx)
            .map(|s| strip_html_tags(&s[1]))
            .unwrap_or_default();
        if !title.is_empty() && !clean_url.is_empty() {
            results.push((title, clean_url, snippet));
        }
    }
    results
}

pub(crate) async fn run_web_search(state: &AppState, query: &str) -> Result<String, String> {
    let q = query.trim();
    if q.is_empty() {
        return Err("Termo de busca vazio".to_string());
    }

    let resp = state
        .http
        .get("https://html.duckduckgo.com/html/")
        .query(&[("q", q), ("kl", "br-pt")])
        .header(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
        )
        .send()
        .await
        .map_err(|e| format!("Falha ao consultar busca na web: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("Motor de busca retornou HTTP {}", resp.status()));
    }
    let html = resp
        .text()
        .await
        .map_err(|e| format!("Falha ao ler resultados da busca: {e}"))?;
    let parsed = parse_duckduckgo_html(&html);
    if parsed.is_empty() {
        return Ok("Nenhum resultado claro encontrado para essa busca.".to_string());
    }
    let formatted: Vec<String> = parsed
        .into_iter()
        .take(5)
        .enumerate()
        .map(|(i, (title, url, snippet))| format!("{}. {title}\n   {url}\n   {snippet}", i + 1))
        .collect();
    Ok(formatted.join("\n\n"))
}

pub(crate) async fn run_fetch_webpage(_state: &AppState, raw_url: &str) -> Result<String, String> {
    let (_final_url, resp) = safe_fetch_url(
        raw_url,
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
        Duration::from_secs(12),
    )
    .await?;
    if !resp.status().is_success() {
        return Err(format!("Página retornou HTTP {}", resp.status()));
    }
    let html = resp
        .text()
        .await
        .map_err(|e| format!("Erro ao ler corpo da página: {e}"))?;

    static RE_SCRIPT: OnceLock<Regex> = OnceLock::new();
    let re_script = RE_SCRIPT.get_or_init(|| {
        Regex::new(r"(?is)<(script|style|noscript)[^>]*>.*?</(script|style|noscript)>")
            .expect("valid script regex")
    });
    let without_scripts = re_script.replace_all(&html, " ");
    let clean = strip_html_tags(&without_scripts);
    let truncated: String = clean.chars().take(5000).collect();
    if truncated.is_empty() {
        Ok("Página carregada, mas sem texto legível extraído.".to_string())
    } else {
        Ok(truncated)
    }
}
