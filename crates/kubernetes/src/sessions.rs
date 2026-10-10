use std::{
    collections::HashMap,
    future::Future,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use tokio::sync::mpsc::UnboundedSender;
use tokio::task::JoinHandle;

use crate::exec::TerminalInput;

struct Session {
    task: JoinHandle<()>,
    input: Option<UnboundedSender<TerminalInput>>,
}

/// Long-running streams (watches, log follows, shells) the frontend can stop by id.
#[derive(Default)]
pub struct Sessions {
    next_id: AtomicU64,
    sessions: Arc<Mutex<HashMap<u64, Session>>>,
}

impl Sessions {
    pub fn spawn<F>(&self, future: F, input: Option<UnboundedSender<TerminalInput>>) -> u64
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let registry = self.sessions.clone();
        // Hold the lock across spawn so a task that finishes instantly cannot remove itself
        // before it has been inserted.
        let mut sessions = self.sessions.lock().unwrap();
        let task = tokio::spawn(async move {
            future.await;
            registry.lock().unwrap().remove(&id);
        });
        sessions.insert(id, Session { task, input });
        id
    }

    pub fn stop(&self, id: u64) {
        if let Some(session) = self.sessions.lock().unwrap().remove(&id) {
            session.task.abort();
        }
    }

    pub fn stop_all(&self) {
        for (_, session) in self.sessions.lock().unwrap().drain() {
            session.task.abort();
        }
    }

    pub fn input(&self, id: u64) -> Option<UnboundedSender<TerminalInput>> {
        self.sessions.lock().unwrap().get(&id).and_then(|s| s.input.clone())
    }
}
