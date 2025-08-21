use std::fmt;

#[derive(Debug)]
pub enum ObjectStorageError {
    UploadIdMissing,
    SessionAlreadyExists(String),
    SessionNotFound(String),
    S3Error(aws_sdk_s3::Error),
    EnvError(std::env::VarError),
    LockError(String),
    ETagMissing,
    UploadFailed,
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
    ValidationError(String),
}

impl fmt::Display for HandlerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use HandlerError::*;
        match self {
            ValidationError(s) => write!(f, "ValidationError: {}", s),
        }
    }
}

impl From<ObjectStorageError> for HandlerError {
    fn from(error: ObjectStorageError) -> Self {
        HandlerError::ValidationError(error.to_string())
    }
}
