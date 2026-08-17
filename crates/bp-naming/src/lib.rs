//! Semantic filename grammar for BachelorPad+.
//!
//! Implements specs.md section 2 and BP-ADR-0003:
//!
//! ```text
//! <Title>[_v<N>]_<DDMMMYYYY>[_<NN>].<ext>
//! ```
//!
//! The crate is deliberately pure: it never touches the filesystem, never
//! reads the clock, and never renames anything. Collision resolution takes an
//! `exists` predicate so the rule can be tested exhaustively without a
//! temporary directory, and so the decision to actually move a user's file
//! stays where it belongs -- behind explicit user approval, per
//! `PROJECT_MEMORY.md`.

#![forbid(unsafe_code)]

mod date;
mod name;
mod sanitize;
mod stamp;

pub use date::{DATE_LEN, format_date, parse_date};
pub use name::{FIRST_COLLISION, FIRST_REVISION, SemanticName};
pub use sanitize::{
    FALLBACK_TITLE, MAX_COMPONENT_BYTES, MAX_EXTENSION_CHARS, sanitize_extension, sanitize_title,
    truncate_bytes,
};
pub use stamp::{Stamp, render};

/// Crate identity used by workspace smoke tests and diagnostics.
pub const CRATE_NAME: &str = "bp-naming";
