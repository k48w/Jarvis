//! # Catálogo e Executor de Ferramentas — `tools.rs`
//!
//! Define todas as ferramentas disponíveis para os modelos ChatGPT e Gemini
//! (`tool_catalog`), a política de permissões (`Auto`, `Review`, `Safe`) com
//! aprovação 2FA (`wait_for_user_approval`), a proteção contra SSRF
//! (`validate_public_http_url`) e as implementações nativas em Rust/Win32
//! (captura de tela DXGI/PrintWindow, automação `SendInput`, áudio CoreAudio,
//! arquivos com snapshot para undo, Git, Terminal, Rotinas e Base RAG).
//!
//! ## Como adicionar uma nova ferramenta
//! 1. Declare um item `ToolSpec` em [`tool_catalog`] com nome, risco (`ToolRisk`) e schema JSON.
//! 2. Adicione o resumo curto de argumentos em `summarize_tool_args`.
//! 3. Implemente o braço correspondente no `match name` dentro de `execute_tool`.

use crate::browser;
use crate::config::PermissionMode;
use crate::knowledge;
use crate::state::AppState;
use crate::storage;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::sync::oneshot;
use uuid::Uuid;

mod files;
mod screen;
mod system;
mod web;

#[cfg(test)]
use files::is_protected_write_path;
use files::{
    emit_file_diff, resolve_user_path, tool_edit_file, tool_list_files, tool_read_file,
    tool_search_files, tool_write_file,
};
pub use files::{
    git_commit_blocking, git_repo_status_blocking, revert_file_change_blocking, tool_git_diff,
    tool_git_log, GitRepoStatus,
};
pub(crate) use screen::{
    acquire_screen_capture_guard, capture_target_window_blocking, list_open_windows_info_blocking,
};
pub use screen::{capture_desktop_screen_blocking, read_clipboard_blocking, ClipboardContent};
#[cfg(test)]
use screen::{
    dib_bytes_to_bgra, encode_bgra_to_jpeg_base64, match_target_window, parse_hotkey_vks,
    OpenWindowInfo,
};
use screen::{screen_click_blocking, screen_hotkey_blocking, screen_type_blocking};
pub use system::is_dangerous_command;
use system::{format_datetime, launch_application, read_system_status_blocking, run_powershell};
pub(crate) use system::{run_set_timer, send_media_key, set_windows_volume_blocking};
#[cfg(test)]
use web::{is_blocked_ip, parse_duckduckgo_html, validate_http_url};
use web::{run_fetch_webpage, run_web_search};
pub use web::{safe_fetch_url, validate_public_http_url};

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
        ToolSpec {
            name: "git_status",
            title: "Ver status Git do repositório",
            description: "Mostra a branch atual, commits à frente/atrás e arquivos modificados ou não rastreados em um repositório Git local.",
            risk: ToolRisk::Read,
            parameters: || json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Pasta do repositório Git (obrigatório se não estiver implícito no projeto atual)." }
                },
                "required": ["path"]
            }),
        },
        ToolSpec {
            name: "git_diff",
            title: "Ver diff Git do repositório",
            description: "Mostra as alterações linha a linha (git diff) no repositório local, opcionalmente filtradas por arquivo ou apenas staged.",
            risk: ToolRisk::Read,
            parameters: || json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Pasta do repositório Git." },
                    "file": { "type": "string", "description": "Arquivo específico opcional dentro do repositório." },
                    "staged": { "type": "boolean", "description": "Se true, mostra apenas alterações staged (--cached)." }
                },
                "required": ["path"]
            }),
        },
        ToolSpec {
            name: "git_log",
            title: "Ver histórico de commits Git",
            description: "Lista os últimos commits do repositório Git com hash, data relativa, autor e mensagem.",
            risk: ToolRisk::Read,
            parameters: || json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Pasta do repositório Git." },
                    "limit": { "type": "integer", "description": "Quantidade de commits (padrão 10, máx 30)." }
                },
                "required": ["path"]
            }),
        },
        ToolSpec {
            name: "git_commit",
            title: "Fazer commit Git",
            description: "Adiciona alterações (git add) e cria um commit local com a mensagem informada. Não faz push automaticamente.",
            risk: ToolRisk::Action,
            parameters: || json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Pasta do repositório Git." },
                    "message": { "type": "string", "description": "Mensagem clara do commit." }
                },
                "required": ["path", "message"]
            }),
        },
        ToolSpec {
            name: "run_terminal",
            title: "Executar no Terminal ao Vivo",
            description: "Executa um comando PowerShell com streaming ao vivo na aba Terminal (ideal para npm test, cargo build, pytest, scripts ou builds de até 180s).",
            risk: ToolRisk::Action,
            parameters: || json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "Comando PowerShell a executar." },
                    "cwd": { "type": "string", "description": "Pasta de trabalho opcional." },
                    "timeout_secs": { "type": "integer", "description": "Tempo máximo em segundos (padrão 60, máx 180)." }
                },
                "required": ["command"]
            }),
        },
        ToolSpec {
            name: "create_skill",
            title: "Criar ou atualizar Skill (/comando)",
            description: "Salva uma macro/skill reutilizável acionável por /comando no chat (ex: /modo-jogo, /deploy, /bomdia).",
            risk: ToolRisk::Read,
            parameters: || json!({
                "type": "object",
                "properties": {
                    "slug": { "type": "string", "description": "Comando sem espaços (ex: modo-jogo)." },
                    "name": { "type": "string", "description": "Nome amigável da skill." },
                    "description": { "type": "string", "description": "Descrição curta." },
                    "prompt": { "type": "string", "description": "Instrução completa que o J.A.R.V.I.S. executará." }
                },
                "required": ["slug", "prompt"]
            }),
        },
        ToolSpec {
            name: "list_skills",
            title: "Listar Skills (/comandos)",
            description: "Lista todas as skills/macros (/comando) disponíveis.",
            risk: ToolRisk::Read,
            parameters: || json!({ "type": "object", "properties": {} }),
        },
        ToolSpec {
            name: "create_routine",
            title: "Criar Rotina Proativa Agendada",
            description: "Agenda uma rotina recorrente (diária em HH:MM ou a cada X minutos) para o J.A.R.V.I.S. executar automaticamente.",
            risk: ToolRisk::Read,
            parameters: || json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Nome curto da rotina (ex: Briefing Matinal)." },
                    "schedule_kind": { "type": "string", "enum": ["daily", "interval"], "description": "Tipo: daily (horário fixo HH:MM) ou interval (a cada X minutos)." },
                    "daily_hhmm": { "type": "string", "description": "Horário diário HH:MM (ex: 09:00) quando schedule_kind='daily'." },
                    "interval_minutes": { "type": "integer", "description": "Intervalo em minutos (1..1440) quando schedule_kind='interval'." },
                    "prompt": { "type": "string", "description": "Instrução que o J.A.R.V.I.S. executará automaticamente." }
                },
                "required": ["name", "schedule_kind", "prompt"]
            }),
        },
        ToolSpec {
            name: "list_routines",
            title: "Listar Rotinas Agendadas",
            description: "Lista todas as rotinas proativas agendadas (ativas e pausadas).",
            risk: ToolRisk::Read,
            parameters: || json!({ "type": "object", "properties": {} }),
        },
        ToolSpec {
            name: "delete_routine",
            title: "Remover Rotina Agendada",
            description: "Remove uma rotina agendada pelo ID.",
            risk: ToolRisk::Read,
            parameters: || json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "ID da rotina." }
                },
                "required": ["id"]
            }),
        },
        ToolSpec {
            name: "kb_index_path",
            title: "Indexar na Base de Conhecimento (RAG)",
            description: "Indexa uma pasta de projeto, documentação ou arquivo local na Base de Conhecimento (RAG) em blocos com numeração de linhas para buscas semânticas rápidas.",
            risk: ToolRisk::Read,
            parameters: || json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Caminho absoluto da pasta ou arquivo local para indexar." },
                    "name": { "type": "string", "description": "Nome amigável opcional para identificar esta fonte." }
                },
                "required": ["path"]
            }),
        },
        ToolSpec {
            name: "kb_search",
            title: "Pesquisar na Base de Conhecimento (RAG)",
            description: "Busca os trechos mais relevantes (BM25 + frase/identificador) nos documentos e códigos indexados na Base de Conhecimento local, retornando arquivo e linhas exatas.",
            risk: ToolRisk::Read,
            parameters: || json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Termos, pergunta técnica, nome de função ou erro a pesquisar." },
                    "limit": { "type": "integer", "description": "Quantidade máxima de trechos (padrão 5, máx 15)." },
                    "source": { "type": "string", "description": "Filtro opcional por nome, caminho ou ID da fonte." }
                },
                "required": ["query"]
            }),
        },
        ToolSpec {
            name: "kb_list",
            title: "Listar Fontes da Base de Conhecimento",
            description: "Lista todas as pastas e arquivos indexados na Base de Conhecimento local (RAG) com contagem de arquivos e blocos.",
            risk: ToolRisk::Read,
            parameters: || json!({ "type": "object", "properties": {} }),
        },
        ToolSpec {
            name: "kb_remove",
            title: "Remover Fonte da Base de Conhecimento",
            description: "Remove uma pasta ou arquivo indexado da Base de Conhecimento local pelo ID ou caminho.",
            risk: ToolRisk::Read,
            parameters: || json!({
                "type": "object",
                "properties": {
                    "id_or_path": { "type": "string", "description": "ID da fonte (ex: kb-...) ou caminho indexado." }
                },
                "required": ["id_or_path"]
            }),
        },
    ]
}

pub fn find_spec(name: &str) -> Option<ToolSpec> {
    tool_catalog().into_iter().find(|t| t.name == name)
}

#[cfg(test)]
pub fn responses_tool_declarations(mode: PermissionMode, web_search_enabled: bool) -> Vec<Value> {
    responses_tool_declarations_with_mcp(mode, web_search_enabled, &[])
}

pub fn responses_tool_declarations_with_mcp(
    mode: PermissionMode,
    web_search_enabled: bool,
    mcp_tools: &[crate::mcp::McpDiscoveredTool],
) -> Vec<Value> {
    let mut out: Vec<Value> = tool_catalog()
        .into_iter()
        .filter(|spec| {
            !matches!(
                check_permission_policy(mode, spec.risk, spec.name, web_search_enabled),
                PermissionDecision::Deny(_)
            )
        })
        .map(|spec| {
            json!({
                "type": "function",
                "name": spec.name,
                "description": spec.description,
                "parameters": (spec.parameters)()
            })
        })
        .collect();
    if mode != PermissionMode::Safe {
        for mt in mcp_tools {
            out.push(json!({
                "type": "function",
                "name": mt.full_name,
                "description": mt.description,
                "parameters": mt.input_schema
            }));
        }
    }
    out
}

pub fn openai_chat_tool_declarations(
    mode: PermissionMode,
    web_search_enabled: bool,
    mcp_tools: &[crate::mcp::McpDiscoveredTool],
) -> Vec<Value> {
    let mut out: Vec<Value> = tool_catalog()
        .into_iter()
        .filter(|spec| {
            !matches!(
                check_permission_policy(mode, spec.risk, spec.name, web_search_enabled),
                PermissionDecision::Deny(_)
            )
        })
        .map(|spec| {
            json!({
                "type": "function",
                "function": {
                    "name": spec.name,
                    "description": spec.description,
                    "parameters": (spec.parameters)()
                }
            })
        })
        .collect();
    if mode != PermissionMode::Safe {
        for mt in mcp_tools {
            out.push(json!({
                "type": "function",
                "function": {
                    "name": mt.full_name,
                    "description": mt.description,
                    "parameters": mt.input_schema
                }
            }));
        }
    }
    out
}

pub fn gemini_tool_declarations_with_mcp(
    mode: PermissionMode,
    web_search_enabled: bool,
    mcp_tools: &[crate::mcp::McpDiscoveredTool],
) -> Vec<Value> {
    let mut decls: Vec<Value> = tool_catalog()
        .into_iter()
        .filter(|spec| {
            !matches!(
                check_permission_policy(mode, spec.risk, spec.name, web_search_enabled),
                PermissionDecision::Deny(_)
            )
        })
        .map(|spec| {
            json!({
                "name": spec.name,
                "description": spec.description,
                "parameters": (spec.parameters)()
            })
        })
        .collect();
    if mode != PermissionMode::Safe {
        for mt in mcp_tools {
            decls.push(json!({
                "name": mt.full_name,
                "description": mt.description,
                "parameters": mt.input_schema
            }));
        }
    }
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
            "get_datetime" | "web_search" | "fetch_webpage" | "save_memory" | "list_timers" | "check_agents" => {
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
                parts.push(if m {
                    "silenciar".into()
                } else {
                    "reativar som".into()
                });
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
        "browser_type" => args["text"]
            .as_str()
            .unwrap_or("")
            .chars()
            .take(40)
            .collect(),
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
        "screen_type" => args["text"]
            .as_str()
            .unwrap_or("")
            .chars()
            .take(36)
            .collect(),
        "screen_hotkey" => args["keys"].as_str().unwrap_or("").to_string(),
        "list_files" | "read_file" | "write_file" | "edit_file" => {
            args["path"].as_str().unwrap_or("").to_string()
        }
        "search_files" => format!(
            "{} em {}",
            args["query"].as_str().unwrap_or(""),
            args["path"].as_str().unwrap_or("")
        ),
        "git_status" | "git_diff" | "git_log" => args["path"].as_str().unwrap_or("").to_string(),
        "git_commit" => args["message"]
            .as_str()
            .unwrap_or("")
            .chars()
            .take(48)
            .collect(),
        "run_terminal" => args["command"]
            .as_str()
            .unwrap_or("")
            .chars()
            .take(54)
            .collect(),
        "kb_index_path" => args["path"].as_str().unwrap_or("").to_string(),
        "kb_search" => args["query"].as_str().unwrap_or("").to_string(),
        "kb_remove" => args["id_or_path"].as_str().unwrap_or("").to_string(),
        _ => String::new(),
    }
}

async fn wait_for_user_approval_fields(
    app: &AppHandle,
    state: &Arc<AppState>,
    conversation_id: &str,
    tool_name: &str,
    title: &str,
    description: &str,
    args_summary: &str,
) -> bool {
    let approval_id = Uuid::new_v4().to_string();
    let (tx, rx) = oneshot::channel::<bool>();
    let req = ToolApprovalRequest {
        approval_id: approval_id.clone(),
        conversation_id: conversation_id.to_string(),
        tool_name: tool_name.to_string(),
        title: title.to_string(),
        description: description.to_string(),
        args_summary: args_summary.to_string(),
    };
    let req_val = serde_json::to_value(&req).unwrap_or_default();
    let generation = state
        .active_cancel
        .lock()
        .await
        .as_ref()
        .map(|(generation, _)| *generation)
        .unwrap_or(0);
    state
        .register_approval_for_generation(approval_id.clone(), req_val, tx, generation)
        .await;
    let _ = app.emit("jarvis:tool-approval-request", req);

    match tokio::time::timeout(Duration::from_secs(120), rx).await {
        Ok(Ok(val)) => val,
        _ => {
            if state.clear_approval(&approval_id).await {
                let _ = app.emit(
                    "jarvis:approval-resolved",
                    json!({ "approvalId": approval_id, "approved": false, "reason": "timeout" }),
                );
            }
            false
        }
    }
}

async fn wait_for_user_approval(
    app: &AppHandle,
    state: &Arc<AppState>,
    conversation_id: &str,
    spec: &ToolSpec,
    args_summary: &str,
) -> bool {
    wait_for_user_approval_fields(
        app,
        state,
        conversation_id,
        spec.name,
        spec.title,
        spec.description,
        args_summary,
    )
    .await
}

pub async fn execute_tool_with_policy(
    app: &AppHandle,
    state: &Arc<AppState>,
    conversation_id: &str,
    tool_id: &str,
    name: &str,
    args: &Value,
) -> ToolRunOutcome {
    // Roteamento dinâmico para ferramentas MCP externas (`mcp__<servidor>__<ferramenta>`)
    if name.starts_with("mcp__") {
        let (mode, web_enabled) = {
            let cfg = state.config.read().await;
            (cfg.permission_mode, cfg.web_search_enabled)
        };
        let title = format!("MCP: {}", name.trim_start_matches("mcp__"));
        let args_summary: String = args.to_string().chars().take(72).collect();
        match check_permission_policy(mode, ToolRisk::Action, name, web_enabled) {
            PermissionDecision::Allow => {}
            PermissionDecision::Deny(reason) => {
                let _ = app.emit(
                    "jarvis:chat-event",
                    json!({
                        "type": "tool_end",
                        "conversationId": conversation_id,
                        "id": tool_id,
                        "name": name,
                        "title": title,
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
                        "name": name,
                        "title": title,
                        "status": "waiting_approval",
                        "summary": args_summary
                    }),
                );
                let approved = wait_for_user_approval_fields(
                    app,
                    state,
                    conversation_id,
                    name,
                    &title,
                    "Executar ferramenta externa em servidor MCP configurado.",
                    &args_summary,
                )
                .await;
                if !approved {
                    let msg = "Execução MCP recusada pelo usuário.".to_string();
                    let _ = app.emit(
                        "jarvis:chat-event",
                        json!({
                            "type": "tool_end",
                            "conversationId": conversation_id,
                            "id": tool_id,
                            "name": name,
                            "title": title,
                            "status": "denied",
                            "summary": msg
                        }),
                    );
                    return ToolRunOutcome {
                        output: msg.clone(),
                        status: "denied".to_string(),
                        summary: msg,
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
                "name": name,
                "title": title,
                "status": "running",
                "summary": args_summary
            }),
        );
        let res = crate::mcp::call_discovered_tool(
            &state.http,
            &state.data_dir,
            &state.mcp_cache,
            name,
            args,
        )
        .await;
        let (status, output, summary) = match res {
            Ok(out) => {
                let s: String = out
                    .lines()
                    .next()
                    .unwrap_or("OK")
                    .chars()
                    .take(80)
                    .collect();
                ("ok".to_string(), out, s)
            }
            Err(err) => ("error".to_string(), err.clone(), err),
        };
        let _ = app.emit(
            "jarvis:chat-event",
            json!({
                "type": "tool_end",
                "conversationId": conversation_id,
                "id": tool_id,
                "name": name,
                "title": title,
                "status": status,
                "summary": summary
            }),
        );
        return ToolRunOutcome {
            output,
            status,
            summary,
            triggered_alert: false,
            attached_image: None,
        };
    }

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
            match validate_public_http_url(raw_url).await {
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
                let _ = app.emit("jarvis:browser-panel", json!({ "open": true, "url": url }));
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
                browser::type_text(
                    app,
                    text,
                    element_id,
                    target_label,
                    clear_first,
                    press_enter,
                )
                .await
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
                Ok(
                    "Subagente não pode criar outros subagentes; conclua a tarefa diretamente."
                        .to_string(),
                )
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
        "list_open_windows" => tokio::task::spawn_blocking(|| {
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
        .and_then(|r| r),
        "see_screen" => {
            let _ = app.emit("jarvis:screen-pulse", json!({ "kind": "capture" }));
            let target_window = args["target_window"]
                .as_str()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);

            // Serialized RAII guard guarantees concurrent captures never expose J.A.R.V.I.S.
            let _capture_guard = acquire_screen_capture_guard(app).await;

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
            drop(_capture_guard);

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
            tokio::task::spawn_blocking(move || screen_click_blocking(x, y, &button, double_click))
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
                    emit_file_diff(
                        app,
                        conversation_id,
                        &resolved,
                        "write_file",
                        existed,
                        &prev_text,
                        &next_text,
                    );
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
                    emit_file_diff(
                        app,
                        conversation_id,
                        &resolved,
                        "edit_file",
                        true,
                        &prev_text,
                        &next_text,
                    );
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
        "git_status" => {
            let raw_path = args["path"].as_str().unwrap_or(".").to_string();
            tokio::task::spawn_blocking(move || {
                let st = git_repo_status_blocking(&raw_path)?;
                if !st.is_git_repo {
                    return Ok(format!("A pasta '{}' não é um repositório Git.", st.path));
                }
                let mut lines = vec![format!(
                    "Repositório: {}\nBranch: {} (ahead {}, behind {})",
                    st.path, st.branch, st.ahead, st.behind
                )];
                if st.changes.is_empty() {
                    lines.push("Árvore limpa — nenhuma alteração pendente.".to_string());
                } else {
                    lines.push(format!("Alterações ({}):", st.changes.len()));
                    for c in st.changes.iter().take(40) {
                        lines.push(format!("  [{}] {}", c.status, c.path));
                    }
                }
                Ok(lines.join("\n"))
            })
            .await
            .map_err(|e| format!("Falha ao consultar git status: {e}"))
            .and_then(|r| r)
        }
        "git_diff" => {
            let raw_path = args["path"].as_str().unwrap_or(".").to_string();
            let file = args["file"].as_str().map(str::to_string);
            let staged = args["staged"].as_bool().unwrap_or(false);
            tokio::task::spawn_blocking(move || tool_git_diff(&raw_path, file.as_deref(), staged))
                .await
                .map_err(|e| format!("Falha ao obter git diff: {e}"))
                .and_then(|r| r)
        }
        "git_log" => {
            let raw_path = args["path"].as_str().unwrap_or(".").to_string();
            let limit = args["limit"].as_u64().unwrap_or(10).clamp(1, 30) as usize;
            tokio::task::spawn_blocking(move || tool_git_log(&raw_path, limit))
                .await
                .map_err(|e| format!("Falha ao obter git log: {e}"))
                .and_then(|r| r)
        }
        "git_commit" => {
            let raw_path = args["path"].as_str().unwrap_or(".").to_string();
            let msg = args["message"].as_str().unwrap_or("").to_string();
            tokio::task::spawn_blocking(move || git_commit_blocking(&raw_path, &msg))
                .await
                .map_err(|e| format!("Falha ao executar git commit: {e}"))
                .and_then(|r| r)
        }
        "run_terminal" => {
            use tauri::Manager;
            let cmd = args["command"].as_str().unwrap_or("");
            let cwd = args["cwd"].as_str();
            let timeout = args["timeout_secs"].as_u64().unwrap_or(60);
            let ctrl = app.state::<Arc<crate::terminal::TerminalController>>();
            crate::terminal::run_and_collect_streaming(app, &ctrl, cmd, cwd, timeout).await
        }
        "create_skill" => {
            let slug = args["slug"].as_str().unwrap_or("");
            let sname = args["name"].as_str().unwrap_or(slug);
            let desc = args["description"].as_str().unwrap_or("");
            let prompt = args["prompt"].as_str().unwrap_or("");
            crate::routines::save_skill(&state.data_dir, slug, sname, desc, prompt)
                .map(|s| format!("Skill '/{}' ({}) salva com sucesso.", s.slug, s.name))
        }
        "list_skills" => crate::routines::list_skills(&state.data_dir).map(|list| {
            list.into_iter()
                .map(|s| format!("- /{} ({}): {}", s.slug, s.name, s.description))
                .collect::<Vec<_>>()
                .join("\n")
        }),
        "create_routine" => {
            let rname = args["name"].as_str().unwrap_or("");
            let kind = args["schedule_kind"].as_str().unwrap_or("interval");
            let interval_min = args["interval_minutes"].as_u64().map(|v| v as u32);
            let hhmm = args["daily_hhmm"].as_str();
            let prompt = args["prompt"].as_str().unwrap_or("");
            crate::routines::create_routine(
                &state.data_dir,
                rname,
                kind,
                interval_min,
                hhmm,
                prompt,
            )
            .map(|r| {
                let sched = if r.schedule_kind == "daily" {
                    format!("todo dia às {}", r.daily_hhmm.unwrap_or_default())
                } else {
                    format!("a cada {} min", r.interval_minutes.unwrap_or(60))
                };
                format!("Rotina '{}' agendada ({sched}, id: {}).", r.name, r.id)
            })
        }
        "list_routines" => crate::routines::list_routines(&state.data_dir).map(|list| {
            if list.is_empty() {
                "Nenhuma rotina agendada.".to_string()
            } else {
                list.into_iter()
                    .map(|r| {
                        let sched = if r.schedule_kind == "daily" {
                            format!("diária {}", r.daily_hhmm.unwrap_or_default())
                        } else {
                            format!("a cada {} min", r.interval_minutes.unwrap_or(60))
                        };
                        let st = if r.enabled { "ativa" } else { "pausada" };
                        format!(
                            "- [{}] {} ({sched}, id: {}): {}",
                            st, r.name, r.id, r.prompt
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        }),
        "delete_routine" => {
            let id = args["id"].as_str().unwrap_or("");
            crate::routines::delete_routine(&state.data_dir, id).map(|ok| {
                if ok {
                    "Rotina removida.".to_string()
                } else {
                    "Rotina não encontrada.".to_string()
                }
            })
        }
        "kb_index_path" => {
            let raw_path = args["path"].as_str().unwrap_or("");
            let custom_name = args["name"].as_str().map(|s| s.to_string());
            let data_dir = state.data_dir.clone();
            let path_owned = raw_path.to_string();
            tokio::task::spawn_blocking(move || {
                knowledge::index_path(&data_dir, &path_owned, custom_name)
            })
            .await
            .map_err(|e| format!("Falha na tarefa de indexação RAG: {e}"))
            .and_then(|res| {
                res.map(|sum| {
                    format!(
                        "Fonte '{}' indexada na Base de Conhecimento (id: {}, {} arquivo(s), {} blocos/chunks).",
                        sum.name, sum.id, sum.file_count, sum.chunk_count
                    )
                })
            })
        }
        "kb_search" => {
            let query = args["query"].as_str().unwrap_or("");
            let limit = args["limit"].as_u64().unwrap_or(5).clamp(1, 15) as usize;
            let source_filter = args["source"].as_str().map(|s| s.to_string());
            let data_dir = state.data_dir.clone();
            let query_owned = query.to_string();
            tokio::task::spawn_blocking(move || {
                knowledge::search_knowledge(
                    &data_dir,
                    &query_owned,
                    limit,
                    source_filter.as_deref(),
                )
            })
            .await
            .map_err(|e| format!("Falha na busca RAG: {e}"))
            .map(|hits| {
                if hits.is_empty() {
                    format!(
                        "Nenhum trecho relevante encontrado na Base de Conhecimento para '{query}'."
                    )
                } else {
                    let mut out = format!(
                        "Encontrados {} trecho(s) relevantes na Base de Conhecimento para '{}':\n\n",
                        hits.len(),
                        query
                    );
                    for (i, hit) in hits.iter().enumerate() {
                        out.push_str(&format!(
                            "--- [{}] Fonte: {} | Arquivo: {} (linhas {}-{}) | Score: {:.2} ---\n{}\n\n",
                            i + 1,
                            hit.source_name,
                            hit.relative_path,
                            hit.start_line,
                            hit.end_line,
                            hit.score,
                            hit.snippet
                        ));
                    }
                    out
                }
            })
        }
        "kb_list" => {
            let sources = knowledge::list_sources(&state.data_dir);
            if sources.is_empty() {
                Ok(
                    "Nenhuma pasta ou arquivo indexado na Base de Conhecimento no momento."
                        .to_string(),
                )
            } else {
                let lines: Vec<String> = sources
                    .into_iter()
                    .map(|s| {
                        format!(
                            "- {} (id: {}): {} arquivo(s), {} chunks | caminho: {}",
                            s.name, s.id, s.file_count, s.chunk_count, s.path
                        )
                    })
                    .collect();
                Ok(lines.join("\n"))
            }
        }
        "kb_remove" => {
            let target = args["id_or_path"].as_str().unwrap_or("");
            knowledge::remove_source(&state.data_dir, target).map(|removed| {
                if removed {
                    format!("Fonte '{target}' removida da Base de Conhecimento.")
                } else {
                    format!("Fonte '{target}' não encontrada na Base de Conhecimento.")
                }
            })
        }
        _ => Err(format!("Ferramenta não implementada: {name}")),
    };

    let (status, output, summary) = match res {
        Ok(out) => {
            let short: String = out
                .lines()
                .next()
                .unwrap_or(&out)
                .chars()
                .take(90)
                .collect();
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
            check_permission_policy(
                PermissionMode::Review,
                ToolRisk::Read,
                "get_system_status",
                true
            ),
            PermissionDecision::Allow
        );
        assert_eq!(
            check_permission_policy(
                PermissionMode::Review,
                ToolRisk::Action,
                "run_command",
                true
            ),
            PermissionDecision::NeedApproval
        );
        assert!(matches!(
            check_permission_policy(PermissionMode::Safe, ToolRisk::Action, "open_url", true),
            PermissionDecision::Deny(_)
        ));
        assert!(matches!(
            check_permission_policy(
                PermissionMode::Safe,
                ToolRisk::Read,
                "get_system_status",
                true
            ),
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

        assert!(
            is_protected_write_path(std::path::Path::new(r"C:\Windows\System32\test.txt"))
                .is_some()
        );
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
            for (i, px) in bgra.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                px[0] = (i % 255) as u8;
                px[1] = 180;
                px[2] = 40;
                px[3] = 255;
            }
            let (cw, ch, rw, rh, b64) =
                encode_bgra_to_jpeg_base64(&bgra, w, h, (w * 4) as usize).unwrap();
            assert_eq!((cw, ch, rw, rh), (1280, 720, 1920, 1080));
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(&b64)
                .unwrap();
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

    #[tokio::test]
    async fn ssrf_validation_blocks_private_and_loopback_addresses() {
        assert!(validate_public_http_url("http://127.0.0.1:47821/")
            .await
            .is_err());
        assert!(validate_public_http_url("http://localhost:8080/api")
            .await
            .is_err());
        assert!(
            validate_public_http_url("http://169.254.169.254/latest/meta-data/")
                .await
                .is_err()
        );
        assert!(validate_public_http_url("http://192.168.1.1/admin")
            .await
            .is_err());
        assert!(validate_public_http_url("http://10.0.0.1/").await.is_err());
        assert!(validate_public_http_url("http://[::1]:3000/")
            .await
            .is_err());
        assert!(is_blocked_ip("::ffff:127.0.0.1".parse().unwrap()));
        assert!(!is_blocked_ip("1.1.1.1".parse().unwrap()));
    }
}
