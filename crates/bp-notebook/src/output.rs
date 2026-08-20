//! What a run produced, as data.
//!
//! **Modelled, never rendered.** specs.md section 12 lists the output types a
//! notebook must carry; deciding what a table looks like, whether HTML is
//! sanitised before it reaches a webview, or how an image is decoded are all
//! questions for the layer that draws them. Keeping them out of here has a
//! security consequence as well as an architectural one: nothing in this crate
//! ever interprets an output, so importing a notebook full of hostile HTML or
//! a malformed image does no work on that content at all.
//!
//! Charts, which specs.md also names, are not a variant. A chart arrives from
//! every runner we would ever have either as an image or as a JSON figure
//! description, and inventing a third representation we could not fill in would
//! be a variant that only ever held someone else's guess.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Which of a process's two output streams a piece of text came from.
///
/// Worth keeping apart even though both are text: a UI colours stderr
/// differently, and a caller deciding whether a run "looked wrong" reads this
/// rather than guessing from content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Stream {
    /// Ordinary output.
    Stdout,
    /// Diagnostics. Not the same as [`Output::Error`], which is a run that
    /// failed; a process can write to stderr and still succeed.
    Stderr,
}

/// One result of running a cell.
///
/// The seven variants are specs.md section 12's list. Binary payloads are held
/// as base64 text and never decoded here -- carrying the encoded form costs one
/// string, and decoding it would mean this crate had an opinion about bytes it
/// has no reason to look at.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Output {
    /// Text a process wrote as it ran.
    Text {
        /// Which stream it came from.
        stream: Stream,
        /// The text, with its newlines intact.
        text: String,
    },
    /// A result set: a header row and the rows under it.
    ///
    /// Rows are expected to be as long as `columns`, and nothing here enforces
    /// it. A ragged result is what a query or a script actually returned, and
    /// padding it out would be this crate inventing cells the run never
    /// produced; the renderer, which knows how wide the screen is, decides what
    /// to do about the gap.
    Table {
        /// Column headers, left to right.
        columns: Vec<String>,
        /// Rows, each in column order.
        rows: Vec<Vec<String>>,
    },
    /// A structured result, kept as JSON so that a viewer can fold it rather
    /// than showing the user a pretty-printed string.
    Json {
        /// The value, exactly as produced.
        value: Value,
    },
    /// A rendered fragment. Held as a string and never parsed: this crate is
    /// not the right place to decide what is safe to put in front of a user.
    Html {
        /// The markup.
        html: String,
    },
    /// A picture.
    Image {
        /// The IANA type, e.g. `image/png`. Kept as a string because a runner
        /// may legitimately produce a format we have never heard of.
        media_type: String,
        /// The bytes, base64 encoded, never decoded here.
        data_base64: String,
        /// Description for a screen reader, when the runner supplied one.
        alt: Option<String>,
    },
    /// A run that failed.
    ///
    /// Three fields rather than one string because the parts are used
    /// differently: the name groups failures, the message is what the user
    /// reads, and the traceback is what they expand only if they want it.
    Error {
        /// The exception or error class, e.g. `ValueError`.
        name: String,
        /// The one-line explanation.
        message: String,
        /// Frames, outermost first. Empty is normal for languages without one.
        traceback: Vec<String>,
    },
    /// A file the run produced.
    ///
    /// Carried by value rather than as a path: this crate touches no
    /// filesystem, and a path would be a promise about a file that may already
    /// be gone by the time anyone looks.
    File {
        /// The suggested name, not a path.
        name: String,
        /// The IANA type, or `application/octet-stream` when unknown.
        media_type: String,
        /// The contents, base64 encoded, never decoded here.
        data_base64: String,
    },
}

impl Output {
    /// Whether this output records a failure, so that a caller can stop a
    /// "run all" without matching on every variant it does not care about.
    pub fn is_error(&self) -> bool {
        matches!(self, Output::Error { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_error_variant_reports_failure() {
        // stderr is not failure: plenty of successful programs write to it.
        assert!(
            !Output::Text {
                stream: Stream::Stderr,
                text: "warning".to_owned()
            }
            .is_error()
        );
        assert!(
            Output::Error {
                name: "ValueError".to_owned(),
                message: "no".to_owned(),
                traceback: Vec::new(),
            }
            .is_error()
        );
    }

    #[test]
    fn a_ragged_table_is_kept_as_the_run_produced_it() {
        // The type permits rows of different widths and the domain does not
        // expect them; we still must not repair them, because the missing
        // column is information about the run.
        let output = Output::Table {
            columns: vec!["a".to_owned(), "b".to_owned()],
            rows: vec![
                vec!["1".to_owned()],
                vec!["1".to_owned(), "2".to_owned(), "3".to_owned()],
            ],
        };
        let json = serde_json::to_string(&output).unwrap();
        let back: Output = serde_json::from_str(&json).unwrap();
        assert_eq!(back, output, "no padding, no truncation");
    }
}
