//! # Telemetria, Áudio CoreAudio, Apps, PowerShell e Timers — `tools/system.rs`
//!
//! Implementa leitura de hardware (sysinfo + bateria Win32), controle de
//! volume/mudo via CoreAudio, teclas de mídia SendInput, abertura de apps,
//! execução segura de PowerShell (com bloqueio de comandos destrutivos) e timers.

use crate::state::{ActiveTimer, AppState, TimerHandle};
use chrono::{Duration as ChronoDuration, Local};
use regex::Regex;
use serde_json::json;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::sync::oneshot;
use uuid::Uuid;
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
        Regex::new(
            r"(?i)(remove-item|rm|del|rmdir|rd)\b.*([a-z]:\\(\s|$|\*)|\\windows\\|\\system32\\)",
        )
        .expect("valid root delete regex")
    });
    if re_root.is_match(&norm) {
        return Some("exclusão em raiz de unidade ou diretório protegido do Windows");
    }

    None
}

pub(crate) fn format_datetime() -> String {
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

pub(crate) fn read_system_status_blocking() -> String {
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
    procs.sort_by_key(|a| std::cmp::Reverse(a.memory()));
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
pub(crate) fn set_windows_volume_blocking(
    level: Option<u8>,
    muted: Option<bool>,
) -> Result<String, String> {
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::Media::Audio::{
        eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator,
    };
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
pub(crate) fn set_windows_volume_blocking(
    _level: Option<u8>,
    _muted: Option<bool>,
) -> Result<String, String> {
    Err("Controle de volume disponível apenas no Windows".to_string())
}

#[cfg(windows)]
pub(crate) fn send_media_key(action: &str) -> Result<String, String> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
        VIRTUAL_KEY, VK_MEDIA_NEXT_TRACK, VK_MEDIA_PLAY_PAUSE, VK_MEDIA_PREV_TRACK, VK_MEDIA_STOP,
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
pub(crate) fn send_media_key(_action: &str) -> Result<String, String> {
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

pub(crate) fn launch_application(app: &str) -> Result<String, String> {
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

pub(crate) async fn run_powershell(command: &str) -> Result<String, String> {
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

pub(crate) async fn run_set_timer(
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
