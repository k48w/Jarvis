use crate::browser;
use crate::config::PermissionMode;
use crate::state::{ActiveTimer, AppState, TimerHandle};
use crate::storage;
use chrono::{Duration as ChronoDuration, Local};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::sync::oneshot;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolRisk {
    Read,
    Action,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionDecision {
    Allow,
    NeedApproval,
    Deny(String),
}

pub struct ToolSpec {
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub risk: ToolRisk,
    pub parameters: fn() -> Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolApprovalRequest {
    pub approval_id: String,
    pub conversation_id: String,
    pub tool_name: String,
    pub title: String,
    pub description: String,
    pub args_summary: String,
}

#[derive(Debug, Clone)]
pub struct ToolRunOutcome {
    pub output: String,
    pub status: String,
    pub summary: String,
    pub triggered_alert: bool,
    pub attached_image: Option<crate::llm::ImageAttachment>,
}

pub fn tool_catalog() -> Vec<ToolSpec> {
    vec![
        ToolSpec {
            name: "get_datetime",
            title: "Consultar data e hora",
            description: "Obtém a data, o dia da semana, a hora local e o fuso horário atual.",
            risk: ToolRisk::Read,
            parameters: || json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolSpec {
            name: "get_system_status",
            title: "Verificar status do PC",
            description: "Lê uso atual de CPU, memória RAM, espaço em disco, bateria/energia e processos que mais consomem RAM.",
            risk: ToolRisk::Read,
            parameters: || json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolSpec {
            name: "web_search",
            title: "Buscar na web",
            description: "Pesquisa informações atualizadas na internet e retorna os principais resultados com título, URL e resumo.",
            risk: ToolRisk::Read,
            parameters: || json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Termos de busca claros e objetivos." }
                },
                "required": ["query"]
            }),
        },
        ToolSpec {
            name: "fetch_webpage",
            title: "Ler página da web",
            description: "Baixa o conteúdo textual de uma URL http/https específica para resumir ou extrair detalhes.",
            risk: ToolRisk::Read,
            parameters: || json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "URL completa começando com http:// ou https://" }
                },
                "required": ["url"]
            }),
        },
        ToolSpec {
            name: "save_memory",
            title: "Salvar na memória",
            description: "Salva um fato duradouro sobre o usuário (nome, preferências, rotina ou projetos) para lembrar em futuras conversas.",
            risk: ToolRisk::Read,
            parameters: || json!({
                "type": "object",
                "properties": {
                    "fact": { "type": "string", "description": "Fato conciso em português para guardar na memória." }
                },
                "required": ["fact"]
            }),
        },
        ToolSpec {
            name: "list_timers",
            title: "Listar timers ativos",
            description: "Lista os timers e lembretes atualmente em contagem regressiva.",
            risk: ToolRisk::Read,
            parameters: || json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolSpec {
            name: "open_url",
            title: "Abrir site no navegador",
            description: "Abre uma URL http:// ou https:// no navegador padrão do Windows.",
            risk: ToolRisk::Action,
            parameters: || json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "URL começando com http:// ou https://" }
                },
                "required": ["url"]
            }),
        },
        ToolSpec {
            name: "open_application",
            title: "Abrir aplicativo",
            description: "Abre um programa ou tela do Windows (ex.: calculadora, bloco de notas, explorador, chrome, edge, vscode, spotify, terminal, configuracoes).",
            risk: ToolRisk::Action,
            parameters: || json!({
                "type": "object",
                "properties": {
                    "app": { "type": "string", "description": "Nome do aplicativo ou executável a abrir." }
                },
                "required": ["app"]
            }),
        },
        ToolSpec {
            name: "set_volume",
            title: "Ajustar volume do Windows",
            description: "Define o volume mestre do Windows (0 a 100) e/ou silencia ou reativa o áudio.",
            risk: ToolRisk::Action,
            parameters: || json!({
                "type": "object",
                "properties": {
                    "level": { "type": "integer", "minimum": 0, "maximum": 100, "description": "Novo volume de 0 a 100." },
                    "muted": { "type": "boolean", "description": "true para silenciar, false para ativar o som." }
                }
            }),
        },
        ToolSpec {
            name: "media_control",
            title: "Controlar reprodução de mídia",
            description: "Envia comando de mídia do sistema: play_pause, next, previous ou stop.",
            risk: ToolRisk::Action,
            parameters: || json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["play_pause", "next", "previous", "stop"],
                        "description": "Ação de controle de mídia."
                    }
                },
                "required": ["action"]
            }),
        },
        ToolSpec {
            name: "set_timer",
            title: "Criar timer / lembrete",
            description: "Cria um timer regressivo que dispara alerta visual e sonoro no J.A.R.V.I.S. ao concluir.",
            risk: ToolRisk::Action,
            parameters: || json!({
                "type": "object",
                "properties": {
                    "seconds": { "type": "integer", "minimum": 1, "maximum": 86400, "description": "Duração do timer em segundos." },
                    "label": { "type": "string", "description": "Nome ou motivo do timer." }
                },
                "required": ["seconds"]
            }),
        },
        ToolSpec {
            name: "cancel_timer",
            title: "Cancelar timer",
            description: "Cancela um timer ativo pelo ID ou 'all' para cancelar todos.",
            risk: ToolRisk::Action,
            parameters: || json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "ID do timer ou 'all'." }
                },
                "required": ["id"]
            }),
        },
        ToolSpec {
            name: "run_command",
            title: "Executar comando PowerShell",
            description: "Executa um comando PowerShell curto no Windows para automação ou consulta avançada (timeout 15s).",
            risk: ToolRisk::Action,
            parameters: || json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "Comando PowerShell a executar." }
                },
                "required": ["command"]
            }),
        },
        ToolSpec {
            name: "trigger_alert",
            title: "Ativar pulso âmbar de alerta",
            description: "Coloca o núcleo do J.A.R.V.I.S. em estado de alerta âmbar para destacar um aviso importante.",
            risk: ToolRisk::Action,
            parameters: || json!({
                "type": "object",
                "properties": {
                    "reason": { "type": "string", "description": "Motivo resumido do alerta." }
                },
                "required": ["reason"]
            }),
        },
        ToolSpec {
            name: "browser_navigate",
            title: "Navegar no navegador embutido",
            description: "Abre o painel visual do navegador WebView2 integrado. Use APENAS quando o usuário pedir explicitamente para abrir/mostrar o navegador ou interagir visualmente com um site (ex: WhatsApp Web, YouTube, login). Para pesquisas comuns, notícias ou fatos, NÃO use esta ferramenta — use web_search ou fetch_webpage.",
            risk: ToolRisk::Read,
            parameters: || json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "URL completa (ex: https://youtube.com) ou termo de busca." }
                },
                "required": ["url"]
            }),
        },
        ToolSpec {
            name: "browser_inspect",
            title: "Inspecionar página do navegador",
            description: "Vê o estado atual do navegador embutido: URL, título, texto visível e todos os botões, links e campos de texto numerados [1..N] com coordenadas.",
            risk: ToolRisk::Read,
            parameters: || json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolSpec {
            name: "browser_click",
            title: "Clicar elemento no navegador",
            description: "Clica de forma nativa em qualquer botão, link ou campo na página atual do navegador usando element_id (de browser_inspect), texto visível ou coordenadas (x, y).",
            risk: ToolRisk::Action,
            parameters: || json!({
                "type": "object",
                "properties": {
                    "element_id": { "type": "integer", "description": "Número [ID] do elemento retornado por browser_inspect." },
                    "text": { "type": "string", "description": "Texto visível ou rótulo do botão/link a clicar (opcional se usar element_id)." },
                    "x": { "type": "number", "description": "Coordenada X opcional no viewport." },
                    "y": { "type": "number", "description": "Coordenada Y opcional no viewport." }
                }
            }),
        },
        ToolSpec {
            name: "browser_type",
            title: "Digitar texto no navegador",
            description: "Foca um campo de texto/busca na página atual (por element_id, target_label ou campo principal), digita o texto e opcionalmente pressiona Enter.",
            risk: ToolRisk::Action,
            parameters: || json!({
                "type": "object",
                "properties": {
                    "text": { "type": "string", "description": "Texto a escrever no campo." },
                    "element_id": { "type": "integer", "description": "Número [ID] do campo retornado por browser_inspect (opcional)." },
                    "target_label": { "type": "string", "description": "Placeholder ou rótulo do campo, ex: 'Pesquisar' (opcional)." },
                    "clear_first": { "type": "boolean", "description": "Se true, limpa o campo antes de digitar (padrão: true)." },
                    "press_enter": { "type": "boolean", "description": "Se true, pressiona Enter após digitar (padrão: false)." }
                },
                "required": ["text"]
            }),
        },
        ToolSpec {
            name: "browser_scroll",
            title: "Rolar página do navegador",
            description: "Rola a página atual do navegador para baixo (down), cima (up), topo (top) ou fim (bottom).",
            risk: ToolRisk::Read,
            parameters: || json!({
                "type": "object",
                "properties": {
                    "direction": { "type": "string", "enum": ["down", "up", "top", "bottom"], "description": "Direção da rolagem." },
                    "amount": { "type": "integer", "description": "Pixels para rolar (padrão 560)." }
                },
                "required": ["direction"]
            }),
        },
        ToolSpec {
            name: "browser_press_key",
            title: "Pressionar tecla no navegador",
            description: "Envia uma tecla especial ao navegador (Enter, Escape, Tab, Backspace, ArrowDown, ArrowUp, PageDown, PageUp, Space).",
            risk: ToolRisk::Action,
            parameters: || json!({
                "type": "object",
                "properties": {
                    "key": { "type": "string", "description": "Nome da tecla, ex: Enter, Escape, Tab." }
                },
                "required": ["key"]
            }),
        },
        ToolSpec {
            name: "browser_back",
            title: "Voltar página no navegador",
            description: "Volta para a página anterior no histórico do navegador embutido.",
            risk: ToolRisk::Read,
            parameters: || json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolSpec {
            name: "browser_close",
            title: "Fechar navegador embutido",
            description: "Fecha/oculta o painel lateral do navegador embutido quando o usuário pedir para fechar.",
            risk: ToolRisk::Read,
            parameters: || json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolSpec {
            name: "spawn_agent",
            title: "Criar subagente em paralelo",
            description: "Cria e executa um NOVO subagente em segundo plano (research, coder, operator, planner). NUNCA chame esta ferramenta se o usuário apenas perguntar se um agente já terminou — para consultar o andamento ou resultado de um agente, use check_agents.",
            risk: ToolRisk::Read,
            parameters: || json!({
                "type": "object",
                "properties": {
                    "role": { "type": "string", "enum": ["research", "coder", "operator", "planner"], "description": "Especialidade do agente." },
                    "name": { "type": "string", "description": "Nome curto do agente (ex: Pesquisador Mercado)." },
                    "task": { "type": "string", "description": "Missão completa que o agente deve executar." }
                },
                "required": ["role", "task"]
            }),
        },
        ToolSpec {
            name: "check_agents",
            title: "Consultar status dos subagentes",
            description: "Consulta o status atual e o relatório/resultado dos subagentes em segundo plano (em andamento ou concluídos). Use SEMPRE que o usuário perguntar se o agente já terminou ou pedir o resumo do que o agente descobriu, sem criar outro agente.",
            risk: ToolRisk::Read,
            parameters: || json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolSpec {
            name: "list_open_windows",
            title: "Listar janelas e apps abertos",
            description: "Lista todas as janelas, jogos e programas abertos ou minimizados no Windows (com nome do processo, título e estado), para identificar qual janela específica inspecionar com see_screen.",
            risk: ToolRisk::Read,
            parameters: || json!({
                "type": "object",
                "properties": {}
            }),
        },
        ToolSpec {
            name: "see_screen",
            title: "Ver a tela ou janela específica",
            description: "Captura a tela do Windows ou uma janela/jogo/aplicativo específico em imagem real (mesmo se estiver atrás de outras janelas ou minimizado). Informe target_window (ex: 'firefox', 'minecraft', 'gta', 'vscode', 'discord') sempre que o usuário pedir para olhar um app ou jogo específico.",
            risk: ToolRisk::Read,
            parameters: || json!({
                "type": "object",
                "properties": {
                    "target_window": { "type": "string", "description": "Nome do aplicativo, jogo ou título da janela específica a enxergar (ex: 'Firefox', 'GTA', 'Minecraft', 'VS Code', 'Discord'). Omita para ver a tela inteira atual." },
                    "reason": { "type": "string", "description": "Motivo curto (opcional)." }
                }
            }),
        },
        ToolSpec {
            name: "screen_click",
            title: "Clicar na tela do Windows",
            description: "Move o cursor e clica nas coordenadas (x, y) da imagem capturada por see_screen (convertidas automaticamente para a resolução real do monitor).",
            risk: ToolRisk::Action,
            parameters: || json!({
                "type": "object",
                "properties": {
                    "x": { "type": "integer", "description": "Coordenada X na imagem retornada por see_screen." },
                    "y": { "type": "integer", "description": "Coordenada Y na imagem retornada por see_screen." },
                    "button": { "type": "string", "enum": ["left", "right"], "description": "Botão do mouse (padrão left)." },
                    "double_click": { "type": "boolean", "description": "Se true, dá duplo clique." }
                },
                "required": ["x", "y"]
            }),
        },
        ToolSpec {
            name: "screen_type",
            title: "Digitar na tela do Windows",
            description: "Digita texto na janela atualmente em foco no Windows (com suporte a acentos/UTF-8) e opcionalmente pressiona Enter.",
            risk: ToolRisk::Action,
            parameters: || json!({
                "type": "object",
                "properties": {
                    "text": { "type": "string", "description": "Texto a digitar." },
                    "press_enter": { "type": "boolean", "description": "Se true, pressiona Enter após digitar." }
                },
                "required": ["text"]
            }),
        },
        ToolSpec {
            name: "screen_hotkey",
            title: "Enviar atalho de teclado no Windows",
            description: "Envia atalho ou tecla especial ao Windows no formato SendKeys (ex: ^c para Ctrl+C, ^v para Ctrl+V, %{TAB} para Alt+Tab, {ENTER}, {ESC}).",
            risk: ToolRisk::Action,
            parameters: || json!({
                "type": "object",
                "properties": {
                    "keys": { "type": "string", "description": "Atalho em formato legível (ex: Ctrl+C, Ctrl+V, Alt+Tab, Enter, Esc) ou SendKeys." }
                },
                "required": ["keys"]
            }),
        },
        ToolSpec {
            name: "list_files",
            title: "Listar arquivos e pastas",
            description: "Lista arquivos e subpastas em um diretório do computador (aceita caminhos completos ou atalhos como ~, Desktop, Documents, Downloads).",
            risk: ToolRisk::Read,
            parameters: || json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Caminho da pasta (ex: C:\\Users\\... ou Desktop ou Documents)." },
                    "max_entries": { "type": "integer", "description": "Máximo de itens a listar (padrão 80)." }
                },
                "required": ["path"]
            }),
        },
        ToolSpec {
            name: "read_file",
            title: "Ler arquivo",
            description: "Lê o conteúdo de um arquivo de texto ou código com numeração de linhas.",
            risk: ToolRisk::Read,
            parameters: || json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Caminho do arquivo a ler." },
                    "start_line": { "type": "integer", "description": "Linha inicial (1-indexada, opcional)." },
                    "end_line": { "type": "integer", "description": "Linha final inclusiva (opcional)." }
                },
                "required": ["path"]
            }),
        },
        ToolSpec {
            name: "write_file",
            title: "Criar ou salvar arquivo",
            description: "Cria um novo arquivo (criando pastas automaticamente) ou sobrescreve/anexa conteúdo UTF-8 em um arquivo existente.",
            risk: ToolRisk::Action,
            parameters: || json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Caminho completo do arquivo (ex: Desktop\\notas.txt)." },
                    "content": { "type": "string", "description": "Conteúdo completo a gravar." },
                    "append": { "type": "boolean", "description": "Se true, adiciona ao final do arquivo em vez de sobrescrever (padrão false)." }
                },
                "required": ["path", "content"]
            }),
        },
        ToolSpec {
            name: "edit_file",
            title: "Editar trecho de arquivo",
            description: "Faz edição cirúrgica em um arquivo substituindo um trecho exato (old_text) por outro (new_text), sem apagar o resto do arquivo.",
            risk: ToolRisk::Action,
            parameters: || json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Caminho do arquivo a editar." },
                    "old_text": { "type": "string", "description": "Trecho exato atual presente no arquivo." },
                    "new_text": { "type": "string", "description": "Novo texto substituto." },
                    "replace_all": { "type": "boolean", "description": "Se true, substitui todas as ocorrências (padrão false)." }
                },
                "required": ["path", "old_text", "new_text"]
            }),
        },
        ToolSpec {
            name: "search_files",
            title: "Buscar texto em arquivos",
            description: "Procura um termo dentro dos arquivos de texto/código de uma pasta (recursivamente, ignorando node_modules, .git e target).",
            risk: ToolRisk::Read,
            parameters: || json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Pasta raiz da busca." },
                    "query": { "type": "string", "description": "Texto a buscar (sem diferenciar maiúsculas/minúsculas)." },
                    "file_extension": { "type": "string", "description": "Extensão opcional para filtrar (ex: ts, rs, py, txt)." },
                    "max_results": { "type": "integer", "description": "Máximo de ocorrências retornadas (padrão 35)." }
                },
                "required": ["path", "query"]
            }),
        },
        ToolSpec {
            name: "read_clipboard",
            title: "Ler área de transferência (Ctrl+C)",
            description: "Lê o conteúdo atual da área de transferência do Windows (texto copiado ou imagem/print copiado). Use quando o usuário disser 'o que eu copiei', 'analisa meu Ctrl+C' ou 'vê esse print que copiei'.",
            risk: ToolRisk::Read,
            parameters: || json!({ "type": "object", "properties": {} }),
        },
    ]
}

pub fn find_spec(name: &str) -> Option<ToolSpec> {
    tool_catalog().into_iter().find(|t| t.name == name)
}

pub fn responses_tool_declarations(mode: PermissionMode, web_search_enabled: bool) -> Vec<Value> {
    tool_catalog()
        .into_iter()
        .filter(|spec| !matches!(check_permission_policy(mode, spec.risk, spec.name, web_search_enabled), PermissionDecision::Deny(_)))
        .map(|spec| {
            json!({
                "type": "function",
                "name": spec.name,
                "description": spec.description,
                "parameters": (spec.parameters)()
            })
        })
        .collect()
}

pub fn gemini_tool_declarations(mode: PermissionMode, web_search_enabled: bool) -> Vec<Value> {
    let decls: Vec<Value> = tool_catalog()
        .into_iter()
        .filter(|spec| !matches!(check_permission_policy(mode, spec.risk, spec.name, web_search_enabled), PermissionDecision::Deny(_)))
        .map(|spec| {
            json!({
                "name": spec.name,
                "description": spec.description,
                "parameters": (spec.parameters)()
            })
        })
        .collect();
    if decls.is_empty() {
        Vec::new()
    } else {
        vec![json!({ "functionDeclarations": decls })]
    }
}

pub fn check_permission_policy(
    mode: PermissionMode,
    risk: ToolRisk,
    name: &str,
    web_search_enabled: bool,
) -> PermissionDecision {
    if matches!(name, "web_search" | "fetch_webpage") && !web_search_enabled {
        return PermissionDecision::Deny(
            "A busca na web está desativada nas configurações do sistema.".to_string(),
        );
    }

    match mode {
        PermissionMode::Auto => PermissionDecision::Allow,
        PermissionMode::Review => match risk {
            ToolRisk::Read => PermissionDecision::Allow,
            ToolRisk::Action => PermissionDecision::NeedApproval,
        },
        PermissionMode::Safe => match name {
            "get_datetime" | "web_search" | "fetch_webpage" | "save_memory" | "list_timers" => {
                PermissionDecision::Allow
            }
            _ => PermissionDecision::Deny(
                "Modo Somente Conversa ativo: ações locais e leitura de hardware do PC estão bloqueadas."
                    .to_string(),
            ),
        },
    }
}

pub fn format_args_summary(name: &str, args: &Value) -> String {
    match name {
        "web_search" => args["query"].as_str().unwrap_or("").to_string(),
        "fetch_webpage" | "open_url" => args["url"].as_str().unwrap_or("").to_string(),
        "open_application" => args["app"].as_str().unwrap_or("").to_string(),
        "set_volume" => {
            let mut parts = Vec::new();
            if let Some(l) = args["level"].as_u64() {
                parts.push(format!("volume {l}%"));
            }
            if let Some(m) = args["muted"].as_bool() {
                parts.push(if m { "silenciar".into() } else { "reativar som".into() });
            }
            parts.join(", ")
        }
        "media_control" => args["action"].as_str().unwrap_or("").to_string(),
        "set_timer" => {
            let secs = args["seconds"].as_u64().unwrap_or(0);
            let label = args["label"].as_str().unwrap_or("Timer");
            format!("{label} ({secs}s)")
        }
        "cancel_timer" => args["id"].as_str().unwrap_or("").to_string(),
        "run_command" => args["command"].as_str().unwrap_or("").to_string(),
        "save_memory" => args["fact"].as_str().unwrap_or("").to_string(),
        "trigger_alert" => args["reason"].as_str().unwrap_or("").to_string(),
        "browser_navigate" => args["url"].as_str().unwrap_or("").to_string(),
        "browser_click" => {
            if let Some(id) = args["element_id"].as_u64() {
                format!("elemento [{id}]")
            } else if let Some(txt) = args["text"].as_str().filter(|s| !s.is_empty()) {
                txt.to_string()
            } else {
                "coordenada".to_string()
            }
        }
        "browser_type" => args["text"].as_str().unwrap_or("").chars().take(40).collect(),
        "browser_scroll" => args["direction"].as_str().unwrap_or("down").to_string(),
        "browser_press_key" => args["key"].as_str().unwrap_or("").to_string(),
        "spawn_agent" => args["name"]
            .as_str()
            .or_else(|| args["role"].as_str())
            .unwrap_or("agente")
            .to_string(),
        "see_screen" => args["target_window"]
            .as_str()
            .filter(|s| !s.trim().is_empty())
            .or_else(|| args["reason"].as_str())
            .unwrap_or("captura visual")
            .to_string(),
        "screen_click" => format!(
            "({}, {})",
            args["x"].as_i64().unwrap_or(0),
            args["y"].as_i64().unwrap_or(0)
        ),
        "screen_type" => args["text"].as_str().unwrap_or("").chars().take(36).collect(),
        "screen_hotkey" => args["keys"].as_str().unwrap_or("").to_string(),
        "list_files" | "read_file" | "write_file" | "edit_file" => {
            args["path"].as_str().unwrap_or("").to_string()
        }
        "search_files" => format!(
            "{} em {}",
            args["query"].as_str().unwrap_or(""),
            args["path"].as_str().unwrap_or("")
        ),
        _ => String::new(),
    }
}

pub fn validate_http_url(raw: &str) -> Result<url::Url, String> {
    let parsed = url::Url::parse(raw.trim()).map_err(|_| "URL inválida".to_string())?;
    match parsed.scheme() {
        "http" | "https" => Ok(parsed),
        _ => Err("Apenas URLs http:// ou https:// são permitidas".to_string()),
    }
}

pub fn is_dangerous_command(command: &str) -> Option<&'static str> {
    let norm = command
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();

    let blocked = [
        ("format ", "comando de formatação de disco"),
        ("format-volume", "formatação de volume"),
        ("diskpart", "particionamento de disco"),
        ("bcdedit", "alteração de boot BCD"),
        ("vssadmin delete", "exclusão de cópias de sombra"),
        ("wbadmin delete", "exclusão de backups"),
        ("reg delete hk", "exclusão no registro do sistema"),
        ("stop-computer", "desligamento do sistema"),
        ("restart-computer", "reinicialização do sistema"),
        ("shutdown /", "desligamento/reinicialização via shutdown"),
        ("shutdown -", "desligamento/reinicialização via shutdown"),
    ];
    for (needle, reason) in blocked {
        if norm.contains(needle) {
            return Some(reason);
        }
    }

    static RE_ROOT_DEL: OnceLock<Regex> = OnceLock::new();
    let re_root = RE_ROOT_DEL.get_or_init(|| {
        Regex::new(r"(?i)(remove-item|rm|del|rmdir|rd)\b.*([a-z]:\\(\s|$|\*)|\\windows\\|\\system32\\)")
            .expect("valid root delete regex")
    });
    if re_root.is_match(&norm) {
        return Some("exclusão em raiz de unidade ou diretório protegido do Windows");
    }

    None
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

fn format_datetime() -> String {
    let now = Local::now();
    let weekday = match now.format("%u").to_string().as_str() {
        "1" => "segunda-feira",
        "2" => "terça-feira",
        "3" => "quarta-feira",
        "4" => "quinta-feira",
        "5" => "sexta-feira",
        "6" => "sábado",
        _ => "domingo",
    };
    format!(
        "Data: {} ({}) · Hora local: {} · Fuso: {}",
        now.format("%d/%m/%Y"),
        weekday,
        now.format("%H:%M:%S"),
        now.format("%:z")
    )
}

fn read_system_status_blocking() -> String {
    use sysinfo::{Disks, ProcessesToUpdate, System};

    let mut sys = System::new_all();
    std::thread::sleep(Duration::from_millis(140));
    sys.refresh_cpu_usage();
    sys.refresh_memory();
    sys.refresh_processes(ProcessesToUpdate::All, true);

    let cpu = sys.global_cpu_usage();
    let total_mem_gb = sys.total_memory() as f64 / 1_073_741_824.0;
    let used_mem_gb = sys.used_memory() as f64 / 1_073_741_824.0;
    let mem_pct = if sys.total_memory() > 0 {
        (sys.used_memory() as f64 / sys.total_memory() as f64) * 100.0
    } else {
        0.0
    };

    let disks = Disks::new_with_refreshed_list();
    let mut disk_lines = Vec::new();
    for d in disks.list() {
        let total = d.total_space() as f64 / 1_073_741_824.0;
        if total < 1.0 {
            continue;
        }
        let avail = d.available_space() as f64 / 1_073_741_824.0;
        let mount = d.mount_point().to_string_lossy();
        disk_lines.push(format!("{mount} {avail:.1} GB livres de {total:.1} GB"));
    }

    let mut procs: Vec<_> = sys.processes().values().collect();
    procs.sort_by(|a, b| b.memory().cmp(&a.memory()));
    let top_procs: Vec<String> = procs
        .into_iter()
        .take(5)
        .map(|p| {
            format!(
                "{} ({:.0} MB)",
                p.name().to_string_lossy(),
                p.memory() as f64 / 1_048_576.0
            )
        })
        .collect();

    let power_info = read_windows_power_status();

    format!(
        "CPU: {cpu:.1}%\nRAM: {used_mem_gb:.1} / {total_mem_gb:.1} GB ({mem_pct:.0}%)\nDiscos: {}\nEnergia: {power_info}\nTop RAM: {}",
        if disk_lines.is_empty() { "N/D".to_string() } else { disk_lines.join(" | ") },
        if top_procs.is_empty() { "N/D".to_string() } else { top_procs.join(", ") }
    )
}

#[cfg(windows)]
fn read_windows_power_status() -> String {
    use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    let mut status = SYSTEM_POWER_STATUS::default();
    let ok = unsafe { GetSystemPowerStatus(&mut status) }.is_ok();
    if !ok {
        return "N/D".to_string();
    }
    let ac = match status.ACLineStatus {
        1 => "Conectado à tomada",
        0 => "Na bateria",
        _ => "Energia desconhecida",
    };
    if status.BatteryLifePercent <= 100 && status.BatteryFlag != 128 {
        format!("{ac}, bateria em {}%", status.BatteryLifePercent)
    } else {
        format!("{ac} (desktop sem bateria)")
    }
}

#[cfg(not(windows))]
fn read_windows_power_status() -> String {
    "N/D".to_string()
}

#[cfg(windows)]
fn set_windows_volume_blocking(level: Option<u8>, muted: Option<bool>) -> Result<String, String> {
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::Media::Audio::{eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED,
    };

    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(|e| format!("Falha ao acessar dispositivo de áudio: {e}"))?;
        let device = enumerator
            .GetDefaultAudioEndpoint(eRender, eConsole)
            .map_err(|e| format!("Nenhum dispositivo de saída de áudio padrão: {e}"))?;
        let endpoint: IAudioEndpointVolume = device
            .Activate(CLSCTX_ALL, None)
            .map_err(|e| format!("Falha ao abrir controle de volume: {e}"))?;

        if let Some(pct) = level {
            let scalar = (pct.min(100) as f32) / 100.0;
            endpoint
                .SetMasterVolumeLevelScalar(scalar, std::ptr::null())
                .map_err(|e| format!("Falha ao definir volume: {e}"))?;
        }
        if let Some(m) = muted {
            endpoint
                .SetMute(m, std::ptr::null())
                .map_err(|e| format!("Falha ao alterar mudo: {e}"))?;
        }

        let current_level = endpoint
            .GetMasterVolumeLevelScalar()
            .map(|v| (v * 100.0).round() as u8)
            .unwrap_or(0);
        let current_mute = endpoint.GetMute().map(|b| b.as_bool()).unwrap_or(false);

        Ok(format!(
            "Volume atual: {current_level}% · Mudo: {}",
            if current_mute { "sim" } else { "não" }
        ))
    }
}

#[cfg(not(windows))]
fn set_windows_volume_blocking(_level: Option<u8>, _muted: Option<bool>) -> Result<String, String> {
    Err("Controle de volume disponível apenas no Windows".to_string())
}

#[cfg(windows)]
fn send_media_key(action: &str) -> Result<String, String> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
        KEYEVENTF_KEYUP, VIRTUAL_KEY, VK_MEDIA_NEXT_TRACK, VK_MEDIA_PLAY_PAUSE,
        VK_MEDIA_PREV_TRACK, VK_MEDIA_STOP,
    };

    let (vk, label): (VIRTUAL_KEY, &str) = match action {
        "play_pause" => (VK_MEDIA_PLAY_PAUSE, "Play/Pause"),
        "next" => (VK_MEDIA_NEXT_TRACK, "Próxima faixa"),
        "previous" => (VK_MEDIA_PREV_TRACK, "Faixa anterior"),
        "stop" => (VK_MEDIA_STOP, "Parar mídia"),
        _ => return Err(format!("Ação de mídia desconhecida: {action}")),
    };

    let inputs = [
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: KEYBD_EVENT_FLAGS(0),
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        },
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: KEYEVENTF_KEYUP,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        },
    ];

    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent == 0 {
        Err("O Windows bloqueou o envio da tecla de mídia".to_string())
    } else {
        Ok(format!("Comando de mídia enviado: {label}"))
    }
}

#[cfg(not(windows))]
fn send_media_key(_action: &str) -> Result<String, String> {
    Err("Controle de mídia disponível apenas no Windows".to_string())
}

fn resolve_app_target(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("Nome do aplicativo vazio".to_string());
    }
    let lower = trimmed.to_ascii_lowercase();
    let mapped = match lower.as_str() {
        "calculadora" | "calculator" | "calc" => "calc.exe",
        "bloco de notas" | "notepad" => "notepad.exe",
        "explorador" | "explorer" | "arquivos" | "explorador de arquivos" => "explorer.exe",
        "paint" => "mspaint.exe",
        "gerenciador de tarefas" | "task manager" | "taskmgr" => "taskmgr.exe",
        "configuracoes" | "configurações" | "settings" => "ms-settings:",
        "chrome" | "google chrome" => "chrome",
        "edge" | "microsoft edge" => "msedge",
        "vscode" | "vs code" | "visual studio code" | "code" => "code",
        "spotify" => "spotify",
        "discord" => "discord",
        "terminal" | "windows terminal" => "wt",
        "powershell" => "powershell.exe",
        "cmd" | "prompt" => "cmd.exe",
        "word" => "winword",
        "excel" => "excel",
        "whatsapp" => "whatsapp:",
        _ => trimmed,
    };

    if mapped
        .chars()
        .any(|c| "&|;<>^\r\n\"".contains(c) || c.is_control())
    {
        return Err("Nome de aplicativo contém caracteres não permitidos".to_string());
    }
    Ok(mapped.to_string())
}

fn launch_application(app: &str) -> Result<String, String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    let target = resolve_app_target(app)?;
    if target.contains(':') && !target.contains('\\') && !target.contains('/') {
        open::that(&target).map_err(|e| format!("Não foi possível abrir {app}: {e}"))?;
        return Ok(format!("Aplicativo aberto: {app}"));
    }

    std::process::Command::new("cmd")
        .args(["/C", "start", "", &target])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("Não foi possível iniciar {app}: {e}"))?;

    Ok(format!("Solicitação enviada para abrir {app}"))
}

async fn run_web_search(state: &AppState, query: &str) -> Result<String, String> {
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

async fn run_fetch_webpage(state: &AppState, raw_url: &str) -> Result<String, String> {
    let parsed = validate_http_url(raw_url)?;
    let resp = state
        .http
        .get(parsed)
        .timeout(Duration::from_secs(12))
        .header(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
        )
        .send()
        .await
        .map_err(|e| format!("Erro ao abrir página: {e}"))?;
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

async fn run_powershell(command: &str) -> Result<String, String> {
    const CREATE_NO_WINDOW: u32 = 0x08000000;

    if let Some(reason) = is_dangerous_command(command) {
        return Err(format!(
            "Comando bloqueado por política de segurança ({reason})."
        ));
    }

    let wrapped = format!(
        "[Console]::OutputEncoding = [System.Text.Encoding]::UTF8; $ErrorActionPreference = 'Continue'; {command}"
    );

    let mut child = tokio::process::Command::new("powershell");
    child
        .args(["-NoProfile", "-NonInteractive", "-Command", &wrapped])
        .creation_flags(CREATE_NO_WINDOW);

    let output = tokio::time::timeout(Duration::from_secs(15), child.output())
        .await
        .map_err(|_| "Comando PowerShell excedeu o tempo limite de 15 segundos.".to_string())?
        .map_err(|e| format!("Falha ao executar PowerShell: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let mut combined = String::new();
    if !stdout.is_empty() {
        combined.push_str(&stdout);
    }
    if !stderr.is_empty() {
        if !combined.is_empty() {
            combined.push_str("\n[stderr] ");
        }
        combined.push_str(&stderr);
    }
    if combined.is_empty() {
        combined = format!("Comando concluído (código {:?}).", output.status.code());
    }
    Ok(combined.chars().take(4000).collect())
}

async fn run_set_timer(
    app: &AppHandle,
    state: &Arc<AppState>,
    seconds: u64,
    label: &str,
) -> Result<String, String> {
    let secs = seconds.clamp(1, 86_400);
    let clean_label = if label.trim().is_empty() {
        "Timer J.A.R.V.I.S.".to_string()
    } else {
        label.trim().chars().take(64).collect()
    };
    let id = Uuid::new_v4().to_string();
    let ends_at = (Local::now() + ChronoDuration::seconds(secs as i64)).to_rfc3339();
    let info = ActiveTimer {
        id: id.clone(),
        label: clean_label.clone(),
        ends_at,
        total_seconds: secs,
    };

    let (cancel_tx, cancel_rx) = oneshot::channel::<()>();
    {
        let mut timers = state.timers.lock().await;
        timers.insert(
            id.clone(),
            TimerHandle {
                info: info.clone(),
                cancel_tx,
            },
        );
    }

    let app_clone = app.clone();
    let state_clone = Arc::clone(state);
    let timer_id = id.clone();
    let timer_label = clean_label.clone();

    tokio::spawn(async move {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(secs)) => {
                let mut timers = state_clone.timers.lock().await;
                let still_active = timers.remove(&timer_id).is_some();
                drop(timers);
                if still_active {
                    let _ = app_clone.emit("jarvis:timer-fired", json!({
                        "id": timer_id,
                        "label": timer_label,
                    }));
                }
            }
            _ = cancel_rx => {}
        }
    });

    Ok(format!(
        "Timer '{clean_label}' iniciado por {secs} segundos (id: {id})."
    ))
}

async fn wait_for_user_approval(
    app: &AppHandle,
    state: &Arc<AppState>,
    conversation_id: &str,
    spec: &ToolSpec,
    args_summary: &str,
) -> bool {
    let approval_id = Uuid::new_v4().to_string();
    let (tx, rx) = oneshot::channel::<bool>();
    {
        let mut map = state.pending_approvals.lock().await;
        map.insert(approval_id.clone(), tx);
    }

    let req = ToolApprovalRequest {
        approval_id: approval_id.clone(),
        conversation_id: conversation_id.to_string(),
        tool_name: spec.name.to_string(),
        title: spec.title.to_string(),
        description: spec.description.to_string(),
        args_summary: args_summary.to_string(),
    };
    let _ = app.emit("jarvis:tool-approval-request", req);

    let approved = match tokio::time::timeout(Duration::from_secs(120), rx).await {
        Ok(Ok(val)) => val,
        _ => {
            let mut map = state.pending_approvals.lock().await;
            map.remove(&approval_id);
            false
        }
    };
    approved
}

pub async fn execute_tool_with_policy(
    app: &AppHandle,
    state: &Arc<AppState>,
    conversation_id: &str,
    tool_id: &str,
    name: &str,
    args: &Value,
) -> ToolRunOutcome {
    let Some(spec) = find_spec(name) else {
        return ToolRunOutcome {
            output: format!("Ferramenta desconhecida: {name}"),
            status: "error".to_string(),
            summary: format!("Ferramenta desconhecida ({name})"),
            triggered_alert: false,
            attached_image: None,
        };
    };

    let (mode, web_enabled) = {
        let cfg = state.config.read().await;
        (cfg.permission_mode, cfg.web_search_enabled)
    };
    let args_summary = format_args_summary(name, args);

    match check_permission_policy(mode, spec.risk, spec.name, web_enabled) {
        PermissionDecision::Allow => {}
        PermissionDecision::Deny(reason) => {
            let _ = app.emit(
                "jarvis:chat-event",
                json!({
                    "type": "tool_end",
                    "conversationId": conversation_id,
                    "id": tool_id,
                    "name": spec.name,
                    "title": spec.title,
                    "status": "denied",
                    "summary": reason
                }),
            );
            return ToolRunOutcome {
                output: reason.clone(),
                status: "denied".to_string(),
                summary: reason,
                triggered_alert: false,
                attached_image: None,
            };
        }
        PermissionDecision::NeedApproval => {
            let _ = app.emit(
                "jarvis:chat-event",
                json!({
                    "type": "tool_start",
                    "conversationId": conversation_id,
                    "id": tool_id,
                    "name": spec.name,
                    "title": format!("Aguardando aprovação: {}", spec.title),
                    "summary": args_summary
                }),
            );
            let approved =
                wait_for_user_approval(app, state, conversation_id, &spec, &args_summary).await;
            if !approved {
                let reason = "Ação recusada pelo usuário.".to_string();
                let _ = app.emit(
                    "jarvis:chat-event",
                    json!({
                        "type": "tool_end",
                        "conversationId": conversation_id,
                        "id": tool_id,
                        "name": spec.name,
                        "title": spec.title,
                        "status": "denied",
                        "summary": reason
                    }),
                );
                return ToolRunOutcome {
                    output: reason.clone(),
                    status: "denied".to_string(),
                    summary: reason,
                    triggered_alert: false,
                    attached_image: None,
                };
            }
        }
    }

    let _ = app.emit(
        "jarvis:chat-event",
        json!({
            "type": "tool_start",
            "conversationId": conversation_id,
            "id": tool_id,
            "name": spec.name,
            "title": spec.title,
            "summary": args_summary
        }),
    );

    let mut triggered_alert = false;
    let mut attached_image: Option<crate::llm::ImageAttachment> = None;
    let res: Result<String, String> = match name {
        "get_datetime" => Ok(format_datetime()),
        "get_system_status" => tokio::task::spawn_blocking(read_system_status_blocking)
            .await
            .map_err(|e| format!("Falha ao consultar sistema: {e}")),
        "web_search" => {
            let query = args["query"].as_str().unwrap_or("");
            run_web_search(state, query).await
        }
        "fetch_webpage" => {
            let raw_url = args["url"].as_str().unwrap_or("");
            run_fetch_webpage(state, raw_url).await
        }
        "save_memory" => {
            let fact = args["fact"].as_str().unwrap_or("");
            storage::add_memory(&state.data_dir, fact)
                .map(|m| format!("Fato salvo na memória permanente: {}", m.content))
        }
        "list_timers" => {
            let list = state.list_active_timers().await;
            if list.is_empty() {
                Ok("Nenhum timer ativo no momento.".to_string())
            } else {
                Ok(list
                    .into_iter()
                    .map(|t| format!("- {} ({}s, id: {})", t.label, t.total_seconds, t.id))
                    .collect::<Vec<_>>()
                    .join("\n"))
            }
        }
        "open_url" => {
            let raw_url = args["url"].as_str().unwrap_or("");
            match validate_http_url(raw_url) {
                Ok(parsed) => open::that(parsed.as_str())
                    .map(|_| format!("Site aberto no navegador: {}", parsed))
                    .map_err(|e| format!("Não foi possível abrir o navegador: {e}")),
                Err(e) => Err(e),
            }
        }
        "open_application" => {
            let app_name = args["app"].as_str().unwrap_or("");
            launch_application(app_name)
        }
        "set_volume" => {
            let level = args["level"].as_u64().map(|v| v.min(100) as u8);
            let muted = args["muted"].as_bool();
            tokio::task::spawn_blocking(move || set_windows_volume_blocking(level, muted))
                .await
                .map_err(|e| format!("Erro ao ajustar áudio: {e}"))
                .and_then(|r| r)
        }
        "media_control" => {
            let action = args["action"].as_str().unwrap_or("");
            send_media_key(action)
        }
        "set_timer" => {
            let secs = args["seconds"].as_u64().unwrap_or(60);
            let label = args["label"].as_str().unwrap_or("Timer J.A.R.V.I.S.");
            run_set_timer(app, state, secs, label).await
        }
        "cancel_timer" => {
            let id = args["id"].as_str().unwrap_or("");
            if state.cancel_timer_by_id(id).await {
                Ok("Timer cancelado.".to_string())
            } else {
                Ok("Nenhum timer encontrado com esse ID.".to_string())
            }
        }
        "run_command" => {
            let cmd = args["command"].as_str().unwrap_or("");
            run_powershell(cmd).await
        }
        "trigger_alert" => {
            let reason = args["reason"].as_str().unwrap_or("Alerta ativado");
            triggered_alert = true;
            let _ = app.emit(
                "jarvis:chat-event",
                json!({
                    "type": "mode",
                    "conversationId": conversation_id,
                    "mode": "alert"
                }),
            );
            Ok(format!("Pulso âmbar de alerta ativado: {reason}"))
        }
        "browser_navigate" => {
            let url = args["url"].as_str().unwrap_or("");
            if conversation_id.starts_with("agent:") {
                if url.starts_with("http://") || url.starts_with("https://") {
                    run_fetch_webpage(state, url).await
                } else {
                    run_web_search(state, url).await
                }
            } else {
                let _ = app.emit(
                    "jarvis:browser-panel",
                    json!({ "open": true, "url": url }),
                );
                tokio::time::sleep(Duration::from_millis(220)).await;
                match browser::navigate(app, url).await {
                    Ok(info) => {
                        let _ = app.emit(
                            "jarvis:browser-panel",
                            json!({ "open": true, "url": info.url, "title": info.title }),
                        );
                        Ok(browser::format_inspect_for_llm(&info))
                    }
                    Err(e) => Err(e),
                }
            }
        }
        "browser_inspect" => {
            if conversation_id.starts_with("agent:") {
                Ok("Subagente em segundo plano não utiliza o navegador visual; use web_search ou fetch_webpage.".to_string())
            } else {
                let _ = app.emit("jarvis:browser-panel", json!({ "open": true }));
                match browser::inspect_page(app, false).await {
                    Ok(info) => Ok(browser::format_inspect_for_llm(&info)),
                    Err(e) => Err(e),
                }
            }
        }
        "browser_click" => {
            if conversation_id.starts_with("agent:") {
                Ok("Subagente em segundo plano não utiliza o navegador visual; use web_search ou fetch_webpage.".to_string())
            } else {
                let _ = app.emit("jarvis:browser-panel", json!({ "open": true }));
                let element_id = args["element_id"].as_u64().map(|v| v as u32);
                let text = args["text"].as_str();
                let x = args["x"].as_f64();
                let y = args["y"].as_f64();
                browser::click(app, element_id, text, x, y).await
            }
        }
        "browser_type" => {
            if conversation_id.starts_with("agent:") {
                Ok("Subagente em segundo plano não utiliza o navegador visual; use web_search ou fetch_webpage.".to_string())
            } else {
                let _ = app.emit("jarvis:browser-panel", json!({ "open": true }));
                let text = args["text"].as_str().unwrap_or("");
                let element_id = args["element_id"].as_u64().map(|v| v as u32);
                let target_label = args["target_label"].as_str();
                let clear_first = args["clear_first"].as_bool().unwrap_or(true);
                let press_enter = args["press_enter"].as_bool().unwrap_or(false);
                browser::type_text(app, text, element_id, target_label, clear_first, press_enter).await
            }
        }
        "browser_scroll" => {
            if conversation_id.starts_with("agent:") {
                Ok("Subagente em segundo plano não utiliza o navegador visual.".to_string())
            } else {
                let direction = args["direction"].as_str().unwrap_or("down");
                let amount = args["amount"].as_i64().map(|v| v as i32);
                browser::scroll_page(app, direction, amount).await
            }
        }
        "browser_press_key" => {
            if conversation_id.starts_with("agent:") {
                Ok("Subagente em segundo plano não utiliza o navegador visual.".to_string())
            } else {
                let key = args["key"].as_str().unwrap_or("Enter");
                browser::press_key(app, key).await
            }
        }
        "browser_back" => {
            if conversation_id.starts_with("agent:") {
                Ok("Subagente em segundo plano não utiliza o navegador visual.".to_string())
            } else {
                match browser::go_back(app).await {
                    Ok(st) => Ok(format!("Voltou para: {} ({})", st.title, st.url)),
                    Err(e) => Err(e),
                }
            }
        }
        "browser_close" => {
            let _ = browser::set_visible(app, false).await;
            let _ = app.emit("jarvis:browser-panel", json!({ "open": false }));
            Ok("Navegador embutido fechado.".to_string())
        }
        "spawn_agent" => {
            if conversation_id.starts_with("agent:") {
                Ok("Subagente não pode criar outros subagentes; conclua a tarefa diretamente.".to_string())
            } else {
                let role = args["role"].as_str().unwrap_or("research");
                let agent_name = args["name"].as_str();
                let task = args["task"].as_str().unwrap_or("");
                match crate::llm::spawn_background_agent(app, state, role, agent_name, task).await {
                    Ok(id) if id.starts_with("ALREADY_RUNNING:") => {
                        let rest = id.trim_start_matches("ALREADY_RUNNING:");
                        let mut parts = rest.splitn(3, ':');
                        let existing_id = parts.next().unwrap_or("agent");
                        let existing_name = parts.next().unwrap_or("Agente");
                        let existing_task = parts.next().unwrap_or("");
                        Ok(format!(
                            "O subagente '{existing_name}' ({existing_id}) já está em andamento executando '{existing_task}'. Não foi criado outro agente duplicado; informe o usuário que ele ainda está trabalhando."
                        ))
                    }
                    Ok(id) => Ok(format!(
                        "Subagente criado e iniciado em segundo plano (id: {id}). A aba Agentes apareceu automaticamente no topo para o usuário acompanhar em tempo real."
                    )),
                    Err(e) => Err(e),
                }
            }
        }
        "check_agents" => {
            let runs = state.agent_runs.lock().await;
            if runs.is_empty() {
                Ok("Nenhum subagente foi iniciado nesta sessão.".to_string())
            } else {
                let mut lines = Vec::new();
                for r in runs.iter().rev().take(6) {
                    let status_pt = match r.status.as_str() {
                        "running" => "EM ANDAMENTO",
                        "done" => "CONCLUÍDO",
                        "cancelled" => "CANCELADO",
                        _ => "ERRO",
                    };
                    if r.output.trim().is_empty() {
                        lines.push(format!(
                            "- [{}] {} ({}) · Missão: {}",
                            status_pt, r.name, r.role, r.task
                        ));
                    } else {
                        let snippet: String = r.output.chars().take(700).collect();
                        lines.push(format!(
                            "- [{}] {} ({}) · Missão: {}\n  Resultado: {}",
                            status_pt, r.name, r.role, r.task, snippet
                        ));
                    }
                }
                Ok(format!(
                    "Status dos subagentes em segundo plano:\n{}",
                    lines.join("\n\n")
                ))
            }
        }
        "list_open_windows" => {
            tokio::task::spawn_blocking(|| {
                let wins = list_open_windows_info_blocking();
                if wins.is_empty() {
                    Ok("Nenhuma janela aberta detectada.".to_string())
                } else {
                    let lines: Vec<String> = wins
                        .into_iter()
                        .map(|w| {
                            format!(
                                "- [{}] {} ({}, {}x{})",
                                w.process_name,
                                w.title,
                                if w.minimized { "minimizada" } else { "aberta" },
                                w.width,
                                w.height
                            )
                        })
                        .collect();
                    Ok(format!("Janelas abertas no Windows:\n{}", lines.join("\n")))
                }
            })
            .await
            .map_err(|e| format!("Falha ao listar janelas: {e}"))
            .and_then(|r| r)
        }
        "see_screen" => {
            let _ = app.emit("jarvis:screen-pulse", json!({ "kind": "capture" }));
            let target_window = args["target_window"]
                .as_str()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);

            // Exclude J.A.R.V.I.S. windows from capture so it sees right through itself!
            let excluded_handles = set_jarvis_capture_exclusion(app, true);
            if !excluded_handles.is_empty() {
                tokio::time::sleep(Duration::from_millis(85)).await;
            }

            let target_clone = target_window.clone();
            let cap_res = tokio::task::spawn_blocking(move || {
                let open_wins = list_open_windows_info_blocking();
                if let Some(ref query) = target_clone {
                    let (shot, matched_label, method) =
                        capture_target_window_blocking(query, &open_wins)?;
                    Ok::<_, String>((shot, open_wins, Some((matched_label, method))))
                } else {
                    let shot = capture_desktop_screen_blocking()?;
                    Ok::<_, String>((shot, open_wins, None))
                }
            })
            .await
            .map_err(|e| format!("Falha ao capturar tela: {e}"))
            .and_then(|r| r);
            let _ = set_jarvis_capture_exclusion(app, false);

            match cap_res {
                Ok(((cap_w, cap_h, real_w, real_h, b64), open_wins, targeted)) => {
                    attached_image = Some(crate::llm::ImageAttachment {
                        mime_type: "image/jpeg".to_string(),
                        data_base64: b64,
                    });
                    let wins_summary: Vec<String> = open_wins
                        .iter()
                        .take(10)
                        .map(|w| format!("[{}] {}", w.process_name, w.title))
                        .collect();
                    let wins_note = if wins_summary.is_empty() {
                        String::new()
                    } else {
                        format!("\nJanelas abertas no Windows: {}", wins_summary.join(" | "))
                    };
                    if let Some((matched_label, method)) = targeted {
                        Ok(format!(
                            "Janela específica capturada: '{matched_label}' via {method} ({cap_w}x{cap_h} de {real_w}x{real_h}). A imagem foi anexada abaixo para você analisar diretamente.{wins_note}"
                        ))
                    } else {
                        Ok(format!(
                            "Tela inteira capturada ({cap_w}x{cap_h} redimensionada de {real_w}x{real_h}; a janela do J.A.R.V.I.S. foi excluída da captura). A imagem foi anexada abaixo para você analisar diretamente.{wins_note}"
                        ))
                    }
                }
                Err(e) => Err(e),
            }
        }
        "screen_click" => {
            let _ = app.emit("jarvis:screen-pulse", json!({ "kind": "action" }));
            let x = args["x"].as_i64().unwrap_or(0) as i32;
            let y = args["y"].as_i64().unwrap_or(0) as i32;
            let button = args["button"].as_str().unwrap_or("left").to_string();
            let double_click = args["double_click"].as_bool().unwrap_or(false);
            tokio::task::spawn_blocking(move || {
                screen_click_blocking(x, y, &button, double_click)
            })
            .await
            .map_err(|e| format!("Falha ao clicar na tela: {e}"))
            .and_then(|r| r)
        }
        "screen_type" => {
            let _ = app.emit("jarvis:screen-pulse", json!({ "kind": "action" }));
            let text = args["text"].as_str().unwrap_or("").to_string();
            let press_enter = args["press_enter"].as_bool().unwrap_or(false);
            tokio::task::spawn_blocking(move || screen_type_blocking(&text, press_enter))
                .await
                .map_err(|e| format!("Falha ao digitar na tela: {e}"))
                .and_then(|r| r)
        }
        "screen_hotkey" => {
            let _ = app.emit("jarvis:screen-pulse", json!({ "kind": "action" }));
            let keys = args["keys"].as_str().unwrap_or("").to_string();
            tokio::task::spawn_blocking(move || screen_hotkey_blocking(&keys))
                .await
                .map_err(|e| format!("Falha ao enviar atalho: {e}"))
                .and_then(|r| r)
        }
        "list_files" => {
            let raw_path = args["path"].as_str().unwrap_or("").to_string();
            let max_entries = args["max_entries"].as_u64().unwrap_or(80).clamp(1, 300) as usize;
            tokio::task::spawn_blocking(move || tool_list_files(&raw_path, max_entries))
                .await
                .map_err(|e| format!("Falha ao listar arquivos: {e}"))
                .and_then(|r| r)
        }
        "read_file" => {
            let raw_path = args["path"].as_str().unwrap_or("").to_string();
            let start_line = args["start_line"].as_u64().map(|v| v as usize);
            let end_line = args["end_line"].as_u64().map(|v| v as usize);
            tokio::task::spawn_blocking(move || tool_read_file(&raw_path, start_line, end_line))
                .await
                .map_err(|e| format!("Falha ao ler arquivo: {e}"))
                .and_then(|r| r)
        }
        "write_file" => {
            let raw_path = args["path"].as_str().unwrap_or("").to_string();
            let content = args["content"].as_str().unwrap_or("").to_string();
            let append = args["append"].as_bool().unwrap_or(false);
            let outcome = tokio::task::spawn_blocking(move || {
                let resolved = resolve_user_path(&raw_path)?;
                let existed = resolved.exists();
                let prev_text = if existed {
                    std::fs::read_to_string(&resolved).unwrap_or_default()
                } else {
                    String::new()
                };
                let msg = tool_write_file(&raw_path, &content, append)?;
                let next_text = std::fs::read_to_string(&resolved).unwrap_or(content);
                Ok::<_, String>((msg, resolved, existed, prev_text, next_text))
            })
            .await
            .map_err(|e| format!("Falha ao salvar arquivo: {e}"))
            .and_then(|r| r);

            match outcome {
                Ok((msg, resolved, existed, prev_text, next_text)) => {
                    emit_file_diff(app, conversation_id, &resolved, "write_file", existed, &prev_text, &next_text);
                    Ok(msg)
                }
                Err(e) => Err(e),
            }
        }
        "edit_file" => {
            let raw_path = args["path"].as_str().unwrap_or("").to_string();
            let old_text = args["old_text"].as_str().unwrap_or("").to_string();
            let new_text = args["new_text"].as_str().unwrap_or("").to_string();
            let replace_all = args["replace_all"].as_bool().unwrap_or(false);
            let outcome = tokio::task::spawn_blocking(move || {
                let resolved = resolve_user_path(&raw_path)?;
                let prev_text = std::fs::read_to_string(&resolved).unwrap_or_default();
                let msg = tool_edit_file(&raw_path, &old_text, &new_text, replace_all)?;
                let next_text = std::fs::read_to_string(&resolved).unwrap_or_default();
                Ok::<_, String>((msg, resolved, prev_text, next_text))
            })
            .await
            .map_err(|e| format!("Falha ao editar arquivo: {e}"))
            .and_then(|r| r);

            match outcome {
                Ok((msg, resolved, prev_text, next_text)) => {
                    emit_file_diff(app, conversation_id, &resolved, "edit_file", true, &prev_text, &next_text);
                    Ok(msg)
                }
                Err(e) => Err(e),
            }
        }
        "search_files" => {
            let raw_path = args["path"].as_str().unwrap_or("").to_string();
            let query = args["query"].as_str().unwrap_or("").to_string();
            let ext = args["file_extension"].as_str().map(str::to_string);
            let max_results = args["max_results"].as_u64().unwrap_or(35).clamp(1, 100) as usize;
            tokio::task::spawn_blocking(move || {
                tool_search_files(&raw_path, &query, ext.as_deref(), max_results)
            })
            .await
            .map_err(|e| format!("Falha ao buscar em arquivos: {e}"))
            .and_then(|r| r)
        }
        "read_clipboard" => {
            let clip_res = tokio::task::spawn_blocking(read_clipboard_blocking)
                .await
                .map_err(|e| format!("Falha ao ler área de transferência: {e}"))
                .and_then(|r| r);
            match clip_res {
                Ok(clip) => {
                    if let (Some(b64), Some(mime)) = (clip.image_base64, clip.image_mime) {
                        attached_image = Some(crate::llm::ImageAttachment {
                            mime_type: mime,
                            data_base64: b64,
                        });
                        let dims = match (clip.width, clip.height) {
                            (Some(w), Some(h)) => format!(" ({w}x{h})"),
                            _ => String::new(),
                        };
                        Ok(format!(
                            "Imagem copiada na área de transferência capturada{dims}. A imagem foi anexada abaixo para você analisar."
                        ))
                    } else if let Some(txt) = clip.text {
                        let preview: String = txt.chars().take(12_000).collect();
                        let truncated = if txt.chars().count() > 12_000 {
                            "\n... (texto truncado em 12.000 caracteres)"
                        } else {
                            ""
                        };
                        Ok(format!(
                            "Texto atual na área de transferência (Ctrl+C):\n{preview}{truncated}"
                        ))
                    } else {
                        Ok("A área de transferência está vazia no momento.".to_string())
                    }
                }
                Err(e) => Err(e),
            }
        }
        _ => Err(format!("Ferramenta não implementada: {name}")),
    };

    let (status, output, summary) = match res {
        Ok(out) => {
            let short: String = out.lines().next().unwrap_or(&out).chars().take(90).collect();
            ("done".to_string(), out, short)
        }
        Err(err) => {
            let short: String = err.chars().take(90).collect();
            ("error".to_string(), err, short)
        }
    };

    let _ = app.emit(
        "jarvis:chat-event",
        json!({
            "type": "tool_end",
            "conversationId": conversation_id,
            "id": tool_id,
            "name": spec.name,
            "title": spec.title,
            "status": status,
            "summary": summary
        }),
    );

    ToolRunOutcome {
        output,
        status,
        summary,
        triggered_alert,
        attached_image,
    }
}

#[cfg(windows)]
fn encode_bgra_to_jpeg_base64(
    bgra: &[u8],
    real_w: u32,
    real_h: u32,
    row_pitch: usize,
) -> Result<(u32, u32, u32, u32, String), String> {
    use base64::Engine as _;
    use image::codecs::jpeg::JpegEncoder;
    use image::imageops::{resize, FilterType};
    use image::RgbImage;

    let w = real_w as usize;
    let h = real_h as usize;
    let mut rgb = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        let row_start = y * row_pitch;
        let row = &bgra[row_start..row_start + w * 4];
        for px in row.chunks_exact(4) {
            rgb.push(px[2]); // R
            rgb.push(px[1]); // G
            rgb.push(px[0]); // B
        }
    }

    let full_img = RgbImage::from_raw(real_w, real_h, rgb)
        .ok_or_else(|| "Buffer de imagem inválido.".to_string())?;

    let max_w = 1280u32;
    let (out_img, cap_w, cap_h) = if real_w > max_w {
        let scale = max_w as f64 / real_w as f64;
        let tw = max_w;
        let th = ((real_h as f64 * scale).round() as u32).max(1);
        (resize(&full_img, tw, th, FilterType::Triangle), tw, th)
    } else {
        (full_img, real_w, real_h)
    };

    let mut jpeg_bytes = Vec::with_capacity(96 * 1024);
    let mut enc = JpegEncoder::new_with_quality(&mut jpeg_bytes, 78);
    enc.encode_image(&out_img)
        .map_err(|e| format!("Falha ao codificar JPEG da tela: {e}"))?;

    let b64 = base64::engine::general_purpose::STANDARD.encode(&jpeg_bytes);
    Ok((cap_w, cap_h, real_w, real_h, b64))
}

#[cfg(windows)]
fn capture_via_dxgi() -> Result<(u32, u32, u32, u32, String), String> {
    use windows::core::Interface;
    use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
    use windows::Win32::Graphics::Direct3D11::{
        D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
        D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAPPED_SUBRESOURCE,
        D3D11_MAP_READ, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
    };
    use windows::Win32::Graphics::Dxgi::{
        IDXGIDevice, IDXGIOutput1, IDXGIResource, DXGI_OUTDUPL_FRAME_INFO,
    };

    unsafe {
        let mut device_opt: Option<ID3D11Device> = None;
        let mut ctx_opt: Option<ID3D11DeviceContext> = None;
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            windows::Win32::Foundation::HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device_opt),
            None,
            Some(&mut ctx_opt),
        )
        .map_err(|e| format!("D3D11CreateDevice falhou: {e}"))?;

        let device = device_opt.ok_or("Dispositivo D3D11 nulo")?;
        let ctx = ctx_opt.ok_or("Contexto D3D11 nulo")?;
        let dxgi_dev: IDXGIDevice = device.cast().map_err(|e| format!("IDXGIDevice: {e}"))?;
        let adapter = dxgi_dev.GetAdapter().map_err(|e| format!("GetAdapter: {e}"))?;
        let output = adapter.EnumOutputs(0).map_err(|e| format!("EnumOutputs: {e}"))?;
        let output1: IDXGIOutput1 = output.cast().map_err(|e| format!("IDXGIOutput1: {e}"))?;
        let dupl = output1
            .DuplicateOutput(&device)
            .map_err(|e| format!("DuplicateOutput: {e}"))?;

        let mut frame_info = DXGI_OUTDUPL_FRAME_INFO::default();
        let mut res_opt: Option<IDXGIResource> = None;
        let mut acquired = false;
        for _ in 0..8 {
            match dupl.AcquireNextFrame(250, &mut frame_info, &mut res_opt) {
                Ok(()) => {
                    if frame_info.LastPresentTime != 0 || res_opt.is_some() {
                        acquired = true;
                        break;
                    }
                    let _ = dupl.ReleaseFrame();
                }
                Err(_) => {
                    std::thread::sleep(Duration::from_millis(60));
                }
            }
        }
        if !acquired {
            return Err("DXGI não recebeu novo frame da tela.".to_string());
        }

        let res = res_opt.ok_or("DXGI retornou recurso vazio")?;
        let desktop_tex: ID3D11Texture2D = res.cast().map_err(|e| format!("ID3D11Texture2D: {e}"))?;
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        desktop_tex.GetDesc(&mut desc);

        let staging_desc = D3D11_TEXTURE2D_DESC {
            Width: desc.Width,
            Height: desc.Height,
            MipLevels: 1,
            ArraySize: 1,
            Format: desc.Format,
            SampleDesc: desc.SampleDesc,
            Usage: D3D11_USAGE_STAGING,
            BindFlags: 0,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            MiscFlags: 0,
        };

        let mut staging_opt: Option<ID3D11Texture2D> = None;
        device
            .CreateTexture2D(&staging_desc, None, Some(&mut staging_opt))
            .map_err(|e| {
                let _ = dupl.ReleaseFrame();
                format!("CreateTexture2D staging: {e}")
            })?;
        let staging = staging_opt.ok_or_else(|| {
            let _ = dupl.ReleaseFrame();
            "Staging texture nula".to_string()
        })?;

        ctx.CopyResource(&staging, &desktop_tex);
        let _ = dupl.ReleaseFrame();

        let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
        ctx.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
            .map_err(|e| format!("Map staging: {e}"))?;

        let row_pitch = mapped.RowPitch as usize;
        let total = row_pitch * (desc.Height as usize);
        let slice = std::slice::from_raw_parts(mapped.pData as *const u8, total);
        let copied = slice.to_vec();
        ctx.Unmap(&staging, 0);

        encode_bgra_to_jpeg_base64(&copied, desc.Width, desc.Height, row_pitch)
    }
}

#[cfg(windows)]
fn capture_via_gdi() -> Result<(u32, u32, u32, u32, String), String> {
    use std::ffi::c_void;
    use windows::core::w;
    use windows::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateDCW, DeleteDC, DeleteObject,
        GetDC, GetDIBits, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
        CAPTUREBLT, DIB_RGB_COLORS, SRCCOPY,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};

    unsafe {
        let real_w = GetSystemMetrics(SM_CXSCREEN);
        let real_h = GetSystemMetrics(SM_CYSCREEN);
        if real_w <= 0 || real_h <= 0 {
            return Err("Resolução de tela indisponível.".to_string());
        }

        let mut used_create_dc = false;
        let mut screen_dc = CreateDCW(w!("DISPLAY"), None, None, None);
        if !screen_dc.is_invalid() {
            used_create_dc = true;
        } else {
            screen_dc = GetDC(None);
        }
        if screen_dc.is_invalid() {
            return Err("Falha ao obter contexto gráfico (DC) da tela.".to_string());
        }

        let release_screen_dc = |dc| {
            if used_create_dc {
                let _ = DeleteDC(dc);
            } else {
                let _ = ReleaseDC(None, dc);
            }
        };

        let mem_dc = CreateCompatibleDC(Some(screen_dc));
        if mem_dc.is_invalid() {
            release_screen_dc(screen_dc);
            return Err("Falha ao criar contexto gráfico em memória.".to_string());
        }

        let hbmp = CreateCompatibleBitmap(screen_dc, real_w, real_h);
        if hbmp.is_invalid() {
            let _ = DeleteDC(mem_dc);
            release_screen_dc(screen_dc);
            return Err("Falha ao alocar bitmap da tela.".to_string());
        }

        let prev = SelectObject(mem_dc, hbmp.into());
        if prev.is_invalid() {
            let _ = DeleteObject(hbmp.into());
            let _ = DeleteDC(mem_dc);
            release_screen_dc(screen_dc);
            return Err("SelectObject falhou ao associar bitmap.".to_string());
        }

        let mut blt_ok = BitBlt(
            mem_dc,
            0,
            0,
            real_w,
            real_h,
            Some(screen_dc),
            0,
            0,
            SRCCOPY | CAPTUREBLT,
        );
        if blt_ok.is_err() {
            blt_ok = BitBlt(
                mem_dc,
                0,
                0,
                real_w,
                real_h,
                Some(screen_dc),
                0,
                0,
                SRCCOPY,
            );
        }

        let _ = SelectObject(mem_dc, prev);

        let mut bgra = vec![0u8; (real_w as usize) * (real_h as usize) * 4];
        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: real_w,
                biHeight: -real_h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let scanlines = if blt_ok.is_ok() {
            GetDIBits(
                screen_dc,
                hbmp,
                0,
                real_h as u32,
                Some(bgra.as_mut_ptr() as *mut c_void),
                &mut bmi,
                DIB_RGB_COLORS,
            )
        } else {
            0
        };

        let _ = DeleteObject(hbmp.into());
        let _ = DeleteDC(mem_dc);
        release_screen_dc(screen_dc);

        if let Err(err) = blt_ok {
            return Err(format!("Falha no BitBlt da tela via GDI: {err}"));
        }
        if scanlines == 0 {
            return Err("Falha no GetDIBits ao extrair pixels da tela.".to_string());
        }

        encode_bgra_to_jpeg_base64(&bgra, real_w as u32, real_h as u32, (real_w as usize) * 4)
    }
}

pub fn capture_desktop_screen_blocking() -> Result<(u32, u32, u32, u32, String), String> {
    #[cfg(windows)]
    {
        match capture_via_dxgi() {
            Ok(shot) => Ok(shot),
            Err(dxgi_err) => capture_via_gdi()
                .map_err(|gdi_err| format!("DXGI ({dxgi_err}) · GDI ({gdi_err})")),
        }
    }
    #[cfg(not(windows))]
    {
        Err("Captura de tela disponível apenas no Windows.".to_string())
    }
}

fn set_jarvis_capture_exclusion(app: &AppHandle, exclude: bool) -> Vec<isize> {
    #[cfg(windows)]
    {
        use tauri::Manager;
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{
            SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE, WDA_NONE,
        };

        let affinity = if exclude {
            WDA_EXCLUDEFROMCAPTURE
        } else {
            WDA_NONE
        };
        let mut applied = Vec::new();
        for label in ["main", "mascot"] {
            if let Some(win) = app.get_webview_window(label) {
                if let Ok(hwnd_raw) = win.hwnd() {
                    let hwnd = HWND(hwnd_raw.0);
                    if unsafe { SetWindowDisplayAffinity(hwnd, affinity) }.is_ok() {
                        applied.push(hwnd_raw.0 as isize);
                    }
                }
            }
        }
        applied
    }
    #[cfg(not(windows))]
    {
        let _ = (app, exclude);
        Vec::new()
    }
}

#[derive(Debug, Clone)]
struct OpenWindowInfo {
    hwnd: isize,
    #[allow(dead_code)]
    pid: u32,
    process_name: String,
    title: String,
    minimized: bool,
    width: i32,
    height: i32,
}

fn list_open_windows_info_blocking() -> Vec<OpenWindowInfo> {
    #[cfg(windows)]
    {
        use std::collections::HashMap;
        use sysinfo::{ProcessesToUpdate, System};
        use windows::core::BOOL;
        use windows::Win32::Foundation::{HWND, LPARAM, RECT};
        use windows::Win32::UI::WindowsAndMessaging::{
            EnumWindows, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId, IsIconic,
            IsWindowVisible,
        };

        struct RawWin {
            hwnd: isize,
            pid: u32,
            title: String,
            minimized: bool,
            width: i32,
            height: i32,
        }

        unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
            if IsWindowVisible(hwnd).as_bool() {
                let mut buf = [0u16; 256];
                let len = GetWindowTextW(hwnd, &mut buf);
                if len > 0 {
                    let title = String::from_utf16_lossy(&buf[..len as usize])
                        .trim()
                        .to_string();
                    let ignored = title.is_empty()
                        || title.starts_with("J.A.R.V.I.S.")
                        || title == "Program Manager"
                        || title == "Windows Input Experience"
                        || title == "Default IME"
                        || title == "MSCTFIME UI";
                    if !ignored {
                        let minimized = IsIconic(hwnd).as_bool();
                        let mut rect = RECT::default();
                        let _ = GetWindowRect(hwnd, &mut rect);
                        let width = (rect.right - rect.left).max(0);
                        let height = (rect.bottom - rect.top).max(0);
                        if minimized || (width >= 140 && height >= 90) {
                            let mut pid = 0u32;
                            let _ = GetWindowThreadProcessId(hwnd, Some(&mut pid));
                            let list = &mut *(lparam.0 as *mut Vec<RawWin>);
                            if list.len() < 32 {
                                list.push(RawWin {
                                    hwnd: hwnd.0 as isize,
                                    pid,
                                    title,
                                    minimized,
                                    width,
                                    height,
                                });
                            }
                        }
                    }
                }
            }
            BOOL(1)
        }

        let mut raw: Vec<RawWin> = Vec::new();
        unsafe {
            let _ = EnumWindows(
                Some(enum_proc),
                LPARAM(&mut raw as *mut Vec<RawWin> as isize),
            );
        }

        let mut sys = System::new();
        sys.refresh_processes(ProcessesToUpdate::All, true);
        let pid_map: HashMap<u32, String> = sys
            .processes()
            .iter()
            .map(|(pid, proc_)| (pid.as_u32(), proc_.name().to_string_lossy().to_string()))
            .collect();

        let mut out = Vec::new();
        for w in raw {
            let process_name = pid_map
                .get(&w.pid)
                .cloned()
                .unwrap_or_else(|| "app.exe".to_string());
            if process_name.eq_ignore_ascii_case("jarvis.exe") {
                continue;
            }
            out.push(OpenWindowInfo {
                hwnd: w.hwnd,
                pid: w.pid,
                process_name,
                title: w.title,
                minimized: w.minimized,
                width: w.width,
                height: w.height,
            });
            if out.len() >= 25 {
                break;
            }
        }
        out
    }
    #[cfg(not(windows))]
    {
        Vec::new()
    }
}

fn match_target_window<'a>(query: &str, wins: &'a [OpenWindowInfo]) -> Option<&'a OpenWindowInfo> {
    let q = query.trim().to_ascii_lowercase();
    if q.is_empty() || wins.is_empty() {
        return None;
    }

    const BROWSERS: &[&str] = &[
        "firefox", "chrome", "msedge", "brave", "opera", "vivaldi", "zen", "arc",
    ];
    const EDITORS: &[&str] = &[
        "code",
        "cursor",
        "windsurf",
        "devenv",
        "idea64",
        "pycharm",
        "rider",
        "zed",
        "sublime_text",
        "notepad++",
    ];
    const NON_GAMES: &[&str] = &[
        "firefox",
        "chrome",
        "msedge",
        "brave",
        "opera",
        "vivaldi",
        "zen",
        "arc",
        "code",
        "cursor",
        "windsurf",
        "devenv",
        "explorer",
        "cmd",
        "powershell",
        "pwsh",
        "windowsterminal",
        "wt",
        "notepad",
        "discord",
        "whatsapp",
        "telegram",
        "spotify",
        "slack",
        "teams",
        "ms-teams",
        "taskmgr",
        "applicationframehost",
        "systemsettings",
        "obs64",
        "jarvis",
        "steamwebhelper",
        "epicgameslauncher",
    ];
    const KNOWN_GAME_HINTS: &[&str] = &[
        "minecraft",
        "javaw",
        "tlauncher",
        "prismlauncher",
        "lunar",
        "gta",
        "fivem",
        "roblox",
        "valorant",
        "riot",
        "cs2",
        "csgo",
        "dota",
        "league",
        "fortnite",
        "elden",
        "cyberpunk",
        "witcher",
        "red dead",
        "rdr2",
        "mumu",
        "bluestacks",
        "ldplayer",
        "nox",
        "retroarch",
        "dolphin",
        "rpcs3",
        "pcsx2",
        "yuzu",
        "ryujinx",
        "cemu",
        "ppsspp",
        "unity",
        "unreal",
        "terraria",
        "stardew",
        "hollow knight",
        "celeste",
        "balatro",
        "palworld",
        "apex",
        "overwatch",
        "genshin",
        "honkai",
        "wuthering",
        "pubg",
        "warzone",
        "fifa",
        "fc24",
        "fc25",
        "rocket league",
        "baldurs",
        "skyrim",
        "fallout",
        "diablo",
        "path of exile",
        "osu",
        "geometry dash",
        "among us",
        "lethal company",
        "helldivers",
        "dark souls",
        "sekiro",
        "subnautica",
        "factorio",
        "rimworld",
        "civilization",
        "sims",
    ];

    let wants_browser = q.contains("navegador") || q.contains("browser");
    let wants_editor = q.contains("vscode")
        || q.contains("vs code")
        || q.contains("visual studio")
        || q.contains("editor");
    let wants_game = q.contains("jogo")
        || q.contains("game")
        || q.contains("partida")
        || q.contains("emulador")
        || q.contains("emulator");

    let stop_words = ["o", "a", "os", "as", "meu", "minha", "janela", "tela", "app", "do", "da", "de", "no", "na"];
    let tokens: Vec<&str> = q
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() >= 2 && !stop_words.contains(t))
        .collect();

    let mut best: Option<(i32, &OpenWindowInfo)> = None;

    for win in wins {
        let proc_lower = win.process_name.to_ascii_lowercase();
        let stem = proc_lower.strip_suffix(".exe").unwrap_or(&proc_lower);
        let title_lower = win.title.to_ascii_lowercase();
        let combined = format!("{stem} {proc_lower} {title_lower}");
        let mut score = 0i32;

        if title_lower == q {
            score += 140;
        }
        if stem == q || proc_lower == q {
            score += 130;
        }
        if proc_lower.contains(&q) || stem.contains(&q) {
            score += 95;
        }
        if title_lower.contains(&q) {
            score += 90;
        }

        if !tokens.is_empty() {
            let matched_tokens = tokens.iter().filter(|t| combined.contains(**t)).count();
            if matched_tokens == tokens.len() {
                score += 75;
            } else if matched_tokens > 0 {
                score += (matched_tokens as i32) * 25;
            }
        }

        if wants_browser && BROWSERS.iter().any(|b| stem.contains(b) || title_lower.contains(b)) {
            score += 85;
        }
        if wants_editor
            && (EDITORS.iter().any(|e| stem == *e || stem.contains(e))
                || title_lower.contains("visual studio code"))
        {
            score += 85;
        }
        if wants_game {
            let is_known_game = KNOWN_GAME_HINTS
                .iter()
                .any(|g| stem.contains(g) || title_lower.contains(g));
            if is_known_game {
                score += 115;
            } else {
                let is_non_game = NON_GAMES.iter().any(|ng| stem == *ng || stem.starts_with(ng));
                if !is_non_game && (win.minimized || (win.width >= 500 && win.height >= 350)) {
                    score += 55;
                }
            }
        }

        if score > 0 && !win.minimized {
            score += 5;
        }

        if score > 0 {
            match best {
                Some((best_score, _)) if score <= best_score => {}
                _ => best = Some((score, win)),
            }
        }
    }

    best.map(|(_, w)| w)
}

#[cfg(windows)]
fn capture_window_printwindow(hwnd_raw: isize) -> Result<(u32, u32, u32, u32, String), String> {
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits,
        ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    };
    use windows::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS};
    use windows::Win32::UI::WindowsAndMessaging::GetWindowRect;

    let hwnd = HWND(hwnd_raw as *mut std::ffi::c_void);
    let mut rect = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut rect) }
        .map_err(|e| format!("GetWindowRect falhou: {e}"))?;

    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    if width < 32 || height < 32 {
        return Err(format!("Janela com dimensões muito pequenas ({width}x{height})."));
    }

    unsafe {
        let screen_dc = GetDC(None);
        if screen_dc.is_invalid() {
            return Err("GetDC(None) retornou nulo.".to_string());
        }
        let mem_dc = CreateCompatibleDC(Some(screen_dc));
        if mem_dc.is_invalid() {
            let _ = ReleaseDC(None, screen_dc);
            return Err("CreateCompatibleDC falhou.".to_string());
        }
        let hbmp = CreateCompatibleBitmap(screen_dc, width, height);
        if hbmp.is_invalid() {
            let _ = DeleteDC(mem_dc);
            let _ = ReleaseDC(None, screen_dc);
            return Err("CreateCompatibleBitmap falhou.".to_string());
        }

        let prev = SelectObject(mem_dc, hbmp.into());
        // PW_RENDERFULLCONTENT = 2 captures DirectComposition / hardware-accelerated windows behind others
        let mut printed = PrintWindow(hwnd, mem_dc, PRINT_WINDOW_FLAGS(2)).as_bool();
        if !printed {
            printed = PrintWindow(hwnd, mem_dc, PRINT_WINDOW_FLAGS(0)).as_bool();
        }
        // MSDN requires deselecting the bitmap before GetDIBits
        SelectObject(mem_dc, prev);

        if !printed {
            let _ = DeleteObject(hbmp.into());
            let _ = DeleteDC(mem_dc);
            let _ = ReleaseDC(None, screen_dc);
            return Err("PrintWindow retornou falso para esta janela.".to_string());
        }

        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut bgra = vec![0u8; (width as usize) * (height as usize) * 4];
        let lines = GetDIBits(
            screen_dc,
            hbmp,
            0,
            height as u32,
            Some(bgra.as_mut_ptr() as *mut _),
            &mut bmi,
            DIB_RGB_COLORS,
        );

        let _ = DeleteObject(hbmp.into());
        let _ = DeleteDC(mem_dc);
        let _ = ReleaseDC(None, screen_dc);

        if lines == 0 {
            return Err("GetDIBits falhou ao extrair pixels da janela.".to_string());
        }

        // Detect all-black DirectX/Vulkan/OpenGL frames so caller falls back to foreground DXGI
        let total_px = (width as usize) * (height as usize);
        let step = (total_px / 600).max(1);
        let mut non_black = 0usize;
        let mut sampled = 0usize;
        let mut idx = 0usize;
        while idx < total_px {
            let off = idx * 4;
            let b = bgra[off] as u16;
            let g = bgra[off + 1] as u16;
            let r = bgra[off + 2] as u16;
            if r + g + b > 24 {
                non_black += 1;
            }
            sampled += 1;
            idx += step;
        }
        if sampled > 0 && non_black * 100 / sampled < 3 {
            return Err(
                "PrintWindow retornou quadro preto (possível jogo DirectX/Vulkan/OpenGL)."
                    .to_string(),
            );
        }

        encode_bgra_to_jpeg_base64(&bgra, width as u32, height as u32, (width as usize) * 4)
    }
}

fn capture_target_window_blocking(
    query: &str,
    open_wins: &[OpenWindowInfo],
) -> Result<((u32, u32, u32, u32, String), String, &'static str), String> {
    let Some(win) = match_target_window(query, open_wins) else {
        let available: Vec<String> = open_wins
            .iter()
            .take(12)
            .map(|w| format!("[{}] {}", w.process_name, w.title))
            .collect();
        return Err(format!(
            "Não encontrei nenhuma janela aberta correspondente a '{query}'. Janelas abertas no momento: {}",
            if available.is_empty() {
                "nenhuma".to_string()
            } else {
                available.join(" | ")
            }
        ));
    };

    let matched_label = format!("[{}] {}", win.process_name, win.title);

    #[cfg(windows)]
    {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::{
            BringWindowToTop, SetForegroundWindow, ShowWindow, SW_RESTORE,
        };

        // 1) Try background capture with zero focus steal if window is not minimized
        if !win.minimized {
            if let Ok(shot) = capture_window_printwindow(win.hwnd) {
                return Ok((
                    shot,
                    matched_label,
                    "PrintWindow (em segundo plano, sem trocar foco)",
                ));
            }
        }

        // 2) Minimized window or DirectX/Vulkan game -> restore/focus briefly & capture via DXGI/GDI
        let hwnd = HWND(win.hwnd as *mut std::ffi::c_void);
        unsafe {
            if win.minimized {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }
            let _ = BringWindowToTop(hwnd);
            let _ = SetForegroundWindow(hwnd);
        }
        std::thread::sleep(Duration::from_millis(280));

        // Retry PrintWindow first (in case it was just minimized and is a regular desktop app)
        if win.minimized {
            if let Ok(shot) = capture_window_printwindow(win.hwnd) {
                return Ok((
                    shot,
                    matched_label,
                    "PrintWindow (após restaurar janela minimizada)",
                ));
            }
        }

        let shot = capture_desktop_screen_blocking()?;
        Ok((
            shot,
            matched_label,
            "DXGI/GDI (janela trazida para primeiro plano)",
        ))
    }
    #[cfg(not(windows))]
    {
        let _ = matched_label;
        Err("Captura de janela específica disponível apenas no Windows.".to_string())
    }
}

fn screen_click_blocking(
    x: i32,
    y: i32,
    button: &str,
    double_click: bool,
) -> Result<String, String> {
    #[cfg(windows)]
    {
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
            MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEINPUT,
        };
        use windows::Win32::UI::WindowsAndMessaging::{
            GetSystemMetrics, SetCursorPos, SM_CXSCREEN,
        };

        unsafe {
            let real_w = GetSystemMetrics(SM_CXSCREEN).max(1);
            let cap_w = real_w.min(1280) as f64;
            let scale = real_w as f64 / cap_w;
            let rx = (x as f64 * scale).round() as i32;
            let ry = (y as f64 * scale).round() as i32;

            SetCursorPos(rx, ry).map_err(|e| format!("Falha ao mover cursor: {e}"))?;
            std::thread::sleep(Duration::from_millis(30));

            let is_right = button.eq_ignore_ascii_case("right");
            let down_flag = if is_right {
                MOUSEEVENTF_RIGHTDOWN
            } else {
                MOUSEEVENTF_LEFTDOWN
            };
            let up_flag = if is_right {
                MOUSEEVENTF_RIGHTUP
            } else {
                MOUSEEVENTF_LEFTUP
            };

            let clicks = if double_click { 2 } else { 1 };
            for i in 0..clicks {
                let inputs = [
                    INPUT {
                        r#type: INPUT_MOUSE,
                        Anonymous: INPUT_0 {
                            mi: MOUSEINPUT {
                                dwFlags: down_flag,
                                ..Default::default()
                            },
                        },
                    },
                    INPUT {
                        r#type: INPUT_MOUSE,
                        Anonymous: INPUT_0 {
                            mi: MOUSEINPUT {
                                dwFlags: up_flag,
                                ..Default::default()
                            },
                        },
                    },
                ];
                let sent = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
                if sent == 0 {
                    return Err("Win32 SendInput não conseguiu enviar o clique.".to_string());
                }
                if i + 1 < clicks {
                    std::thread::sleep(Duration::from_millis(65));
                }
            }

            Ok(format!(
                "Clique ({button}{}) executado na tela em ({rx},{ry}).",
                if double_click { ", duplo" } else { "" }
            ))
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (x, y, button, double_click);
        Err("Controle de mouse disponível apenas no Windows.".to_string())
    }
}

#[cfg(windows)]
fn send_vk(vk: u16, key_up: bool) {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
        VIRTUAL_KEY,
    };
    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                dwFlags: if key_up {
                    KEYEVENTF_KEYUP
                } else {
                    KEYBD_EVENT_FLAGS(0)
                },
                ..Default::default()
            },
        },
    };
    unsafe {
        let _ = SendInput(&[input], std::mem::size_of::<INPUT>() as i32);
    }
}

fn screen_type_blocking(text: &str, press_enter: bool) -> Result<String, String> {
    #[cfg(windows)]
    {
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
            KEYEVENTF_UNICODE, VIRTUAL_KEY,
        };

        let mut inputs: Vec<INPUT> = Vec::with_capacity(text.len() * 2 + 2);
        for unit in text.encode_utf16() {
            inputs.push(INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(0),
                        wScan: unit,
                        dwFlags: KEYEVENTF_UNICODE,
                        ..Default::default()
                    },
                },
            });
            inputs.push(INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(0),
                        wScan: unit,
                        dwFlags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
                        ..Default::default()
                    },
                },
            });
        }

        if !inputs.is_empty() {
            unsafe {
                let sent = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
                if sent == 0 {
                    return Err("Win32 SendInput não conseguiu digitar o texto.".to_string());
                }
            }
        }

        if press_enter {
            std::thread::sleep(Duration::from_millis(35));
            send_vk(0x0D, false);
            send_vk(0x0D, true);
        }

        Ok(format!(
            "Texto digitado na janela em foco{}.",
            if press_enter { " e Enter pressionado" } else { "" }
        ))
    }
    #[cfg(not(windows))]
    {
        let _ = (text, press_enter);
        Err("Digitação global disponível apenas no Windows.".to_string())
    }
}

fn parse_hotkey_vks(raw: &str) -> Result<Vec<u16>, String> {
    let norm = raw.trim();
    if norm.is_empty() {
        return Err("Atalho vazio.".to_string());
    }
    let expanded = match norm.to_ascii_lowercase().as_str() {
        "^c" => "ctrl+c",
        "^v" => "ctrl+v",
        "^a" => "ctrl+a",
        "^z" => "ctrl+z",
        "^s" => "ctrl+s",
        "^w" => "ctrl+w",
        "^t" => "ctrl+t",
        "^l" => "ctrl+l",
        "%{tab}" => "alt+tab",
        "%{f4}" => "alt+f4",
        "{enter}" => "enter",
        "{esc}" => "esc",
        "{tab}" => "tab",
        "{backspace}" => "backspace",
        "{delete}" => "delete",
        _ => norm,
    };

    let mut vks = Vec::new();
    for part in expanded.split('+').map(str::trim).filter(|s| !s.is_empty()) {
        let lower = part.to_ascii_lowercase();
        let vk = match lower.as_str() {
            "ctrl" | "control" => 0x11,
            "alt" => 0x12,
            "shift" => 0x10,
            "win" | "super" | "meta" => 0x5B,
            "enter" | "return" => 0x0D,
            "esc" | "escape" => 0x1B,
            "tab" => 0x09,
            "space" | "espaco" | "espaço" => 0x20,
            "backspace" => 0x08,
            "delete" | "del" => 0x2E,
            "up" | "arrowup" => 0x26,
            "down" | "arrowdown" => 0x28,
            "left" | "arrowleft" => 0x25,
            "right" | "arrowright" => 0x27,
            "home" => 0x24,
            "end" => 0x23,
            "pageup" => 0x21,
            "pagedown" => 0x22,
            "f1" => 0x70,
            "f2" => 0x71,
            "f3" => 0x72,
            "f4" => 0x73,
            "f5" => 0x74,
            "f6" => 0x75,
            "f7" => 0x76,
            "f8" => 0x77,
            "f9" => 0x78,
            "f10" => 0x79,
            "f11" => 0x7A,
            "f12" => 0x7B,
            s if s.len() == 1 => {
                let b = s.as_bytes()[0].to_ascii_uppercase();
                if b.is_ascii_alphanumeric() {
                    b as u16
                } else {
                    return Err(format!("Tecla não suportada no atalho: '{part}'"));
                }
            }
            _ => return Err(format!("Tecla desconhecida no atalho: '{part}'")),
        };
        vks.push(vk);
    }

    if vks.is_empty() {
        Err("Nenhuma tecla válida no atalho.".to_string())
    } else {
        Ok(vks)
    }
}

fn screen_hotkey_blocking(keys: &str) -> Result<String, String> {
    #[cfg(windows)]
    {
        let vks = parse_hotkey_vks(keys)?;
        for &vk in &vks {
            send_vk(vk, false);
        }
        std::thread::sleep(Duration::from_millis(35));
        for &vk in vks.iter().rev() {
            send_vk(vk, true);
        }
        Ok(format!("Atalho '{keys}' enviado nativamente."))
    }
    #[cfg(not(windows))]
    {
        let _ = keys;
        Err("Atalhos globais disponíveis apenas no Windows.".to_string())
    }
}

fn resolve_user_path(raw: &str) -> Result<std::path::PathBuf, String> {
    let trimmed = raw.trim().trim_matches('"');
    if trimmed.is_empty() {
        return Err("Caminho vazio.".to_string());
    }
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".to_string());

    let lower = trimmed.to_ascii_lowercase();
    if trimmed == "~" {
        return Ok(std::path::PathBuf::from(home));
    }
    if let Some(rest) = trimmed.strip_prefix("~/").or_else(|| trimmed.strip_prefix("~\\")) {
        return Ok(std::path::PathBuf::from(home).join(rest));
    }
    if lower == "desktop" || lower == "área de trabalho" || lower == "area de trabalho" {
        return Ok(std::path::PathBuf::from(home).join("Desktop"));
    }
    if let Some(rest) = trimmed
        .strip_prefix("Desktop\\")
        .or_else(|| trimmed.strip_prefix("Desktop/"))
        .or_else(|| trimmed.strip_prefix("desktop\\"))
        .or_else(|| trimmed.strip_prefix("desktop/"))
    {
        return Ok(std::path::PathBuf::from(home).join("Desktop").join(rest));
    }
    if lower == "documents" || lower == "documentos" {
        return Ok(std::path::PathBuf::from(home).join("Documents"));
    }
    if let Some(rest) = trimmed
        .strip_prefix("Documents\\")
        .or_else(|| trimmed.strip_prefix("Documents/"))
        .or_else(|| trimmed.strip_prefix("documents\\"))
        .or_else(|| trimmed.strip_prefix("documents/"))
    {
        return Ok(std::path::PathBuf::from(home).join("Documents").join(rest));
    }
    if lower == "downloads" {
        return Ok(std::path::PathBuf::from(home).join("Downloads"));
    }
    if let Some(rest) = trimmed
        .strip_prefix("Downloads\\")
        .or_else(|| trimmed.strip_prefix("Downloads/"))
        .or_else(|| trimmed.strip_prefix("downloads\\"))
        .or_else(|| trimmed.strip_prefix("downloads/"))
    {
        return Ok(std::path::PathBuf::from(home).join("Downloads").join(rest));
    }

    let expanded = trimmed.replace("%USERPROFILE%", &home).replace("%userprofile%", &home);
    Ok(std::path::PathBuf::from(expanded))
}

fn is_protected_write_path(path: &std::path::Path) -> Option<&'static str> {
    let norm = path
        .to_string_lossy()
        .replace('/', "\\")
        .to_ascii_lowercase();
    if norm.contains("\\windows\\system32") || norm.contains("\\windows\\syswow64") {
        return Some("diretório crítico do sistema Windows");
    }
    if norm.starts_with("c:\\windows\\") || norm == "c:\\windows" {
        return Some("pasta protegida C:\\Windows");
    }
    if norm.starts_with("c:\\program files\\") || norm.starts_with("c:\\program files (x86)\\") {
        return Some("pasta protegida de programas instalados");
    }
    None
}

fn tool_list_files(raw_path: &str, max_entries: usize) -> Result<String, String> {
    let dir = resolve_user_path(raw_path)?;
    if !dir.exists() {
        return Err(format!("Pasta não encontrada: {}", dir.display()));
    }
    if !dir.is_dir() {
        return Err(format!("O caminho não é uma pasta: {}", dir.display()));
    }
    let read = std::fs::read_dir(&dir)
        .map_err(|e| format!("Não foi possível abrir pasta '{}': {e}", dir.display()))?;

    let mut dirs = Vec::new();
    let mut files = Vec::new();
    for entry in read.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Ok(meta) = entry.metadata() {
            if meta.is_dir() {
                dirs.push(format!("[DIR]  {name}\\"));
            } else {
                let sz = meta.len();
                let sz_str = if sz >= 1_048_576 {
                    format!("{:.1} MB", sz as f64 / 1_048_576.0)
                } else if sz >= 1024 {
                    format!("{:.1} KB", sz as f64 / 1024.0)
                } else {
                    format!("{sz} B")
                };
                files.push(format!("[ARQ]  {name} ({sz_str})"));
            }
        }
    }
    dirs.sort_by_key(|s| s.to_ascii_lowercase());
    files.sort_by_key(|s| s.to_ascii_lowercase());
    let total = dirs.len() + files.len();
    let combined: Vec<String> = dirs.into_iter().chain(files).take(max_entries).collect();
    if combined.is_empty() {
        return Ok(format!("Pasta vazia: {}", dir.display()));
    }
    let mut out = format!(
        "Conteúdo de {} ({} de {} itens):\n",
        dir.display(),
        combined.len(),
        total
    );
    out.push_str(&combined.join("\n"));
    Ok(out)
}

fn tool_read_file(
    raw_path: &str,
    start_line: Option<usize>,
    end_line: Option<usize>,
) -> Result<String, String> {
    let path = resolve_user_path(raw_path)?;
    let meta = std::fs::metadata(&path)
        .map_err(|e| format!("Arquivo não encontrado '{}': {e}", path.display()))?;
    if meta.is_dir() {
        return Err("O caminho informado é uma pasta. Use list_files.".to_string());
    }
    if meta.len() > 2_500_000 {
        return Err("Arquivo muito grande (> 2.5 MB) para leitura direta completa.".to_string());
    }
    let raw = std::fs::read_to_string(&path)
        .map_err(|e| format!("Não foi possível ler '{}' como texto UTF-8: {e}", path.display()))?;

    let lines: Vec<&str> = raw.lines().collect();
    let total = lines.len();
    let start = start_line.unwrap_or(1).max(1);
    let max_end = (start + 399).min(total.max(1));
    let end = end_line.unwrap_or(max_end).min(total).min(start + 399);

    if total == 0 {
        return Ok(format!("Arquivo vazio: {}", path.display()));
    }
    if start > total {
        return Err(format!(
            "Linha inicial {start} excede o total de {total} linhas em {}.",
            path.display()
        ));
    }

    let mut out = format!(
        "Arquivo: {} (linhas {start}-{end} de {total})\n",
        path.display()
    );
    for idx in (start - 1)..end {
        out.push_str(&format!("{}: {}\n", idx + 1, lines[idx]));
        if out.len() > 24_000 {
            out.push_str("\n... (conteúdo truncado; use start_line/end_line para continuar)");
            break;
        }
    }
    Ok(out)
}

fn tool_write_file(raw_path: &str, content: &str, append: bool) -> Result<String, String> {
    let path = resolve_user_path(raw_path)?;
    if let Some(reason) = is_protected_write_path(&path) {
        return Err(format!(
            "Escrita bloqueada por segurança ({reason}): {}",
            path.display()
        ));
    }
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Falha ao criar pasta '{}': {e}", parent.display()))?;
        }
    }
    if append {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| format!("Falha ao abrir '{}': {e}", path.display()))?;
        f.write_all(content.as_bytes())
            .map_err(|e| format!("Falha ao gravar em '{}': {e}", path.display()))?;
        Ok(format!(
            "Conteúdo adicionado ao final de {} ({} bytes).",
            path.display(),
            content.len()
        ))
    } else {
        std::fs::write(&path, content.as_bytes())
            .map_err(|e| format!("Falha ao salvar '{}': {e}", path.display()))?;
        Ok(format!(
            "Arquivo salvo com sucesso: {} ({} bytes, {} linhas).",
            path.display(),
            content.len(),
            content.lines().count()
        ))
    }
}

fn tool_edit_file(
    raw_path: &str,
    old_text: &str,
    new_text: &str,
    replace_all: bool,
) -> Result<String, String> {
    if old_text.is_empty() {
        return Err("old_text não pode ser vazio.".to_string());
    }
    let path = resolve_user_path(raw_path)?;
    if let Some(reason) = is_protected_write_path(&path) {
        return Err(format!(
            "Edição bloqueada por segurança ({reason}): {}",
            path.display()
        ));
    }
    let current = std::fs::read_to_string(&path)
        .map_err(|e| format!("Falha ao ler '{}' para edição: {e}", path.display()))?;

    let matches = current.matches(old_text).count();
    if matches == 0 {
        return Err(format!(
            "O trecho exato informado em old_text não foi encontrado em {}. Leia o arquivo com read_file antes de editar.",
            path.display()
        ));
    }

    let (updated, replaced_count) = if replace_all {
        (current.replace(old_text, new_text), matches)
    } else {
        (current.replacen(old_text, new_text, 1), 1)
    };

    std::fs::write(&path, updated.as_bytes())
        .map_err(|e| format!("Falha ao gravar edição em '{}': {e}", path.display()))?;

    Ok(format!(
        "Arquivo editado com sucesso: {} ({replaced_count} substituição(ões) aplicada(s)).",
        path.display()
    ))
}

fn tool_search_files(
    raw_path: &str,
    query: &str,
    ext_filter: Option<&str>,
    max_results: usize,
) -> Result<String, String> {
    let q = query.trim();
    if q.is_empty() {
        return Err("Informe um termo de busca não vazio.".to_string());
    }
    let root = resolve_user_path(raw_path)?;
    if !root.exists() {
        return Err(format!("Caminho não encontrado: {}", root.display()));
    }
    let q_lower = q.to_ascii_lowercase();
    let ext_norm = ext_filter
        .map(|s| s.trim().trim_start_matches('.').to_ascii_lowercase())
        .filter(|s| !s.is_empty());

    let mut stack = vec![root.clone()];
    let mut hits = Vec::new();
    let mut scanned = 0usize;

    while let Some(dir) = stack.pop() {
        if hits.len() >= max_results || scanned > 2500 {
            break;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            if hits.len() >= max_results || scanned > 2500 {
                break;
            }
            let p = entry.path();
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if p.is_dir() {
                if matches!(
                    name.as_str(),
                    "node_modules" | ".git" | "target" | "dist" | ".venv" | "__pycache__" | "build"
                ) {
                    continue;
                }
                stack.push(p);
                continue;
            }
            if let Some(ref want_ext) = ext_norm {
                let got = p
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|s| s.to_ascii_lowercase())
                    .unwrap_or_default();
                if &got != want_ext {
                    continue;
                }
            }
            let Ok(meta) = entry.metadata() else { continue };
            if meta.len() > 600_000 {
                continue;
            }
            scanned += 1;
            let Ok(text) = std::fs::read_to_string(&p) else { continue };
            for (i, line) in text.lines().enumerate() {
                if line.to_ascii_lowercase().contains(&q_lower) {
                    let rel = p.strip_prefix(&root).unwrap_or(&p);
                    let preview: String = line.trim().chars().take(120).collect();
                    hits.push(format!("{}:{}: {}", rel.display(), i + 1, preview));
                    if hits.len() >= max_results {
                        break;
                    }
                }
            }
        }
    }

    if hits.is_empty() {
        Ok(format!(
            "Nenhuma ocorrência de '{}' encontrada em {} ({scanned} arquivos verificados).",
            q,
            root.display()
        ))
    } else {
        Ok(format!(
            "Encontradas {} ocorrência(s) de '{}' em {}:\n{}",
            hits.len(),
            q,
            root.display(),
            hits.join("\n")
        ))
    }
}

fn emit_file_diff(
    app: &AppHandle,
    conversation_id: &str,
    path: &std::path::Path,
    tool: &str,
    existed_before: bool,
    old_content: &str,
    new_content: &str,
) {
    const MAX_DIFF_CHARS: usize = 160_000;
    let old_trimmed: String = old_content.chars().take(MAX_DIFF_CHARS).collect();
    let new_trimmed: String = new_content.chars().take(MAX_DIFF_CHARS).collect();
    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("arquivo")
        .to_string();

    let _ = app.emit(
        "jarvis:file-diff",
        json!({
            "id": Uuid::new_v4().to_string(),
            "conversationId": conversation_id,
            "path": path.to_string_lossy().to_string(),
            "fileName": file_name,
            "tool": tool,
            "existedBefore": existed_before,
            "oldContent": old_trimmed,
            "newContent": new_trimmed,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }),
    );
}

pub fn revert_file_change_blocking(
    raw_path: &str,
    existed_before: bool,
    old_content: &str,
) -> Result<String, String> {
    let path = resolve_user_path(raw_path)?;
    if let Some(reason) = is_protected_write_path(&path) {
        return Err(format!(
            "Reversão bloqueada por segurança ({reason}): {}",
            path.display()
        ));
    }
    if existed_before {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                let _ = std::fs::create_dir_all(parent);
            }
        }
        std::fs::write(&path, old_content.as_bytes())
            .map_err(|e| format!("Falha ao restaurar '{}': {e}", path.display()))?;
        Ok(format!("Arquivo restaurado: {}", path.display()))
    } else if path.exists() {
        std::fs::remove_file(&path)
            .map_err(|e| format!("Falha ao remover arquivo criado '{}': {e}", path.display()))?;
        Ok(format!("Arquivo recém-criado removido: {}", path.display()))
    } else {
        Ok(format!("Arquivo já não existe: {}", path.display()))
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardContent {
    pub kind: String,
    pub text: Option<String>,
    pub image_base64: Option<String>,
    pub image_mime: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

fn dib_bytes_to_bgra(dib: &[u8]) -> Result<(u32, u32, Vec<u8>), String> {
    if dib.len() < 40 {
        return Err("Cabeçalho DIB inválido na área de transferência.".to_string());
    }
    let header_size = u32::from_le_bytes([dib[0], dib[1], dib[2], dib[3]]) as usize;
    let width_i32 = i32::from_le_bytes([dib[4], dib[5], dib[6], dib[7]]);
    let height_i32 = i32::from_le_bytes([dib[8], dib[9], dib[10], dib[11]]);
    let bpp = u16::from_le_bytes([dib[14], dib[15]]);
    let compression = u32::from_le_bytes([dib[16], dib[17], dib[18], dib[19]]);
    let clr_used = u32::from_le_bytes([dib[32], dib[33], dib[34], dib[35]]) as usize;

    if width_i32 <= 0 || height_i32 == 0 || width_i32 > 16_384 || height_i32.unsigned_abs() > 16_384 {
        return Err("Dimensões inválidas na imagem da área de transferência.".to_string());
    }
    if bpp != 24 && bpp != 32 {
        return Err(format!("Formato de cor DIB não suportado ({bpp} bpp)."));
    }

    let width = width_i32 as u32;
    let height = height_i32.unsigned_abs();
    let top_down = height_i32 < 0;

    // BI_BITFIELDS (3) adds 12 bytes of channel masks right after a 40-byte BITMAPINFOHEADER
    let extra_masks = if header_size == 40 && compression == 3 { 12 } else { 0 };
    let palette_bytes = clr_used * 4;
    let pixel_offset = header_size + extra_masks + palette_bytes;
    let src_stride = (((width as usize) * (bpp as usize) + 31) / 32) * 4;
    let needed = pixel_offset + src_stride * (height as usize);
    if dib.len() < needed {
        return Err("Dados de pixel incompletos na imagem da área de transferência.".to_string());
    }

    let mut bgra = vec![0u8; (width as usize) * (height as usize) * 4];
    for y in 0..(height as usize) {
        let src_y = if top_down { y } else { (height as usize) - 1 - y };
        let src_row = &dib[pixel_offset + src_y * src_stride..pixel_offset + (src_y + 1) * src_stride];
        let dst_row = &mut bgra[y * (width as usize) * 4..(y + 1) * (width as usize) * 4];
        if bpp == 32 {
            for x in 0..(width as usize) {
                dst_row[x * 4] = src_row[x * 4];
                dst_row[x * 4 + 1] = src_row[x * 4 + 1];
                dst_row[x * 4 + 2] = src_row[x * 4 + 2];
                dst_row[x * 4 + 3] = 255;
            }
        } else {
            for x in 0..(width as usize) {
                dst_row[x * 4] = src_row[x * 3];
                dst_row[x * 4 + 1] = src_row[x * 3 + 1];
                dst_row[x * 4 + 2] = src_row[x * 3 + 2];
                dst_row[x * 4 + 3] = 255;
            }
        }
    }

    Ok((width, height, bgra))
}

pub fn read_clipboard_blocking() -> Result<ClipboardContent, String> {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::HGLOBAL;
        use windows::Win32::System::DataExchange::{
            CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
        };
        use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};

        const CF_DIB: u32 = 8;
        const CF_UNICODETEXT: u32 = 13;

        unsafe {
            // Brief retry in case another app holds clipboard momentarily
            let mut opened = false;
            for _ in 0..4 {
                if OpenClipboard(None).is_ok() {
                    opened = true;
                    break;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            if !opened {
                return Err("Não foi possível abrir a área de transferência do Windows.".to_string());
            }

            let result = (|| -> Result<ClipboardContent, String> {
                // Prefer text when copied text is available, otherwise check image
                if IsClipboardFormatAvailable(CF_UNICODETEXT).is_ok() {
                    if let Ok(handle) = GetClipboardData(CF_UNICODETEXT) {
                        let hglobal = HGLOBAL(handle.0);
                        let size = GlobalSize(hglobal);
                        let ptr = GlobalLock(hglobal);
                        if !ptr.is_null() && size >= 2 {
                            let max_units = size / 2;
                            let slice = std::slice::from_raw_parts(ptr as *const u16, max_units);
                            let len = slice.iter().position(|&c| c == 0).unwrap_or(max_units);
                            let text = String::from_utf16_lossy(&slice[..len]);
                            let _ = GlobalUnlock(hglobal);
                            if !text.trim().is_empty() {
                                return Ok(ClipboardContent {
                                    kind: "text".to_string(),
                                    text: Some(text),
                                    image_base64: None,
                                    image_mime: None,
                                    width: None,
                                    height: None,
                                });
                            }
                        } else if !ptr.is_null() {
                            let _ = GlobalUnlock(hglobal);
                        }
                    }
                }

                if IsClipboardFormatAvailable(CF_DIB).is_ok() {
                    if let Ok(handle) = GetClipboardData(CF_DIB) {
                        let hglobal = HGLOBAL(handle.0);
                        let size = GlobalSize(hglobal);
                        let ptr = GlobalLock(hglobal);
                        if !ptr.is_null() && size >= 40 {
                            let bytes = std::slice::from_raw_parts(ptr as *const u8, size).to_vec();
                            let _ = GlobalUnlock(hglobal);
                            let (w, h, bgra) = dib_bytes_to_bgra(&bytes)?;
                            let (cw, ch, _, _, b64) =
                                encode_bgra_to_jpeg_base64(&bgra, w, h, (w as usize) * 4)?;
                            return Ok(ClipboardContent {
                                kind: "image".to_string(),
                                text: None,
                                image_base64: Some(b64),
                                image_mime: Some("image/jpeg".to_string()),
                                width: Some(cw),
                                height: Some(ch),
                            });
                        } else if !ptr.is_null() {
                            let _ = GlobalUnlock(hglobal);
                        }
                    }
                }

                Ok(ClipboardContent {
                    kind: "empty".to_string(),
                    text: None,
                    image_base64: None,
                    image_mime: None,
                    width: None,
                    height: None,
                })
            })();

            let _ = CloseClipboard();
            result
        }
    }
    #[cfg(not(windows))]
    {
        Ok(ClipboardContent {
            kind: "empty".to_string(),
            text: None,
            image_base64: None,
            image_mime: None,
            width: None,
            height: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_policy_respects_modes() {
        assert_eq!(
            check_permission_policy(PermissionMode::Auto, ToolRisk::Action, "open_url", true),
            PermissionDecision::Allow
        );
        assert_eq!(
            check_permission_policy(PermissionMode::Review, ToolRisk::Read, "get_system_status", true),
            PermissionDecision::Allow
        );
        assert_eq!(
            check_permission_policy(PermissionMode::Review, ToolRisk::Action, "run_command", true),
            PermissionDecision::NeedApproval
        );
        assert!(matches!(
            check_permission_policy(PermissionMode::Safe, ToolRisk::Action, "open_url", true),
            PermissionDecision::Deny(_)
        ));
        assert!(matches!(
            check_permission_policy(PermissionMode::Safe, ToolRisk::Read, "get_system_status", true),
            PermissionDecision::Deny(_)
        ));
        assert!(matches!(
            check_permission_policy(PermissionMode::Auto, ToolRisk::Read, "web_search", false),
            PermissionDecision::Deny(_)
        ));
    }

    #[test]
    fn dangerous_commands_and_urls_are_validated() {
        assert!(is_dangerous_command("Get-Date").is_none());
        assert!(is_dangerous_command("Remove-Item -Recurse C:\\Windows\\System32").is_some());
        assert!(is_dangerous_command("diskpart").is_some());
        assert!(is_dangerous_command("shutdown /s /t 0").is_some());

        assert!(validate_http_url("https://openai.com").is_ok());
        assert!(validate_http_url("file:///C:/Windows").is_err());
        assert!(validate_http_url("javascript:alert(1)").is_err());
    }

    #[test]
    fn duckduckgo_html_parser_extracts_results() {
        let html = r#"
          <div class="result">
            <a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Frust-lang.org%2F&amp;rut=1">Rust &amp; Friends</a>
            <a class="result__snippet">A language empowering everyone to build reliable software.</a>
          </div>
        "#;
        let parsed = parse_duckduckgo_html(html);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].0, "Rust & Friends");
        assert_eq!(parsed[0].1, "https://rust-lang.org/");
        assert!(parsed[0].2.contains("reliable software"));
    }

    #[test]
    fn system_status_and_datetime_return_data() {
        let dt = format_datetime();
        assert!(dt.contains("Hora local:"));
        let sys = read_system_status_blocking();
        assert!(sys.contains("CPU:"));
        assert!(sys.contains("RAM:"));
    }

    #[test]
    fn responses_tool_declarations_use_flat_schema() {
        let tools = responses_tool_declarations(PermissionMode::Auto, true);
        assert!(!tools.is_empty());
        for t in &tools {
            assert_eq!(t["type"].as_str(), Some("function"));
            assert!(t["name"].is_string());
            assert!(t["parameters"].is_object());
            assert!(t.get("function").is_none());
        }
    }

    #[test]
    fn file_tools_write_read_edit_search_and_protect_system() {
        let dir = std::env::temp_dir().join(format!("jarvis-file-test-{}", Uuid::new_v4()));
        let file = dir.join("demo.txt");
        let path_str = file.to_string_lossy().to_string();
        let dir_str = dir.to_string_lossy().to_string();

        tool_write_file(&path_str, "linha 1\nolá mundo\nlinha 3\n", false).unwrap();
        let read = tool_read_file(&path_str, Some(1), Some(10)).unwrap();
        assert!(read.contains("2: olá mundo"));

        tool_edit_file(&path_str, "olá mundo", "olá J.A.R.V.I.S.", false).unwrap();
        let edited = tool_read_file(&path_str, None, None).unwrap();
        assert!(edited.contains("olá J.A.R.V.I.S."));

        let found = tool_search_files(&dir_str, "j.a.r.v.i.s.", Some("txt"), 10).unwrap();
        assert!(found.contains("demo.txt:2:"));

        let listed = tool_list_files(&dir_str, 20).unwrap();
        assert!(listed.contains("demo.txt"));

        assert!(is_protected_write_path(std::path::Path::new(r"C:\Windows\System32\test.txt")).is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn native_screen_capture_and_hotkey_parser() {
        let vks = parse_hotkey_vks("Ctrl+Shift+T").unwrap();
        assert_eq!(vks, vec![0x11, 0x10, b'T' as u16]);
        assert_eq!(parse_hotkey_vks("^c").unwrap(), vec![0x11, b'C' as u16]);

        #[cfg(windows)]
        {
            use base64::Engine as _;
            // Verify pure-Rust BGRA -> 1280px downscale -> JPEG encoding pipeline
            let w = 1920u32;
            let h = 1080u32;
            let mut bgra = vec![0u8; (w * h * 4) as usize];
            for (i, px) in bgra.chunks_exact_mut(4).enumerate() {
                px[0] = (i % 255) as u8;
                px[1] = 180;
                px[2] = 40;
                px[3] = 255;
            }
            let (cw, ch, rw, rh, b64) =
                encode_bgra_to_jpeg_base64(&bgra, w, h, (w * 4) as usize).unwrap();
            assert_eq!((cw, ch, rw, rh), (1280, 720, 1920, 1080));
            let bytes = base64::engine::general_purpose::STANDARD.decode(&b64).unwrap();
            assert_eq!(&bytes[..3], &[0xFF, 0xD8, 0xFF]);

            // On an interactive unlocked desktop, also verify live DXGI/GDI capture
            if let Ok((lw, lh, lrw, lrh, lb64)) = capture_desktop_screen_blocking() {
                assert!(lw > 0 && lh > 0 && lrw >= lw && lrh >= lh);
                assert!(lb64.len() > 1000);
            }
        }
    }

    #[test]
    fn target_window_matcher_understands_apps_and_games() {
        let wins = vec![
            OpenWindowInfo {
                hwnd: 1,
                pid: 100,
                process_name: "firefox.exe".to_string(),
                title: "YouTube — Mozilla Firefox".to_string(),
                minimized: false,
                width: 1920,
                height: 1040,
            },
            OpenWindowInfo {
                hwnd: 2,
                pid: 200,
                process_name: "Code.exe".to_string(),
                title: "Jarvis-Cinematic - Visual Studio Code".to_string(),
                minimized: false,
                width: 1440,
                height: 900,
            },
            OpenWindowInfo {
                hwnd: 3,
                pid: 300,
                process_name: "javaw.exe".to_string(),
                title: "Minecraft 1.21.4 - Multiplayer".to_string(),
                minimized: false,
                width: 1920,
                height: 1080,
            },
        ];

        assert_eq!(
            match_target_window("meu jogo", &wins).map(|w| w.hwnd),
            Some(3)
        );
        assert_eq!(
            match_target_window("Minecraft", &wins).map(|w| w.hwnd),
            Some(3)
        );
        assert_eq!(
            match_target_window("navegador", &wins).map(|w| w.hwnd),
            Some(1)
        );
        assert_eq!(
            match_target_window("vs code", &wins).map(|w| w.hwnd),
            Some(2)
        );

        // Even an unknown indie game should be selected for "jogo" over Firefox/VS Code
        let indie = vec![
            wins[0].clone(),
            wins[1].clone(),
            OpenWindowInfo {
                hwnd: 4,
                pid: 400,
                process_name: "MeuIndieLegal.exe".to_string(),
                title: "Aventura Espacial".to_string(),
                minimized: false,
                width: 1600,
                height: 900,
            },
        ];
        assert_eq!(match_target_window("jogo", &indie).map(|w| w.hwnd), Some(4));
    }

    #[test]
    fn clipboard_dib_decoder_and_file_revert_work() {
        // Build a tiny 2x2 24-bit bottom-up DIB (stride = ((2*24+31)/32)*4 = 8 bytes per row)
        let mut dib = vec![0u8; 40 + 16];
        dib[0..4].copy_from_slice(&40u32.to_le_bytes()); // header size
        dib[4..8].copy_from_slice(&2i32.to_le_bytes()); // width = 2
        dib[8..12].copy_from_slice(&2i32.to_le_bytes()); // height = 2 (bottom-up)
        dib[12..14].copy_from_slice(&1u16.to_le_bytes()); // planes = 1
        dib[14..16].copy_from_slice(&24u16.to_le_bytes()); // 24 bpp
        // Bottom row (y=1 in top-down) -> blue pixel
        dib[40..43].copy_from_slice(&[255, 10, 20]);
        // Top row (y=0 in top-down) -> red pixel
        dib[48..51].copy_from_slice(&[5, 15, 250]);

        let (w, h, bgra) = dib_bytes_to_bgra(&dib).unwrap();
        assert_eq!((w, h), (2, 2));
        // Top-left pixel should be the red pixel from src_y = 1
        assert_eq!(&bgra[0..4], &[5, 15, 250, 255]);
        // Bottom-left pixel should be the blue pixel from src_y = 0
        assert_eq!(&bgra[8..12], &[255, 10, 20, 255]);

        // Verify revert_file_change_blocking for both edited & newly-created files
        let dir = std::env::temp_dir().join(format!("jarvis-revert-test-{}", Uuid::new_v4()));
        let f1 = dir.join("edited.txt");
        let f1_str = f1.to_string_lossy().to_string();
        tool_write_file(&f1_str, "novo", false).unwrap();
        revert_file_change_blocking(&f1_str, true, "antigo").unwrap();
        assert_eq!(std::fs::read_to_string(&f1).unwrap(), "antigo");

        revert_file_change_blocking(&f1_str, false, "").unwrap();
        assert!(!f1.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
