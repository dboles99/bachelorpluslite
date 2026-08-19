//! Safe file loading and saving for BachelorPad+.
//!
//! The contract this crate exists to uphold (specs.md section 6): a save
//! either fully succeeds or leaves the previous file untouched. There is no
//! window in which the user's file is a half-written truncated ruin because
//! the power went out, the disk filled up, or the process was killed.
//!
//! The mechanism is the standard one: write a temporary file in the *same*
//! directory as the target, flush it all the way to the storage device,
//! optionally read it back, and only then atomically rename it over the
//! target. Same directory matters -- a rename across filesystems is a copy,
//! and a copy is not atomic.
//!
//! `load` and [`encode`] are a pair, and the property that makes the pair
//! worth having is that they are inverses: writing a document's text back
//! under its own encoding and reading it again returns the same text, the
//! same encoding and the same line endings. Keeping the encoder in the shell
//! -- which is where it started -- meant that property could not be stated by
//! any test that did not open a window.

#![forbid(unsafe_code)]

mod encode;
mod load;
mod save;
mod utf16;
mod watch;

pub use encode::{LineEndingPolicy, LineEndingSurvey, encode};
pub use load::{LoadError, LoadedFile, load};
pub use save::{Overwrite, SaveError, SaveOptions, SaveOutcome, atomic_write, resolve_in_dir};
pub use watch::{DiskState, FileStamp, check};

/// Crate identity used by workspace smoke tests and diagnostics.
pub const CRATE_NAME: &str = "bp-files";
