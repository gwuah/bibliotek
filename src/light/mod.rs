//! Light Extension Sync Module
//!
//! Provides synchronization between the Light browser extension and the Commonplace library.
//! Light is a minimalist text highlighting extension that stores highlights in the browser's
//! local storage. This module enables syncing those highlights to the Commonplace database.
//!
//! # Architecture
//!
//! - Light extension stores highlights keyed by URL
//! - Each highlight has a unique `groupID` (timestamp-based)
//! - Sync is idempotent: existing highlights (by groupID) are skipped
//!
//! # Usage
//!
//! ```rust,ignore
//! use bibliotek::light;
//!
//! let app = Router::new()
//!     .nest("/light", light::routes())
//!     .with_state(app_state);
//! ```

mod handler;
mod routes;

pub use routes::routes;

