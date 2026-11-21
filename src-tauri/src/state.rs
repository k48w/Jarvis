use crate::config::AppConfig;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use tokio::sync::{oneshot, watch, Mutex, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveTimer {
    pub id: String,
    pub label: String,
    pub ends_at: String,
    pub total_seconds: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRunSnapshot {
    pub id: String,
    pub role: String,
    pub name: String,
    pub task: String,
    pub status: String,
    pub output: String,
}

pub struct PendingApprovalEntry {
    pub request: Value,
    pub tx: oneshot::Sender<bool>,
    pub generation: u64,
}

pub struct TimerHandle {
    pub info: ActiveTimer,
    pub cancel_tx: oneshot::Sender<()>,
}

pub struct AppState {
    pub data_dir: PathBuf,
    pub config: RwLock<AppConfig>,
    pub http: reqwest::Client,
    pub active_cancel: Mutex<Option<(u64, watch::Sender<bool>)>>,
    pub cancel_gen: AtomicU64,
    pub agent_cancels: Mutex<HashMap<String, watch::Sender<bool>>>,
    pub agent_runs: Mutex<Vec<AgentRunSnapshot>>,
    pub pending_approvals: Mutex<HashMap<String, PendingApprovalEntry>>,
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
            cancel_gen: AtomicU64::new(0),
            agent_cancels: Mutex::new(HashMap::new()),
            agent_runs: Mutex::new(Vec::new()),
            pending_approvals: Mutex::new(HashMap::new()),
            timers: Mutex::new(HashMap::new()),
            quitting: AtomicBool::new(false),
        })
    }

    pub async fn begin_chat_cancel(&self) -> (u64, watch::Receiver<bool>) {
        let gen = self.cancel_gen.fetch_add(1, Ordering::SeqCst) + 1;
        let (tx, rx) = watch::channel(false);
        let mut active = self.active_cancel.lock().await;
        if let Some((_, prev_tx)) = active.take() {
            let _ = prev_tx.send(true);
        }
        *active = Some((gen, tx));
        (gen, rx)
    }

    pub async fn finish_chat_cancel(&self, gen: u64) {
        let mut active = self.active_cancel.lock().await;
        if let Some((current_gen, _)) = active.as_ref() {
            if *current_gen == gen {
                *active = None;
            }
        }
    }

    pub async fn cancel_active_chat(&self) -> bool {
        let mut active = self.active_cancel.lock().await;
        if let Some((_, tx)) = active.take() {
            let _ = tx.send(true);
            true
        } else {
            false
        }
    }

    #[cfg(test)]
    pub async fn register_approval(&self, id: String, request: Value, tx: oneshot::Sender<bool>) {
        self.register_approval_for_generation(id, request, tx, 0)
            .await;
    }

    pub async fn register_approval_for_generation(
        &self,
        id: String,
        request: Value,
        tx: oneshot::Sender<bool>,
        generation: u64,
    ) {
        let mut map = self.pending_approvals.lock().await;
        map.insert(
            id,
            PendingApprovalEntry {
                request,
                tx,
                generation,
            },
        );
    }

    pub async fn resolve_approval(&self, id: &str, approved: bool) -> bool {
        let mut map = self.pending_approvals.lock().await;
        if let Some(entry) = map.remove(id) {
            let _ = entry.tx.send(approved);
            true
        } else {
            false
        }
    }

    pub async fn clear_approval(&self, id: &str) -> bool {
        let mut map = self.pending_approvals.lock().await;
        map.remove(id).is_some()
    }

    pub async fn first_pending_approval(&self) -> Option<Value> {
        let map = self.pending_approvals.lock().await;
        map.values().next().map(|e| e.request.clone())
    }

    pub async fn cancel_all_pending_approvals(&self) -> Vec<String> {
        self.cancel_pending_approvals_for_generation(None).await
    }

    pub async fn cancel_pending_approvals_for_generation(
        &self,
        generation: Option<u64>,
    ) -> Vec<String> {
        let mut map = self.pending_approvals.lock().await;
        let mut ids = Vec::with_capacity(map.len());
        let matching: Vec<String> = map
            .iter()
            .filter(|(_, entry)| generation.is_none() || Some(entry.generation) == generation)
            .map(|(id, _)| id.clone())
            .collect();
        for id in matching {
            if let Some(entry) = map.remove(&id) {
                ids.push(id);
                let _ = entry.tx.send(false);
            }
        }
        ids
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn chat_cancel_generation_prevents_stale_cleanup_race() {
        let state = AppState::new(std::env::temp_dir(), AppConfig::default()).unwrap();

        let (gen1, rx1) = state.begin_chat_cancel().await;
        let (gen2, rx2) = state.begin_chat_cancel().await;
        assert!(gen2 > gen1);
        assert!(
            *rx1.borrow(),
            "First task must be marked cancelled when superseded"
        );
        assert!(!*rx2.borrow(), "Second task must remain active");

        // First task finishes late and attempts cleanup: must NOT clear second task
        state.finish_chat_cancel(gen1).await;
        assert!(state.active_cancel.lock().await.is_some());

        // Cancelling active chat must signal the second task
        assert!(state.cancel_active_chat().await);
        assert!(
            *rx2.borrow(),
            "Second task must receive cancellation signal"
        );
    }

    #[tokio::test]
    async fn approval_state_is_idempotent_and_clears_cleanly() {
        let state = AppState::new(std::env::temp_dir(), AppConfig::default()).unwrap();

        // Approve
        let (tx1, rx1) = oneshot::channel();
        state
            .register_approval("req-1".into(), json!({"approvalId": "req-1"}), tx1)
            .await;
        assert!(state.first_pending_approval().await.is_some());
        assert!(state.resolve_approval("req-1", true).await);
        assert_eq!(rx1.await.unwrap(), true);
        // Duplicate resolution must safely return false without breaking state
        assert!(!state.resolve_approval("req-1", false).await);
        assert!(state.first_pending_approval().await.is_none());

        // Deny
        let (tx2, rx2) = oneshot::channel();
        state
            .register_approval("req-2".into(), json!({"approvalId": "req-2"}), tx2)
            .await;
        assert!(state.resolve_approval("req-2", false).await);
        assert_eq!(rx2.await.unwrap(), false);
        assert!(state.first_pending_approval().await.is_none());
    }
}
