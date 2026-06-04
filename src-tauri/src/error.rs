use serde::Serialize;

/// Application-level error. Serialized to the frontend as a stable string code so
/// the UI can react without leaking sensitive internals.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("vault is locked")]
    Locked,

    #[error("a vault already exists")]
    AlreadyExists,

    #[error("no vault exists yet")]
    NoVault,

    #[error("invalid master password")]
    InvalidPassword,

    #[error("cryptography error")]
    Crypto,

    #[error("database error: {0}")]
    Db(String),

    #[error("data error: {0}")]
    Data(String),

    #[error("internal error: {0}")]
    Internal(String),
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        AppError::Db(e.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::Data(e.to_string())
    }
}

/// Serialize as a flat object `{ "code": "...", "message": "..." }`.
impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let code = match self {
            AppError::Locked => "LOCKED",
            AppError::AlreadyExists => "ALREADY_EXISTS",
            AppError::NoVault => "NO_VAULT",
            AppError::InvalidPassword => "INVALID_PASSWORD",
            AppError::Crypto => "CRYPTO",
            AppError::Db(_) => "DB",
            AppError::Data(_) => "DATA",
            AppError::Internal(_) => "INTERNAL",
        };
        let mut s = serializer.serialize_struct("AppError", 2)?;
        s.serialize_field("code", code)?;
        s.serialize_field("message", &self.to_string())?;
        s.end()
    }
}

pub type AppResult<T> = Result<T, AppError>;
