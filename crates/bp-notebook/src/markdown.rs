//! A Markdown document read as a notebook (ADR-0045).
//!
//! A README with fenced code blocks is already the thing [`crate::Notebook`]
//! models: an ordered sequence of prose and code, each piece knowing its own
//! language. So this does not invent a second model -- it produces the one
//! this crate already has, and everything built on `Notebook` works on a
//! `.md` file without knowing it came from one.
//!
//! ## What names a block
//!
//! **The fence's info string, through [`CellKind::from_language_name`].**
//! That is the same function a `.ipynb`'s kernelspec goes through, so
//! ` ```python `, ` ```py ` and a notebook whose kernel is `python3` all
//! arrive as the same kind, and none of them is a place where a second
//! spelling table could drift from the first.
//!
//! A fence with **no** info string, or one this build does not recognise,
//! becomes [`CellKind::Raw`] -- carried verbatim and never run. That is the
//! same treatment `ipynb` gives a code cell whose language it cannot
//! identify, and for the same reason: "keep the bytes and do nothing" is the
//! only safe answer to text whose language we would otherwise be guessing at.
//!
//! ## What is a fence
//!
//! CommonMark's rule, as far as it decides which bytes are code:
//!
//! * three or more backticks or three or more tildes, opening and closing;
//! * the closing fence uses the same character and is at least as long;
//! * up to three spaces of indentation before either;
//! * a backtick-fenced block's info string may not contain a backtick, which
//!   is how ` ``` ` inside a paragraph does not open one;
//! * an unclosed fence runs to the end of the document.
//!
//! **Indented code blocks are not fences and are left in the prose.** Four
//! spaces makes something code in CommonMark, but it names no language --
//! so it could never be run, and lifting it out of the prose would produce a
//! `Raw` cell that says nothing the prose did not already say.

use crate::cell::CellKind;
use crate::notebook::Notebook;

/// A Markdown document, as cells, with the line each one began on.
///
/// Shaped like `ipynb`'s `Import`: the useful thing plus the context a caller
/// needs to say something honest about it. Here that context is line numbers,
/// because a `.md` file has no cell numbers -- a reader looking for the block
/// a menu row names looks for a *line*.
#[derive(Debug, Clone)]
pub struct MarkdownDocument {
    notebook: Notebook,
    lines: Vec<usize>,
}

impl MarkdownDocument {
    /// The cells, borrowed.
    #[must_use]
    pub fn notebook(&self) -> &Notebook {
        &self.notebook
    }

    /// The cells, taken.
    #[must_use]
    pub fn into_notebook(self) -> Notebook {
        self.notebook
    }

    /// The 1-based line the `index`-th cell began on.
    ///
    /// For a code cell that is the line of its opening fence, not of its
    /// first line of code: the fence is what a reader sees when they get
    /// there.
    #[must_use]
    pub fn line_of(&self, index: usize) -> Option<usize> {
        self.lines.get(index).copied()
    }
}

/// Read a Markdown document as a sequence of prose and code cells.
///
/// Never fails. Markdown has no invalid form -- an unclosed fence is a fence
/// that reaches the end, not an error -- so there is nothing here for a
/// `Result` to carry, and returning one would make every caller handle a case
/// that cannot happen.
#[must_use]
pub fn parse(source: &str) -> MarkdownDocument {
    let mut notebook = Notebook::new();
    let mut lines = Vec::new();
    let mut prose: Vec<&str> = Vec::new();
    let mut prose_line = 1usize;

    let mut input = source.lines().enumerate().peekable();
    while let Some((index, line)) = input.next() {
        let Some(fence) = Fence::opening(line) else {
            if prose.is_empty() {
                prose_line = index + 1;
            }
            prose.push(line);
            continue;
        };

        // The prose before this fence is a cell of its own, so that a code
        // cell's position among the cells matches its position in the file.
        if !prose.is_empty() {
            flush_prose(&mut notebook, &mut lines, &mut prose, prose_line);
        }

        let mut body: Vec<&str> = Vec::new();
        for (_, next) in input.by_ref() {
            if fence.closes(next) {
                break;
            }
            body.push(next);
        }
        notebook.push(fence.kind, joined(&body));
        // The opening fence's line, 1-based.
        lines.push(index + 1);
    }
    if !prose.is_empty() {
        flush_prose(&mut notebook, &mut lines, &mut prose, prose_line);
    }

    MarkdownDocument { notebook, lines }
}

/// Push the prose gathered so far as one Markdown cell.
fn flush_prose(notebook: &mut Notebook, lines: &mut Vec<usize>, prose: &mut Vec<&str>, at: usize) {
    notebook.push(CellKind::Markdown, joined(prose));
    lines.push(at);
    prose.clear();
}

/// Lines back into text, with the trailing newline a cell's source carries.
fn joined(lines: &[&str]) -> String {
    if lines.is_empty() {
        return String::new();
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

/// An open fence, and what it takes to close it.
struct Fence {
    marker: u8,
    length: usize,
    kind: CellKind,
}

impl Fence {
    /// Read `line` as an opening fence, if it is one.
    fn opening(line: &str) -> Option<Self> {
        let (indent, rest) = split_indent(line);
        // More than three spaces makes it an indented code block, which is
        // not a fence at all.
        if indent > 3 {
            return None;
        }
        let marker = match rest.as_bytes().first()? {
            b'`' => b'`',
            b'~' => b'~',
            _ => return None,
        };
        let length = rest.bytes().take_while(|b| *b == marker).count();
        if length < 3 {
            return None;
        }
        let info = rest[length..].trim();
        // A backtick in a backtick fence's info string means this is not an
        // opening fence -- it is the toolkit's rule for telling ``` used
        // inline from ``` opening a block.
        if marker == b'`' && info.contains('`') {
            return None;
        }
        // The first word: ```python {highlight=1} names Python.
        let language = info.split_whitespace().next().unwrap_or("");
        Some(Self {
            marker,
            length,
            // Raw for both "no language given" and "a language this build
            // does not know" -- keep the bytes and do nothing.
            kind: CellKind::from_language_name(language).unwrap_or(CellKind::Raw),
        })
    }

    /// Whether `line` closes this fence.
    fn closes(&self, line: &str) -> bool {
        let (indent, rest) = split_indent(line);
        if indent > 3 {
            return false;
        }
        let run = rest.bytes().take_while(|b| *b == self.marker).count();
        // At least as long as the opener, and nothing but the marker after it.
        run >= self.length && rest[run..].trim().is_empty()
    }
}

/// Leading spaces, and what follows them.
fn split_indent(line: &str) -> (usize, &str) {
    let indent = line.bytes().take_while(|b| *b == b' ').count();
    (indent, &line[indent..])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(source: &str) -> Vec<CellKind> {
        parse(source)
            .into_notebook()
            .cells()
            .iter()
            .map(|cell| cell.kind())
            .collect()
    }

    #[test]
    fn a_fence_becomes_a_cell_of_the_language_it_names() {
        let document = parse("intro\n\n```python\nprint(1)\n```\n");
        let notebook = document.notebook();

        assert_eq!(notebook.cells().len(), 2);
        assert_eq!(notebook.cells()[0].kind(), CellKind::Markdown);
        assert_eq!(notebook.cells()[1].kind(), CellKind::Python);
        assert_eq!(notebook.cells()[1].source(), "print(1)\n");
    }

    #[test]
    fn the_info_string_goes_through_the_same_table_a_kernelspec_does() {
        // One spelling table, not two. `py`, `sh` and `pwsh` are aliases
        // `CellKind::from_language_name` already knows, and a second list
        // here would be a second thing to keep in step.
        assert_eq!(kinds("```py\nx\n```\n"), vec![CellKind::Python]);
        assert_eq!(kinds("```bash\nx\n```\n"), vec![CellKind::Shell]);
        assert_eq!(kinds("```pwsh\nx\n```\n"), vec![CellKind::PowerShell]);
    }

    #[test]
    fn a_fence_naming_no_language_is_raw_and_never_runnable() {
        // The same answer `ipynb` gives a code cell it cannot identify:
        // keep the bytes and do nothing. Guessing at the language of an
        // unlabelled block is how a shell script gets run as Python.
        assert_eq!(kinds("```\nsomething\n```\n"), vec![CellKind::Raw]);
        assert!(!CellKind::Raw.is_executable());
    }

    #[test]
    fn a_fence_naming_a_language_this_build_does_not_know_is_also_raw() {
        assert_eq!(kinds("```brainfuck\n+++\n```\n"), vec![CellKind::Raw]);
    }

    #[test]
    fn only_the_first_word_of_an_info_string_names_the_language() {
        // ```python {highlight=1} is python, and attributes after the
        // language are common in real documentation.
        assert_eq!(
            kinds("```python {highlight=1}\nx\n```\n"),
            vec![CellKind::Python]
        );
    }

    #[test]
    fn a_tilde_fence_is_a_fence_and_a_backtick_does_not_close_it() {
        // CommonMark: the closing fence must use the same character. Getting
        // this wrong would end a block early and put its tail in the prose.
        let document = parse("~~~python\nprint('```')\n~~~\n");
        let notebook = document.notebook();

        assert_eq!(notebook.cells().len(), 1);
        assert_eq!(notebook.cells()[0].kind(), CellKind::Python);
        assert_eq!(notebook.cells()[0].source(), "print('```')\n");
    }

    #[test]
    fn a_longer_closing_fence_closes_and_a_shorter_one_does_not() {
        // The rule that lets a block contain a shorter run of backticks.
        let document = parse("````python\n```\nstill code\n````\n");
        let notebook = document.notebook();

        assert_eq!(notebook.cells().len(), 1);
        assert_eq!(notebook.cells()[0].source(), "```\nstill code\n");
    }

    #[test]
    fn an_unclosed_fence_runs_to_the_end_rather_than_being_an_error() {
        // Markdown has no invalid form, which is why `parse` returns no
        // `Result`. A document being edited is unclosed most of the time.
        let document = parse("intro\n```python\nprint(1)\n");
        let notebook = document.notebook();

        assert_eq!(notebook.cells().len(), 2);
        assert_eq!(notebook.cells()[1].kind(), CellKind::Python);
        assert_eq!(notebook.cells()[1].source(), "print(1)\n");
    }

    #[test]
    fn an_indented_code_block_stays_in_the_prose() {
        // Four spaces is code in CommonMark and names no language, so it
        // could never be run; lifting it out would make a Raw cell that says
        // nothing the prose did not.
        let document = parse("intro\n\n    print(1)\n\nmore\n");
        assert_eq!(document.notebook().cells().len(), 1);
        assert_eq!(document.notebook().cells()[0].kind(), CellKind::Markdown);
    }

    #[test]
    fn a_fence_indented_up_to_three_spaces_is_still_a_fence() {
        assert_eq!(kinds("   ```python\nx\n   ```\n"), vec![CellKind::Python]);
    }

    #[test]
    fn a_backtick_inside_a_paragraph_does_not_open_a_block() {
        // ```` ```code``` ```` inline is prose. Opening a block on it would
        // swallow the rest of the document.
        let document = parse("use ```x``` inline\nmore prose\n");
        assert_eq!(document.notebook().cells().len(), 1);
        assert_eq!(document.notebook().cells()[0].kind(), CellKind::Markdown);
    }

    #[test]
    fn a_code_cell_reports_the_line_of_its_opening_fence() {
        // The fence is what a reader sees when they go to the line, not the
        // first line of code inside it.
        let document = parse("one\ntwo\n```python\nprint(1)\n```\n");

        assert_eq!(document.line_of(0), Some(1), "the prose starts at line 1");
        assert_eq!(document.line_of(1), Some(3), "the fence is on line 3");
    }

    #[test]
    fn several_blocks_keep_their_order_and_their_lines() {
        let source = "a\n```python\none()\n```\nb\n```sh\ntwo\n```\n";
        let document = parse(source);
        let notebook = document.notebook();

        let kinds: Vec<CellKind> = notebook.cells().iter().map(|c| c.kind()).collect();
        assert_eq!(
            kinds,
            vec![
                CellKind::Markdown,
                CellKind::Python,
                CellKind::Markdown,
                CellKind::Shell
            ]
        );
        assert_eq!(document.line_of(1), Some(2));
        assert_eq!(document.line_of(3), Some(6));
    }

    #[test]
    fn a_document_with_no_fences_is_one_prose_cell() {
        let document = parse("just words\nand more words\n");
        assert_eq!(document.notebook().cells().len(), 1);
        assert_eq!(document.notebook().cells()[0].kind(), CellKind::Markdown);
    }

    #[test]
    fn an_empty_document_has_no_cells() {
        assert!(parse("").into_notebook().cells().is_empty());
    }

    #[test]
    fn a_run_request_can_be_made_from_a_markdown_fence_like_any_other_cell() {
        // The whole point of producing a `Notebook` rather than a new type:
        // consent, refusal of prose, and everything built on `Notebook` work
        // unchanged on a document that was never a notebook.
        let notebook = parse("```python\nprint(1)\n```\n").into_notebook();
        let cell = notebook.cells()[0].id();

        let request = notebook
            .request_run(cell, crate::UserGesture::from_user_command())
            .expect("a python fence is runnable");
        assert_eq!(request.source(), "print(1)\n");
        assert_eq!(request.language(), CellKind::Python);
    }

    #[test]
    fn a_prose_cell_is_refused_a_run_exactly_as_a_markdown_notebook_cell_is() {
        let notebook = parse("just words\n").into_notebook();
        let cell = notebook.cells()[0].id();

        assert!(
            notebook
                .request_run(cell, crate::UserGesture::from_user_command())
                .is_err(),
            "prose has nothing to run"
        );
    }
}
