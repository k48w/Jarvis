//! # Cliente Nativo MCP (Model Context Protocol) — `mcp.rs`
//!
//! Permite conectar servidores MCP externos (Notion, GitHub, Spotify, Postgres,
//! Discord, ferramentas locais em Node/Python ou endpoints Streamable HTTP)
//! diretamente ao Jarvis. As ferramentas descobertas via JSON-RPC 2.0
//! (`initialize` → `tools/list`) são incorporadas automaticamente ao catálogo
//! de ChatGPT, Gemini e modelos locais Ollama/LM Studio sob o prefixo
//! `mcp__<servidor>__<ferramenta>` e executadas sob a política de permissões.

use crate::config::atomic_write;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::RwLock;
use url::Url;
use uuid::Uuid;

const MCP_SERVERS_FILE: &str = "mcp_servers.json";
const MCP_TIMEOUT: Duration = Duration::from_secs(18);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerConfig {
    pub id: String,
    pub name: String,
    /// `"stdio"` (processo local) ou `"http"` (JSON-RPC / Streamable HTTP).
    pub transport: String,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpDiscoveredTool {
    pub server_id: String,
    pub server_name: String,
    pub full_name: String,
    pub remote_name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerStatus {
    pub server: McpServerConfig,
    pub tool_count: usize,
    pub tools: Vec<String>,
    pub last_error: Option<String>,
}

#[derive(Default)]
pub struct McpRuntimeCache {
    pub tools: RwLock<Vec<McpDiscoveredTool>>,
    pub last_errors: RwLock<std::collections::HashMap<String, String>>,
}

fn servers_path(data_dir: &Path) -> PathBuf {
    data_dir.join(MCP_SERVERS_FILE)
}

pub fn list_servers(data_dir: &Path) -> Vec<McpServerConfig> {
    let Ok(raw) = fs::read_to_string(servers_path(data_dir)) else {
        return Vec::new();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

fn save_servers(data_dir: &Path, list: &[McpServerConfig]) -> Result<(), String> {
    let body = serde_json::to_string_pretty(list)
        .map_err(|e| format!("Falha ao serializar servidores MCP: {e}"))?;
    atomic_write(&servers_path(data_dir), body.as_bytes())
}

pub fn upsert_server(
    data_dir: &Path,
    name: &str,
    transport: &str,
    command: Option<String>,
    args: Vec<String>,
    url: Option<String>,
    enabled: bool,
) -> Result<McpServerConfig, String> {
    let clean_name = name.trim();
    if clean_name.is_empty() {
        return Err("Informe um nome para o servidor MCP.".to_string());
    }
    let norm_transport = transport.trim().to_ascii_lowercase();
    if norm_transport != "stdio" && norm_transport != "http" {
        return Err("Transporte MCP deve ser 'stdio' ou 'http'.".to_string());
    }
    let clean_cmd = command
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let clean_url = url.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());

    if norm_transport == "stdio" && clean_cmd.is_none() {
        return Err(
            "Servidores MCP 'stdio' exigem o comando executável (ex.: npx ou python).".to_string(),
        );
    }
    if norm_transport == "http" {
        let Some(u) = clean_url.as_deref() else {
            return Err("Servidores MCP 'http' exigem uma URL http:// ou https://.".to_string());
        };
        let parsed = Url::parse(u).map_err(|e| format!("URL MCP inválida: {e}"))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err("URL MCP deve começar com http:// ou https://.".to_string());
        }
    }

    let mut list = list_servers(data_dir);
    let slug = sanitize_segment(clean_name);
    let id = list
        .iter()
        .find(|s| s.name.eq_ignore_ascii_case(clean_name))
        .map(|s| s.id.clone())
        .unwrap_or_else(|| format!("{slug}-{}", &Uuid::new_v4().simple().to_string()[..6]));

    let item = McpServerConfig {
        id: id.clone(),
        name: clean_name.to_string(),
        transport: norm_transport,
        command: clean_cmd,
        args,
        url: clean_url,
        enabled,
    };

    list.retain(|s| s.id != id);
    list.push(item.clone());
    save_servers(data_dir, &list)?;
    Ok(item)
}

pub fn toggle_server(data_dir: &Path, id: &str, enabled: bool) -> Result<McpServerConfig, String> {
    let mut list = list_servers(data_dir);
    let Some(target) = list.iter_mut().find(|s| s.id == id) else {
        return Err("Servidor MCP não encontrado.".to_string());
    };
    target.enabled = enabled;
    let out = target.clone();
    save_servers(data_dir, &list)?;
    Ok(out)
}

pub fn delete_server(data_dir: &Path, id: &str) -> Result<bool, String> {
    let mut list = list_servers(data_dir);
    let before = list.len();
    list.retain(|s| s.id != id);
    if list.len() == before {
        return Ok(false);
    }
    save_servers(data_dir, &list)?;
    Ok(true)
}

pub fn sanitize_segment(raw: &str) -> String {
    let cleaned: String = raw
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let trimmed = cleaned.trim_matches('_');
    if trimmed.is_empty() {
        "srv".to_string()
    } else {
        trimmed.chars().take(24).collect()
    }
}

pub fn make_mcp_tool_name(server_name: &str, remote_tool: &str) -> String {
    format!(
        "mcp__{}__{}",
        sanitize_segment(server_name),
        sanitize_segment(remote_tool)
    )
}

/// Extrai payload JSON-RPC tanto de respostas JSON diretas quanto de frames SSE (`data: {...}`).
pub fn parse_jsonrpc_body(raw: &str) -> Result<Value, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("Servidor MCP retornou corpo vazio.".to_string());
    }
    if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
        return Ok(v);
    }
    // Tenta extrair último evento `data:` de uma resposta Streamable HTTP SSE
    let mut last_json: Option<Value> = None;
    for line in trimmed.lines() {
        if let Some(rest) = line.trim().strip_prefix("data:") {
            let payload = rest.trim();
            if let Ok(v) = serde_json::from_str::<Value>(payload) {
                last_json = Some(v);
            }
        }
    }
    last_json.ok_or_else(|| "Resposta MCP não está em JSON-RPC válido.".to_string())
}

async fn rpc_http_request(
    client: &reqwest::Client,
    url: &str,
    method: &str,
    params: Value,
) -> Result<Value, String> {
    let req_body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params
    });
    let resp = tokio::time::timeout(
        MCP_TIMEOUT,
        client
            .post(url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .json(&req_body)
            .send(),
    )
    .await
    .map_err(|_| "Timeout ao comunicar com servidor MCP HTTP.".to_string())?
    .map_err(|e| format!("Falha HTTP ao conectar no servidor MCP: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!(
            "Servidor MCP HTTP retornou status {}.",
            resp.status()
        ));
    }
    let text = resp
        .text()
        .await
        .map_err(|e| format!("Falha ao ler resposta do servidor MCP: {e}"))?;
    let val = parse_jsonrpc_body(&text)?;
    if let Some(err) = val.get("error") {
        return Err(format!("Erro MCP JSON-RPC: {err}"));
    }
    Ok(val.get("result").cloned().unwrap_or(Value::Null))
}

async fn rpc_stdio_session(
    command: &str,
    args: &[String],
    method: &str,
    params: Value,
) -> Result<Value, String> {
    let mut cmd = Command::new(command);
    cmd.args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Falha ao iniciar servidor MCP stdio '{command}': {e}"))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "Falha ao abrir stdin do processo MCP.".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Falha ao abrir stdout do processo MCP.".to_string())?;
    let mut reader = BufReader::new(stdout).lines();

    let init_msg = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "Jarvis", "version": env!("CARGO_PKG_VERSION") }
        }
    });
    let notif_msg = json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    });
    let call_msg = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": method,
        "params": params
    });

    let write_all = async {
        stdin
            .write_all(format!("{init_msg}\n{notif_msg}\n{call_msg}\n").as_bytes())
            .await
            .map_err(|e| format!("Erro ao escrever no servidor MCP: {e}"))?;
        stdin.flush().await.ok();

        let mut target_result: Option<Value> = None;
        while let Ok(Some(line)) = reader.next_line().await {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
                if v.get("id").and_then(Value::as_i64) == Some(2) {
                    if let Some(err) = v.get("error") {
                        return Err(format!("Erro MCP JSON-RPC: {err}"));
                    }
                    target_result = Some(v.get("result").cloned().unwrap_or(Value::Null));
                    break;
                }
            }
        }
        target_result
            .ok_or_else(|| "Servidor MCP stdio encerrou sem responder ao método.".to_string())
    };

    let outcome = tokio::time::timeout(MCP_TIMEOUT, write_all)
        .await
        .map_err(|_| "Timeout ao aguardar resposta do servidor MCP stdio.".to_string())?;
    let _ = child.kill().await;
    outcome
}

/// Descobre todas as ferramentas de todos os servidores MCP ativos e atualiza o cache em memória.
pub async fn refresh_mcp_tools(
    client: &reqwest::Client,
    data_dir: &Path,
    cache: &McpRuntimeCache,
) -> Vec<McpServerStatus> {
    let servers = list_servers(data_dir);
    let mut all_tools = Vec::new();
    let mut statuses = Vec::new();
    let mut errors_map = std::collections::HashMap::new();

    for srv in servers {
        if !srv.enabled {
            statuses.push(McpServerStatus {
                server: srv,
                tool_count: 0,
                tools: Vec::new(),
                last_error: None,
            });
            continue;
        }
        let res = match srv.transport.as_str() {
            "http" => {
                if let Some(u) = srv.url.as_deref() {
                    rpc_http_request(client, u, "tools/list", json!({})).await
                } else {
                    Err("URL MCP ausente.".to_string())
                }
            }
            "stdio" => {
                if let Some(cmd) = srv.command.as_deref() {
                    rpc_stdio_session(cmd, &srv.args, "tools/list", json!({})).await
                } else {
                    Err("Comando MCP ausente.".to_string())
                }
            }
            _ => Err("Transporte MCP não suportado.".to_string()),
        };

        match res {
            Ok(result) => {
                let mut srv_tool_names = Vec::new();
                if let Some(arr) = result["tools"].as_array() {
                    for t in arr {
                        let Some(rname) = t["name"].as_str() else {
                            continue;
                        };
                        let desc = t["description"]
                            .as_str()
                            .unwrap_or("Ferramenta externa via MCP")
                            .to_string();
                        let schema = t
                            .get("inputSchema")
                            .cloned()
                            .filter(|v| v.is_object())
                            .unwrap_or_else(|| json!({ "type": "object", "properties": {} }));
                        let full = make_mcp_tool_name(&srv.name, rname);
                        srv_tool_names.push(rname.to_string());
                        all_tools.push(McpDiscoveredTool {
                            server_id: srv.id.clone(),
                            server_name: srv.name.clone(),
                            full_name: full,
                            remote_name: rname.to_string(),
                            description: format!("[MCP: {}] {}", srv.name, desc),
                            input_schema: schema,
                        });
                    }
                }
                statuses.push(McpServerStatus {
                    server: srv,
                    tool_count: srv_tool_names.len(),
                    tools: srv_tool_names,
                    last_error: None,
                });
            }
            Err(err) => {
                errors_map.insert(srv.id.clone(), err.clone());
                statuses.push(McpServerStatus {
                    server: srv,
                    tool_count: 0,
                    tools: Vec::new(),
                    last_error: Some(err),
                });
            }
        }
    }

    *cache.tools.write().await = all_tools;
    *cache.last_errors.write().await = errors_map;
    statuses
}

/// Executa uma ferramenta MCP descoberta (`mcp__<servidor>__<ferramenta>`).
pub async fn call_discovered_tool(
    client: &reqwest::Client,
    data_dir: &Path,
    cache: &McpRuntimeCache,
    full_name: &str,
    arguments: &Value,
) -> Result<String, String> {
    let tool = {
        let guard = cache.tools.read().await;
        guard.iter().find(|t| t.full_name == full_name).cloned()
    }
    .ok_or_else(|| format!("Ferramenta MCP '{full_name}' não encontrada no cache ativo."))?;

    let srv = list_servers(data_dir)
        .into_iter()
        .find(|s| s.id == tool.server_id && s.enabled)
        .ok_or_else(|| {
            format!(
                "Servidor MCP '{}' está desativado ou foi removido.",
                tool.server_name
            )
        })?;

    let params = json!({
        "name": tool.remote_name,
        "arguments": if arguments.is_object() { arguments.clone() } else { json!({}) }
    });

    let result = match srv.transport.as_str() {
        "http" => {
            let u = srv
                .url
                .as_deref()
                .ok_or_else(|| "URL MCP ausente.".to_string())?;
            rpc_http_request(client, u, "tools/call", params).await?
        }
        "stdio" => {
            let cmd = srv
                .command
                .as_deref()
                .ok_or_else(|| "Comando MCP ausente.".to_string())?;
            rpc_stdio_session(cmd, &srv.args, "tools/call", params).await?
        }
        _ => return Err("Transporte MCP inválido.".to_string()),
    };

    if let Some(content) = result["content"].as_array() {
        let parts: Vec<String> = content
            .iter()
            .filter_map(|c| c["text"].as_str().map(|s| s.to_string()))
            .collect();
        if !parts.is_empty() {
            return Ok(parts.join("\n"));
        }
    }
    Ok(serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_config_crud_and_jsonrpc_parser_work() {
        let dir = std::env::temp_dir().join(format!("jarvis-mcp-test-{}", Uuid::new_v4().simple()));
        fs::create_dir_all(&dir).unwrap();

        let srv = upsert_server(
            &dir,
            "GitHub Local",
            "http",
            None,
            Vec::new(),
            Some("https://example.com/mcp".to_string()),
            true,
        )
        .unwrap();
        assert_eq!(list_servers(&dir).len(), 1);
        assert_eq!(
            make_mcp_tool_name(&srv.name, "list-issues"),
            "mcp__github_local__list_issues"
        );

        let toggled = toggle_server(&dir, &srv.id, false).unwrap();
        assert!(!toggled.enabled);

        assert!(delete_server(&dir, &srv.id).unwrap());
        assert!(list_servers(&dir).is_empty());

        // Parser JSON-RPC direto e SSE
        let direct =
            parse_jsonrpc_body(r#"{"jsonrpc":"2.0","id":1,"result":{"tools":[]}}"#).unwrap();
        assert!(direct["result"]["tools"].is_array());
        let sse = parse_jsonrpc_body(
            "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"ok\":true}}\n\n",
        )
        .unwrap();
        assert_eq!(sse["result"]["ok"], Value::Bool(true));

        let _ = fs::remove_dir_all(&dir);
    }
}
