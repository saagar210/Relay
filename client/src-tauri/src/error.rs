use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Crypto error: {0}")]
    Crypto(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Transfer error: {0}")]
    Transfer(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("WebSocket error: {0}")]
    WebSocket(String),

    #[error("Session expired")]
    SessionExpired,

    #[error("Transfer cancelled")]
    Cancelled,

    #[error("Peer rejected transfer")]
    PeerRejected,

    #[error("Checksum mismatch for file: {0}")]
    ChecksumMismatch(String),

    #[error("Code already in use")]
    CodeInUse,

    #[error("Connection timeout")]
    ConnectionTimeout,

    #[error("Invalid transfer code: {0}")]
    InvalidCode(String),
}

impl serde::Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
