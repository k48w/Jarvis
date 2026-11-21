use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::{oneshot, Mutex};
use uuid::Uuid;

#[derive(Default)]
pub struct TerminalController {
    running: Mutex<HashMap<String, oneshot::Sender<()>>>,
}

impl TerminalController {
    pub async fn cancel(&self, id: &str) -> bool {
        let mut map = self.running.lock().await;
        if id.eq_ignore_ascii_case("all") {
            let had = !map.is_empty();
            for (_, tx) in map.drain() {
                let _ = tx.send(());
            }
            return had;
        }
        if let Some(tx) = map.remove(id) {
            let _ = tx.send(());
            true
        } else {
            false
        }
    }
}

fn resolve_cwd(cwd: Option<&str>) -> PathBuf {
    if let Some(raw) = cwd.map(str::trim).filter(|s| !s.is_empty()) {
        let p = PathBuf::from(raw);
        if p.exists() && p.is_dir() {
            return p;
        }
    }
    std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

pub async fn start_streaming_command(
    app: &AppHandle,
    ctrl: &Arc<TerminalController>,
    command: String,
    cwd: Option<String>,
) -> Result<String, String> {
    let clean = command.trim().to_string();
    if clean.is_empty() {
        return Err("Comando vazio.".to_string());
    }
    if let Some(reason) = crate::tools::is_dangerous_command(&clean) {
        return Err(format!("Comando bloqueado por segurança ({reason})."));
    }

    let work_dir = resolve_cwd(cwd.as_deref());
    let work_dir_str = work_dir.to_string_lossy().to_string();
    let id = Uuid::new_v4().to_string();

    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-NonInteractive", "-Command", &clean])
        .current_dir(&work_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Falha ao iniciar PowerShell: {e}"))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let (cancel_tx, mut cancel_rx) = oneshot::channel::<()>();

    {
        let mut map = ctrl.running.lock().await;
        map.insert(id.clone(), cancel_tx);
    }

    let _ = app.emit(
        "jarvis:terminal-event",
        json!({
            "id": id,
            "kind": "start",
            "command": clean,
            "cwd": work_dir_str,
        }),
    );

    let app_out = app.clone();
    let id_out = id.clone();
    let stdout_task = tokio::spawn(async move {
        if let Some(out) = stdout {
            let mut lines = BufReader::new(out).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = app_out.emit(
                    "jarvis:terminal-event",
                    json!({ "id": id_out, "kind": "stdout", "line": line }),
                );
            }
        }
    });

    let app_err = app.clone();
    let id_err = id.clone();
    let stderr_task = tokio::spawn(async move {
        if let Some(err) = stderr {
            let mut lines = BufReader::new(err).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = app_err.emit(
                    "jarvis:terminal-event",
                    json!({ "id": id_err, "kind": "stderr", "line": line }),
                );
            }
        }
    });

    let app_fin = app.clone();
    let ctrl_fin = Arc::clone(ctrl);
    let id_fin = id.clone();

    tokio::spawn(async move {
        let mut cancelled = false;
        let exit_code: i32 = tokio::select! {
            status = child.wait() => {
                status.ok().and_then(|s| s.code()).unwrap_or(-1)
            }
            _ = &mut cancel_rx => {
                cancelled = true;
                let _ = child.kill().await;
                -1
            }
        };

        let _ = stdout_task.await;
        let _ = stderr_task.await;
        {
            let mut map = ctrl_fin.running.lock().await;
            map.remove(&id_fin);
        }

        let _ = app_fin.emit(
            "jarvis:terminal-event",
            json!({
                "id": id_fin,
                "kind": "exit",
                "exitCode": exit_code,
                "cancelled": cancelled,
            }),
        );
    });

    Ok(id)
}

pub async fn run_and_collect_streaming(
    app: &AppHandle,
    ctrl: &Arc<TerminalController>,
    command: &str,
    cwd: Option<&str>,
    timeout_secs: u64,
) -> Result<String, String> {
    let clean = command.trim().to_string();
    if clean.is_empty() {
        return Err("Comando vazio.".to_string());
    }
    if let Some(reason) = crate::tools::is_dangerous_command(&clean) {
        return Err(format!("Comando bloqueado por segurança ({reason})."));
    }

    let work_dir = resolve_cwd(cwd);
    let work_dir_str = work_dir.to_string_lossy().to_string();
    let id = Uuid::new_v4().to_string();

    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-NonInteractive", "-Command", &clean])
        .current_dir(&work_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Falha ao iniciar PowerShell: {e}"))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let (cancel_tx, mut cancel_rx) = oneshot::channel::<()>();
    {
        let mut map = ctrl.running.lock().await;
        map.insert(id.clone(), cancel_tx);
    }

    let _ = app.emit(
        "jarvis:terminal-event",
        json!({
            "id": id,
            "kind": "start",
            "command": clean,
            "cwd": work_dir_str,
        }),
    );

    let collected = Arc::new(Mutex::new(Vec::<String>::new()));

    let app_out = app.clone();
    let id_out = id.clone();
    let col_out = Arc::clone(&collected);
    let stdout_task = tokio::spawn(async move {
        if let Some(out) = stdout {
            let mut lines = BufReader::new(out).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = app_out.emit(
                    "jarvis:terminal-event",
                    json!({ "id": id_out, "kind": "stdout", "line": line }),
                );
                let mut g = col_out.lock().await;
                if g.len() < 200 {
                    g.push(line);
                }
            }
        }
    });

    let app_err = app.clone();
    let id_err = id.clone();
    let col_err = Arc::clone(&collected);
    let stderr_task = tokio::spawn(async move {
        if let Some(err) = stderr {
            let mut lines = BufReader::new(err).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let _ = app_err.emit(
                    "jarvis:terminal-event",
                    json!({ "id": id_err, "kind": "stderr", "line": line }),
                );
                let mut g = col_err.lock().await;
                if g.len() < 200 {
                    g.push(format!("[stderr] {line}"));
                }
            }
        }
    });

    let mut cancelled = false;
    let mut timed_out = false;
    let exit_code: i32 = tokio::select! {
        status = child.wait() => {
            status.ok().and_then(|s| s.code()).unwrap_or(-1)
        }
        _ = &mut cancel_rx => {
            cancelled = true;
            let _ = child.kill().await;
            -1
        }
        _ = tokio::time::sleep(std::time::Duration::from_secs(timeout_secs.clamp(5, 180))) => {
            timed_out = true;
            let _ = child.kill().await;
            -1
        }
    };

    let _ = stdout_task.await;
    let _ = stderr_task.await;
    {
        let mut map = ctrl.running.lock().await;
        map.remove(&id);
    }
    let _ = app.emit(
        "jarvis:terminal-event",
        json!({
            "id": id,
            "kind": "exit",
            "exitCode": exit_code,
            "cancelled": cancelled || timed_out,
        }),
    );

    let out_lines = collected.lock().await.join("\n");
    let preview: String = out_lines.chars().take(6000).collect();
    if cancelled {
        Ok(format!("Comando cancelado pelo usuário.\n{preview}"))
    } else if timed_out {
        Ok(format!(
            "Comando atingiu limite de {timeout_secs}s e foi encerrado.\n{preview}"
        ))
    } else if preview.trim().is_empty() {
        Ok(format!(
            "Comando finalizado (exit code {exit_code}) sem saída."
        ))
    } else {
        Ok(format!("Exit code {exit_code}:\n{preview}"))
    }
}
