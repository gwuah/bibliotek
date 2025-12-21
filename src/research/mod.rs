mod handler;
mod routes;

pub use routes::routes;

pub fn migrations() -> &'static [(&'static str, &'static str)] {
    &[(
        "research_001_config.sql",
        include_str!("migrations/001_config.sql"),
    )]
}
