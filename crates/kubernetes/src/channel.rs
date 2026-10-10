//! Typed, in-process stream delivery. No serialization or webview IPC.
use std::sync::Arc;

pub struct Channel<T>(Arc<dyn Fn(T) -> Result<(), ()> + Send + Sync>);

#[derive(Debug, Clone, Copy, thiserror::Error)]
#[error("stream receiver disconnected")]
pub struct Disconnected;

impl<T> Clone for Channel<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> Channel<T> {
    pub fn new(send: impl Fn(T) -> Result<(), ()> + Send + Sync + 'static) -> Self {
        Self(Arc::new(send))
    }
    pub fn send(&self, value: T) -> Result<(), Disconnected> {
        (self.0)(value).map_err(|()| Disconnected)
    }
}
