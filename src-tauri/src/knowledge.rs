//! # Base de Conhecimento Local (RAG) — `knowledge.rs`
//!
//! Motor de indexação e busca contextual local (Retrieval-Augmented Generation)
//! escrito em Rust puro, sem dependências externas de banco vetorial.
//!
//! ## Como funciona
//! 1. **Indexação (`index_path`)**: Percorre um arquivo ou pasta local (ignorando
//!    `node_modules`, `.git`, `target`, `dist` e binários), divide arquivos de texto
//!    e código em blocos (*chunks*) com sobreposição e registra o intervalo exato
//!    de linhas (`start_line..=end_line`).
//! 2. **Busca Híbrida (`search_knowledge`)**: Combina pontuação **BM25** (TF-IDF com
//!    saturação e normalização por tamanho do documento) com bônus para correspondência
//!    exata de frase, identificadores de código e nome de arquivo.
//! 3. **Persistência**: O índice fica salvo em `knowledge_base.json` dentro do
//!    diretório de dados do aplicativo (`AppState.data_dir`).

use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

const KNOWLEDGE_FILE: &str = "knowledge_base.json";
const MAX_FILES_PER_SOURCE: usize = 300;
const MAX_FILE_BYTES: u64 = 512 * 1024;
const CHUNK_LINES: usize = 36;
const CHUNK_OVERLAP: usize = 6;
const MAX_CHUNKS_PER_SOURCE: usize = 2500;

/// Extensões de texto, documentação e código-fonte suportadas pelo indexador RAG.
const ALLOWED_EXTENSIONS: &[&str] = &[
    "md",
    "txt",
    "rs",
    "ts",
    "tsx",
    "js",
    "jsx",
    "py",
    "json",
    "toml",
    "yaml",
    "yml",
    "html",
    "css",
    "scss",
    "sql",
    "sh",
    "ps1",
    "c",
    "cpp",
    "h",
    "hpp",
    "go",
    "java",
    "cs",
    "kt",
    "swift",
    "rb",
    "php",
    "lua",
    "xml",
    "ini",
    "cfg",
    "env.example",
    "csv",
    "log",
];

/// Diretórios ignorados automaticamente ao indexar uma árvore de projeto.
const IGNORED_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    ".next",
    ".turbo",
    ".venv",
    "venv",
    "__pycache__",
    ".idea",
    ".vscode",
    "coverage",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeSourceSummary {
    pub id: String,
    pub name: String,
    pub path: String,
    pub file_count: usize,
    pub chunk_count: usize,
    pub indexed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeChunk {
    pub id: String,
    pub source_id: String,
    pub source_name: String,
    pub file_path: String,
    pub relative_path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeHit {
    pub chunk_id: String,
    pub source_id: String,
    pub source_name: String,
    pub file_path: String,
    pub relative_path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub score: f64,
    pub snippet: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct KnowledgeStore {
    pub sources: Vec<KnowledgeSourceSummary>,
    pub chunks: Vec<KnowledgeChunk>,
}

fn store_path(data_dir: &Path) -> PathBuf {
    data_dir.join(KNOWLEDGE_FILE)
}

fn load_store(data_dir: &Path) -> KnowledgeStore {
    let p = store_path(data_dir);
    let Ok(raw) = fs::read_to_string(p) else {
        return KnowledgeStore::default();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

fn save_store(data_dir: &Path, store: &KnowledgeStore) -> Result<(), String> {
    fs::create_dir_all(data_dir)
        .map_err(|e| format!("Falha ao criar pasta da base de conhecimento: {e}"))?;
    let body = serde_json::to_string_pretty(store)
        .map_err(|e| format!("Falha ao serializar base de conhecimento: {e}"))?;
    fs::write(store_path(data_dir), body)
        .map_err(|e| format!("Falha ao gravar base de conhecimento: {e}"))
}

/// Lista todas as fontes (pastas ou arquivos) indexadas na Base de Conhecimento.
pub fn list_sources(data_dir: &Path) -> Vec<KnowledgeSourceSummary> {
    load_store(data_dir).sources
}

/// Remove uma fonte indexada pelo `id` ou pelo `path` exato.
pub fn remove_source(data_dir: &Path, id_or_path: &str) -> Result<bool, String> {
    let needle = id_or_path.trim();
    if needle.is_empty() {
        return Err("Informe o ID ou caminho da fonte para remover.".to_string());
    }
    let mut store = load_store(data_dir);
    let target_ids: HashSet<String> = store
        .sources
        .iter()
        .filter(|s| s.id == needle || s.path.eq_ignore_ascii_case(needle))
        .map(|s| s.id.clone())
        .collect();
    if target_ids.is_empty() {
        return Ok(false);
    }
    store.sources.retain(|s| !target_ids.contains(&s.id));
    store.chunks.retain(|c| !target_ids.contains(&c.source_id));
    save_store(data_dir, &store)?;
    Ok(true)
}

/// Indexa um arquivo ou diretório local na Base de Conhecimento (RAG).
/// Se o caminho já estava indexado, atualiza seus blocos sem duplicar.
pub fn index_path(
    data_dir: &Path,
    raw_path: &str,
    custom_name: Option<String>,
) -> Result<KnowledgeSourceSummary, String> {
    let trimmed = raw_path.trim().trim_matches('"');
    if trimmed.is_empty() {
        return Err("Informe um caminho válido para indexar.".to_string());
    }
    let target = PathBuf::from(trimmed);
    if !target.exists() {
        return Err(format!("O caminho '{}' não existe.", trimmed));
    }
    let canonical = fs::canonicalize(&target).unwrap_or_else(|_| target.clone());
    let clean_path_str = strip_verbatim_prefix(&canonical.to_string_lossy());

    let default_name = canonical
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| clean_path_str.clone());
    let source_name = custom_name
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or(default_name);

    let mut files = Vec::new();
    if canonical.is_file() {
        if is_indexable_file(&canonical) {
            files.push(canonical.clone());
        } else {
            return Err(
                "O arquivo informado não possui extensão de texto/código suportada ou excede 512 KB."
                    .to_string(),
            );
        }
    } else if canonical.is_dir() {
        collect_indexable_files(&canonical, &mut files, 0);
    } else {
        return Err("O caminho informado não é um arquivo nem pasta válida.".to_string());
    }

    if files.is_empty() {
        return Err(
            "Nenhum arquivo de texto, documentação ou código suportado foi encontrado nesse caminho."
                .to_string(),
        );
    }

    let mut store = load_store(data_dir);
    let existing_id = store
        .sources
        .iter()
        .find(|s| s.path.eq_ignore_ascii_case(&clean_path_str))
        .map(|s| s.id.clone());
    let source_id = existing_id.unwrap_or_else(|| format!("kb-{}", Uuid::new_v4().simple()));

    store.sources.retain(|s| s.id != source_id);
    store.chunks.retain(|c| c.source_id != source_id);

    let mut new_chunks = Vec::new();
    let mut indexed_files = 0usize;

    for file in files {
        if new_chunks.len() >= MAX_CHUNKS_PER_SOURCE {
            break;
        }
        let Ok(raw_text) = fs::read_to_string(&file) else {
            continue;
        };
        if raw_text.trim().is_empty() {
            continue;
        }
        indexed_files += 1;
        let file_str = strip_verbatim_prefix(&file.to_string_lossy());
        let rel_str = file
            .strip_prefix(&canonical)
            .ok()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                file.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| file_str.clone())
            });

        let file_chunks = chunk_text_with_lines(
            &source_id,
            &source_name,
            &file_str,
            &rel_str,
            &raw_text,
            MAX_CHUNKS_PER_SOURCE - new_chunks.len(),
        );
        new_chunks.extend(file_chunks);
    }

    if new_chunks.is_empty() {
        return Err("Os arquivos encontrados estavam vazios ou ilegíveis em UTF-8.".to_string());
    }

    let summary = KnowledgeSourceSummary {
        id: source_id,
        name: source_name,
        path: clean_path_str,
        file_count: indexed_files,
        chunk_count: new_chunks.len(),
        indexed_at: Utc::now().to_rfc3339(),
    };

    store.sources.insert(0, summary.clone());
    store.chunks.extend(new_chunks);
    save_store(data_dir, &store)?;
    Ok(summary)
}

/// Pesquisa os trechos mais relevantes na Base de Conhecimento usando BM25 + correspondência exata.
pub fn search_knowledge(
    data_dir: &Path,
    query: &str,
    limit: usize,
    source_filter: Option<&str>,
) -> Vec<KnowledgeHit> {
    let clean_query = query.trim();
    if clean_query.is_empty() {
        return Vec::new();
    }
    let store = load_store(data_dir);
    if store.chunks.is_empty() {
        return Vec::new();
    }

    let query_tokens = tokenize(clean_query);
    let query_lower = clean_query.to_lowercase();
    let max_results = limit.clamp(1, 20);

    let candidate_chunks: Vec<&KnowledgeChunk> = store
        .chunks
        .iter()
        .filter(|c| match source_filter {
            Some(f) if !f.trim().is_empty() => {
                let fl = f.trim().to_lowercase();
                c.source_id.eq_ignore_ascii_case(f.trim())
                    || c.source_name.to_lowercase().contains(&fl)
                    || c.relative_path.to_lowercase().contains(&fl)
            }
            _ => true,
        })
        .collect();

    if candidate_chunks.is_empty() {
        return Vec::new();
    }

    // Pré-computa frequências de termos por documento e tamanho médio para BM25
    let n_docs = candidate_chunks.len() as f64;
    let mut doc_tokens: Vec<HashMap<String, usize>> = Vec::with_capacity(candidate_chunks.len());
    let mut doc_lengths: Vec<usize> = Vec::with_capacity(candidate_chunks.len());
    let mut df: HashMap<String, usize> = HashMap::new();

    for chunk in &candidate_chunks {
        let toks = tokenize(&chunk.content);
        doc_lengths.push(toks.len().max(1));
        let mut counts = HashMap::new();
        for t in toks {
            *counts.entry(t).or_insert(0) += 1;
        }
        for unique_t in counts.keys() {
            *df.entry(unique_t.clone()).or_insert(0) += 1;
        }
        doc_tokens.push(counts);
    }

    let avg_dl = (doc_lengths.iter().sum::<usize>() as f64 / n_docs).max(1.0);
    let k1 = 1.5_f64;
    let b = 0.75_f64;

    let mut scored: Vec<KnowledgeHit> = Vec::new();

    for (idx, chunk) in candidate_chunks.iter().enumerate() {
        let counts = &doc_tokens[idx];
        let dl = doc_lengths[idx] as f64;
        let mut score = 0.0_f64;

        for qt in &query_tokens {
            let tf = *counts.get(qt).unwrap_or(&0) as f64;
            if tf > 0.0 {
                let doc_freq = *df.get(qt).unwrap_or(&1) as f64;
                let idf = ((n_docs - doc_freq + 0.5) / (doc_freq + 0.5) + 1.0).ln();
                let denom = tf + k1 * (1.0 - b + b * (dl / avg_dl));
                score += idf * ((tf * (k1 + 1.0)) / denom);
            }
        }

        let content_lower = chunk.content.to_lowercase();
        let path_lower = chunk.relative_path.to_lowercase();

        // Bônus para frase exata ou identificador técnico no texto
        if query_lower.len() >= 3 && content_lower.contains(&query_lower) {
            score += 3.5;
        }

        // Bônus quando o nome do arquivo bate com termos da busca
        for qt in &query_tokens {
            if qt.len() >= 3 && path_lower.contains(qt) {
                score += 1.4;
            }
        }

        if score > 0.05 {
            scored.push(KnowledgeHit {
                chunk_id: chunk.id.clone(),
                source_id: chunk.source_id.clone(),
                source_name: chunk.source_name.clone(),
                file_path: chunk.file_path.clone(),
                relative_path: chunk.relative_path.clone(),
                start_line: chunk.start_line,
                end_line: chunk.end_line,
                score: (score * 100.0).round() / 100.0,
                snippet: chunk.content.clone(),
            });
        }
    }

    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    scored.truncate(max_results);
    scored
}

fn strip_verbatim_prefix(path: &str) -> String {
    path.strip_prefix(r"\\?\").unwrap_or(path).to_string()
}

fn is_indexable_file(path: &Path) -> bool {
    let Ok(meta) = fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() || meta.len() == 0 || meta.len() > MAX_FILE_BYTES {
        return false;
    }
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    // Nunca indexar arquivos de segredo ou chaves privadas
    if file_name == ".env"
        || file_name.starts_with(".env.")
        || file_name == "secrets.json"
        || file_name.ends_with(".pem")
        || file_name.ends_with(".key")
    {
        return false;
    }

    if matches!(
        file_name.as_str(),
        "dockerfile" | "makefile" | "readme" | "license"
    ) {
        return true;
    }

    let Some(ext) = path.extension().map(|e| e.to_string_lossy().to_lowercase()) else {
        return false;
    };
    ALLOWED_EXTENSIONS.contains(&ext.as_str())
}

fn collect_indexable_files(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) {
    if depth > 7 || out.len() >= MAX_FILES_PER_SOURCE {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut sorted_entries: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    sorted_entries.sort();

    for path in sorted_entries {
        if out.len() >= MAX_FILES_PER_SOURCE {
            break;
        }
        if path.is_dir() {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if IGNORED_DIRS.iter().any(|ig| ig.eq_ignore_ascii_case(&name)) {
                continue;
            }
            collect_indexable_files(&path, out, depth + 1);
        } else if is_indexable_file(&path) {
            out.push(path);
        }
    }
}

fn chunk_text_with_lines(
    source_id: &str,
    source_name: &str,
    file_path: &str,
    relative_path: &str,
    text: &str,
    max_chunks: usize,
) -> Vec<KnowledgeChunk> {
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() || max_chunks == 0 {
        return Vec::new();
    }

    let mut out = Vec::new();
    let step = (CHUNK_LINES.saturating_sub(CHUNK_OVERLAP)).max(1);
    let mut start_idx = 0usize;

    while start_idx < lines.len() && out.len() < max_chunks {
        let end_idx = (start_idx + CHUNK_LINES).min(lines.len());
        let slice = &lines[start_idx..end_idx];
        let body = slice.join("\n");
        if !body.trim().is_empty() {
            out.push(KnowledgeChunk {
                id: format!("{}-L{}-{}", source_id, start_idx + 1, end_idx),
                source_id: source_id.to_string(),
                source_name: source_name.to_string(),
                file_path: file_path.to_string(),
                relative_path: relative_path.to_string(),
                start_line: start_idx + 1,
                end_line: end_idx,
                content: body,
            });
        }
        if end_idx >= lines.len() {
            break;
        }
        start_idx += step;
    }

    out
}

fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|tok| tok.len() >= 2)
        .map(|tok| tok.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knowledge_base_indexes_searches_and_removes_cleanly() {
        let data_dir =
            std::env::temp_dir().join(format!("jarvis-kb-test-{}", Uuid::new_v4().simple()));
        let docs_dir = data_dir.join("sample_project");
        fs::create_dir_all(&docs_dir).unwrap();

        fs::write(
            docs_dir.join("arquitetura.md"),
            "# Arquitetura do Motor\nO subsistema QuantumFlux controla a telemetria DXGI em 144Hz.\nOs segredos usam criptografia DPAPI nativa no Windows.",
        )
        .unwrap();
        fs::write(
            docs_dir.join("motor.rs"),
            "pub fn inicializar_quantum_flux(freq_hz: u32) -> bool {\n    freq_hz == 144\n}\n",
        )
        .unwrap();
        // Segredos (.env) devem ser ignorados automaticamente
        fs::write(docs_dir.join(".env"), "SECRET_KEY=super_secreto_123").unwrap();

        let summary = index_path(&data_dir, &docs_dir.to_string_lossy(), None).unwrap();
        assert_eq!(summary.file_count, 2);
        assert!(summary.chunk_count >= 2);

        let hits = search_knowledge(&data_dir, "QuantumFlux telemetria DXGI", 5, None);
        assert!(!hits.is_empty());
        assert!(hits[0].snippet.contains("QuantumFlux"));

        // Garante que o arquivo .env nunca foi indexado
        let env_hits = search_knowledge(&data_dir, "super_secreto_123", 5, None);
        assert!(env_hits.is_empty());

        // Remover a fonte limpa a lista e os chunks
        assert!(remove_source(&data_dir, &summary.id).unwrap());
        assert!(list_sources(&data_dir).is_empty());

        let _ = fs::remove_dir_all(&data_dir);
    }
}
