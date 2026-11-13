pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const GIT_REVISION: &str = env!("LIOTEK_GIT_REVISION");

pub fn identity() -> String {
    format!(
        "liotek {VERSION} (revision {GIT_REVISION}, schema {})",
        crate::db::SUPPORTED_SCHEMA_VERSION
    )
}
