//! Turning a key press into an editing command.
//!
//! Deliberately toolkit-agnostic. The shell translates whatever Slint hands
//! it into a [`Key`], and everything after that is a pure function with a
//! table of expectations attached -- which is the only way the keyboard
//! behaviour of an editor can be checked without a keyboard.
//!
//! Chords the editor does not claim come back as [`Command::Ignore`] rather
//! than being swallowed. Ctrl+S has to reach the window's own binding, and an
//! editor view that eats every key press is how Ctrl+S stops saving.

use crate::Motion;

/// A key, independent of any toolkit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    Backspace,
    Delete,
    Enter,
    Tab,
    Escape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Modifiers {
    pub control: bool,
    pub shift: bool,
    pub alt: bool,
}

impl Modifiers {
    /// Whether this looks like AltGr rather than a Ctrl chord.
    ///
    /// Windows reports AltGr as Ctrl+Alt, so a German keyboard's `AltGr+Q`
    /// arrives indistinguishable from `Ctrl+Alt+Q`. Treating it as a chord
    /// makes `@` untypeable, which is a worse failure than a Ctrl+Alt
    /// shortcut that does not fire.
    const fn is_altgr(self) -> bool {
        self.control && self.alt
    }

    /// Whether this is a Ctrl chord the editor should interpret.
    const fn is_chord(self) -> bool {
        self.control && !self.alt
    }
}

/// What a key press means.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Insert(String),
    /// The Tab key. Not an `Insert` with its text already decided, because
    /// what a soft tab inserts depends on the caret's visual column -- at
    /// column 3 with width 4 it is two spaces, not four -- and `command_for`
    /// does not know where the caret is. `Editor::apply` expands it.
    Indent,
    Move {
        motion: Motion,
        select: bool,
    },
    DeleteBackward,
    DeleteForward,
    DeleteWordBackward,
    DeleteWordForward,
    Undo,
    Redo,
    SelectAll,
    /// The shell owns the OS clipboard, so these leave the editor.
    Copy,
    Cut,
    Paste,
    /// The Insert key, between inserting and overtyping.
    ///
    /// Leaves the editor like the clipboard commands do, because the mode is
    /// the window's rather than a document's: Notepad keeps it across tabs,
    /// and an editor per document holding its own copy would let two tabs
    /// disagree about what the next keystroke does.
    ToggleOverwrite,
    /// Not ours. The view must let it through rather than swallowing it.
    Ignore,
}

/// What `key` means, given the modifiers held and how tall the view is.
///
/// `page_rows` is a parameter because a page is a property of the window, not
/// of the document, and the editor has no business knowing the window.
pub fn command_for(key: Key, modifiers: Modifiers, page_rows: usize) -> Command {
    let select = modifiers.shift;
    let move_to = |motion| Command::Move { motion, select };

    match key {
        Key::Left if modifiers.is_chord() => move_to(Motion::WordLeft),
        Key::Right if modifiers.is_chord() => move_to(Motion::WordRight),
        Key::Left => move_to(Motion::Left),
        Key::Right => move_to(Motion::Right),

        Key::Up => move_to(Motion::Up),
        Key::Down => move_to(Motion::Down),

        Key::Home if modifiers.is_chord() => move_to(Motion::DocumentStart),
        Key::End if modifiers.is_chord() => move_to(Motion::DocumentEnd),
        Key::Home => move_to(Motion::LineStart),
        Key::End => move_to(Motion::LineEnd),

        Key::PageUp => move_to(Motion::PageUp(page_rows)),
        Key::PageDown => move_to(Motion::PageDown(page_rows)),

        // Ctrl+Insert and Shift+Insert are the older copy and paste, and
        // Notepad still honours both. Only a bare Insert changes the mode: a
        // Shift+Insert that flipped it instead of pasting would change what
        // every later keystroke does, and say nothing.
        Key::Insert if modifiers.is_chord() => Command::Copy,
        Key::Insert if modifiers.shift => Command::Paste,
        Key::Insert => Command::ToggleOverwrite,

        Key::Backspace if modifiers.is_chord() => Command::DeleteWordBackward,
        Key::Delete if modifiers.is_chord() => Command::DeleteWordForward,
        Key::Backspace => Command::DeleteBackward,
        Key::Delete => Command::DeleteForward,

        // A newline is an ordinary insertion, which is what makes it undo,
        // coalesce and replace a selection like everything else. So is a tab,
        // once expanded -- see `Command::Indent`.
        Key::Enter => Command::Insert("\n".to_owned()),
        Key::Tab => Command::Indent,
        Key::Escape => Command::Ignore,

        Key::Char(ch) if modifiers.is_chord() => match ch.to_ascii_lowercase() {
            'a' => Command::SelectAll,
            'c' => Command::Copy,
            'x' => Command::Cut,
            'v' => Command::Paste,
            'z' if modifiers.shift => Command::Redo,
            'z' => Command::Undo,
            'y' => Command::Redo,
            // Ctrl+S, Ctrl+F and the rest belong to the window.
            _ => Command::Ignore,
        },

        // Plain text, or AltGr text. A control character reaching here is a
        // key the backend did not name, not something to put in the document.
        Key::Char(ch) if !modifiers.control || modifiers.is_altgr() => {
            if ch.is_control() {
                Command::Ignore
            } else {
                Command::Insert(ch.to_string())
            }
        }
        Key::Char(_) => Command::Ignore,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: Modifiers = Modifiers {
        control: false,
        shift: false,
        alt: false,
    };
    const CTRL: Modifiers = Modifiers {
        control: true,
        shift: false,
        alt: false,
    };
    const SHIFT: Modifiers = Modifiers {
        control: false,
        shift: true,
        alt: false,
    };

    fn cmd(key: Key, modifiers: Modifiers) -> Command {
        command_for(key, modifiers, 20)
    }

    #[test]
    fn typing_a_character_inserts_it() {
        assert_eq!(cmd(Key::Char('a'), NONE), Command::Insert("a".to_owned()));
        assert_eq!(cmd(Key::Char('A'), SHIFT), Command::Insert("A".to_owned()));
        assert_eq!(cmd(Key::Char('日'), NONE), Command::Insert("日".to_owned()));
    }

    #[test]
    fn enter_and_tab_are_ordinary_insertions() {
        // Which is what makes them undo and replace a selection like any
        // other typing.
        assert_eq!(cmd(Key::Enter, NONE), Command::Insert("\n".to_owned()));
        assert_eq!(cmd(Key::Tab, NONE), Command::Indent);
    }

    #[test]
    fn shift_turns_a_movement_into_a_selection() {
        assert_eq!(
            cmd(Key::Right, SHIFT),
            Command::Move {
                motion: Motion::Right,
                select: true
            }
        );
        assert_eq!(
            cmd(Key::Right, NONE),
            Command::Move {
                motion: Motion::Right,
                select: false
            }
        );
    }

    #[test]
    fn control_moves_by_word_and_by_document() {
        assert_eq!(
            cmd(Key::Left, CTRL),
            Command::Move {
                motion: Motion::WordLeft,
                select: false
            }
        );
        assert_eq!(
            cmd(Key::Home, CTRL),
            Command::Move {
                motion: Motion::DocumentStart,
                select: false
            }
        );
        assert_eq!(
            cmd(Key::End, CTRL),
            Command::Move {
                motion: Motion::DocumentEnd,
                select: false
            }
        );
    }

    #[test]
    fn control_and_shift_together_select_by_word() {
        let both = Modifiers {
            control: true,
            shift: true,
            alt: false,
        };
        assert_eq!(
            cmd(Key::Right, both),
            Command::Move {
                motion: Motion::WordRight,
                select: true
            }
        );
    }

    #[test]
    fn a_page_is_as_tall_as_the_caller_says() {
        assert_eq!(
            command_for(Key::PageDown, NONE, 42),
            Command::Move {
                motion: Motion::PageDown(42),
                select: false
            }
        );
    }

    #[test]
    fn control_with_backspace_deletes_a_word() {
        assert_eq!(cmd(Key::Backspace, CTRL), Command::DeleteWordBackward);
        assert_eq!(cmd(Key::Delete, CTRL), Command::DeleteWordForward);
        assert_eq!(cmd(Key::Backspace, NONE), Command::DeleteBackward);
        assert_eq!(cmd(Key::Delete, NONE), Command::DeleteForward);
    }

    #[test]
    fn the_editing_chords_are_claimed() {
        assert_eq!(cmd(Key::Char('a'), CTRL), Command::SelectAll);
        assert_eq!(cmd(Key::Char('c'), CTRL), Command::Copy);
        assert_eq!(cmd(Key::Char('x'), CTRL), Command::Cut);
        assert_eq!(cmd(Key::Char('v'), CTRL), Command::Paste);
        assert_eq!(cmd(Key::Char('z'), CTRL), Command::Undo);
        assert_eq!(cmd(Key::Char('y'), CTRL), Command::Redo);
    }

    #[test]
    fn control_shift_z_redoes_as_well() {
        let both = Modifiers {
            control: true,
            shift: true,
            alt: false,
        };
        assert_eq!(cmd(Key::Char('z'), both), Command::Redo);
    }

    #[test]
    fn a_chord_is_recognised_whatever_the_caps_lock_says() {
        assert_eq!(cmd(Key::Char('A'), CTRL), Command::SelectAll);
        assert_eq!(cmd(Key::Char('Z'), CTRL), Command::Undo);
    }

    #[test]
    fn chords_the_editor_does_not_claim_are_left_alone() {
        // Ctrl+S has to reach the window's binding. An editor view that
        // swallows every key press is how Ctrl+S stops saving.
        assert_eq!(cmd(Key::Char('s'), CTRL), Command::Ignore);
        assert_eq!(cmd(Key::Char('o'), CTRL), Command::Ignore);
        assert_eq!(cmd(Key::Char('f'), CTRL), Command::Ignore);
        assert_eq!(cmd(Key::Escape, NONE), Command::Ignore);
    }

    #[test]
    fn altgr_types_a_character_rather_than_firing_a_chord() {
        // Windows reports AltGr as Ctrl+Alt. Treating it as a chord makes `@`
        // untypeable on a German keyboard.
        let altgr = Modifiers {
            control: true,
            shift: false,
            alt: true,
        };
        assert_eq!(cmd(Key::Char('@'), altgr), Command::Insert("@".to_owned()));
        assert_eq!(cmd(Key::Char('a'), altgr), Command::Insert("a".to_owned()));
    }

    #[test]
    fn a_control_character_is_not_put_into_the_document() {
        // Some backends deliver an unnamed key as its control code.
        assert_eq!(cmd(Key::Char('\u{1}'), NONE), Command::Ignore);
        assert_eq!(cmd(Key::Char('\u{7}'), NONE), Command::Ignore);
    }

    #[test]
    fn alt_alone_is_not_an_editing_command() {
        let alt = Modifiers {
            control: false,
            shift: false,
            alt: true,
        };
        // Alt is the menu-bar modifier; typing under it still inserts, which
        // matches every other plain-text field.
        assert_eq!(cmd(Key::Char('f'), alt), Command::Insert("f".to_owned()));
    }

    #[test]
    fn a_bare_insert_toggles_overwrite_and_its_chords_are_copy_and_paste() {
        // Notepad's older clipboard keys. A Shift+Insert that flipped the
        // mode instead would silently change what every later keystroke does.
        assert_eq!(cmd(Key::Insert, NONE), Command::ToggleOverwrite);
        assert_eq!(cmd(Key::Insert, CTRL), Command::Copy);
        assert_eq!(cmd(Key::Insert, SHIFT), Command::Paste);
    }

    #[test]
    fn every_navigation_key_moves_the_caret() {
        // The keys a user reported as not working, pinned in one place.
        for (key, motion) in [
            (Key::Left, Motion::Left),
            (Key::Right, Motion::Right),
            (Key::Up, Motion::Up),
            (Key::Down, Motion::Down),
            (Key::Home, Motion::LineStart),
            (Key::End, Motion::LineEnd),
            (Key::PageUp, Motion::PageUp(20)),
            (Key::PageDown, Motion::PageDown(20)),
        ] {
            assert_eq!(
                cmd(key, NONE),
                Command::Move {
                    motion,
                    select: false
                },
                "{key:?}"
            );
        }
    }
}
