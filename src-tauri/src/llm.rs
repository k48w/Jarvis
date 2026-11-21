//! # Orquestrador de Modelos de IA e Subagentes — `llm.rs`
//!
//! Conecta o Jarvis aos provedores **OpenAI ChatGPT (OAuth 2.0 + PKCE Responses API)**
//! e **Google Gemini (API Key ou Google OAuth)** com suporte a:
//! - Streaming de tokens SSE em tempo real (`jarvis:stream-chunk`).
//! - Ciclo multi-turno de chamadas de ferramentas (`tools::execute_tool_with_policy`).
//! - Subagentes autônomos em background (`spawn_agent_task`) com deduplicação de missões.
//! - Transcrição de voz (STT via ElevenLabs Scribe v1 ou Gemini) e síntese TTS.

use crate::config::{
    get_api_key, save_config_file, AppConfig, GeminiAuthMode, KeySlot, Provider, SttEngine,
};
use crate::google_oauth;
use crate::oauth;
use crate::state::AppState;
use crate::storage::{self, MemoryItem, StoredMessage, ToolTrace};
use crate::tools;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Instant;
use tauri::{AppHandle, Emitter};
use tokio::sync::watch;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionTestResult {
    pub ok: bool,
    pub message: String,
    pub latency_ms: u64,
}

#[derive(Debug, Clone)]
pub struct ImageAttachment {
    pub mime_type: String,
    pub data_base64: String,
}

#[derive(Default)]
struct ResponseCallAcc {
    call_id: String,
    name: String,
    arguments: String,
}

fn build_system_prompt(
    cfg: &AppConfig,
    memories: &[MemoryItem],
    project_path: Option<&str>,
    agent_runs: &[crate::state::AgentRunSnapshot],
) -> String {
    let mut prompt = cfg.system_prompt.trim().to_string();
    let mode_note = match cfg.permission_mode {
        crate::config::PermissionMode::Auto => {
            "Modo de permissão atual: Permissão Total. Ferramentas de sistema, busca e ações locais no PC são executadas automaticamente."
        }
        crate::config::PermissionMode::Review => {
            "Modo de permissão atual: Revisão Automática. Leitura/pesquisa executam automaticamente; ações que alteram o PC pedem confirmação do usuário."
        }
        crate::config::PermissionMode::Safe => {
            "Modo de permissão atual: Somente Conversa. Ações locais no PC e leitura de hardware estão desativadas."
        }
    };
    prompt.push_str("\n\n");
    prompt.push_str(mode_note);
    prompt.push_str(
        "\n\nREGRA ESTRITA DO NAVEGADOR EMBUTIDO vs. BUSCA RÁPIDA:\n\
- Para perguntas gerais, notícias, fatos, clima, cotações ou pesquisas rápidas na web, NUNCA abra o navegador embutido (`browser_navigate`); responda diretamente ou use `web_search` e `fetch_webpage` silenciosamente em segundo plano.\n\
- SÓ chame `browser_navigate` (e demais ferramentas `browser_*`) quando o usuário pedir EXPLICITAMENTE para abrir/mostrar o navegador ou interagir visualmente com um site/app web (ex.: 'abre o navegador', 'olha meu WhatsApp Web', 'abre o YouTube aqui do lado', 'clica em tal botão no site'). Use `browser_close` quando ele pedir para fechar.\n\n\
Visão inteligente de tela e janelas/jogos do Windows: você NÃO precisa pedir print ao usuário. Quando ele pedir para ver um programa ou jogo específico (ex.: 'olha meu jogo', 'vê o Firefox', 'olha o Discord', 'vê o VS Code'), primeiro entenda QUAL janela ele quer ver: se você não souber o nome exato do jogo/app aberto, chame `list_open_windows` para descobrir a janela certa e então chame `see_screen(target_window: \"<nome>\")` — isso captura aquela janela/jogo diretamente mesmo quando o usuário está com o navegador ou outro app aberto por cima! Use `see_screen` sem `target_window` apenas quando o usuário perguntar de forma genérica o que está na tela atual. Se ele pedir para clicar/digitar no app, use `screen_click` / `screen_type` / `screen_hotkey`.\n\n\
REGRA ESTRITA DE SUBAGENTES (`spawn_agent` vs. `check_agents`):\n\
- Use `spawn_agent(role, name, task)` APENAS quando o usuário pedir para criar um agente ou iniciar uma NOVA missão longa em segundo plano.\n\
- NUNCA chame `spawn_agent` quando o usuário perguntar se o agente já terminou, qual o status do agente ou o que o agente descobriu! Para isso, chame `check_agents` (ou consulte a lista de agentes ativos/concluídos abaixo) e resuma o resultado para o usuário.\n\n\
Arquivos locais: você também pode listar, ler, buscar, criar e editar arquivos reais do computador usando `list_files`, `read_file`, `search_files`, `write_file` e `edit_file` (sempre leia o arquivo antes de editar um trecho; todas as edições aparecem com diff visual e botão de desfazer para o usuário).\n\n\
Área de transferência (Ctrl+C): quando o usuário disser 'o que eu copiei', 'analisa meu Ctrl+C', 'lê o print que copiei' ou 'área de transferência', chame `read_clipboard` para ler automaticamente o texto ou a imagem copiada.",
    );

    if !agent_runs.is_empty() {
        prompt.push_str("\n\nStatus atual dos Subagentes em Segundo Plano (NÃO crie outro agente duplicado se já estiver aqui):\n");
        for ag in agent_runs.iter().take(6) {
            let excerpt: String = ag.output.chars().take(280).collect();
            prompt.push_str(&format!(
                "- [{}] {} (id: {}, missão: \"{}\"){}",
                ag.status.to_uppercase(),
                ag.name,
                ag.id,
                ag.task,
                if excerpt.is_empty() {
                    String::new()
                } else {
                    format!(" → Resultado parcial/final: {}", excerpt.replace('\n', " "))
                }
            ));
            prompt.push('\n');
        }
    }

    if cfg.tts_engine == crate::config::TtsEngine::Elevenlabs
        && cfg.tts_reply_mode != crate::config::TtsReplyMode::Full
    {
        prompt.push_str(
            "\n\nEconomia de voz ativa: apenas a sua PRIMEIRA frase curta (máximo 120 caracteres) será sintetizada em voz alta. Portanto: 1) Para cumprimentos, confirmações e perguntas simples, responda em uma única frase curta de até 12 palavras. 2) Quando precisar dar uma resposta longa ou técnica, faça a primeira frase ser um resumo curto e completo (ex.: 'Aqui está o resumo, senhor; deixei os detalhes na tela.') e coloque o restante abaixo.",
        );
    }

    if let Some(pp) = project_path.map(str::trim).filter(|s| !s.is_empty()) {
        prompt.push_str("\n\nProjeto vinculado a esta conversa:\n- Pasta raiz: ");
        prompt.push_str(pp);
        prompt.push_str(
            "\nQuando o usuário se referir a arquivos ou código do projeto, use este caminho como base nas ferramentas list_files, read_file, search_files, write_file e edit_file.",
        );
        if let Ok(entries) = std::fs::read_dir(pp) {
            let mut items: Vec<String> = entries
                .flatten()
                .take(35)
                .map(|e| {
                    let name = e.file_name().to_string_lossy().into_owned();
                    if e.path().is_dir() {
                        format!("{name}/")
                    } else {
                        name
                    }
                })
                .collect();
            items.sort();
            if !items.is_empty() {
                prompt.push_str("\n- Itens na raiz do projeto: ");
                prompt.push_str(&items.join(", "));
            }
        }
    }

    if !memories.is_empty() {
        prompt.push_str("\n\nMemórias persistentes sobre o usuário:\n");
        for item in memories.iter().take(40) {
            prompt.push_str("- ");
            prompt.push_str(&item.content);
            prompt.push('\n');
        }
    }
    prompt
}

fn emit_delta(app: &AppHandle, conv_id: &str, text: &str) {
    if !text.is_empty() {
        let _ = app.emit(
            "jarvis:chat-event",
            json!({
                "type": "delta",
                "conversationId": conv_id,
                "text": text
            }),
        );
    }
}

fn check_cancelled(cancel_rx: &watch::Receiver<bool>) -> Result<(), String> {
    if *cancel_rx.borrow() {
        Err("CANCELLED".to_string())
    } else {
        Ok(())
    }
}

#[derive(Clone)]
enum GeminiAuth {
    ApiKey(String),
    OAuth {
        token: String,
        project_id: Option<String>,
    },
}

impl GeminiAuth {
    fn apply(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match self {
            GeminiAuth::ApiKey(key) => req.header("x-goog-api-key", key),
            GeminiAuth::OAuth { token, project_id } => {
                let req = req.bearer_auth(token);
                if let Some(pid) = project_id
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    req.header("x-goog-user-project", pid)
                } else {
                    req
                }
            }
        }
    }
}

async fn resolve_gemini_auth(state: &Arc<AppState>, cfg: &AppConfig) -> Result<GeminiAuth, String> {
    if cfg.gemini_auth_mode == GeminiAuthMode::Oauth {
        let (token, session_proj) =
            google_oauth::get_valid_google_token(&state.http, &state.data_dir).await?;
        let project_id = if !cfg.google_oauth_project_id.trim().is_empty() {
            Some(cfg.google_oauth_project_id.trim().to_string())
        } else {
            session_proj
        };
        Ok(GeminiAuth::OAuth { token, project_id })
    } else {
        let key = get_api_key(&state.data_dir, KeySlot::Gemini).ok_or_else(|| {
            "Chave da API Gemini não configurada. Abra Ajustes → IA para adicionar sua chave ou usar Conta Google (OAuth)."
                .to_string()
        })?;
        Ok(GeminiAuth::ApiKey(key))
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_gemini(
    app: &AppHandle,
    state: &Arc<AppState>,
    conv_id: &str,
    cfg: &AppConfig,
    auth: &GeminiAuth,
    system_prompt: &str,
    history: &[StoredMessage],
    latest_image: Option<&ImageAttachment>,
    cancel_rx: &mut watch::Receiver<bool>,
    tool_traces: &mut Vec<ToolTrace>,
    response_mode: &mut String,
) -> Result<String, String> {
    let valid_history: Vec<&StoredMessage> = history
        .iter()
        .filter(|m| !m.text.trim().is_empty())
        .collect();
    let last_user_idx = valid_history.iter().rposition(|m| m.role == "user");

    let mut contents: Vec<Value> = valid_history
        .iter()
        .enumerate()
        .map(|(idx, m)| {
            let role = if m.role == "user" { "user" } else { "model" };
            let mut parts = vec![json!({ "text": m.text })];
            if Some(idx) == last_user_idx {
                if let Some(img) = latest_image {
                    parts.push(json!({
                        "inline_data": {
                            "mime_type": img.mime_type,
                            "data": img.data_base64
                        }
                    }));
                }
            }
            json!({
                "role": role,
                "parts": parts
            })
        })
        .collect();

    let mcp_tools = state.mcp_cache.tools.read().await.clone();
    let decls = tools::gemini_tool_declarations_with_mcp(
        cfg.permission_mode,
        cfg.web_search_enabled,
        &mcp_tools,
    );
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:streamGenerateContent?alt=sse",
        cfg.gemini_model.trim()
    );

    let mut full_text = String::new();

    for _round in 0..14 {
        check_cancelled(cancel_rx)?;

        let mut payload = json!({
            "system_instruction": { "parts": [{ "text": system_prompt }] },
            "contents": contents
        });
        if !decls.is_empty() {
            payload["tools"] = Value::Array(decls.clone());
        }

        let req = auth.apply(state.http.post(&url)).json(&payload);
        let resp = tokio::select! {
            _ = cancel_rx.changed() => return Err("CANCELLED".to_string()),
            r = req.send() => {
                r.map_err(|e| format!("Erro de rede ao conectar no Gemini: {e}"))?
            }
        };

        if !resp.status().is_success() {
            let status = resp.status();
            let err_body = resp.text().await.unwrap_or_default();
            return Err(format!("Gemini retornou erro ({status}): {err_body}"));
        }

        let mut stream = resp.bytes_stream();
        let mut buf = String::new();
        let mut round_text = String::new();
        let mut model_parts: Vec<Value> = Vec::new();
        let mut function_calls: Vec<(String, Value)> = Vec::new();

        loop {
            let next_chunk = tokio::select! {
                _ = cancel_rx.changed() => return Err("CANCELLED".to_string()),
                c = stream.next() => c
            };
            let Some(chunk_res) = next_chunk else { break };
            let chunk = chunk_res.map_err(|e| format!("Erro no streaming do Gemini: {e}"))?;
            buf.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(nl) = buf.find('\n') {
                let line = buf[..nl].trim_end_matches('\r').to_string();
                buf.drain(..=nl);
                let Some(data) = line.strip_prefix("data:") else {
                    continue;
                };
                let data = data.trim();
                if data.is_empty() || data == "[DONE]" {
                    continue;
                }
                let Ok(parsed) = serde_json::from_str::<Value>(data) else {
                    continue;
                };
                let Some(parts) = parsed["candidates"][0]["content"]["parts"].as_array() else {
                    continue;
                };
                for part in parts {
                    if let Some(fc) = part.get("functionCall") {
                        let name = fc["name"].as_str().unwrap_or("").to_string();
                        let args = fc.get("args").cloned().unwrap_or_else(|| json!({}));
                        model_parts.push(part.clone());
                        function_calls.push((name, args));
                    } else if let Some(text) = part["text"].as_str() {
                        if part["thought"].as_bool() == Some(true) {
                            continue;
                        }
                        if !full_text.is_empty()
                            && round_text.is_empty()
                            && !full_text.ends_with(['\n', ' '])
                        {
                            emit_delta(app, conv_id, "\n\n");
                            full_text.push_str("\n\n");
                        }
                        round_text.push_str(text);
                        full_text.push_str(text);
                        emit_delta(app, conv_id, text);
                    }
                }
            }
        }

        if function_calls.is_empty() {
            break;
        }

        if !round_text.is_empty() {
            model_parts.insert(0, json!({ "text": round_text }));
        }
        contents.push(json!({ "role": "model", "parts": model_parts }));

        let mut response_parts = Vec::new();
        for (name, args) in function_calls {
            check_cancelled(cancel_rx)?;
            let call_id = Uuid::new_v4().to_string();
            let outcome =
                tools::execute_tool_with_policy(app, state, conv_id, &call_id, &name, &args).await;
            if outcome.triggered_alert {
                *response_mode = "alert".to_string();
            }
            let title = tools::find_spec(&name)
                .map(|s| s.title.to_string())
                .unwrap_or_else(|| name.clone());
            tool_traces.push(ToolTrace {
                id: call_id,
                name: name.clone(),
                title,
                status: outcome.status,
                summary: outcome.summary,
            });
            response_parts.push(json!({
                "functionResponse": {
                    "name": name,
                    "response": { "result": outcome.output }
                }
            }));
            if let Some(img) = outcome.attached_image {
                response_parts.push(json!({
                    "inline_data": {
                        "mime_type": img.mime_type,
                        "data": img.data_base64
                    }
                }));
            }
        }
        contents.push(json!({ "role": "user", "parts": response_parts }));
    }

    Ok(full_text)
}

#[allow(clippy::too_many_arguments)]
async fn run_openai_responses(
    app: &AppHandle,
    state: &Arc<AppState>,
    conv_id: &str,
    cfg: &AppConfig,
    system_prompt: &str,
    history: &[StoredMessage],
    latest_image: Option<&ImageAttachment>,
    cancel_rx: &mut watch::Receiver<bool>,
    tool_traces: &mut Vec<ToolTrace>,
    response_mode: &mut String,
) -> Result<String, String> {
    let token = oauth::get_valid_access_token(&state.http, &state.data_dir).await?;
    let valid_history: Vec<&StoredMessage> = history
        .iter()
        .filter(|m| !m.text.trim().is_empty())
        .collect();
    let last_user_idx = valid_history.iter().rposition(|m| m.role == "user");

    let mut input: Vec<Value> = valid_history
        .iter()
        .enumerate()
        .map(|(idx, m)| {
            let role = if m.role == "user" { "user" } else { "assistant" };
            if Some(idx) == last_user_idx {
                if let Some(img) = latest_image {
                    return json!({
                        "role": role,
                        "content": [
                            { "type": "input_text", "text": m.text },
                            {
                                "type": "input_image",
                                "image_url": format!("data:{};base64,{}", img.mime_type, img.data_base64)
                            }
                        ]
                    });
                }
            }
            json!({
                "role": role,
                "content": m.text
            })
        })
        .collect();

    let mcp_tools = state.mcp_cache.tools.read().await.clone();
    let decls = tools::responses_tool_declarations_with_mcp(
        cfg.permission_mode,
        cfg.web_search_enabled,
        &mcp_tools,
    );
    let url = "https://api.openai.com/v1/responses";
    let mut full_text = String::new();
    let mut active_model = if cfg.chatgpt_model.trim().is_empty() {
        "gpt-6.2-sol".to_string()
    } else {
        cfg.chatgpt_model.trim().to_string()
    };
    let mut persisted_model = cfg.chatgpt_model.trim().to_string();
    let mut rejected_models: Vec<String> = Vec::new();
    let mut cached_catalog: Option<Vec<String>> = None;

    for _round in 0..14 {
        check_cancelled(cancel_rx)?;

        let mut resp_opt = None;
        for _attempt in 0..3 {
            let mut payload = json!({
                "model": active_model,
                "instructions": system_prompt,
                "input": input,
                "stream": true,
                "store": false
            });
            if !decls.is_empty() {
                payload["tools"] = Value::Array(decls.clone());
            }

            let resp = tokio::select! {
                _ = cancel_rx.changed() => return Err("CANCELLED".to_string()),
                r = state.http.post(url).bearer_auth(&token).json(&payload).send() => {
                    r.map_err(|e| format!("Erro de rede ao conectar ao ChatGPT: {e}"))?
                }
            };

            if resp.status().is_success() {
                resp_opt = Some(resp);
                break;
            }

            let status = resp.status();
            let err_body = resp.text().await.unwrap_or_default();
            let lower_err = err_body.to_ascii_lowercase();
            let unsupported_model = status.as_u16() == 400
                && (lower_err.contains("model_not_found")
                    || lower_err.contains("is not supported"));
            if unsupported_model {
                rejected_models.push(active_model.clone());
                if cached_catalog.is_none() {
                    cached_catalog = Some(
                        oauth::list_chatgpt_models_with_token(&state.http, &token)
                            .await
                            .unwrap_or_default(),
                    );
                }
                let catalog = cached_catalog.as_deref().unwrap_or(&[]);
                let next_model = oauth::pick_best_chatgpt_model(catalog, &rejected_models);
                if next_model != active_model
                    && !rejected_models
                        .iter()
                        .any(|r| r.eq_ignore_ascii_case(&next_model))
                {
                    active_model = next_model;
                    continue;
                }
            }
            return Err(format!(
                "ChatGPT (Responses API) retornou erro ({status}): {err_body}"
            ));
        }

        let resp = resp_opt.ok_or_else(|| {
            "Nenhum modelo compatível do ChatGPT disponível para esta conta.".to_string()
        })?;

        if active_model != persisted_model {
            persisted_model = active_model.clone();
            let mut cfg_guard = state.config.write().await;
            cfg_guard.chatgpt_model = active_model.clone();
            let _ = save_config_file(&state.data_dir, &cfg_guard);
            drop(cfg_guard);
            let _ = app.emit(
                "jarvis:chat-event",
                json!({
                    "type": "model_updated",
                    "conversationId": conv_id,
                    "provider": "chatgpt",
                    "model": active_model
                }),
            );
        }

        let mut stream = resp.bytes_stream();
        let mut buf = String::new();
        let mut round_text = String::new();
        let mut calls_by_key: BTreeMap<String, ResponseCallAcc> = BTreeMap::new();
        let mut call_order: Vec<String> = Vec::new();

        loop {
            let next_chunk = tokio::select! {
                _ = cancel_rx.changed() => return Err("CANCELLED".to_string()),
                c = stream.next() => c
            };
            let Some(chunk_res) = next_chunk else { break };
            let chunk = chunk_res.map_err(|e| format!("Erro no streaming do ChatGPT: {e}"))?;
            buf.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(nl) = buf.find('\n') {
                let line = buf[..nl].trim_end_matches('\r').to_string();
                buf.drain(..=nl);
                let Some(data) = line.strip_prefix("data:") else {
                    continue;
                };
                let data = data.trim();
                if data.is_empty() || data == "[DONE]" {
                    continue;
                }
                let Ok(parsed) = serde_json::from_str::<Value>(data) else {
                    continue;
                };
                let ev_type = parsed["type"].as_str().unwrap_or("");
                match ev_type {
                    "response.output_text.delta" => {
                        if let Some(text) = parsed["delta"].as_str() {
                            if !full_text.is_empty()
                                && round_text.is_empty()
                                && !full_text.ends_with(['\n', ' '])
                            {
                                emit_delta(app, conv_id, "\n\n");
                                full_text.push_str("\n\n");
                            }
                            round_text.push_str(text);
                            full_text.push_str(text);
                            emit_delta(app, conv_id, text);
                        }
                    }
                    "response.output_item.added" => {
                        let item = &parsed["item"];
                        if item["type"].as_str() == Some("function_call") {
                            let key =
                                item["id"].as_str().map(str::to_string).unwrap_or_else(|| {
                                    format!("idx_{}", parsed["output_index"].as_u64().unwrap_or(0))
                                });
                            if !calls_by_key.contains_key(&key) {
                                call_order.push(key.clone());
                            }
                            let entry = calls_by_key.entry(key).or_default();
                            if let Some(cid) = item["call_id"].as_str() {
                                entry.call_id = cid.to_string();
                            }
                            if let Some(name) = item["name"].as_str() {
                                entry.name = name.to_string();
                            }
                            if let Some(args) = item["arguments"].as_str() {
                                if !args.is_empty() {
                                    entry.arguments = args.to_string();
                                }
                            }
                        }
                    }
                    "response.function_call_arguments.delta" => {
                        let key = parsed["item_id"]
                            .as_str()
                            .map(str::to_string)
                            .unwrap_or_else(|| {
                                format!("idx_{}", parsed["output_index"].as_u64().unwrap_or(0))
                            });
                        if !calls_by_key.contains_key(&key) {
                            call_order.push(key.clone());
                        }
                        let entry = calls_by_key.entry(key).or_default();
                        if let Some(delta) = parsed["delta"].as_str() {
                            entry.arguments.push_str(delta);
                        }
                    }
                    "response.function_call_arguments.done" => {
                        let key = parsed["item_id"]
                            .as_str()
                            .map(str::to_string)
                            .unwrap_or_else(|| {
                                format!("idx_{}", parsed["output_index"].as_u64().unwrap_or(0))
                            });
                        if let Some(entry) = calls_by_key.get_mut(&key) {
                            if let Some(args) = parsed["arguments"].as_str() {
                                if !args.is_empty() {
                                    entry.arguments = args.to_string();
                                }
                            }
                        }
                    }
                    "response.output_item.done" => {
                        let item = &parsed["item"];
                        if item["type"].as_str() == Some("function_call") {
                            let key =
                                item["id"].as_str().map(str::to_string).unwrap_or_else(|| {
                                    format!("idx_{}", parsed["output_index"].as_u64().unwrap_or(0))
                                });
                            if !calls_by_key.contains_key(&key) {
                                call_order.push(key.clone());
                            }
                            let entry = calls_by_key.entry(key).or_default();
                            if let Some(cid) = item["call_id"].as_str() {
                                entry.call_id = cid.to_string();
                            }
                            if let Some(name) = item["name"].as_str() {
                                entry.name = name.to_string();
                            }
                            if let Some(args) = item["arguments"].as_str() {
                                if !args.is_empty() {
                                    entry.arguments = args.to_string();
                                }
                            }
                        }
                    }
                    "response.failed" | "error" => {
                        let msg = parsed["error"]["message"]
                            .as_str()
                            .or_else(|| parsed["response"]["error"]["message"].as_str())
                            .unwrap_or("Falha na resposta do ChatGPT.");
                        return Err(format!("ChatGPT retornou erro: {msg}"));
                    }
                    _ => {}
                }
            }
        }

        if call_order.is_empty() {
            break;
        }

        if !round_text.is_empty() {
            input.push(json!({ "role": "assistant", "content": round_text }));
        }

        let mut round_images: Vec<ImageAttachment> = Vec::new();
        for key in call_order {
            let Some(call) = calls_by_key.remove(&key) else {
                continue;
            };
            if call.name.is_empty() {
                continue;
            }
            check_cancelled(cancel_rx)?;
            let call_id = if call.call_id.is_empty() {
                Uuid::new_v4().to_string()
            } else {
                call.call_id
            };
            let args_str = if call.arguments.is_empty() {
                "{}"
            } else {
                &call.arguments
            };
            input.push(json!({
                "type": "function_call",
                "call_id": call_id,
                "name": call.name,
                "arguments": args_str
            }));

            let args: Value = serde_json::from_str(args_str).unwrap_or_else(|_| json!({}));
            let outcome =
                tools::execute_tool_with_policy(app, state, conv_id, &call_id, &call.name, &args)
                    .await;
            if outcome.triggered_alert {
                *response_mode = "alert".to_string();
            }
            if let Some(img) = outcome.attached_image {
                round_images.push(img);
            }
            let title = tools::find_spec(&call.name)
                .map(|s| s.title.to_string())
                .unwrap_or_else(|| call.name.clone());
            tool_traces.push(ToolTrace {
                id: call_id.clone(),
                name: call.name,
                title,
                status: outcome.status,
                summary: outcome.summary,
            });
            input.push(json!({
                "type": "function_call_output",
                "call_id": call_id,
                "output": outcome.output
            }));
        }

        for img in round_images {
            input.push(json!({
                "role": "user",
                "content": [
                    {
                        "type": "input_text",
                        "text": "Imagem visual capturada agora pela ferramenta de tela/navegador. Analise-a diretamente para responder ou agir:"
                    },
                    {
                        "type": "input_image",
                        "image_url": format!("data:{};base64,{}", img.mime_type, img.data_base64)
                    }
                ]
            }));
        }
    }

    Ok(full_text)
}

/// Normaliza a URL base local (Ollama `http://127.0.0.1:11434/v1` ou LM Studio `http://127.0.0.1:1234/v1`).
pub fn normalize_local_base_url(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim().trim_end_matches('/');
    let base = if trimmed.is_empty() {
        "http://127.0.0.1:11434/v1"
    } else {
        trimmed
    };
    let parsed = url::Url::parse(base).map_err(|e| format!("URL local inválida: {e}"))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("A URL do servidor local deve começar com http:// ou https://".to_string());
    }
    Ok(base.to_string())
}

/// Descobre modelos instalados no servidor local (Ollama ou LM Studio).
pub async fn list_local_models(
    http: &reqwest::Client,
    raw_base_url: &str,
) -> Result<Vec<String>, String> {
    let base = normalize_local_base_url(raw_base_url)?;
    let models_url = format!("{base}/models");
    if let Ok(resp) = http
        .get(&models_url)
        .timeout(std::time::Duration::from_secs(6))
        .send()
        .await
    {
        if resp.status().is_success() {
            if let Ok(val) = resp.json::<Value>().await {
                let mut ids: Vec<String> = val["data"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|m| m["id"].as_str().map(|s| s.to_string()))
                    .collect();
                if !ids.is_empty() {
                    ids.sort();
                    ids.dedup();
                    return Ok(ids);
                }
            }
        }
    }

    // Fallback nativo do Ollama (`/api/tags`) caso `/v1/models` não esteja disponível
    if let Ok(parsed) = url::Url::parse(&base) {
        if let Some(host) = parsed.host_str() {
            let port_part = parsed.port().map(|p| format!(":{p}")).unwrap_or_default();
            let tags_url = format!("{}://{}{}/api/tags", parsed.scheme(), host, port_part);
            if let Ok(resp) = http
                .get(&tags_url)
                .timeout(std::time::Duration::from_secs(6))
                .send()
                .await
            {
                if resp.status().is_success() {
                    if let Ok(val) = resp.json::<Value>().await {
                        let mut ids: Vec<String> = val["models"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|m| m["name"].as_str().map(|s| s.to_string()))
                            .collect();
                        if !ids.is_empty() {
                            ids.sort();
                            ids.dedup();
                            return Ok(ids);
                        }
                    }
                }
            }
        }
    }

    Err(format!(
        "Nenhum servidor Ollama/LM Studio respondeu em '{base}'. Verifique se ele está rodando."
    ))
}

#[allow(clippy::too_many_arguments)]
async fn run_local_openai_compat(
    app: &AppHandle,
    state: &Arc<AppState>,
    conv_id: &str,
    cfg: &AppConfig,
    system_prompt: &str,
    history: &[StoredMessage],
    latest_image: Option<&ImageAttachment>,
    cancel_rx: &mut watch::Receiver<bool>,
    tool_traces: &mut Vec<ToolTrace>,
    response_mode: &mut String,
) -> Result<String, String> {
    let base = normalize_local_base_url(&cfg.local_base_url)?;
    let url = format!("{base}/chat/completions");
    let model = if cfg.local_model.trim().is_empty() {
        "qwen2.5:7b".to_string()
    } else {
        cfg.local_model.trim().to_string()
    };

    let valid_history: Vec<&StoredMessage> = history
        .iter()
        .filter(|m| !m.text.trim().is_empty())
        .collect();
    let last_user_idx = valid_history.iter().rposition(|m| m.role == "user");

    let mut messages: Vec<Value> = vec![json!({
        "role": "system",
        "content": system_prompt
    })];
    for (idx, m) in valid_history.iter().enumerate() {
        let role = if m.role == "assistant" {
            "assistant"
        } else {
            "user"
        };
        if Some(idx) == last_user_idx {
            if let Some(img) = latest_image {
                let data_url = format!("data:{};base64,{}", img.mime_type, img.data_base64);
                messages.push(json!({
                    "role": "user",
                    "content": [
                        { "type": "text", "text": m.text },
                        { "type": "image_url", "image_url": { "url": data_url } }
                    ]
                }));
                continue;
            }
        }
        messages.push(json!({ "role": role, "content": m.text }));
    }

    let mcp_tools = state.mcp_cache.tools.read().await.clone();
    let decls = tools::openai_chat_tool_declarations(
        cfg.permission_mode,
        cfg.web_search_enabled,
        &mcp_tools,
    );
    let mut use_tools = !decls.is_empty();
    let mut full_text = String::new();

    for _round in 0..10 {
        check_cancelled(cancel_rx)?;

        let mut payload = json!({
            "model": model,
            "messages": messages,
            "stream": true
        });
        if use_tools {
            payload["tools"] = json!(decls);
        }

        let resp = state
            .http
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(|e| {
                format!("Não foi possível conectar ao servidor local ({url}): {e}. Confirme se o Ollama ou LM Studio está aberto.")
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err_text = resp.text().await.unwrap_or_default();
            // Alguns modelos locais menores não suportam tool-calling: tenta novamente sem `tools`
            if use_tools
                && (status.as_u16() == 400 || err_text.to_ascii_lowercase().contains("tool"))
            {
                use_tools = false;
                continue;
            }
            return Err(format!(
                "Servidor local ({model}) retornou HTTP {status}: {err_text}"
            ));
        }

        let mut stream = resp.bytes_stream();
        let mut sse_buf = String::new();
        let mut round_text = String::new();
        // index -> (call_id, name, arguments_json_str)
        let mut pending_calls: BTreeMap<usize, (String, String, String)> = BTreeMap::new();

        while let Some(chunk_res) = stream.next().await {
            check_cancelled(cancel_rx)?;
            let chunk = chunk_res.map_err(|e| format!("Erro no stream do modelo local: {e}"))?;
            sse_buf.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(nl) = sse_buf.find('\n') {
                let line = sse_buf[..nl].trim().to_string();
                sse_buf.drain(..=nl);
                let Some(data) = line.strip_prefix("data:") else {
                    continue;
                };
                let data = data.trim();
                if data.is_empty() || data == "[DONE]" {
                    continue;
                }
                let Ok(ev) = serde_json::from_str::<Value>(data) else {
                    continue;
                };
                let delta = &ev["choices"][0]["delta"];
                if let Some(txt) = delta["content"].as_str() {
                    if !txt.is_empty() {
                        round_text.push_str(txt);
                        full_text.push_str(txt);
                        let _ = app.emit(
                            "jarvis:chat-event",
                            json!({
                                "type": "delta",
                                "conversationId": conv_id,
                                "text": txt
                            }),
                        );
                    }
                }
                if let Some(tcs) = delta["tool_calls"].as_array() {
                    for tc in tcs {
                        let idx = tc["index"].as_u64().unwrap_or(0) as usize;
                        let entry = pending_calls.entry(idx).or_insert_with(|| {
                            (
                                format!("call_{}", &Uuid::new_v4().simple().to_string()[..8]),
                                String::new(),
                                String::new(),
                            )
                        });
                        if let Some(id) = tc["id"].as_str().filter(|s| !s.is_empty()) {
                            entry.0 = id.to_string();
                        }
                        if let Some(n) = tc["function"]["name"].as_str() {
                            entry.1.push_str(n);
                        }
                        if let Some(a) = tc["function"]["arguments"].as_str() {
                            entry.2.push_str(a);
                        }
                    }
                }
            }
        }

        if pending_calls.is_empty() {
            break;
        }

        let assistant_calls: Vec<Value> = pending_calls
            .values()
            .map(|(cid, name, arg_s)| {
                json!({
                    "id": cid,
                    "type": "function",
                    "function": { "name": name, "arguments": arg_s }
                })
            })
            .collect();
        messages.push(json!({
            "role": "assistant",
            "content": if round_text.is_empty() { Value::Null } else { json!(round_text) },
            "tool_calls": assistant_calls
        }));

        for (cid, name, arg_s) in pending_calls.into_values() {
            check_cancelled(cancel_rx)?;
            let parsed_args: Value = serde_json::from_str(&arg_s).unwrap_or_else(|_| json!({}));
            let outcome =
                tools::execute_tool_with_policy(app, state, conv_id, &cid, &name, &parsed_args)
                    .await;
            if outcome.triggered_alert {
                *response_mode = "alert".to_string();
            }
            tool_traces.push(ToolTrace {
                id: cid.clone(),
                name: name.clone(),
                title: tools::find_spec(&name)
                    .map(|s| s.title.to_string())
                    .unwrap_or_else(|| name.clone()),
                status: outcome.status.clone(),
                summary: outcome.summary.clone(),
            });
            messages.push(json!({
                "role": "tool",
                "tool_call_id": cid,
                "content": outcome.output
            }));
        }
    }

    Ok(full_text)
}

pub async fn run_chat_turn(
    app: &AppHandle,
    state: &Arc<AppState>,
    conv_id: &str,
    history: &[StoredMessage],
    project_path: Option<String>,
    latest_image: Option<ImageAttachment>,
    mut cancel_rx: watch::Receiver<bool>,
) -> Result<(String, String, Vec<ToolTrace>), String> {
    let cfg = state.config.read().await.clone();
    let memories = storage::list_memories(&state.data_dir).unwrap_or_default();
    let agent_runs = state.agent_runs.lock().await.clone();
    let system_prompt = build_system_prompt(&cfg, &memories, project_path.as_deref(), &agent_runs);

    let window_start = history.len().saturating_sub(24);
    let recent = &history[window_start..];

    let mut tool_traces = Vec::new();
    let mut response_mode = "responding".to_string();

    let text = match cfg.provider {
        Provider::Gemini => {
            let auth = resolve_gemini_auth(state, &cfg).await?;
            run_gemini(
                app,
                state,
                conv_id,
                &cfg,
                &auth,
                &system_prompt,
                recent,
                latest_image.as_ref(),
                &mut cancel_rx,
                &mut tool_traces,
                &mut response_mode,
            )
            .await?
        }
        Provider::Chatgpt => {
            run_openai_responses(
                app,
                state,
                conv_id,
                &cfg,
                &system_prompt,
                recent,
                latest_image.as_ref(),
                &mut cancel_rx,
                &mut tool_traces,
                &mut response_mode,
            )
            .await?
        }
        Provider::Local => {
            run_local_openai_compat(
                app,
                state,
                conv_id,
                &cfg,
                &system_prompt,
                recent,
                latest_image.as_ref(),
                &mut cancel_rx,
                &mut tool_traces,
                &mut response_mode,
            )
            .await?
        }
    };

    let final_text = if text.trim().is_empty() && !tool_traces.is_empty() {
        "Concluído.".to_string()
    } else {
        text
    };

    Ok((final_text, response_mode, tool_traces))
}

pub async fn run_agent_turn(
    app: &AppHandle,
    state: &Arc<AppState>,
    agent_id: &str,
    role_prompt: &str,
    task: &str,
    mut cancel_rx: watch::Receiver<bool>,
) -> Result<(String, String, Vec<ToolTrace>), String> {
    let cfg = state.config.read().await.clone();
    let memories = storage::list_memories(&state.data_dir).unwrap_or_default();

    let mut system_prompt = format!(
        "Você é um subagente especializado do ecossistema J.A.R.V.I.S. rodando silenciosamente em segundo plano no Windows.\n\
         Função designada: {role_prompt}\n\
         Comece SEMPRE com um parágrafo inicial curto (1 a 2 frases diretas resumindo a conclusão principal) e depois coloque os detalhes estruturados em Markdown em português do Brasil.\n\
         REGRA ESTRITA: NUNCA use `browser_navigate`, `browser_click`, `browser_type` ou `spawn_agent`. Para pesquisar na web em segundo plano, use EXCLUSIVAMENTE `web_search` e `fetch_webpage`."
    );
    if !memories.is_empty() {
        system_prompt.push_str("\n\nContexto do usuário:\n");
        for item in memories.iter().take(20) {
            system_prompt.push_str("- ");
            system_prompt.push_str(&item.content);
            system_prompt.push('\n');
        }
    }

    let single_msg = [StoredMessage {
        id: Uuid::new_v4().to_string(),
        role: "user".to_string(),
        text: task.to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        mode: None,
        tool_events: Vec::new(),
    }];

    let mut tool_traces = Vec::new();
    let mut response_mode = "responding".to_string();

    let text = match cfg.provider {
        Provider::Gemini => {
            let auth = resolve_gemini_auth(state, &cfg).await?;
            run_gemini(
                app,
                state,
                agent_id,
                &cfg,
                &auth,
                &system_prompt,
                &single_msg,
                None,
                &mut cancel_rx,
                &mut tool_traces,
                &mut response_mode,
            )
            .await?
        }
        Provider::Chatgpt => {
            run_openai_responses(
                app,
                state,
                agent_id,
                &cfg,
                &system_prompt,
                &single_msg,
                None,
                &mut cancel_rx,
                &mut tool_traces,
                &mut response_mode,
            )
            .await?
        }
        Provider::Local => {
            run_local_openai_compat(
                app,
                state,
                agent_id,
                &cfg,
                &system_prompt,
                &single_msg,
                None,
                &mut cancel_rx,
                &mut tool_traces,
                &mut response_mode,
            )
            .await?
        }
    };

    let final_text = if text.trim().is_empty() && !tool_traces.is_empty() {
        "Missão concluída com sucesso.".to_string()
    } else {
        text
    };

    Ok((final_text, response_mode, tool_traces))
}

fn build_concise_agent_speech(agent_name: &str, raw_output: &str) -> (String, String) {
    let mut clean_lines: Vec<String> = Vec::new();
    let mut in_code_block = false;
    for line in raw_output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            continue;
        }
        if in_code_block || trimmed.is_empty() {
            continue;
        }
        let stripped = trimmed
            .trim_start_matches('#')
            .trim_start_matches('-')
            .trim_start_matches('*')
            .trim()
            .replace("**", "")
            .replace('`', "");
        if !stripped.is_empty() {
            clean_lines.push(stripped);
        }
    }

    let joined = if clean_lines.is_empty() {
        raw_output.trim().replace("**", "").replace('`', "")
    } else {
        clean_lines.join(" ")
    };

    let sentences = split_sentences(&joined);

    // Spoken: 1-2 COMPLETE sentences, never cut mid-sentence.
    let mut spoken_body = String::new();
    for sent in &sentences {
        let sent = shorten_sentence(sent, 190);
        if spoken_body.is_empty() {
            spoken_body = sent;
        } else if spoken_body.chars().count() + sent.chars().count() < 230 {
            spoken_body.push(' ');
            spoken_body.push_str(&sent);
        } else {
            break;
        }
        if spoken_body.chars().count() >= 110 {
            break;
        }
    }
    let spoken_summary = if spoken_body.is_empty() {
        format!("Senhor, o agente {agent_name} concluiu a tarefa.")
    } else {
        format!("Senhor, o agente {agent_name} terminou. {spoken_body}")
    };

    // Card: up to ~4 complete sentences (~520 chars), no trailing ellipsis.
    let mut card = String::new();
    for sent in sentences.iter().take(6) {
        if !card.is_empty() && card.chars().count() + sent.chars().count() > 520 {
            break;
        }
        if !card.is_empty() {
            card.push(' ');
        }
        card.push_str(sent);
        if card.chars().count() >= 380 {
            break;
        }
    }
    let card_summary = if card.trim().is_empty() {
        "Missão concluída com sucesso.".to_string()
    } else {
        card
    };

    (spoken_summary, card_summary)
}

fn split_sentences(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut cur = String::new();
    for (i, &c) in chars.iter().enumerate() {
        cur.push(c);
        let next_is_break = chars.get(i + 1).map(|n| n.is_whitespace()).unwrap_or(true);
        if matches!(c, '.' | '!' | '?') && next_is_break {
            let t = cur.trim().to_string();
            // avoid splitting on short abbreviations like "Dr." / "n."
            if t.chars().count() >= 25 {
                out.push(t);
                cur.clear();
            }
        }
    }
    let rest = cur.trim();
    if !rest.is_empty() {
        out.push(if rest.ends_with(['.', '!', '?', ':']) {
            rest.to_string()
        } else {
            format!("{rest}.")
        });
    }
    out
}

fn shorten_sentence(sent: &str, max: usize) -> String {
    if sent.chars().count() <= max {
        return sent.trim_end_matches(':').trim().to_string();
    }
    let head: String = sent.chars().take(max).collect();
    let cut = head
        .rfind([',', ';', '—', '–'])
        .filter(|&p| p > max / 2)
        .unwrap_or_else(|| head.rfind(' ').unwrap_or(head.len()));
    let mut s = head[..cut]
        .trim_end_matches(|c: char| !c.is_alphanumeric())
        .to_string();
    s.push('.');
    s
}
fn tasks_are_similar_or_status_check(existing_task: &str, new_task: &str) -> bool {
    let new_lower = new_task.to_ascii_lowercase();
    let status_hints = [
        "terminou",
        "concluiu",
        "status",
        "andamento",
        "pronto",
        "acabou",
        "resultado do agente",
        "verificar agente",
        "checar agente",
    ];
    if status_hints.iter().any(|h| new_lower.contains(h)) {
        return true;
    }

    let words = |s: &str| -> std::collections::HashSet<String> {
        s.to_ascii_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.chars().count() >= 4)
            .map(str::to_string)
            .collect()
    };
    let a = words(existing_task);
    let b = words(new_task);
    if a.is_empty() || b.is_empty() {
        return existing_task.eq_ignore_ascii_case(new_task);
    }
    let inter = a.intersection(&b).count();
    let min_len = a.len().min(b.len());
    inter * 2 >= min_len
}

pub fn spawn_background_agent(
    app: &AppHandle,
    state: &Arc<AppState>,
    role: &str,
    name: Option<&str>,
    task: &str,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, String>> + Send>> {
    let app_owned = app.clone();
    let state_owned = Arc::clone(state);
    let role_owned = role.to_string();
    let name_owned = name.map(str::to_string);
    let task_owned = task.to_string();

    Box::pin(async move {
        let app = &app_owned;
        let state = &state_owned;
        let role = role_owned.as_str();
        let name = name_owned.as_deref();
        let task = task_owned.as_str();

        let clean_task = task.trim().to_string();
        if clean_task.is_empty() {
            return Err("Descreva a missão do agente antes de iniciar.".to_string());
        }

        // Prevent duplicate/status-check agents while allowing up to 3 parallel distinct missions
        {
            let runs = state.agent_runs.lock().await;
            let running: Vec<&crate::state::AgentRunSnapshot> =
                runs.iter().filter(|r| r.status == "running").collect();
            if let Some(existing) = running
                .iter()
                .find(|r| tasks_are_similar_or_status_check(&r.task, &clean_task))
            {
                return Ok(format!(
                    "ALREADY_RUNNING:{}:{}:{}",
                    existing.id, existing.name, existing.task
                ));
            }
            if running.len() >= 3 {
                let first = running[0];
                return Ok(format!(
                    "ALREADY_RUNNING:{}:{}:{} (limite de 3 agentes simultâneos atingido)",
                    first.id, first.name, first.task
                ));
            }
        }

        let normalized_role = match role.trim().to_ascii_lowercase().as_str() {
            "research" | "pesquisa" | "pesquisador" => "research",
            "coder" | "codigo" | "código" | "dev" => "coder",
            "operator" | "operador" | "windows" => "operator",
            "planner" | "planejador" | "estrategista" => "planner",
            _ => "research",
        };

        let display_name = name
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| match normalized_role {
                "research" => "Pesquisador Web".to_string(),
                "coder" => "Engenheiro de Código".to_string(),
                "operator" => "Operador Windows".to_string(),
                "planner" => "Estrategista".to_string(),
                _ => "Agente Autônomo".to_string(),
            });

        let role_prompt = match normalized_role {
            "research" => "Pesquisador Web — busque fontes atualizadas usando web_search e fetch_webpage, cruze fatos, destaque dados relevantes e entregue um relatório claro com conclusões práticas.",
            "coder" => "Engenheiro de Software — analise problemas técnicos, escreva código limpo, edite arquivos se necessário e apresente soluções prontas.",
            "operator" => "Operador Windows — execute automações locais e verifique arquivos ou processos conforme solicitado com segurança.",
            "planner" => "Estrategista e Planejador — decomponha o objetivo em etapas concretas, prioridades, riscos e próximos passos acionáveis.",
            _ => "Agente Autônomo J.A.R.V.I.S. — execute a missão solicitada com máxima clareza, precisão e iniciativa.",
        }
        .to_string();

        let agent_id = format!("agent:{}", Uuid::new_v4());
        let (cancel_tx, cancel_rx) = watch::channel(false);
        {
            let mut map = state.agent_cancels.lock().await;
            map.insert(agent_id.clone(), cancel_tx);
        }
        {
            let mut runs = state.agent_runs.lock().await;
            runs.push(crate::state::AgentRunSnapshot {
                id: agent_id.clone(),
                role: normalized_role.to_string(),
                name: display_name.clone(),
                task: clean_task.clone(),
                status: "running".to_string(),
                output: String::new(),
            });
            if runs.len() > 16 {
                let drain_count = runs.len() - 16;
                runs.drain(0..drain_count);
            }
        }

        let _ = app.emit(
            "jarvis:agent-spawned",
            json!({
                "id": agent_id,
                "role": normalized_role,
                "name": display_name,
                "task": clean_task,
            }),
        );

        let app_clone = app.clone();
        let state_arc = Arc::clone(state);
        let spawned_id = agent_id.clone();
        let display_name_clone = display_name.clone();
        let task_clone = clean_task.clone();

        tokio::spawn(async move {
            let result = run_agent_turn(
                &app_clone,
                &state_arc,
                &spawned_id,
                &role_prompt,
                &clean_task,
                cancel_rx,
            )
            .await;

            {
                let mut map = state_arc.agent_cancels.lock().await;
                map.remove(&spawned_id);
            }

            match result {
                Ok((reply_text, response_mode, traces)) => {
                    {
                        let mut runs = state_arc.agent_runs.lock().await;
                        if let Some(entry) = runs.iter_mut().find(|r| r.id == spawned_id) {
                            entry.status = "done".to_string();
                            entry.output = reply_text.clone();
                        }
                    }
                    let (spoken_summary, card_summary) =
                        build_concise_agent_speech(&display_name_clone, &reply_text);
                    let msg = StoredMessage {
                        id: Uuid::new_v4().to_string(),
                        role: "assistant".to_string(),
                        text: reply_text,
                        timestamp: chrono::Utc::now().to_rfc3339(),
                        mode: Some(response_mode),
                        tool_events: traces,
                    };
                    let _ = app_clone.emit(
                        "jarvis:chat-event",
                        json!({
                            "type": "done",
                            "conversationId": spawned_id,
                            "message": msg
                        }),
                    );
                    let _ = app_clone.emit(
                        "jarvis:agent-completed-summary",
                        json!({
                            "agentId": spawned_id,
                            "agentName": display_name_clone,
                            "task": task_clone,
                            "spokenSummary": spoken_summary,
                            "cardSummary": card_summary
                        }),
                    );
                }
                Err(err) if err == "CANCELLED" => {
                    {
                        let mut runs = state_arc.agent_runs.lock().await;
                        if let Some(entry) = runs.iter_mut().find(|r| r.id == spawned_id) {
                            entry.status = "cancelled".to_string();
                            entry.output = "Cancelado pelo usuário.".to_string();
                        }
                    }
                    let _ = app_clone.emit(
                        "jarvis:chat-event",
                        json!({
                            "type": "cancelled",
                            "conversationId": spawned_id
                        }),
                    );
                }
                Err(err) => {
                    {
                        let mut runs = state_arc.agent_runs.lock().await;
                        if let Some(entry) = runs.iter_mut().find(|r| r.id == spawned_id) {
                            entry.status = "error".to_string();
                            entry.output = err.clone();
                        }
                    }
                    let _ = app_clone.emit(
                        "jarvis:chat-event",
                        json!({
                            "type": "error",
                            "conversationId": spawned_id,
                            "error": err
                        }),
                    );
                }
            }
        });

        Ok(agent_id)
    })
}

pub async fn test_provider_connection(state: &Arc<AppState>) -> ConnectionTestResult {
    let cfg = state.config.read().await.clone();
    let started = Instant::now();

    let res: Result<String, String> = match cfg.provider {
        Provider::Gemini => match resolve_gemini_auth(state, &cfg).await {
            Err(e) => Err(e),
            Ok(auth) => {
                let url = format!(
                    "https://generativelanguage.googleapis.com/v1beta/models/{}",
                    cfg.gemini_model.trim()
                );
                match auth.apply(state.http.get(&url)).send().await {
                    Ok(r) if r.status().is_success() => {
                        let via = if cfg.gemini_auth_mode == GeminiAuthMode::Oauth {
                            "OAuth Google"
                        } else {
                            "API Key"
                        };
                        Ok(format!(
                            "Conectado ao Gemini ({via} · {})",
                            cfg.gemini_model
                        ))
                    }
                    Ok(r) => {
                        let s = r.status();
                        let t = r.text().await.unwrap_or_default();
                        Err(format!("Gemini recusou ({s}): {t}"))
                    }
                    Err(e) => Err(format!("Falha de rede: {e}")),
                }
            }
        },
        Provider::Chatgpt => {
            match oauth::get_valid_access_token(&state.http, &state.data_dir).await {
                Err(e) => Err(e),
                Ok(token) => match oauth::list_chatgpt_models_with_token(&state.http, &token).await
                {
                    Ok(models) => {
                        let plan_ok = oauth::load_session(&state.data_dir)
                            .map(|s| s.plan_usage_granted)
                            .unwrap_or(false);
                        if plan_ok {
                            Ok(format!(
                                "ChatGPT OAuth conectado ({} modelos; ativo: {})",
                                models.len(),
                                cfg.chatgpt_model
                            ))
                        } else {
                            Ok(format!(
                                "Conectado ao ChatGPT ({} modelos), porém o escopo chatgpt.tokens.use.direct não foi concedido.",
                                models.len()
                            ))
                        }
                    }
                    Err(e) => Err(e),
                },
            }
        }
        Provider::Local => match list_local_models(&state.http, &cfg.local_base_url).await {
            Ok(models) => Ok(format!(
                "IA Local conectada em {} ({} modelo(s); ativo: {})",
                cfg.local_base_url,
                models.len(),
                cfg.local_model
            )),
            Err(e) => Err(e),
        },
    };

    let latency_ms = started.elapsed().as_millis() as u64;
    match res {
        Ok(message) => ConnectionTestResult {
            ok: true,
            message,
            latency_ms,
        },
        Err(message) => ConnectionTestResult {
            ok: false,
            message,
            latency_ms,
        },
    }
}

pub async fn transcribe_audio(
    state: &Arc<AppState>,
    audio_base64: &str,
    mime_type: &str,
) -> Result<String, String> {
    let cfg = state.config.read().await.clone();
    let gemini_key = get_api_key(&state.data_dir, KeySlot::Gemini);
    let elevenlabs_key = get_api_key(&state.data_dir, KeySlot::Elevenlabs);
    let google_oauth_ready = google_oauth::load_google_session(&state.data_dir).is_some();

    let chosen = match cfg.stt_engine {
        SttEngine::Browser => return Err("BROWSER_STT_SELECTED".to_string()),
        SttEngine::Elevenlabs => SttEngine::Elevenlabs,
        SttEngine::Gemini => SttEngine::Gemini,
        SttEngine::Auto => {
            if (cfg.gemini_auth_mode == GeminiAuthMode::Oauth && google_oauth_ready)
                || gemini_key.is_some()
                || google_oauth_ready
            {
                SttEngine::Gemini
            } else if elevenlabs_key.is_some() {
                SttEngine::Elevenlabs
            } else {
                return Err("NO_STT_PROVIDER".to_string());
            }
        }
    };

    match chosen {
        SttEngine::Elevenlabs => {
            use base64::Engine as _;
            let key = elevenlabs_key.ok_or_else(|| {
                "Chave da ElevenLabs não configurada para transcrição de voz.".to_string()
            })?;
            let audio_bytes = base64::engine::general_purpose::STANDARD
                .decode(audio_base64.trim())
                .map_err(|e| format!("Áudio base64 inválido: {e}"))?;

            let mut last_err = String::new();
            for model_id in ["scribe_v2", "scribe_v1"] {
                let part = reqwest::multipart::Part::bytes(audio_bytes.clone())
                    .file_name("speech.wav")
                    .mime_str(if mime_type.trim().is_empty() {
                        "audio/wav"
                    } else {
                        mime_type.trim()
                    })
                    .map_err(|e| format!("MIME de áudio inválido: {e}"))?;
                let form = reqwest::multipart::Form::new()
                    .text("model_id", model_id.to_string())
                    .text("language_code", "por")
                    .text("tag_audio_events", "false")
                    .part("file", part);

                let resp = state
                    .http
                    .post("https://api.elevenlabs.io/v1/speech-to-text")
                    .timeout(std::time::Duration::from_secs(18))
                    .header("xi-api-key", &key)
                    .multipart(form)
                    .send()
                    .await
                    .map_err(|e| format!("Erro na transcrição ElevenLabs: {e}"))?;

                if resp.status().is_success() {
                    let parsed: Value = resp
                        .json()
                        .await
                        .map_err(|e| format!("Resposta inválida da ElevenLabs STT: {e}"))?;
                    let text = parsed["text"].as_str().unwrap_or("").trim().to_string();
                    return Ok(text);
                }

                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                last_err = format!("ElevenLabs STT ({model_id}) retornou {status}: {body}");
                // Retry with scribe_v1 only if model not found / bad request
                if status.as_u16() != 400 && status.as_u16() != 404 && status.as_u16() != 422 {
                    break;
                }
            }
            Err(last_err)
        }
        SttEngine::Gemini => {
            let auth = if cfg.gemini_auth_mode == GeminiAuthMode::Oauth
                || (gemini_key.is_none() && google_oauth_ready)
            {
                let (token, session_proj) =
                    google_oauth::get_valid_google_token(&state.http, &state.data_dir).await?;
                let project_id = if !cfg.google_oauth_project_id.trim().is_empty() {
                    Some(cfg.google_oauth_project_id.trim().to_string())
                } else {
                    session_proj
                };
                GeminiAuth::OAuth { token, project_id }
            } else {
                let key = gemini_key.ok_or_else(|| {
                    "Chave Gemini ou login Google OAuth não configurado para transcrição."
                        .to_string()
                })?;
                GeminiAuth::ApiKey(key)
            };
            let url = format!(
                "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
                cfg.gemini_model.trim()
            );
            let body = json!({
                "contents": [{
                    "parts": [
                        { "text": "Transcreva fielmente a fala neste áudio em português do Brasil. Retorne apenas a transcrição limpa, sem comentários nem aspas. Se houver apenas silêncio, retorne string vazia." },
                        { "inlineData": { "mimeType": mime_type, "data": audio_base64 } }
                    ]
                }]
            });
            let resp = auth
                .apply(state.http.post(&url))
                .json(&body)
                .send()
                .await
                .map_err(|e| format!("Erro na transcrição Gemini: {e}"))?;
            if !resp.status().is_success() {
                let s = resp.status();
                let t = resp.text().await.unwrap_or_default();
                return Err(format!("Transcrição Gemini falhou ({s}): {t}"));
            }
            let json: Value = resp
                .json()
                .await
                .map_err(|e| format!("Resposta inválida na transcrição Gemini: {e}"))?;
            let text = json["candidates"][0]["content"]["parts"][0]["text"]
                .as_str()
                .unwrap_or("")
                .trim()
                .to_string();
            Ok(text)
        }
        _ => Err("BROWSER_STT_SELECTED".to_string()),
    }
}
