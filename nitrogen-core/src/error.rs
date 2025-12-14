//! Error types for Nitrogen

use thiserror::Error;

/// Result type alias using NitrogenError
pub type Result<T> = std::result::Result<T, NitrogenError>;

/// Main error type for Nitrogen operations
#[derive(Debug, Error)]
pub enum NitrogenError {
    /// Portal/D-Bus communication error
    #[error("Portal error: {0}")]
    Portal(String),

    /// PipeWire error
    #[error("PipeWire error: {0}")]
    PipeWire(String),

    /// Encoder error
    #[error("Encoder error: {0}")]
    Encoder(String),

    /// NVENC-specific error
    #[error("NVENC error: {0}")]
    Nvenc(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),

    /// Source not found
    #[error("Source not found: {0}")]
    SourceNotFound(String),

    /// Capture session not active
    #[error("No active capture session")]
    NoActiveSession,

    /// Session already running
    #[error("Capture session already running")]
    SessionAlreadyRunning,

    /// Unsupported operation
    #[error("Unsupported: {0}")]
    Unsupported(String),

    /// I/O error
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Generic error with context
    #[error("{context}: {source}")]
    WithContext {
        context: String,
        #[source]
        source: Box<NitrogenError>,
    },
}

impl NitrogenError {
    /// Create a portal error
    pub fn portal(msg: impl Into<String>) -> Self {
        Self::Portal(msg.into())
    }

    /// Create a PipeWire error
    pub fn pipewire(msg: impl Into<String>) -> Self {
        Self::PipeWire(msg.into())
    }

    /// Create an encoder error
    pub fn encoder(msg: impl Into<String>) -> Self {
        Self::Encoder(msg.into())
    }

    /// Create an NVENC error
    pub fn nvenc(msg: impl Into<String>) -> Self {
        Self::Nvenc(msg.into())
    }

    /// Create a config error
    pub fn config(msg: impl Into<String>) -> Self {
        Self::Config(msg.into())
    }

    /// Add context to an error
    pub fn with_context(self, context: impl Into<String>) -> Self {
        Self::WithContext {
            context: context.into(),
            source: Box::new(self),
        }
    }
}

/// Extension trait for adding context to Results
pub trait ResultExt<T> {
    /// Add context to an error
    fn context(self, context: impl Into<String>) -> Result<T>;
}

impl<T> ResultExt<T> for Result<T> {
    fn context(self, context: impl Into<String>) -> Result<T> {
        self.map_err(|e| e.with_context(context))
    }
}

// Conversions from external error types

impl From<ashpd::Error> for NitrogenError {
    fn from(err: ashpd::Error) -> Self {
        Self::Portal(err.to_string())
    }
}

impl From<zbus::Error> for NitrogenError {
    fn from(err: zbus::Error) -> Self {
        Self::Portal(format!("D-Bus error: {}", err))
    }
}

impl From<pipewire::Error> for NitrogenError {
    fn from(err: pipewire::Error) -> Self {
        Self::PipeWire(err.to_string())
    }
}

impl From<ffmpeg_next::Error> for NitrogenError {
    fn from(err: ffmpeg_next::Error) -> Self {
        Self::Encoder(err.to_string())
    }
}
