//! # Skills (`/comandos`) e Agendador de Rotinas Proativas — `routines.rs`
//!
//! Gerencia macros reutilizáveis acionadas por `/slug` no chat (`skills.json`)
//! e o loop em background que dispara rotinas agendadas (`routines.json`) em
//! horário diário fixo (`HH:MM`) ou por intervalo em minutos.

use crate::config::atomic_write;
use chrono::{DateTime, Local, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillItem {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub description: String,
    pub prompt: String,
    #[serde(default)]
    pub builtin: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutineItem {
    pub id: String,
    pub name: String,
    /// "interval" | "daily"
    pub schedule_kind: String,
    #[serde(default)]
    pub interval_minutes: Option<u32>,
    #[serde(default)]
    pub daily_hhmm: Option<String>,
    pub prompt: String,
    pub enabled: bool,
    #[serde(default)]
    pub last_run_at: Option<String>,
    pub created_at: String,
}

fn skills_path(data_dir: &Path) -> PathBuf {
    data_dir.join("skills.json")
}

fn routines_path(data_dir: &Path) -> PathBuf {
    data_dir.join("routines.json")
}

fn default_skills() -> Vec<SkillItem> {
    let now = Utc::now().to_rfc3339();
    vec![
        SkillItem {
            id: "builtin-briefing".to_string(),
            slug: "briefing".to_string(),
            name: "Briefing do Dia".to_string(),
            description: "Hora local, saúde do PC e 3 principais notícias do dia.".to_string(),
            prompt: "Consulte a data/hora atual, o status do sistema e faça uma busca rápida (web_search) pelas 3 principais notícias de hoje no Brasil e no mundo. Entregue um briefing elegante e conciso.".to_string(),
            builtin: true,
            created_at: now.clone(),
        },
        SkillItem {
            id: "builtin-saude-pc".to_string(),
            slug: "saude-pc".to_string(),
            name: "Diagnóstico do PC".to_string(),
            description: "Inspeciona CPU, RAM, discos e processos pesados.".to_string(),
            prompt: "Use get_system_status e me dê um diagnóstico rápido de CPU, RAM, disco, energia e os processos que mais consomem memória, destacando se algo merece atenção.".to_string(),
            builtin: true,
            created_at: now.clone(),
        },
        SkillItem {
            id: "builtin-foco".to_string(),
            slug: "foco".to_string(),
            name: "Modo Foco (Pomodoro)".to_string(),
            description: "Volume em 25% e timer de foco de 25 minutos.".to_string(),
            prompt: "Ajuste o volume do Windows para 25% usando set_volume, crie um timer de 1500 segundos chamado 'Sessão de Foco' usando set_timer e me deseje uma ótima sessão de forma breve.".to_string(),
            builtin: true,
            created_at: now.clone(),
        },
        SkillItem {
            id: "builtin-git-review".to_string(),
            slug: "git-review".to_string(),
            name: "Revisão Git do Projeto".to_string(),
            description: "Lê git_status + git_diff e sugere mensagem de commit.".to_string(),
            prompt: "Use git_status e git_diff na pasta do projeto atual, resuma objetivamente as alterações encontradas e sugira uma mensagem de commit clara no padrão Conventional Commits.".to_string(),
            builtin: true,
            created_at: now,
        },
    ]
}

pub fn normalize_slug(raw: &str) -> String {
    raw.trim()
        .trim_start_matches('/')
        .to_ascii_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

pub fn list_skills(data_dir: &Path) -> Result<Vec<SkillItem>, String> {
    let path = skills_path(data_dir);
    if !path.exists() {
        let defaults = default_skills();
        let bytes = serde_json::to_vec_pretty(&defaults)
            .map_err(|e| format!("Falha ao serializar skills: {e}"))?;
        let _ = atomic_write(&path, &bytes);
        return Ok(defaults);
    }
    let raw = fs::read_to_string(&path).map_err(|e| format!("Falha ao ler skills: {e}"))?;
    let mut items: Vec<SkillItem> = serde_json::from_str(&raw).unwrap_or_else(|_| default_skills());
    if items.is_empty() {
        items = default_skills();
    }
    Ok(items)
}

pub fn save_skill(
    data_dir: &Path,
    slug: &str,
    name: &str,
    description: &str,
    prompt: &str,
) -> Result<SkillItem, String> {
    let clean_slug = normalize_slug(slug);
    if clean_slug.is_empty() {
        return Err("Informe um comando/slug válido (ex: modo-jogo).".to_string());
    }
    let clean_prompt = prompt.trim();
    if clean_prompt.is_empty() {
        return Err("Informe o prompt/instrução da skill.".to_string());
    }
    let clean_name = if name.trim().is_empty() {
        clean_slug.clone()
    } else {
        name.trim().to_string()
    };

    let mut items = list_skills(data_dir)?;
    let now = Utc::now().to_rfc3339();
    if let Some(existing) = items.iter_mut().find(|s| s.slug == clean_slug) {
        existing.name = clean_name;
        existing.description = description.trim().to_string();
        existing.prompt = clean_prompt.to_string();
        let updated = existing.clone();
        let bytes = serde_json::to_vec_pretty(&items)
            .map_err(|e| format!("Falha ao serializar skills: {e}"))?;
        atomic_write(&skills_path(data_dir), &bytes)?;
        return Ok(updated);
    }

    let item = SkillItem {
        id: Uuid::new_v4().to_string(),
        slug: clean_slug,
        name: clean_name,
        description: description.trim().to_string(),
        prompt: clean_prompt.to_string(),
        builtin: false,
        created_at: now,
    };
    items.push(item.clone());
    let bytes = serde_json::to_vec_pretty(&items)
        .map_err(|e| format!("Falha ao serializar skills: {e}"))?;
    atomic_write(&skills_path(data_dir), &bytes)?;
    Ok(item)
}

pub fn delete_skill(data_dir: &Path, id_or_slug: &str) -> Result<bool, String> {
    let mut items = list_skills(data_dir)?;
    let target_slug = normalize_slug(id_or_slug);
    let before = items.len();
    items.retain(|s| s.id != id_or_slug && s.slug != target_slug);
    if items.len() == before {
        return Ok(false);
    }
    let bytes = serde_json::to_vec_pretty(&items)
        .map_err(|e| format!("Falha ao serializar skills: {e}"))?;
    atomic_write(&skills_path(data_dir), &bytes)?;
    Ok(true)
}

pub fn list_routines(data_dir: &Path) -> Result<Vec<RoutineItem>, String> {
    let path = routines_path(data_dir);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = fs::read_to_string(&path).map_err(|e| format!("Falha ao ler rotinas: {e}"))?;
    Ok(serde_json::from_str(&raw).unwrap_or_default())
}

fn persist_routines(data_dir: &Path, items: &[RoutineItem]) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(items)
        .map_err(|e| format!("Falha ao serializar rotinas: {e}"))?;
    atomic_write(&routines_path(data_dir), &bytes)
}

pub fn create_routine(
    data_dir: &Path,
    name: &str,
    schedule_kind: &str,
    interval_minutes: Option<u32>,
    daily_hhmm: Option<&str>,
    prompt: &str,
) -> Result<RoutineItem, String> {
    let clean_name = name.trim();
    let clean_prompt = prompt.trim();
    if clean_name.is_empty() || clean_prompt.is_empty() {
        return Err("Informe nome e instrução para a rotina.".to_string());
    }
    let kind = match schedule_kind.trim().to_ascii_lowercase().as_str() {
        "daily" | "diaria" | "diária" => "daily",
        _ => "interval",
    };

    let (int_min, hhmm) = if kind == "daily" {
        let raw_time = daily_hhmm.unwrap_or("09:00").trim();
        let parts: Vec<&str> = raw_time.split(':').collect();
        if parts.len() != 2
            || parts[0].parse::<u32>().map(|h| h > 23).unwrap_or(true)
            || parts[1].parse::<u32>().map(|m| m > 59).unwrap_or(true)
        {
            return Err("Horário diário inválido. Use o formato HH:MM (ex: 09:00).".to_string());
        }
        let h: u32 = parts[0].parse().unwrap_or(9);
        let m: u32 = parts[1].parse().unwrap_or(0);
        (None, Some(format!("{h:02}:{m:02}")))
    } else {
        let mins = interval_minutes.unwrap_or(60).clamp(1, 1440);
        (Some(mins), None)
    };

    let mut items = list_routines(data_dir)?;
    let item = RoutineItem {
        id: Uuid::new_v4().to_string(),
        name: clean_name.to_string(),
        schedule_kind: kind.to_string(),
        interval_minutes: int_min,
        daily_hhmm: hhmm,
        prompt: clean_prompt.to_string(),
        enabled: true,
        last_run_at: None,
        created_at: Utc::now().to_rfc3339(),
    };
    items.push(item.clone());
    persist_routines(data_dir, &items)?;
    Ok(item)
}

pub fn toggle_routine(data_dir: &Path, id: &str, enabled: bool) -> Result<RoutineItem, String> {
    let mut items = list_routines(data_dir)?;
    let found = items
        .iter_mut()
        .find(|r| r.id == id)
        .ok_or_else(|| "Rotina não encontrada.".to_string())?;
    found.enabled = enabled;
    let updated = found.clone();
    persist_routines(data_dir, &items)?;
    Ok(updated)
}

pub fn delete_routine(data_dir: &Path, id: &str) -> Result<bool, String> {
    let mut items = list_routines(data_dir)?;
    let before = items.len();
    items.retain(|r| r.id != id);
    if items.len() == before {
        return Ok(false);
    }
    persist_routines(data_dir, &items)?;
    Ok(true)
}

pub fn start_routines_scheduler(app: AppHandle, data_dir: PathBuf) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        // Skip first immediate tick so startup stays clean
        interval.tick().await;
        loop {
            interval.tick().await;
            let Ok(mut items) = list_routines(&data_dir) else {
                continue;
            };
            let now_utc = Utc::now();
            let now_local = Local::now();
            let hhmm_now = now_local.format("%H:%M").to_string();
            let today_local = now_local.format("%Y-%m-%d").to_string();
            let mut changed = false;

            for r in items.iter_mut() {
                if !r.enabled {
                    continue;
                }
                let due = if r.schedule_kind == "daily" {
                    let matches_time = r.daily_hhmm.as_deref() == Some(hhmm_now.as_str());
                    let ran_today = r
                        .last_run_at
                        .as_deref()
                        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                        .map(|dt| {
                            dt.with_timezone(&Local).format("%Y-%m-%d").to_string() == today_local
                        })
                        .unwrap_or(false);
                    matches_time && !ran_today
                } else {
                    let mins = r.interval_minutes.unwrap_or(60).max(1) as i64;
                    let ref_str = r.last_run_at.as_deref().unwrap_or(r.created_at.as_str());
                    DateTime::parse_from_rfc3339(ref_str)
                        .map(|dt| (now_utc - dt.with_timezone(&Utc)).num_minutes() >= mins)
                        .unwrap_or(false)
                };

                if due {
                    r.last_run_at = Some(now_utc.to_rfc3339());
                    changed = true;
                    let _ = app.emit(
                        "jarvis:routine-trigger",
                        json!({
                            "id": r.id,
                            "name": r.name,
                            "prompt": r.prompt,
                        }),
                    );
                }
            }

            if changed {
                let _ = persist_routines(&data_dir, &items);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skills_and_routines_crud_works() {
        let dir = std::env::temp_dir().join(format!("jarvis-routines-test-{}", Uuid::new_v4()));
        let defaults = list_skills(&dir).unwrap();
        assert!(defaults.len() >= 4);

        let saved = save_skill(
            &dir,
            "/Modo Jogo!",
            "Modo Jogo",
            "Prepara PC",
            "Ajuste volume",
        )
        .unwrap();
        assert_eq!(saved.slug, "modo-jogo");
        assert!(delete_skill(&dir, "modo-jogo").unwrap());

        let r = create_routine(
            &dir,
            "Briefing",
            "daily",
            None,
            Some("09:30"),
            "Me dê o briefing",
        )
        .unwrap();
        assert_eq!(r.daily_hhmm.as_deref(), Some("09:30"));
        let toggled = toggle_routine(&dir, &r.id, false).unwrap();
        assert!(!toggled.enabled);
        assert!(delete_routine(&dir, &r.id).unwrap());

        let _ = fs::remove_dir_all(&dir);
    }
}
