//! # Sistema de Arquivos, Diff com Undo e Git Nativo — `tools/files.rs`
//!
//! Operações seguras de leitura, escrita, edição cirúrgica, busca em arquivos,
//! snapshot para reversão em 1 clique (`revert_file_change_blocking`), proteção
//! de diretórios críticos do Windows e comandos Git (`status`, `diff`, `log`, `commit`).

use serde_json::json;
use tauri::{AppHandle, Emitter};
use uuid::Uuid;
pub(crate) fn resolve_user_path(raw: &str) -> Result<std::path::PathBuf, String> {
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
    if let Some(rest) = trimmed
        .strip_prefix("~/")
        .or_else(|| trimmed.strip_prefix("~\\"))
    {
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

    let expanded = trimmed
        .replace("%USERPROFILE%", &home)
        .replace("%userprofile%", &home);
    Ok(std::path::PathBuf::from(expanded))
}

pub(crate) fn is_protected_write_path(path: &std::path::Path) -> Option<&'static str> {
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

pub(crate) fn tool_list_files(raw_path: &str, max_entries: usize) -> Result<String, String> {
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

pub(crate) fn tool_read_file(
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
    let raw = std::fs::read_to_string(&path).map_err(|e| {
        format!(
            "Não foi possível ler '{}' como texto UTF-8: {e}",
            path.display()
        )
    })?;

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
    for (idx, line) in lines.iter().enumerate().take(end).skip(start - 1) {
        out.push_str(&format!("{}: {}\n", idx + 1, line));
        if out.len() > 24_000 {
            out.push_str("\n... (conteúdo truncado; use start_line/end_line para continuar)");
            break;
        }
    }
    Ok(out)
}

pub(crate) fn tool_write_file(
    raw_path: &str,
    content: &str,
    append: bool,
) -> Result<String, String> {
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

pub(crate) fn tool_edit_file(
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

pub(crate) fn tool_search_files(
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
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
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
            let Ok(text) = std::fs::read_to_string(&p) else {
                continue;
            };
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

pub(crate) fn emit_file_diff(
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
pub struct GitChangeItem {
    pub status: String,
    pub path: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitRepoStatus {
    pub is_git_repo: bool,
    pub path: String,
    pub branch: String,
    pub ahead: i32,
    pub behind: i32,
    pub changes: Vec<GitChangeItem>,
}

fn run_git_cmd(dir: &std::path::Path, args: &[&str]) -> Result<std::process::Output, String> {
    let mut cmd = std::process::Command::new("git");
    cmd.args(args).current_dir(dir);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.output()
        .map_err(|e| format!("Falha ao executar git (verifique se o Git está instalado): {e}"))
}

pub fn git_repo_status_blocking(raw_path: &str) -> Result<GitRepoStatus, String> {
    let dir = resolve_user_path(raw_path)?;
    if !dir.exists() || !dir.is_dir() {
        return Err(format!("Pasta não encontrada: {}", dir.display()));
    }
    let out = run_git_cmd(&dir, &["status", "--porcelain=v1", "-b"])?;
    if !out.status.success() {
        return Ok(GitRepoStatus {
            is_git_repo: false,
            path: dir.to_string_lossy().to_string(),
            branch: String::new(),
            ahead: 0,
            behind: 0,
            changes: Vec::new(),
        });
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let mut branch = "HEAD".to_string();
    let mut ahead = 0i32;
    let mut behind = 0i32;
    let mut changes = Vec::new();

    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix("## ") {
            let clean = rest.trim();
            branch = clean
                .split("...")
                .next()
                .unwrap_or(clean)
                .trim_start_matches("No commits yet on ")
                .to_string();
            if let Some(bracket_start) = clean.find('[') {
                let meta = &clean[bracket_start + 1..clean.len().saturating_sub(1)];
                for part in meta.split(',') {
                    let p = part.trim();
                    if let Some(v) = p.strip_prefix("ahead ") {
                        ahead = v.trim().parse().unwrap_or(0);
                    } else if let Some(v) = p.strip_prefix("behind ") {
                        behind = v.trim().parse().unwrap_or(0);
                    }
                }
            }
        } else if line.len() >= 4 {
            let st = line[..2].trim().to_string();
            let file_path = line[3..].trim().trim_matches('"').to_string();
            if !file_path.is_empty() {
                changes.push(GitChangeItem {
                    status: if st.is_empty() { "M".to_string() } else { st },
                    path: file_path,
                });
            }
        }
    }

    Ok(GitRepoStatus {
        is_git_repo: true,
        path: dir.to_string_lossy().to_string(),
        branch,
        ahead,
        behind,
        changes,
    })
}

pub fn tool_git_diff(raw_path: &str, file: Option<&str>, staged: bool) -> Result<String, String> {
    let dir = resolve_user_path(raw_path)?;
    if !dir.exists() || !dir.is_dir() {
        return Err(format!("Pasta não encontrada: {}", dir.display()));
    }
    let mut args = vec!["diff", "--no-color"];
    if staged {
        args.push("--cached");
    }
    if let Some(f) = file.map(str::trim).filter(|s| !s.is_empty()) {
        args.push("--");
        args.push(f);
    }
    let out = run_git_cmd(&dir, &args)?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("git diff falhou: {}", err.trim()));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    if text.trim().is_empty() {
        return Ok("Nenhuma diferença encontrada no git diff.".to_string());
    }
    let preview: String = text.chars().take(12_000).collect();
    if text.chars().count() > 12_000 {
        Ok(format!(
            "{preview}\n... (diff truncado em 12.000 caracteres)"
        ))
    } else {
        Ok(preview)
    }
}

pub fn tool_git_log(raw_path: &str, limit: usize) -> Result<String, String> {
    let dir = resolve_user_path(raw_path)?;
    if !dir.exists() || !dir.is_dir() {
        return Err(format!("Pasta não encontrada: {}", dir.display()));
    }
    let limit_arg = format!("-n{}", limit.clamp(1, 30));
    let out = run_git_cmd(
        &dir,
        &["log", &limit_arg, "--pretty=format:%h · %cr · %an · %s"],
    )?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("git log falhou: {}", err.trim()));
    }
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if text.is_empty() {
        Ok("Nenhum commit encontrado neste repositório.".to_string())
    } else {
        Ok(text)
    }
}

pub fn git_commit_blocking(raw_path: &str, message: &str) -> Result<String, String> {
    let dir = resolve_user_path(raw_path)?;
    if !dir.exists() || !dir.is_dir() {
        return Err(format!("Pasta não encontrada: {}", dir.display()));
    }
    let msg = message.trim();
    if msg.is_empty() {
        return Err("Informe uma mensagem de commit válida.".to_string());
    }
    let st = git_repo_status_blocking(raw_path)?;
    if !st.is_git_repo {
        return Err(format!(
            "A pasta '{}' não é um repositório Git.",
            dir.display()
        ));
    }
    if st.changes.is_empty() {
        return Err("Não há alterações pendentes para commitar.".to_string());
    }
    let add_out = run_git_cmd(&dir, &["add", "-A"])?;
    if !add_out.status.success() {
        return Err(format!(
            "git add falhou: {}",
            String::from_utf8_lossy(&add_out.stderr).trim()
        ));
    }
    let commit_out = run_git_cmd(&dir, &["commit", "-m", msg])?;
    if !commit_out.status.success() {
        return Err(format!(
            "git commit falhou: {}",
            String::from_utf8_lossy(&commit_out.stderr).trim()
        ));
    }
    let summary = String::from_utf8_lossy(&commit_out.stdout)
        .lines()
        .next()
        .unwrap_or("Commit criado com sucesso.")
        .trim()
        .to_string();
    Ok(format!("Commit criado em {}: {summary}", st.branch))
}
