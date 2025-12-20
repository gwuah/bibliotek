//! Commonplace Module
//!
//! A self-contained library for centralizing reading annotations, notes, and vocabulary
//! across all reading devices. Named after the "commonplace book" - a personal journal
//! for recording knowledge, quotes, and ideas from reading.
//!
//! # Features
//!
//! - CRUD operations for resources, annotations, comments, notes, and words
//! - Ready-to-use HTTP handlers and routes
//! - Database migrations included
//!
//! # Usage
//!
//! ```rust,ignore
//! use bibliotek::commonplace;
//!
//! // Get the migrations to run
//! for (name, sql) in commonplace::migrations() {
//!     // Run migration...
//! }
//!
//! // Mount the routes
//! let app = Router::new()
//!     .nest("/commonplace", commonplace::routes())
//!     .with_state(app_state);
//!
//! // Use the library directly
//! let lib = commonplace::Commonplace::new(connection);
//! let resource = lib.create_resource(input).await?;
//! ```

mod handler;
mod lib;
mod routes;

// Re-export the core library types and functions
pub use lib::*;

// Re-export the routes function
pub use routes::routes;

// ============================================================================
// Migrations
// ============================================================================

/// Returns the migrations for the commonplace module.
///
/// These should be run during application startup to ensure the database
/// schema is up to date.
///
/// # Example
///
/// ```rust,ignore
/// for (name, sql) in commonplace::migrations() {
///     conn.execute_batch(sql).await?;
/// }
/// ```
pub fn migrations() -> &'static [(&'static str, &'static str)] {
    &[(
        "commonplace_001_schema.sql",
        include_str!("migrations/001_schema.sql"),
    )]
}
