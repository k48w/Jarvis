//! # Persistência de Conversas, Projetos e Memórias — `storage.rs`
//!
//! Armazena e indexa o histórico de conversas (`conversations.json`), pastas
//! de projetos vinculados (`projects.json`) e memórias de longo prazo do
//! usuário (`memories.json`) com gravação atômica em disco.

use crate::config::atomic_write;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolTrace {
    pub id: String,
    pub name: String,
    pub title: String,
    pub status: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredMessage {
    pub id: String,
    pub role: String,
    pub text: String,
    pub timestamp: String,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub tool_events: Vec<ToolTrace>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub project_path: Option<String>,
    pub messages: Vec<StoredMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationSummary {
    pub id: String,
    pub title: String,
    pub updated_at: String,
    pub message_count: usize,
    pub preview: String,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub project_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectItem {
    pub id: String,
    pub name: String,
    pub path: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryItem {
    pub id: String,
    pub content: String,
    pub created_at: String,
}

fn validate_id(id: &str) -> Result<&str, String> {
    if !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        Ok(id)
    } else {
        Err("ID de conversa inválido".to_string())
    }
}

fn conversations_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("conversations")
}

fn conversation_path(data_dir: &Path, id: &str) -> Result<PathBuf, String> {
    let valid = validate_id(id)?;
    Ok(conversations_dir(data_dir).join(format!("{valid}.json")))
}

fn memory_path(data_dir: &Path) -> PathBuf {
    data_dir.join("memory.json")
}

fn projects_path(data_dir: &Path) -> PathBuf {
    data_dir.join("projects.json")
}

pub fn create_conversation(
    data_dir: &Path,
    title: Option<&str>,
    project_id: Option<&str>,
    project_path: Option<&str>,
) -> Result<Conversation, String> {
    let now = Utc::now().to_rfc3339();
    let id = Uuid::new_v4().to_string();
    let clean_title = title
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("Nova conversa")
        .to_string();
    let conv = Conversation {
        id,
        title: clean_title,
        created_at: now.clone(),
        updated_at: now,
        project_id: project_id
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        project_path: project_path
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        messages: Vec::new(),
    };
    save_conversation(data_dir, &conv)?;
    Ok(conv)
}

pub fn save_conversation(data_dir: &Path, conv: &Conversation) -> Result<(), String> {
    let path = conversation_path(data_dir, &conv.id)?;
    let bytes = serde_json::to_vec_pretty(conv)
        .map_err(|e| format!("Falha ao serializar conversa: {e}"))?;
    atomic_write(&path, &bytes)
}

pub fn get_conversation(data_dir: &Path, id: &str) -> Result<Conversation, String> {
    let path = conversation_path(data_dir, id)?;
    let raw = fs::read_to_string(&path).map_err(|e| format!("Conversa não encontrada: {e}"))?;
    serde_json::from_str(&raw).map_err(|e| format!("Conversa corrompida: {e}"))
}

pub fn list_conversations(data_dir: &Path) -> Result<Vec<ConversationSummary>, String> {
    let dir = conversations_dir(data_dir);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let entries = fs::read_dir(&dir).map_err(|e| format!("Falha ao listar conversas: {e}"))?;
    let mut list = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let Ok(raw) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(conv) = serde_json::from_str::<Conversation>(&raw) else {
            continue;
        };
        let preview = conv
            .messages
            .last()
            .map(|m| {
                let mut text: String = m.text.chars().take(72).collect();
                if m.text.chars().count() > 72 {
                    text.push('…');
                }
                text
            })
            .unwrap_or_default();
        list.push(ConversationSummary {
            id: conv.id,
            title: conv.title,
            updated_at: conv.updated_at,
            message_count: conv.messages.len(),
            preview,
            project_id: conv.project_id,
            project_path: conv.project_path,
        });
    }
    list.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(list)
}

pub fn rename_conversation(data_dir: &Path, id: &str, title: &str) -> Result<Conversation, String> {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return Err("O título não pode ficar vazio".to_string());
    }
    let mut conv = get_conversation(data_dir, id)?;
    conv.title = trimmed.chars().take(80).collect();
    conv.updated_at = Utc::now().to_rfc3339();
    save_conversation(data_dir, &conv)?;
    Ok(conv)
}

pub fn delete_conversation(data_dir: &Path, id: &str) -> Result<(), String> {
    let path = conversation_path(data_dir, id)?;
    if path.exists() {
        fs::remove_file(&path).map_err(|e| format!("Falha ao excluir conversa: {e}"))?;
    }
    Ok(())
}

pub fn set_conversation_project(
    data_dir: &Path,
    id: &str,
    project_id: Option<&str>,
    project_path: Option<&str>,
) -> Result<Conversation, String> {
    let mut conv = get_conversation(data_dir, id)?;
    conv.project_id = project_id
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    conv.project_path = project_path
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    conv.updated_at = Utc::now().to_rfc3339();
    save_conversation(data_dir, &conv)?;
    Ok(conv)
}

pub fn list_projects(data_dir: &Path) -> Result<Vec<ProjectItem>, String> {
    let path = projects_path(data_dir);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = fs::read_to_string(&path).map_err(|e| format!("Falha ao ler projetos: {e}"))?;
    let items = serde_json::from_str::<Vec<ProjectItem>>(&raw).unwrap_or_default();
    Ok(items)
}

pub fn add_project(
    data_dir: &Path,
    raw_path: &str,
    custom_name: Option<&str>,
) -> Result<ProjectItem, String> {
    let clean_path = raw_path.trim().trim_matches('"');
    if clean_path.is_empty() {
        return Err("Informe o caminho da pasta do projeto.".to_string());
    }
    let p = Path::new(clean_path);
    if !p.exists() || !p.is_dir() {
        return Err(format!("Pasta do projeto não encontrada: {clean_path}"));
    }
    let canonical = fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let path_str = canonical
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_string();

    let name = custom_name
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| {
            canonical
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "Projeto".to_string());

    let mut list = list_projects(data_dir)?;
    if let Some(existing) = list
        .iter()
        .find(|item| item.path.eq_ignore_ascii_case(&path_str))
    {
        return Ok(existing.clone());
    }

    let item = ProjectItem {
        id: Uuid::new_v4().to_string(),
        name,
        path: path_str,
        created_at: Utc::now().to_rfc3339(),
    };
    list.insert(0, item.clone());
    let bytes =
        serde_json::to_vec_pretty(&list).map_err(|e| format!("Falha ao salvar projetos: {e}"))?;
    atomic_write(&projects_path(data_dir), &bytes)?;
    Ok(item)
}

pub fn delete_project(data_dir: &Path, id: &str) -> Result<(), String> {
    let mut list = list_projects(data_dir)?;
    list.retain(|item| item.id != id);
    let bytes =
        serde_json::to_vec_pretty(&list).map_err(|e| format!("Falha ao salvar projetos: {e}"))?;
    atomic_write(&projects_path(data_dir), &bytes)?;
    Ok(())
}

pub fn append_message(
    data_dir: &Path,
    conv_id: &str,
    msg: StoredMessage,
) -> Result<Conversation, String> {
    let mut conv = get_conversation(data_dir, conv_id)?;
    if conv.messages.is_empty() && msg.role == "user" && conv.title == "Nova conversa" {
        let mut auto_title: String = msg.text.trim().chars().take(48).collect();
        if msg.text.trim().chars().count() > 48 {
            auto_title.push('…');
        }
        if !auto_title.is_empty() {
            conv.title = auto_title;
        }
    }
    conv.updated_at = msg.timestamp.clone();
    conv.messages.push(msg);
    save_conversation(data_dir, &conv)?;
    Ok(conv)
}

pub fn list_memories(data_dir: &Path) -> Result<Vec<MemoryItem>, String> {
    let path = memory_path(data_dir);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = fs::read_to_string(&path).map_err(|e| format!("Falha ao ler memória: {e}"))?;
    serde_json::from_str(&raw).map_err(|e| format!("Memória corrompida: {e}"))
}

pub fn add_memory(data_dir: &Path, content: &str) -> Result<MemoryItem, String> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Err("Memória vazia".to_string());
    }
    let mut items = list_memories(data_dir).unwrap_or_default();
    if let Some(existing) = items
        .iter()
        .find(|m| m.content.eq_ignore_ascii_case(trimmed))
        .cloned()
    {
        return Ok(existing);
    }
    let item = MemoryItem {
        id: Uuid::new_v4().to_string(),
        content: trimmed.chars().take(400).collect(),
        created_at: Utc::now().to_rfc3339(),
    };
    items.push(item.clone());
    let bytes =
        serde_json::to_vec_pretty(&items).map_err(|e| format!("Falha ao salvar memória: {e}"))?;
    atomic_write(&memory_path(data_dir), &bytes)?;
    Ok(item)
}

pub fn delete_memory(data_dir: &Path, id: &str) -> Result<(), String> {
    let mut items = list_memories(data_dir).unwrap_or_default();
    let before = items.len();
    items.retain(|m| m.id != id);
    if items.len() != before {
        let bytes = serde_json::to_vec_pretty(&items)
            .map_err(|e| format!("Falha ao atualizar memória: {e}"))?;
        atomic_write(&memory_path(data_dir), &bytes)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("jarvis-storage-test-{name}-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn conversation_crud_and_title_generation() {
        let dir = temp_dir("conv");
        let proj = add_project(&dir, &dir.to_string_lossy(), Some("DemoProj")).unwrap();
        assert_eq!(proj.name, "DemoProj");
        assert_eq!(list_projects(&dir).unwrap().len(), 1);

        let conv = create_conversation(&dir, None, Some(&proj.id), Some(&proj.path)).unwrap();
        assert_eq!(conv.title, "Nova conversa");
        assert_eq!(conv.project_id.as_deref(), Some(proj.id.as_str()));

        let updated = append_message(
            &dir,
            &conv.id,
            StoredMessage {
                id: "m1".into(),
                role: "user".into(),
                text: "Qual o status do meu sistema agora?".into(),
                timestamp: Utc::now().to_rfc3339(),
                mode: None,
                tool_events: vec![],
            },
        )
        .unwrap();
        assert_eq!(updated.title, "Qual o status do meu sistema agora?");

        let list = list_conversations(&dir).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].message_count, 1);

        let renamed = rename_conversation(&dir, &conv.id, "Diagnóstico").unwrap();
        assert_eq!(renamed.title, "Diagnóstico");

        let loaded = get_conversation(&dir, &conv.id).unwrap();
        assert_eq!(loaded.messages.len(), 1);

        assert!(get_conversation(&dir, "../evil").is_err());

        delete_conversation(&dir, &conv.id).unwrap();
        assert!(list_conversations(&dir).unwrap().is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn memory_add_dedup_and_delete() {
        let dir = temp_dir("mem");
        let a = add_memory(&dir, "Meu nome é Lucas").unwrap();
        let dup = add_memory(&dir, "meu nome é lucas").unwrap();
        assert_eq!(a.id, dup.id);
        let b = add_memory(&dir, "Prefiro respostas curtas").unwrap();
        assert_eq!(list_memories(&dir).unwrap().len(), 2);

        delete_memory(&dir, &a.id).unwrap();
        let remaining = list_memories(&dir).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, b.id);
        let _ = fs::remove_dir_all(&dir);
    }
}
