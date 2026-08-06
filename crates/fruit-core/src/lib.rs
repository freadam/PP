//! # fruit-core
//!
//! Everything Fruit knows how to do, with no UI and no Tauri dependency.
//!
//! The split is deliberate. §6.8 moves SQL out of the webview and into typed,
//! intent-based commands; putting those commands in a plain library crate means
//! the invariants in §6.5 — one running timer, no midnight-crossing blocks,
//! caches that match their views — are covered by ordinary `cargo test` rather
//! than by clicking around a running app.
//!
//! ```no_run
//! use fruit_core::{Store, model::NewTask};
//!
//! let mut store = Store::open_in_memory().unwrap();
//! store.seed_first_run("Europe/London").unwrap();
//! let task = store.create_task(NewTask { title: "Ship it".into(), ..Default::default() }).unwrap();
//! assert_eq!(task.tracked_sec, 0);
//! ```

pub mod clock;
pub mod connector;
pub mod db;
pub mod error;
pub mod ids;
pub mod model;
pub mod ics;
pub mod parser;
pub mod rrule;
pub mod spool;
pub mod store;
pub mod time;
pub mod xlsx;

pub use error::{AppError, Result};
pub use store::Store;

/// Bumped with the crate; surfaced in Settings → About alongside the git SHA.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
