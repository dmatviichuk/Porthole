use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{}", .0.message)]
    Api(Box<kube::core::Status>),
    #[error(transparent)]
    Kube(kube::Error),
    #[error(transparent)]
    Kubeconfig(#[from] kube::config::KubeconfigError),
    #[error("session {0} is no longer running")]
    SessionGone(u64),
    #[error("invalid YAML: {0}")]
    Yaml(String),
    #[error("{0}")]
    Invalid(String),
}

impl From<kube::Error> for Error {
    fn from(err: kube::Error) -> Self {
        // API errors carry a readable message from the server; the generic Display adds noise.
        match err {
            kube::Error::Api(status) => Error::Api(status),
            other => Error::Kube(other),
        }
    }
}

/// Commands reject with a plain string so the frontend can show it as is.
impl Serialize for Error {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
