use std::{error::Error, fmt};

#[derive(Debug)]
pub enum ObjectStorageError {
    UploadIdMissing,
    SessionAlreadyExists(String),
    SessionNotFound(String),
    S3Error(Box<dyn Error + Send + Sync + 'static>),
    EnvError(std::env::VarError),
    LockError(String),
    ETagMissing,
    UploadFailed,
}

impl std::error::Error for ObjectStorageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        use ObjectStorageError::*;
        match self {
            S3Error(e) => Some(e.as_ref() as &dyn Error),
            _ => None,
        }
    }
}

impl fmt::Display for ObjectStorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use ObjectStorageError::*;
        match self {
            UploadIdMissing => write!(f, "UploadIdMissing"),
            SessionAlreadyExists(s) => write!(f, "SessionAlreadyExists: {}", s),
            SessionNotFound(s) => write!(f, "SessionNotFound: {}", s),
            S3Error(e) => write!(f, "S3Error: {}", e),
            EnvError(e) => write!(f, "EnvError: {}", e),
            LockError(s) => write!(f, "LockError: {}", s),
            ETagMissing => write!(f, "ETagMissing"),
            UploadFailed => write!(f, "UploadFailed"),
        }
    }
}

impl From<std::env::VarError> for ObjectStorageError {
    fn from(error: std::env::VarError) -> Self {
        ObjectStorageError::EnvError(error)
    }
}

#[derive(Debug)]
pub enum HandlerError {
    ObjectStorageError(ObjectStorageError),
    ValidationError(String),
}

impl fmt::Display for HandlerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use HandlerError::*;
        match self {
            ObjectStorageError(s) => write!(f, "ObjectStorageError: {}", crate::unpack_error(s)),
            ValidationError(s) => write!(f, "ValidationError: {}", s),
        }
    }
}

impl std::error::Error for HandlerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        use HandlerError::*;
        match self {
            ObjectStorageError(e) => Some(e),
            _ => None,
        }
    }
}

impl From<ObjectStorageError> for HandlerError {
    fn from(error: ObjectStorageError) -> Self {
        HandlerError::ObjectStorageError(error)
    }
}
