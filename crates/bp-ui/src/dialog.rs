//! The questions this window asks, and what each answer is allowed to do
//! (ADR-0084).
//!
//! Owns three things: the wording of every question, the queue that puts one
//! on screen at a time, and **the rule that only an explicit answer destroys
//! anything**. The rule is the reason this module exists. Every question used
//! to be an `rfd` dialog, and on Linux `rfd` runs `zenity` and answers
//! *Cancel* when it is missing; the recovery prompt read anything but *Yes*
//! as "discard the journal", so one missing package deleted the unsaved work
//! the journal exists to keep. Here a question that is dismissed, escaped or
//! never answered keeps the work, and the functions that say so are tested.
//!
//! It does not own the look -- that is `ui/message_dialog.slint` -- nor what
//! follows an answer, which is the caller's continuation. It does not wait:
//! Slint has no nested event loop, so a question is asked with the rest of
//! the work attached, and the rest runs when a button is chosen.
//!
//! The file pickers are not here. They stay native through `rfd`, and on
//! Linux their fallback when no desktop portal answers is `zenity` too; a
//! picker that fails returns nothing, which is a dismissed picker and costs
//! no data. ADR-0084 names that as the one program this product can start.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use slint::ComponentHandle;

use crate::AppWindow;

/// Which button was chosen. There is deliberately no "yes" or "no": the
/// labels are the question's, and what matters is which *kind* of answer it
/// was.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Answer {
    /// The first button: Save, Restore, Replace, OK.
    Accept,
    /// The destructive alternative: Don't Save, Discard. **Only ever a click
    /// on that button** -- nothing else produces it.
    Decline,
    /// Cancel, Escape, or anything this does not recognise.
    Cancel,
}

impl Answer {
    /// Read what `message_dialog.slint` reported.
    ///
    /// **An unknown number is `Cancel`, never `Decline`.** The whole defect
    /// this module replaces was a failure read as a refusal; a value nobody
    /// expected must not be read as consent to lose something either.
    pub(crate) const fn from_choice(choice: i32) -> Self {
        match choice {
            0 => Self::Accept,
            1 => Self::Decline,
            _ => Self::Cancel,
        }
    }

    const fn choice(self) -> i32 {
        match self {
            Self::Accept => 0,
            Self::Decline => 1,
            Self::Cancel => 2,
        }
    }
}

/// One question, as it will be shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Question {
    pub(crate) heading: String,
    pub(crate) body: String,
    pub(crate) warning: bool,
    pub(crate) accept: String,
    /// Empty when the question has no destructive alternative.
    pub(crate) decline: String,
    /// Empty when the question has no Cancel button. Escape still cancels.
    pub(crate) cancel: String,
    /// Where Enter goes when it opens.
    pub(crate) default: Answer,
}

impl Question {
    /// Static information with one button.
    pub(crate) fn info(heading: &str, body: &str) -> Self {
        Self {
            heading: heading.to_owned(),
            body: body.to_owned(),
            warning: false,
            accept: "OK".to_owned(),
            decline: String::new(),
            cancel: String::new(),
            default: Answer::Accept,
        }
    }

    /// Before closing a document, or the window, with unsaved work in it.
    pub(crate) fn unsaved(name: &str) -> Self {
        Self {
            heading: "Unsaved changes".to_owned(),
            body: format!("{name} has unsaved changes. Save them before closing?"),
            warning: true,
            accept: "Save".to_owned(),
            decline: "Don't Save".to_owned(),
            cancel: "Cancel".to_owned(),
            default: Answer::Accept,
        }
    }

    /// Before reloading a document with unsaved work: reloading *is* the
    /// discard, so the only yes is the destructive one, and it is not the
    /// default.
    pub(crate) fn reload(name: &str) -> Self {
        Self {
            heading: "Reload from disk".to_owned(),
            body: format!(
                "{name} has unsaved changes. Reloading replaces them with what is on disk."
            ),
            warning: true,
            accept: "Discard and Reload".to_owned(),
            decline: String::new(),
            cancel: "Cancel".to_owned(),
            default: Answer::Cancel,
        }
    }

    /// At startup, when an earlier run left unsaved work in the journal.
    ///
    /// Three answers, and the third is the point: *Not Now* keeps the work
    /// and asks again next time, so a question closed without thinking is
    /// not a decision to lose anything.
    pub(crate) fn recovery(names: &[&str], count: usize) -> Self {
        let shown = names.join("\n");
        let more = count.saturating_sub(names.len());
        let tail = if more == 0 {
            String::new()
        } else {
            format!("\n…and {more} more")
        };
        Self {
            heading: "Unsaved work recovered".to_owned(),
            body: format!(
                "{} closed with {count} unsaved document(s):\n\n{shown}{tail}\n\n\
                 Restore them now? Not Now keeps them and asks again next time.",
                bp_platform::DISPLAY_NAME,
            ),
            warning: true,
            accept: "Restore".to_owned(),
            decline: "Discard".to_owned(),
            cancel: "Not Now".to_owned(),
            default: Answer::Accept,
        }
    }

    /// Before saving over a file somebody else changed. The safe choice is
    /// *not* to save, so Enter cancels.
    pub(crate) fn changed_on_disk(name: &str) -> Self {
        Self {
            heading: "Changed on disk".to_owned(),
            body: format!(
                "{name} has changed on disk since you opened it. Saving will \
                 overwrite those changes."
            ),
            warning: true,
            accept: "Save Anyway".to_owned(),
            decline: String::new(),
            cancel: "Cancel".to_owned(),
            default: Answer::Cancel,
        }
    }

    /// Before Replace All, with the plan shown.
    ///
    /// The listing is bounded: a preview of forty thousand replacements is
    /// not a preview.
    pub(crate) fn replace_all(plan: &bp_search::ReplacePlan) -> Self {
        const SHOWN: usize = 12;
        Self {
            heading: "Replace All".to_owned(),
            body: format!("{}\n\n{}", plan.summary(), plan.preview(SHOWN)),
            warning: true,
            accept: "Replace".to_owned(),
            decline: String::new(),
            cancel: "Cancel".to_owned(),
            default: Answer::Accept,
        }
    }

    /// Before registering as the default editor. ADR-0012 is about the user
    /// keeping control, so Enter does not register.
    pub(crate) fn registration(body: &str) -> Self {
        Self {
            heading: "Set as default editor".to_owned(),
            body: body.to_owned(),
            warning: true,
            accept: "Continue".to_owned(),
            decline: String::new(),
            cancel: "Cancel".to_owned(),
            default: Answer::Cancel,
        }
    }
}

/// What to do with last session's journal, given the answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Recovery {
    Restore,
    /// Delete what was offered. Only an explicit Discard.
    Forget,
    /// Leave it on disk and ask again at the next launch.
    Keep,
}

pub(crate) const fn recovery(answer: Answer) -> Recovery {
    match answer {
        Answer::Accept => Recovery::Restore,
        Answer::Decline => Recovery::Forget,
        Answer::Cancel => Recovery::Keep,
    }
}

/// What to do with a document's unsaved work before it closes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Unsaved {
    Save,
    /// Close without saving. Only an explicit Don't Save.
    Discard,
    /// Leave the document open, as it was.
    Stay,
}

pub(crate) const fn unsaved(answer: Answer) -> Unsaved {
    match answer {
        Answer::Accept => Unsaved::Save,
        Answer::Decline => Unsaved::Discard,
        Answer::Cancel => Unsaved::Stay,
    }
}

/// A confirmation: only the Accept button confirms.
pub(crate) const fn confirmed(answer: Answer) -> bool {
    matches!(answer, Answer::Accept)
}

type Then = Box<dyn FnOnce(Answer)>;

/// The questions waiting to be asked, one on screen at a time.
///
/// A queue rather than a slot because an answer can ask another question --
/// Save on an unsaved document can find the file changed on disk -- and a
/// report can arrive while something else is being asked.
pub(crate) struct Dialogs {
    ui: slint::Weak<AppWindow>,
    queue: RefCell<VecDeque<(Question, Then)>>,
}

impl Dialogs {
    pub(crate) fn new(ui: &AppWindow) -> Rc<Self> {
        let dialogs = Rc::new(Self {
            ui: ui.as_weak(),
            queue: RefCell::new(VecDeque::new()),
        });
        let weak = Rc::downgrade(&dialogs);
        ui.on_dialog_answered(move |choice| {
            if let Some(dialogs) = weak.upgrade() {
                dialogs.answered(Answer::from_choice(choice));
            }
        });
        dialogs
    }

    /// Put `question` on screen, or behind the one already there, and run
    /// `then` with the answer.
    pub(crate) fn ask(&self, question: Question, then: impl FnOnce(Answer) + 'static) {
        let first = {
            let mut queue = self.queue.borrow_mut();
            queue.push_back((question, Box::new(then)));
            queue.len() == 1
        };
        if first {
            self.show_front();
        }
    }

    /// Something to read, with one button and nothing to decide.
    pub(crate) fn inform(&self, heading: &str, body: &str) {
        self.ask(Question::info(heading, body), |_| {});
    }

    /// Whether a question is on screen. The window will not close under one:
    /// closing is itself a question about unsaved work, and it cannot be
    /// asked while another is waiting for an answer.
    pub(crate) fn is_open(&self) -> bool {
        !self.queue.borrow().is_empty()
    }

    fn show_front(&self) {
        let Some(ui) = self.ui.upgrade() else { return };
        let queue = self.queue.borrow();
        let Some((q, _)) = queue.front() else {
            ui.set_dialog_open(false);
            return;
        };
        ui.set_dialog_heading(q.heading.as_str().into());
        ui.set_dialog_body(q.body.as_str().into());
        ui.set_dialog_warning(q.warning);
        ui.set_dialog_accept(q.accept.as_str().into());
        ui.set_dialog_decline(q.decline.as_str().into());
        ui.set_dialog_cancel(q.cancel.as_str().into());
        ui.set_dialog_default(q.default.choice());
        ui.set_dialog_open(true);
        ui.invoke_focus_dialog();
    }

    fn answered(&self, answer: Answer) {
        // The borrow ends before the continuation runs: it may well ask
        // another question, which needs the queue.
        let Some((_, then)) = self.queue.borrow_mut().pop_front() else {
            return;
        };
        if self.queue.borrow().is_empty() {
            if let Some(ui) = self.ui.upgrade() {
                ui.set_dialog_open(false);
                ui.invoke_focus_after_question();
            }
        } else {
            self.show_front();
        }
        then(answer);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_choice_nobody_expected_is_a_cancel_and_never_a_decline() {
        // The defect ADR-0084 replaced was a failure read as a refusal. An
        // out-of-range value from the dialog must not be read as consent to
        // lose anything.
        for strange in [-1, 3, 42, i32::MIN, i32::MAX] {
            assert_eq!(Answer::from_choice(strange), Answer::Cancel, "{strange}");
        }
        assert_eq!(Answer::from_choice(0), Answer::Accept);
        assert_eq!(Answer::from_choice(1), Answer::Decline);
        assert_eq!(Answer::from_choice(2), Answer::Cancel);
    }

    #[test]
    fn a_choice_round_trips_through_what_the_dialog_reports() {
        for answer in [Answer::Accept, Answer::Decline, Answer::Cancel] {
            assert_eq!(Answer::from_choice(answer.choice()), answer);
        }
    }

    #[test]
    fn only_an_explicit_discard_forgets_last_sessions_work() {
        // What `rfd` did without `zenity`: every question came back Cancel,
        // and the recovery prompt treated that as Discard.
        assert_eq!(recovery(Answer::Cancel), Recovery::Keep);
        assert_eq!(recovery(Answer::Decline), Recovery::Forget);
        assert_eq!(recovery(Answer::Accept), Recovery::Restore);
    }

    #[test]
    fn only_an_explicit_dont_save_closes_a_document_without_saving() {
        assert_eq!(unsaved(Answer::Cancel), Unsaved::Stay);
        assert_eq!(unsaved(Answer::Decline), Unsaved::Discard);
        assert_eq!(unsaved(Answer::Accept), Unsaved::Save);
    }

    #[test]
    fn only_the_accept_button_confirms() {
        assert!(confirmed(Answer::Accept));
        assert!(!confirmed(Answer::Decline));
        assert!(!confirmed(Answer::Cancel));
    }

    #[test]
    fn every_question_that_can_destroy_work_offers_a_way_out_that_does_not() {
        // A question whose only buttons all lose something is not a question.
        // Escape always cancels, but a visible Cancel is what a person
        // reaches for, and the recovery prompt's is the one that keeps work.
        let plan = bp_search::plan_replace_all("a a", &bp_search::Query::literal("a"), "b")
            .expect("a literal query always plans");
        for q in [
            Question::unsaved("notes.txt"),
            Question::reload("notes.txt"),
            Question::recovery(&["notes.txt"], 1),
            Question::changed_on_disk("notes.txt"),
            Question::replace_all(&plan),
            Question::registration("what would change"),
        ] {
            assert!(!q.cancel.is_empty(), "{} has no Cancel", q.heading);
            assert!(q.warning, "{} should look like a warning", q.heading);
        }
    }

    #[test]
    fn overwriting_somebody_elses_changes_is_never_the_default() {
        // Enter is what a hurried person presses. On these questions it must
        // be the answer that writes nothing.
        assert_eq!(Question::changed_on_disk("a").default, Answer::Cancel);
        assert_eq!(Question::reload("a").default, Answer::Cancel);
        assert_eq!(Question::registration("a").default, Answer::Cancel);
    }

    #[test]
    fn the_recovery_question_says_what_not_now_does() {
        // A third button nobody understands is a coin toss. The body says
        // the work is kept.
        let q = Question::recovery(&["a.txt", "b.txt"], 2);
        assert!(q.body.contains("a.txt") && q.body.contains("b.txt"));
        assert!(q.body.contains("Not Now keeps them"), "{}", q.body);
        assert_eq!(q.cancel, "Not Now");
    }

    #[test]
    fn a_long_recovery_list_says_how_many_it_did_not_show() {
        let q = Question::recovery(&["a", "b", "c", "d", "e"], 8);
        assert!(q.body.contains("8 unsaved"), "{}", q.body);
        assert!(q.body.contains("and 3 more"), "{}", q.body);
    }
}
