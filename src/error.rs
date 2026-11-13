use crate::blob::BlobError;
use crate::db::DbError;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0} not found")]
    NotFound(&'static str),

    #[error("{message}")]
    Invalid { code: &'static str, message: String },

    #[error("{message}")]
    Conflict { code: &'static str, message: String },

    #[error("this operation requires the admin token")]
    Unauthorized,

    #[error(transparent)]
    Blob(#[from] BlobError),

    #[error(transparent)]
    Database(#[from] DbError),

    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl Error {
    pub fn invalid(code: &'static str, message: impl Into<String>) -> Error {
        Error::Invalid {
            code,
            message: message.into(),
        }
    }

    pub fn conflict(code: &'static str, message: impl Into<String>) -> Error {
        Error::Conflict {
            code,
            message: message.into(),
        }
    }
}

impl From<libsql::Error> for Error {
    fn from(error: libsql::Error) -> Error {
        Error::Database(DbError::Sqlite(error))
    }
}
