#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod browser;
mod config;
mod google_oauth;
mod llm;
mod mic;
mod oauth;
mod state;
mod storage;
mod tools;
mod tts;

use chrono::Utc;
use config::{
    clear_api_key as remove_api_key, get_all_key_status, get_api_key, load_config,
    save_api_key as store_api_key, save_config_file, AllKeyStatus, AppConfig, GeminiAuthMode,
    KeySlot, TtsEngine,
};
use google_oauth::GoogleAuthStatus;
use llm::{ConnectionTestResult, ImageAttachment};
use oauth::{ChatGptAuthStatus, ChatGptLoginResult};
use serde::Serialize;
use serde_json::json;
use state::{ActiveTimer, AppState};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use storage::{Conversation, ConversationSummary, MemoryItem, ProjectItem, StoredMessage};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tokio::sync::watch;
use tts::{ElevenLabsVoice, SpeechOutput};
use uuid::Uuid;

const MASCOT_LABEL: &str = "mascot";

#[cfg(windows)]
fn grant_webview_microphone_permission(win: &tauri::WebviewWindow) {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_PERMISSION_KIND, COREWEBVIEW2_PERMISSION_KIND_MICROPHONE,
        COREWEBVIEW2_PERMISSION_STATE_ALLOW,
    };
    use webview2_com::PermissionRequestedEventHandler;

    let _ = win.with_webview(|wv| unsafe {
        if let Ok(core) = wv.controller().CoreWebView2() {
            let mut token = 0i64;
            let handler = PermissionRequestedEventHandler::create(Box::new(|_sender, args| {
                if let Some(args) = args {
                    let mut kind = COREWEBVIEW2_PERMISSION_KIND(0);
                    if args.PermissionKind(&mut kind).is_ok()
                        && kind == COREWEBVIEW2_PERMISSION_KIND_MICROPHONE
                    {
                        let _ = args.SetState(COREWEBVIEW2_PERMISSION_STATE_ALLOW);
                    }
                }
                Ok(())
            }));
            let _ = core.add_PermissionRequested(&handler, &mut token);
        }
    });
}

fn show_main_window(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
    }
}

fn ensure_mascot_window(app: &AppHandle, show: bool) -> Result<(), String> {
    if let Some(existing) = app.get_webview_window(MASCOT_LABEL) {
        if show {
            let _ = existing.show();
        } else {
            let _ = existing.hide();
        }
        return Ok(());
    }

    if !show {
        return Ok(());
    }

    let (pos_x, pos_y) = if let Ok(Some(mon)) = app.primary_monitor() {
        let scale = mon.scale_factor();
        let w = mon.size().width as f64 / scale;
        let h = mon.size().height as f64 / scale;
        ((w - 278.0).max(24.0), (h - 310.0).max(24.0))
    } else {
        (1040.0, 500.0)
    };

    let win = tauri::WebviewWindowBuilder::new(
        app,
        MASCOT_LABEL,
        tauri::WebviewUrl::App("index.html?window=mascot".into()),
    )
    .title("J.A.R.V.I.S. Mascote")
    .inner_size(264.0, 272.0)
    .position(pos_x, pos_y)
    .decorations(false)
    .transparent(true)
    .shadow(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(false)
    .focused(false)
    .build()
    .map_err(|e| format!("Falha ao criar janela do mascote: {e}"))?;

    let _ = win.show();
    Ok(())
}

fn toggle_mascot_window(app: &AppHandle) {
    if let Some(win) = app.get_webview_window(MASCOT_LABEL) {
        let vis = win.is_visible().unwrap_or(false);
        if vis {
            let _ = win.hide();
        } else {
            let _ = win.show();
        }
    } else {
        let _ = ensure_mascot_window(app, true);
    }
}

fn toggle_main_window(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let visible = win.is_visible().unwrap_or(false);
        let minimized = win.is_minimized().unwrap_or(false);
        let focused = win.is_focused().unwrap_or(false);
        if visible && !minimized && focused {
            let _ = win.hide();
        } else {
            let _ = win.unminimize();
            let _ = win.show();
            let _ = win.set_focus();
        }
    }
}

#[tauri::command]
async fn get_config(state: State<'_, Arc<AppState>>) -> Result<AppConfig, String> {
    Ok(state.config.read().await.clone())
}

#[tauri::command]
async fn save_config(
    state: State<'_, Arc<AppState>>,
    config: AppConfig,
) -> Result<AppConfig, String> {
    save_config_file(&state.data_dir, &config)?;
    let normalized = load_config(&state.data_dir);
    let mut guard = state.config.write().await;
    *guard = normalized.clone();
    Ok(normalized)
}

#[tauri::command]
async fn get_key_status(state: State<'_, Arc<AppState>>) -> Result<AllKeyStatus, String> {
    Ok(get_all_key_status(&state.data_dir))
}

#[tauri::command]
async fn save_api_key(
    state: State<'_, Arc<AppState>>,
    slot: KeySlot,
    key: String,
) -> Result<AllKeyStatus, String> {
    store_api_key(&state.data_dir, slot, &key)?;
    Ok(get_all_key_status(&state.data_dir))
}

#[tauri::command]
async fn clear_api_key(
    state: State<'_, Arc<AppState>>,
    slot: KeySlot,
) -> Result<AllKeyStatus, String> {
    remove_api_key(&state.data_dir, slot)?;
    Ok(get_all_key_status(&state.data_dir))
}

#[tauri::command]
async fn test_connection(state: State<'_, Arc<AppState>>) -> Result<ConnectionTestResult, String> {
    Ok(llm::test_provider_connection(&state).await)
}

#[tauri::command]
async fn update_global_shortcut(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    shortcut: String,
) -> Result<AppConfig, String> {
    let next = shortcut.trim().to_string();
    let prev = state.config.read().await.global_shortcut.clone();

    let gs = app.global_shortcut();
    let _ = gs.unregister_all();

    if !next.is_empty() {
        if let Err(err) = gs.register(next.as_str()) {
            if !prev.trim().is_empty() {
                let _ = gs.register(prev.as_str());
            }
            return Err(format!("Não foi possível registrar o atalho '{next}': {err}"));
        }
    }

    let mut cfg = state.config.write().await;
    cfg.global_shortcut = next;
    save_config_file(&state.data_dir, &cfg)?;
    Ok(cfg.clone())
}

#[tauri::command]
async fn list_conversations(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<ConversationSummary>, String> {
    storage::list_conversations(&state.data_dir)
}

#[tauri::command]
async fn get_conversation(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<Conversation, String> {
    storage::get_conversation(&state.data_dir, &id)
}

#[tauri::command]
async fn create_conversation(
    state: State<'_, Arc<AppState>>,
    title: Option<String>,
    project_id: Option<String>,
) -> Result<Conversation, String> {
    let resolved_project = match project_id.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(pid) => storage::list_projects(&state.data_dir)?
            .into_iter()
            .find(|p| p.id == pid),
        None => None,
    };
    storage::create_conversation(
        &state.data_dir,
        title.as_deref(),
        resolved_project.as_ref().map(|p| p.id.as_str()),
        resolved_project.as_ref().map(|p| p.path.as_str()),
    )
}

#[tauri::command]
async fn set_conversation_project(
    state: State<'_, Arc<AppState>>,
    id: String,
    project_id: Option<String>,
) -> Result<Conversation, String> {
    let resolved_project = match project_id.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(pid) => storage::list_projects(&state.data_dir)?
            .into_iter()
            .find(|p| p.id == pid),
        None => None,
    };
    storage::set_conversation_project(
        &state.data_dir,
        &id,
        resolved_project.as_ref().map(|p| p.id.as_str()),
        resolved_project.as_ref().map(|p| p.path.as_str()),
    )
}

#[tauri::command]
async fn list_projects(state: State<'_, Arc<AppState>>) -> Result<Vec<ProjectItem>, String> {
    storage::list_projects(&state.data_dir)
}

#[tauri::command]
async fn add_project(
    state: State<'_, Arc<AppState>>,
    path: String,
    name: Option<String>,
) -> Result<ProjectItem, String> {
    storage::add_project(&state.data_dir, &path, name.as_deref())
}

#[tauri::command]
async fn delete_project(state: State<'_, Arc<AppState>>, id: String) -> Result<(), String> {
    storage::delete_project(&state.data_dir, &id)
}

#[tauri::command]
async fn pick_project_folder() -> Result<Option<String>, String> {
    tokio::task::spawn_blocking(|| -> Result<Option<String>, String> {
        #[cfg(windows)]
        {
            use windows::Win32::System::Com::{
                CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize,
                CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
            };
            use windows::Win32::UI::Shell::{
                FileOpenDialog, IFileOpenDialog, FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS,
                SIGDN_FILESYSPATH,
            };

            unsafe {
                let init_ok = CoInitializeEx(None, COINIT_APARTMENTTHREADED).is_ok();
                let res = (|| -> Result<Option<String>, String> {
                    let dialog: IFileOpenDialog =
                        CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)
                            .map_err(|e| format!("Falha ao abrir seletor de pasta: {e}"))?;
                    let opts = dialog
                        .GetOptions()
                        .map_err(|e| format!("Falha ao configurar seletor: {e}"))?;
                    dialog
                        .SetOptions(opts | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM)
                        .map_err(|e| format!("Falha ao configurar seletor: {e}"))?;
                    if dialog.Show(None).is_err() {
                        return Ok(None); // User cancelled
                    }
                    let item = dialog
                        .GetResult()
                        .map_err(|e| format!("Falha ao ler pasta selecionada: {e}"))?;
                    let pwstr = item
                        .GetDisplayName(SIGDN_FILESYSPATH)
                        .map_err(|e| format!("Falha ao ler caminho da pasta: {e}"))?;
                    let path_str = pwstr.to_string().unwrap_or_default();
                    CoTaskMemFree(Some(pwstr.0 as _));
                    if path_str.trim().is_empty() {
                        Ok(None)
                    } else {
                        Ok(Some(path_str))
                    }
                })();
                if init_ok {
                    CoUninitialize();
                }
                res
            }
        }
        #[cfg(not(windows))]
        {
            Ok(None)
        }
    })
    .await
    .map_err(|e| format!("Falha ao abrir seletor de pasta: {e}"))?
}

#[tauri::command]
async fn toggle_fullscreen(app: AppHandle) -> Result<bool, String> {
    let win = app
        .get_webview_window("main")
        .ok_or_else(|| "Janela principal não encontrada.".to_string())?;
    let next = !win.is_fullscreen().unwrap_or(false);
    win.set_fullscreen(next)
        .map_err(|e| format!("Falha ao alternar tela cheia: {e}"))?;
    Ok(next)
}

#[tauri::command]
async fn get_fullscreen(app: AppHandle) -> Result<bool, String> {
    let win = app
        .get_webview_window("main")
        .ok_or_else(|| "Janela principal não encontrada.".to_string())?;
    Ok(win.is_fullscreen().unwrap_or(false))
}

#[tauri::command]
async fn rename_conversation(
    state: State<'_, Arc<AppState>>,
    id: String,
    title: String,
) -> Result<Conversation, String> {
    storage::rename_conversation(&state.data_dir, &id, &title)
}

#[tauri::command]
async fn delete_conversation(
    state: State<'_, Arc<AppState>>,
    id: String,
) -> Result<(), String> {
    storage::delete_conversation(&state.data_dir, &id)
}

#[tauri::command]
async fn list_memories(state: State<'_, Arc<AppState>>) -> Result<Vec<MemoryItem>, String> {
    storage::list_memories(&state.data_dir)
}

#[tauri::command]
async fn add_memory(
    state: State<'_, Arc<AppState>>,
    content: String,
) -> Result<MemoryItem, String> {
    storage::add_memory(&state.data_dir, &content)
}

#[tauri::command]
async fn delete_memory(state: State<'_, Arc<AppState>>, id: String) -> Result<(), String> {
    storage::delete_memory(&state.data_dir, &id)
}

#[tauri::command]
async fn send_message(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    conversation_id: String,
    text: String,
    image_base64: Option<String>,
    image_mime: Option<String>,
) -> Result<Conversation, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("Mensagem vazia".to_string());
    }

    let attachment = image_base64
        .map(|b| b.trim().to_string())
        .filter(|b| !b.is_empty())
        .map(|data_base64| ImageAttachment {
            mime_type: image_mime
                .map(|m| m.trim().to_string())
                .filter(|m| !m.is_empty())
                .unwrap_or_else(|| "image/jpeg".to_string()),
            data_base64,
        });

    {
        let mut active = state.active_cancel.lock().await;
        if let Some(prev_tx) = active.take() {
            let _ = prev_tx.send(true);
        }
    }
    state.cancel_all_pending_approvals().await;

    let stored_text = if attachment.is_some() {
        format!("{trimmed}\n[Captura de tela anexada]")
    } else {
        trimmed.to_string()
    };

    let user_msg = StoredMessage {
        id: Uuid::new_v4().to_string(),
        role: "user".to_string(),
        text: stored_text,
        timestamp: Utc::now().to_rfc3339(),
        mode: None,
        tool_events: Vec::new(),
    };
    let updated_conv = storage::append_message(&state.data_dir, &conversation_id, user_msg)?;

    let (cancel_tx, cancel_rx) = watch::channel(false);
    {
        let mut active = state.active_cancel.lock().await;
        *active = Some(cancel_tx);
    }

    let state_arc = Arc::clone(&state);
    let conv_id = updated_conv.id.clone();
    let history = updated_conv.messages.clone();
    let project_path = updated_conv.project_path.clone();

    tokio::spawn(async move {
        let result = llm::run_chat_turn(
            &app,
            &state_arc,
            &conv_id,
            &history,
            project_path,
            attachment,
            cancel_rx,
        )
        .await;

        {
            let mut active = state_arc.active_cancel.lock().await;
            *active = None;
        }
        state_arc.cancel_all_pending_approvals().await;

        match result {
            Ok((reply_text, response_mode, traces)) => {
                let assistant_msg = StoredMessage {
                    id: Uuid::new_v4().to_string(),
                    role: "assistant".to_string(),
                    text: reply_text,
                    timestamp: Utc::now().to_rfc3339(),
                    mode: Some(response_mode),
                    tool_events: traces,
                };
                match storage::append_message(&state_arc.data_dir, &conv_id, assistant_msg.clone())
                {
                    Ok(_) => {
                        let _ = app.emit(
                            "jarvis:chat-event",
                            json!({
                                "type": "done",
                                "conversationId": conv_id,
                                "message": assistant_msg
                            }),
                        );
                    }
                    Err(e) => {
                        let _ = app.emit(
                            "jarvis:chat-event",
                            json!({
                                "type": "error",
                                "conversationId": conv_id,
                                "error": e
                            }),
                        );
                    }
                }
            }
            Err(err) if err == "CANCELLED" => {
                let _ = app.emit(
                    "jarvis:chat-event",
                    json!({
                        "type": "cancelled",
                        "conversationId": conv_id
                    }),
                );
            }
            Err(err) => {
                let _ = app.emit(
                    "jarvis:chat-event",
                    json!({
                        "type": "error",
                        "conversationId": conv_id,
                        "error": err
                    }),
                );
            }
        }
    });

    Ok(updated_conv)
}

#[tauri::command]
async fn cancel_chat(state: State<'_, Arc<AppState>>) -> Result<(), String> {
    {
        let mut active = state.active_cancel.lock().await;
        if let Some(tx) = active.take() {
            let _ = tx.send(true);
        }
    }
    state.cancel_all_pending_approvals().await;
    Ok(())
}

#[tauri::command]
async fn resolve_tool_approval(
    state: State<'_, Arc<AppState>>,
    approval_id: String,
    approved: bool,
) -> Result<(), String> {
    let mut map = state.pending_approvals.lock().await;
    if let Some(tx) = map.remove(&approval_id) {
        let _ = tx.send(approved);
    }
    Ok(())
}

#[tauri::command]
async fn list_active_timers(state: State<'_, Arc<AppState>>) -> Result<Vec<ActiveTimer>, String> {
    Ok(state.list_active_timers().await)
}

#[tauri::command]
async fn cancel_timer(state: State<'_, Arc<AppState>>, id: String) -> Result<bool, String> {
    Ok(state.cancel_timer_by_id(&id).await)
}

#[tauri::command]
async fn list_elevenlabs_voices(
    state: State<'_, Arc<AppState>>,
) -> Result<Vec<ElevenLabsVoice>, String> {
    let key = get_api_key(&state.data_dir, KeySlot::Elevenlabs)
        .ok_or_else(|| "Salve sua chave da ElevenLabs para listar as vozes da conta.".to_string())?;
    tts::list_elevenlabs_voices(&state.http, &key).await
}

#[tauri::command]
async fn synthesize_speech(
    state: State<'_, Arc<AppState>>,
    text: String,
) -> Result<SpeechOutput, String> {
    let cfg = state.config.read().await.clone();
    match cfg.tts_engine {
        TtsEngine::Off => Err("TTS desativado".to_string()),
        TtsEngine::Elevenlabs => {
            let key = get_api_key(&state.data_dir, KeySlot::Elevenlabs).ok_or_else(|| {
                "Chave da ElevenLabs não configurada. Abra Ajustes → Voz para colar sua chave."
                    .to_string()
            })?;
            let bytes = tts::synthesize_elevenlabs_mp3(
                &state.http,
                &state.data_dir,
                &key,
                &text,
                &cfg.elevenlabs_voice_id,
                &cfg.elevenlabs_model_id,
                cfg.tts_rate,
            )
            .await?;
            Ok(tts::encode_speech_output("audio/mpeg", &bytes))
        }
    }
}

#[tauri::command]
async fn transcribe_audio(
    state: State<'_, Arc<AppState>>,
    audio_base64: String,
    mime_type: String,
) -> Result<String, String> {
    llm::transcribe_audio(&state, &audio_base64, &mime_type).await
}

#[tauri::command]
async fn list_microphones() -> Result<Vec<mic::MicDeviceInfo>, String> {
    Ok(tokio::task::spawn_blocking(mic::list_microphones)
        .await
        .unwrap_or_default())
}

#[tauri::command]
async fn start_native_mic_recording(
    app: AppHandle,
    mic_ctrl: State<'_, Arc<mic::MicController>>,
    vad_enabled: bool,
    device_id: Option<String>,
) -> Result<(), String> {
    mic_ctrl.start_recording(app, vad_enabled, device_id)
}

#[tauri::command]
async fn stop_native_mic_recording(
    mic_ctrl: State<'_, Arc<mic::MicController>>,
) -> Result<mic::RecordedAudio, String> {
    let ctrl = Arc::clone(&mic_ctrl);
    tokio::task::spawn_blocking(move || ctrl.stop_recording())
        .await
        .map_err(|e| format!("Falha ao finalizar gravação: {e}"))
}

#[tauri::command]
async fn start_native_mic_test(
    app: AppHandle,
    mic_ctrl: State<'_, Arc<mic::MicController>>,
    device_id: Option<String>,
) -> Result<(), String> {
    mic_ctrl.start_mic_test(app, device_id)
}

#[tauri::command]
async fn stop_native_mic_test(
    mic_ctrl: State<'_, Arc<mic::MicController>>,
) -> Result<(), String> {
    mic_ctrl.stop_mic_test();
    Ok(())
}

#[tauri::command]
async fn set_native_wake_word(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    mic_ctrl: State<'_, Arc<mic::MicController>>,
    enabled: bool,
    device_id: Option<String>,
) -> Result<(), String> {
    mic_ctrl.set_wake_word(app, Arc::clone(&state), enabled, device_id)
}

#[tauri::command]
async fn set_native_wake_paused(
    mic_ctrl: State<'_, Arc<mic::MicController>>,
    paused: bool,
) -> Result<(), String> {
    mic_ctrl.set_wake_paused(paused);
    Ok(())
}

#[tauri::command]
async fn get_chatgpt_auth_status(
    state: State<'_, Arc<AppState>>,
) -> Result<ChatGptAuthStatus, String> {
    Ok(oauth::get_auth_status(&state.data_dir))
}

#[tauri::command]
async fn start_chatgpt_login(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    force_register: Option<bool>,
) -> Result<ChatGptLoginResult, String> {
    let (status, models) =
        oauth::start_chatgpt_login(&state.http, &state.data_dir, force_register.unwrap_or(false))
            .await?;

    show_main_window(&app);

    let mut cfg = state.config.write().await;
    let current = cfg.chatgpt_model.trim().to_string();
    let legacy = current.is_empty() || current == "gpt-5" || current.starts_with("gpt-5.");
    let missing_in_catalog = !models.is_empty() && !models.iter().any(|m| m == &current);
    if legacy || missing_in_catalog {
        cfg.chatgpt_model = oauth::pick_best_chatgpt_model(&models, &[]);
        let _ = save_config_file(&state.data_dir, &cfg);
    }
    let selected_model = cfg.chatgpt_model.clone();
    drop(cfg);

    Ok(ChatGptLoginResult {
        status,
        models,
        selected_model,
    })
}

#[tauri::command]
async fn logout_chatgpt(
    state: State<'_, Arc<AppState>>,
    reset_registration: Option<bool>,
) -> Result<ChatGptAuthStatus, String> {
    oauth::logout_chatgpt(
        &state.http,
        &state.data_dir,
        reset_registration.unwrap_or(false),
    )
    .await
}

#[tauri::command]
async fn list_chatgpt_models(state: State<'_, Arc<AppState>>) -> Result<Vec<String>, String> {
    oauth::list_chatgpt_models(&state.http, &state.data_dir).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GoogleImportResult {
    config: AppConfig,
    key_status: AllKeyStatus,
    google_auth: GoogleAuthStatus,
}

#[tauri::command]
async fn get_google_auth_status(
    state: State<'_, Arc<AppState>>,
) -> Result<GoogleAuthStatus, String> {
    Ok(google_oauth::get_google_auth_status(&state.data_dir))
}

#[tauri::command]
async fn import_google_oauth_json(
    state: State<'_, Arc<AppState>>,
    json_text: String,
) -> Result<GoogleImportResult, String> {
    let (client_id, client_secret, project_id) =
        google_oauth::parse_google_credentials_json(&json_text)?;
    store_api_key(
        &state.data_dir,
        KeySlot::GoogleClientSecret,
        &client_secret,
    )?;

    let mut cfg = state.config.write().await;
    cfg.gemini_auth_mode = GeminiAuthMode::Oauth;
    cfg.google_oauth_client_id = client_id;
    if let Some(pid) = project_id {
        if !pid.trim().is_empty() {
            cfg.google_oauth_project_id = pid;
        }
    }
    save_config_file(&state.data_dir, &cfg)?;
    let updated_cfg = cfg.clone();
    drop(cfg);

    Ok(GoogleImportResult {
        config: updated_cfg,
        key_status: get_all_key_status(&state.data_dir),
        google_auth: google_oauth::get_google_auth_status(&state.data_dir),
    })
}

#[tauri::command]
async fn start_google_login(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
) -> Result<GoogleAuthStatus, String> {
    let cfg = state.config.read().await.clone();
    let client_id = cfg.google_oauth_client_id.trim().to_string();
    if client_id.is_empty() {
        return Err(
            "Informe o OAuth Client ID do Google Cloud (ou importe o arquivo JSON) antes de entrar."
                .to_string(),
        );
    }
    let client_secret =
        get_api_key(&state.data_dir, KeySlot::GoogleClientSecret).ok_or_else(|| {
            "Informe o OAuth Client Secret do Google Cloud (ou importe o arquivo JSON) antes de entrar."
                .to_string()
        })?;
    let project_id = if cfg.google_oauth_project_id.trim().is_empty() {
        None
    } else {
        Some(cfg.google_oauth_project_id.trim().to_string())
    };

    let status = google_oauth::start_google_login(
        &state.http,
        &state.data_dir,
        &client_id,
        &client_secret,
        project_id.as_deref(),
    )
    .await?;

    show_main_window(&app);

    if cfg.gemini_auth_mode != GeminiAuthMode::Oauth {
        let mut guard = state.config.write().await;
        guard.gemini_auth_mode = GeminiAuthMode::Oauth;
        let _ = save_config_file(&state.data_dir, &guard);
    }

    Ok(status)
}

#[tauri::command]
async fn logout_google(state: State<'_, Arc<AppState>>) -> Result<GoogleAuthStatus, String> {
    google_oauth::logout_google(&state.http, &state.data_dir).await
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScreenCaptureOutput {
    data_base64: String,
    mime_type: String,
    width: u32,
    height: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowserPageOutput {
    url: String,
    final_url: String,
    title: String,
    text: String,
}

#[tauri::command]
async fn start_agent_task(
    app: AppHandle,
    state: State<'_, Arc<AppState>>,
    role: String,
    task: String,
) -> Result<String, String> {
    llm::spawn_background_agent(&app, &state, &role, None, &task).await
}

#[tauri::command]
async fn cancel_agent_task(
    state: State<'_, Arc<AppState>>,
    agent_id: String,
) -> Result<(), String> {
    let mut map = state.agent_cancels.lock().await;
    if let Some(tx) = map.remove(&agent_id) {
        let _ = tx.send(true);
    }
    Ok(())
}

#[tauri::command]
async fn capture_screen() -> Result<ScreenCaptureOutput, String> {
    let (width, height, _rw, _rh, data_base64) =
        tokio::task::spawn_blocking(tools::capture_desktop_screen_blocking)
            .await
            .map_err(|e| format!("Falha na tarefa de captura de tela: {e}"))??;
    Ok(ScreenCaptureOutput {
        data_base64,
        mime_type: "image/jpeg".to_string(),
        width,
        height,
    })
}

#[tauri::command]
async fn fetch_browser_page(
    state: State<'_, Arc<AppState>>,
    url: String,
) -> Result<BrowserPageOutput, String> {
    let parsed = tools::validate_http_url(&url)?;
    let resp = state
        .http
        .get(parsed.as_str())
        .header(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36",
        )
        .send()
        .await
        .map_err(|e| format!("Falha ao carregar página: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("O site retornou HTTP {}.", resp.status()));
    }

    let final_url = resp.url().to_string();
    let html = resp
        .text()
        .await
        .map_err(|e| format!("Falha ao ler conteúdo da página: {e}"))?;

    let title = regex::Regex::new(r"(?is)<title[^>]*>(.*?)</title>")
        .ok()
        .and_then(|re| re.captures(&html))
        .map(|c| c[1].trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| parsed.host_str().unwrap_or("Página Web").to_string());

    let no_script = regex::Regex::new(r"(?is)<(script|style|noscript|svg)[^>]*>.*?</(script|style|noscript|svg)>")
        .map(|re| re.replace_all(&html, " ").into_owned())
        .unwrap_or(html);
    let with_breaks = regex::Regex::new(r"(?i)</(p|div|h1|h2|h3|h4|li|br|tr|section|article)>")
        .map(|re| re.replace_all(&no_script, "\n").into_owned())
        .unwrap_or(no_script);
    let no_tags = regex::Regex::new(r"(?s)<[^>]+>")
        .map(|re| re.replace_all(&with_breaks, " ").into_owned())
        .unwrap_or(with_breaks);

    let decoded = no_tags
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ");

    let mut clean_lines = Vec::new();
    for line in decoded.lines() {
        let collapsed: String = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if collapsed.len() >= 2 {
            clean_lines.push(collapsed);
        }
    }
    let joined = clean_lines.join("\n\n");
    let text: String = joined.chars().take(12_000).collect();

    Ok(BrowserPageOutput {
        url: parsed.to_string(),
        final_url,
        title,
        text,
    })
}

#[tauri::command]
async fn open_native_browser_window(
    _app: AppHandle,
    url: String,
    title: Option<String>,
) -> Result<(), String> {
    let _ = title;
    let parsed = tools::validate_http_url(&url)?;
    open::that(parsed.as_str()).map_err(|e| format!("Falha ao abrir no navegador padrão: {e}"))?;
    Ok(())
}

#[tauri::command]
async fn browser_sync_bounds(
    app: AppHandle,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    visible: bool,
    initial_url: Option<String>,
) -> Result<browser::BrowserStatus, String> {
    browser::sync_bounds(&app, x, y, width, height, visible, initial_url.as_deref()).await
}

#[tauri::command]
async fn browser_set_visible(app: AppHandle, visible: bool) -> Result<(), String> {
    browser::set_visible(&app, visible).await
}

#[tauri::command]
async fn browser_status(app: AppHandle) -> Result<browser::BrowserStatus, String> {
    browser::get_status(&app).await
}

#[tauri::command]
async fn browser_inspect_live(
    app: AppHandle,
    show_overlay: Option<bool>,
) -> Result<browser::BrowserInspectResult, String> {
    browser::inspect_page(&app, show_overlay.unwrap_or(false)).await
}

#[tauri::command]
async fn browser_navigate(
    app: AppHandle,
    url: String,
) -> Result<browser::BrowserInspectResult, String> {
    browser::navigate(&app, &url).await
}

#[tauri::command]
async fn browser_snapshot(
    app: AppHandle,
    show_overlay: Option<bool>,
) -> Result<browser::BrowserSnapshot, String> {
    browser::capture_snapshot(&app, show_overlay.unwrap_or(false)).await
}

#[tauri::command]
async fn browser_back(app: AppHandle) -> Result<browser::BrowserStatus, String> {
    browser::go_back(&app).await
}

#[tauri::command]
async fn browser_forward(app: AppHandle) -> Result<browser::BrowserStatus, String> {
    browser::go_forward(&app).await
}

#[tauri::command]
async fn browser_reload(app: AppHandle) -> Result<browser::BrowserStatus, String> {
    browser::reload(&app).await
}

#[tauri::command]
async fn browser_click(
    app: AppHandle,
    element_id: Option<u32>,
    text: Option<String>,
    x: Option<f64>,
    y: Option<f64>,
    show_overlay: Option<bool>,
) -> Result<browser::BrowserInspectResult, String> {
    let _ = browser::click(&app, element_id, text.as_deref(), x, y).await?;
    browser::inspect_page(&app, show_overlay.unwrap_or(false)).await
}

#[tauri::command]
async fn browser_type(
    app: AppHandle,
    text: String,
    element_id: Option<u32>,
    target_label: Option<String>,
    clear_first: Option<bool>,
    press_enter: Option<bool>,
    show_overlay: Option<bool>,
) -> Result<browser::BrowserInspectResult, String> {
    let _ = browser::type_text(
        &app,
        &text,
        element_id,
        target_label.as_deref(),
        clear_first.unwrap_or(false),
        press_enter.unwrap_or(false),
    )
    .await?;
    browser::inspect_page(&app, show_overlay.unwrap_or(false)).await
}

#[tauri::command]
async fn browser_scroll(
    app: AppHandle,
    direction: String,
    amount: Option<i32>,
    show_overlay: Option<bool>,
) -> Result<browser::BrowserInspectResult, String> {
    let _ = browser::scroll_page(&app, &direction, amount).await?;
    browser::inspect_page(&app, show_overlay.unwrap_or(false)).await
}

#[tauri::command]
async fn browser_press_key(
    app: AppHandle,
    key: String,
    show_overlay: Option<bool>,
) -> Result<browser::BrowserInspectResult, String> {
    let _ = browser::press_key(&app, &key).await?;
    browser::inspect_page(&app, show_overlay.unwrap_or(false)).await
}

#[tauri::command]
async fn read_clipboard_content() -> Result<tools::ClipboardContent, String> {
    tokio::task::spawn_blocking(tools::read_clipboard_blocking)
        .await
        .map_err(|e| format!("Falha ao ler área de transferência: {e}"))?
}

#[tauri::command]
async fn revert_file_change(
    path: String,
    existed_before: bool,
    old_content: String,
) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        tools::revert_file_change_blocking(&path, existed_before, &old_content)
    })
    .await
    .map_err(|e| format!("Falha ao reverter arquivo: {e}"))?
}

#[tauri::command]
async fn set_mascot_visible(app: AppHandle, visible: bool) -> Result<(), String> {
    ensure_mascot_window(&app, visible)
}

#[tauri::command]
async fn sync_mascot_state(
    app: AppHandle,
    mode: String,
    speaking: bool,
    theme: String,
    reduced: bool,
    bubble: Option<String>,
    reaction: Option<String>,
    muted: Option<bool>,
    wake_word: Option<bool>,
) -> Result<(), String> {
    let _ = app.emit_to(
        MASCOT_LABEL,
        "jarvis:mascot-state",
        json!({
            "mode": mode,
            "speaking": speaking,
            "theme": theme,
            "reduced": reduced,
            "bubble": bubble,
            "reaction": reaction,
            "muted": muted.unwrap_or(false),
            "wakeWord": wake_word.unwrap_or(false),
        }),
    );
    Ok(())
}

#[tauri::command]
async fn mascot_action(app: AppHandle, action: String) -> Result<(), String> {
    match action.as_str() {
        "listen" => {
            let _ = app.emit_to("main", "jarvis:tray-action", json!({ "action": "listen" }));
        }
        "open_chat" => {
            show_main_window(&app);
            let _ = app.emit_to("main", "jarvis:tray-action", json!({ "action": "focus_chat" }));
        }
        "new_chat" => {
            show_main_window(&app);
            let _ = app.emit_to("main", "jarvis:tray-action", json!({ "action": "new_chat" }));
        }
        "see_screen" => {
            let _ = app.emit_to("main", "jarvis:tray-action", json!({ "action": "see_screen" }));
        }
        "toggle_mute" => {
            let _ = app.emit_to("main", "jarvis:tray-action", json!({ "action": "toggle_mute" }));
        }
        "toggle_wake" => {
            let _ = app.emit_to("main", "jarvis:tray-action", json!({ "action": "toggle_wake" }));
        }
        "cycle_theme" => {
            let _ = app.emit_to("main", "jarvis:tray-action", json!({ "action": "cycle_theme" }));
        }
        "hide" => {
            let _ = ensure_mascot_window(&app, false);
            let _ = app.emit_to("main", "jarvis:tray-action", json!({ "action": "hide_mascot" }));
        }
        _ => {}
    }
    Ok(())
}

fn setup_tray(app: &AppHandle, state: Arc<AppState>) -> tauri::Result<()> {
    let toggle_item =
        MenuItem::with_id(app, "toggle", "Mostrar / Ocultar J.A.R.V.I.S.", true, None::<&str>)?;
    let mascot_item =
        MenuItem::with_id(app, "mascot", "Mostrar / Ocultar Mascote", true, None::<&str>)?;
    let listen_item = MenuItem::with_id(app, "listen", "Ouvir agora", true, None::<&str>)?;
    let new_chat_item = MenuItem::with_id(app, "new_chat", "Nova conversa", true, None::<&str>)?;
    let sep = PredefinedMenuItem::separator(app)?;
    let quit_item = MenuItem::with_id(app, "quit", "Sair", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[&toggle_item, &mascot_item, &listen_item, &new_chat_item, &sep, &quit_item],
    )?;

    let icon = app
        .default_window_icon()
        .cloned()
        .or_else(|| tauri::image::Image::from_bytes(include_bytes!("../icons/icon.ico")).ok())
        .ok_or_else(|| tauri::Error::AssetNotFound("icons/icon.ico".into()))?;

    TrayIconBuilder::with_id("main-tray")
        .icon(icon)
        .tooltip("J.A.R.V.I.S. — The Singularity Edition")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "toggle" => toggle_main_window(app),
            "mascot" => toggle_mascot_window(app),
            "listen" => {
                show_main_window(app);
                let _ = app.emit("jarvis:tray-action", json!({ "action": "listen" }));
            }
            "new_chat" => {
                show_main_window(app);
                let _ = app.emit("jarvis:tray-action", json!({ "action": "new_chat" }));
            }
            "quit" => {
                state.quitting.store(true, Ordering::SeqCst);
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_main_window(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

fn main() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        toggle_main_window(app);
                    }
                })
                .build(),
        )
        .setup(|app| {
            let data_dir = match std::env::var_os("JARVIS_DATA_DIR").filter(|v| !v.is_empty()) {
                Some(custom) => std::path::PathBuf::from(custom),
                None => app.path().app_data_dir()?,
            };
            std::fs::create_dir_all(&data_dir)?;

            let cfg = load_config(&data_dir);
            let initial_shortcut = cfg.global_shortcut.clone();
            let app_state = Arc::new(
                AppState::new(data_dir, cfg)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?,
            );

            app.manage(Arc::clone(&app_state));
            app.manage(Arc::new(mic::MicController::default()));

            #[cfg(windows)]
            if let Some(main_win) = app.get_webview_window("main") {
                grant_webview_microphone_permission(&main_win);
            }

            if let Err(err) = setup_tray(app.handle(), Arc::clone(&app_state)) {
                eprintln!("Aviso: falha ao inicializar bandeja do sistema: {err}");
            }

            if !initial_shortcut.trim().is_empty() {
                if let Err(err) = app.global_shortcut().register(initial_shortcut.as_str()) {
                    eprintln!(
                        "Aviso: atalho global '{}' indisponível: {err}",
                        initial_shortcut
                    );
                }
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == browser::BROWSER_LABEL || window.label() == MASCOT_LABEL {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    let state = window.state::<Arc<AppState>>();
                    if !state.quitting.load(Ordering::SeqCst) {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                }
                return;
            }
            if window.label() != "main" {
                return;
            }
            if let WindowEvent::CloseRequested { api, .. } = event {
                let state = window.state::<Arc<AppState>>();
                let is_quitting = state.quitting.load(Ordering::SeqCst);
                let minimize_to_tray = state
                    .config
                    .try_read()
                    .map(|c| c.minimize_to_tray)
                    .unwrap_or(true);
                if !is_quitting && minimize_to_tray {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            save_config,
            get_key_status,
            save_api_key,
            clear_api_key,
            test_connection,
            update_global_shortcut,
            list_conversations,
            get_conversation,
            create_conversation,
            set_conversation_project,
            rename_conversation,
            delete_conversation,
            list_projects,
            add_project,
            delete_project,
            pick_project_folder,
            toggle_fullscreen,
            get_fullscreen,
            list_memories,
            add_memory,
            delete_memory,
            send_message,
            cancel_chat,
            resolve_tool_approval,
            list_active_timers,
            cancel_timer,
            list_elevenlabs_voices,
            synthesize_speech,
            transcribe_audio,
            list_microphones,
            start_native_mic_recording,
            stop_native_mic_recording,
            start_native_mic_test,
            stop_native_mic_test,
            set_native_wake_word,
            set_native_wake_paused,
            get_chatgpt_auth_status,
            start_chatgpt_login,
            logout_chatgpt,
            list_chatgpt_models,
            get_google_auth_status,
            import_google_oauth_json,
            start_google_login,
            logout_google,
            start_agent_task,
            cancel_agent_task,
            capture_screen,
            fetch_browser_page,
            open_native_browser_window,
            browser_sync_bounds,
            browser_set_visible,
            browser_status,
            browser_inspect_live,
            browser_navigate,
            browser_snapshot,
            browser_back,
            browser_forward,
            browser_reload,
            browser_click,
            browser_type,
            browser_scroll,
            browser_press_key,
            read_clipboard_content,
            revert_file_change,
            set_mascot_visible,
            sync_mascot_state,
            mascot_action,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run the JARVIS desktop application");
}
