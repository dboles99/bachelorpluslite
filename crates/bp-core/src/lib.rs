//! Core document and workspace model for BachelorPad+.
//!
//! This crate is the bottom of the dependency rule in
//! `docs/architecture/ARCHITECTURE.md`: it knows about documents, save state,
//! encodings and tabs, and nothing about UI toolkits, the filesystem, the
//! semantic layer or security policy. Everything above it may depend on this;
//! it depends on nothing of ours.
//!
//! It holds no text. Buffers live in `bp-buffer`, which keeps this model
//! cheap to clone and test, and means rendering the status bar never has to
//! borrow the editing buffer.

#![forbid(unsafe_code)]

mod document;
mod encoding;
mod save_state;
mod workspace;

pub use document::{Document, DocumentId, NO_LOCATION, UNTITLED};
pub use encoding::{Encoding, LineEnding};
pub use save_state::{CheckpointTime, DiskSaveTime, SaveState};
pub use workspace::Workspace;

/// Crate identity used by workspace smoke tests and diagnostics.
pub const CRATE_NAME: &str = "bp-core";
