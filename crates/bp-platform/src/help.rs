//! Where the shipped help lives, and nothing else.
//!
//! Help > User Guide opens `app-help/index.md` **as a document, in a new
//! tab**. Not in a browser, and that is the decision this module exists to
//! record rather than a limitation ([ADR-0075](../../../docs/decisions/ADR-0075.md)):
//! this crate's own header says a library that spawns processes on the user's
//! behalf is a library that can be talked into spawning a different one, and
//! [ADR-0057](../../../docs/decisions/ADR-0057.md) removed the last thing in
//! this product that ran anything. A Help menu is a poor reason to put one
//! back.
//!
//! So the help is Markdown, it sits beside the executable, and the editor
//! shows it the way it shows any other document. No browser, no process, no
//! network -- and it works on a machine with none of those.
//!
//! ## Why this is here rather than in `bp-ui`
//!
//! Joining a path is a platform question, and `bp-ui` connects rather than
//! decides. It is the same split [`editor::icon_beside`](crate::editor::icon_beside)
//! already makes, and for the same reason: the answer is testable on Linux
//! for Windows and on Windows for Linux, which is the whole design of this
//! crate.

use crate::Platform;

/// The directory the help ships in, beside the executable.
///
/// Beside rather than in a documentation directory the operating system
/// knows about, for the reason the icon is beside it
/// ([ADR-0068](../../../docs/decisions/ADR-0068.md)): with no installer
/// ([ADR-0067](../../../docs/decisions/ADR-0067.md)) there is no step that
/// could place a file anywhere else, so anything the product needs at runtime
/// has to travel in the archive.
pub const HELP_DIR_NAME: &str = "app-help";

/// The page Help opens, which links to every other page.
pub const HELP_INDEX_FILE_NAME: &str = "index.md";

/// Where the help index sits, given where the executable does.
///
/// `None` when `executable` has no parent -- a bare name rather than a path.
/// Returning `None` rather than guessing keeps this from inventing a path
/// that a caller would then report as missing help, which is a worse message
/// than "I could not work out where I am".
#[must_use]
pub fn index_beside(platform: Platform, executable: &str) -> Option<String> {
    let dir = crate::paths::parent(platform, executable)?;
    Some(crate::paths::join(
        platform,
        dir,
        &[HELP_DIR_NAME, HELP_INDEX_FILE_NAME],
    ))
}

#[cfg(test)]
mod tests {
    use super::{HELP_DIR_NAME, HELP_INDEX_FILE_NAME, index_beside};
    use crate::Platform;

    #[test]
    fn the_help_index_sits_beside_the_executable_on_every_platform() {
        for platform in Platform::ALL {
            let separator = match platform {
                Platform::Windows => '\\',
                Platform::Linux => '/',
            };
            let executable = match platform {
                Platform::Windows => "C:\\Apps\\BachelorPadLite\\bachelorpad.exe",
                Platform::Linux => "/opt/bachelorpad-lite/bachelorpad",
            };
            let found = index_beside(*platform, executable)
                .unwrap_or_else(|| panic!("{platform:?} gave no path for {executable}"));

            assert!(
                found.ends_with(&format!("{HELP_DIR_NAME}{separator}{HELP_INDEX_FILE_NAME}")),
                "{platform:?} produced {found}"
            );
            assert!(
                !found.contains(match platform {
                    Platform::Windows => '/',
                    Platform::Linux => '\\',
                }),
                "{platform:?} produced a path with the other platform's separator: {found}"
            );
        }
    }

    /// The same refusal [`crate::editor::icon_beside`] makes, for the same
    /// reason: a bare name has no directory, and guessing one would produce a
    /// path that looks answerable and is not.
    #[test]
    fn a_bare_executable_name_gives_no_help_path_rather_than_a_guessed_one() {
        for platform in Platform::ALL {
            assert_eq!(index_beside(*platform, "bachelorpad"), None);
        }
    }
}
