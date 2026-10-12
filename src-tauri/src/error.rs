//! Typed IPC errors with stable codes for the frontend.

use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{message}")]
    Coded { code: &'static str, message: String },
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
}

impl AppError {
    pub fn coded(code: &'static str, message: impl Into<String>) -> Self {
        Self::Coded {
            code,
            message: message.into(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorPayload {
    pub code: String,
    pub message: String,
}

impl From<AppError> for ErrorPayload {
    fn from(value: AppError) -> Self {
        match value {
            AppError::Coded { code, message } => Self {
                code: code.to_string(),
                message,
            },
            AppError::Io(err) => Self {
                code: "IO".into(),
                message: err.to_string(),
            },
            AppError::Sqlx(err) => Self {
                code: "CATALOG_SQL".into(),
                message: err.to_string(),
            },
            AppError::Serde(err) => Self {
                code: "SERDE".into(),
                message: err.to_string(),
            },
        }
    }
}

impl From<AppError> for String {
    fn from(value: AppError) -> Self {
        let payload = ErrorPayload::from(value);
        serde_json::to_string(&payload)
            .unwrap_or_else(|_| r#"{"code":"UNKNOWN","message":"Unknown error"}"#.to_string())
    }
}
