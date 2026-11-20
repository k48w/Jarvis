use crate::config::AppConfig;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use tokio::sync::{oneshot, watch, Mutex, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveTimer {
    pub id: String,
    pub label: String,
    pub ends_at: String,
    pub total_seconds: u64,
}

pub struct TimerHandle {
    pub info: ActiveTimer,
    pub cancel_tx: oneshot::Sender<()>,
}

pub struct AppState {
    pub data_dir: PathBuf,
    pub config: RwLock<AppConfig>,
    pub http: reqwest::Client,
    pub active_cancel: Mutex<Option<watch::Sender<bool>>>,
    pub agent_cancels: Mutex<HashMap<String, watch::Sender<bool>>>,
    pub pending_approvals: Mutex<HashMap<String, oneshot::Sender<bool>>>,
    pub timers: Mutex<HashMap<String, TimerHandle>>,
    pub quitting: AtomicBool,
}

impl AppState {
    pub fn new(data_dir: PathBuf, config: AppConfig) -> Result<Self, String> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(90))
            .user_agent("JARVIS-Desktop/8.0")
            .build()
            .map_err(|e| format!("Falha ao inicializar cliente HTTP: {e}"))?;

        Ok(Self {
            data_dir,
            config: RwLock::new(config),
            http,
            active_cancel: Mutex::new(None),
            agent_cancels: Mutex::new(HashMap::new()),
            pending_approvals: Mutex::new(HashMap::new()),
            timers: Mutex::new(HashMap::new()),
            quitting: AtomicBool::new(false),
        })
    }

    pub async fn list_active_timers(&self) -> Vec<ActiveTimer> {
        let map = self.timers.lock().await;
        let mut out: Vec<ActiveTimer> = map.values().map(|h| h.info.clone()).collect();
        out.sort_by(|a, b| a.ends_at.cmp(&b.ends_at));
        out
    }

    pub async fn cancel_timer_by_id(&self, id: &str) -> bool {
        let mut map = self.timers.lock().await;
        if id.eq_ignore_ascii_case("all") {
            let had = !map.is_empty();
            for (_, handle) in map.drain() {
                let _ = handle.cancel_tx.send(());
            }
            return had;
        }
        if let Some(handle) = map.remove(id) {
            let _ = handle.cancel_tx.send(());
            true
        } else {
            false
        }
    }

    pub async fn cancel_all_pending_approvals(&self) {
        let mut map = self.pending_approvals.lock().await;
        for (_, tx) in map.drain() {
            let _ = tx.send(false);
        }
    }
}
