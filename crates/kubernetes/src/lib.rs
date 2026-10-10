//! Kubernetes discovery, watches, metrics, logs and container exec.
pub mod channel;
pub mod clusters;
pub mod error;
pub mod exec;
pub mod logs;
pub mod metrics;
pub mod pods;
pub mod quantity;
pub mod resources;
pub mod sessions;
pub mod shell_env;
pub mod summary;
pub mod watch;

#[cfg(test)]
mod e2e;
