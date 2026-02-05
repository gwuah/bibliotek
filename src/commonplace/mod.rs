mod handler;
mod lib;
mod routes;

pub use lib::*;
pub use routes::routes;

pub fn migrations() -> &'static [(&'static str, &'static str)] {
    &[
        ("commonplace_001_schema.sql", include_str!("migrations/001_schema.sql")),
        ("commonplace_002_external_id.sql", include_str!("migrations/002_external_id.sql")),
        ("commonplace_003_sync_metadata.sql", include_str!("migrations/003_sync_metadata.sql")),
        ("commonplace_004_resource_config.sql", include_str!("migrations/004_resource_config.sql")),
    ]
}
