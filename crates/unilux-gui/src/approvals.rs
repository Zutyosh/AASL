//! Approval requests from agents, answered in the Agents page.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Duration;

use tokio::sync::oneshot;
use unilux_agent::ConfirmHook;

/// Open requests: id to (message, reply channel).
type Pending = BTreeMap<u64, (String, Option<oneshot::Sender<bool>>)>;

static PENDING: Mutex<Pending> = Mutex::new(BTreeMap::new());
static NEXT: Mutex<u64> = Mutex::new(0);

/// Requests unanswered after this long are refused.
const TIMEOUT: Duration = Duration::from_secs(120);

pub fn hook() -> ConfirmHook {
    std::sync::Arc::new(|message: String| {
        let (tx, rx) = oneshot::channel();
        let id = {
            let mut n = NEXT.lock().unwrap();
            *n += 1;
            *n
        };
        PENDING.lock().unwrap().insert(id, (message, Some(tx)));
        Box::pin(async move {
            let answer = tokio::time::timeout(TIMEOUT, rx)
                .await
                .ok()
                .and_then(|r| r.ok())
                .unwrap_or(false);
            PENDING.lock().unwrap().remove(&id);
            answer
        })
    })
}

pub fn pending() -> Vec<(u64, String)> {
    PENDING
        .lock()
        .unwrap()
        .iter()
        .map(|(id, (m, _))| (*id, m.clone()))
        .collect()
}

pub fn answer(id: u64, yes: bool) {
    if let Some((_, tx)) = PENDING.lock().unwrap().get_mut(&id)
        && let Some(tx) = tx.take()
    {
        let _ = tx.send(yes);
    }
}
