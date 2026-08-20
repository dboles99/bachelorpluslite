//! The Data menu, and what it does to the document underneath it.
//!
//! `bp-data` owns every parser, formatter and converter. This module owns the
//! two things it deliberately does not: which of them a menu id means for the
//! format in front of the user, and what happens to the document when one
//! succeeds or fails.
//!
//! ## Every operation here is an ordinary edit
//!
//! Formatting JSON, minifying it, converting a CSV to JSON Lines -- all of
//! them replace the buffer and leave the document unsaved. Ctrl+Z takes it
//! back, and nothing reaches the disk until the user saves. That is the whole
//! reason [`AppState::apply_to_active`] exists rather than each arm writing
//! its own success path: an operation that quietly wrote to disk, or one that
//! could not be undone, would be a different kind of thing wearing the same
//! menu.
//!
//! A failure changes nothing. `bp-data`'s error is reported and the buffer is
//! left exactly as it was, which is why the refusals YAML makes (ADR-0023 --
//! depth, alias expansion, duplicate keys) are safe to surface as a sentence
//! rather than as a dialog.
//!
//! ## Why the pairing is `(id, format)` and not `id`
//!
//! A row is offered for the format it applies to, and the same id means
//! something different for a JSON document and a CSV one. Matching on the
//! pair rather than on the id alone is what makes an id from a stale menu do
//! nothing instead of guessing -- the fall-through arm is a deliberate no-op,
//! not an oversight.

use bp_formats::Format;

use super::AppState;
use crate::menus::action;

impl AppState {
    /// Replace the active document's text with the result of a data
    /// operation, leaving it unsaved.
    ///
    /// Applied as an ordinary edit: the user can undo it, and nothing reaches
    /// disk until they save.
    fn apply_to_active(&mut self, result: Result<String, bp_data::DataError>) {
        match result {
            Ok(text) => {
                self.error = None;
                self.edit(text);
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    /// Apply a YAML rewrite, and say what it cost or why it refused.
    ///
    /// Two things ADR-0023 requires the caller to say, and neither is
    /// `bp-data`'s to say for it.
    ///
    /// On success: `saphyr` parses YAML into data, so a comment -- which is
    /// not data -- has nothing to be put back from, and an alias is resolved
    /// into a copy of what it pointed at. Both preserve what the document
    /// *means* while changing what it says. The menu row warns before the
    /// click; this says it again after, because the status bar is what is on
    /// screen when the user looks at the result.
    ///
    /// On failure: that **nothing changed**. The library's own sentence
    /// follows verbatim rather than being re-worded here -- the three
    /// refusals ADR-0023 designs for each name their limit and where it was
    /// hit, and a shell that paraphrased them would be a second place for
    /// that wording to drift.
    fn apply_yaml(&mut self, result: Result<String, bp_data::DataError>, did: &str) {
        match result {
            Ok(text) => {
                self.edit(text);
                self.error = Some(format!(
                    "✓ {did} — comments were not kept and aliases were expanded; Ctrl+Z undoes it"
                ));
            }
            Err(e) => self.error = Some(format!("this YAML was left unchanged — {e}")),
        }
    }

    /// Run a data operation, reporting the outcome in the status bar.
    pub(crate) fn run_data_action(&mut self, id: i32) {
        let text = self.active_text().to_owned();
        match (id, self.format()) {
            (action::DATA_VALIDATE, Format::Json) => {
                self.error = Some(match bp_data::json_validate(&text) {
                    Ok(()) => "✓ valid JSON".to_owned(),
                    Err(e) => format!("invalid JSON — {e}"),
                });
            }
            (action::DATA_VALIDATE, Format::JsonLines) => {
                self.error = Some(bp_data::jsonl_validate(&text).summary());
            }
            (action::DATA_VALIDATE, Format::Toml) => {
                self.error = Some(match bp_data::toml_validate(&text) {
                    Ok(()) => "✓ valid TOML".to_owned(),
                    Err(e) => format!("invalid TOML — {e}"),
                });
            }
            // The document count is part of the answer rather than a nicety.
            // A YAML file is a *stream*: Format and Minify write every
            // document back and Convert to JSON refuses more than one, so the
            // first place a user learns there are three must not be the
            // refusal (ADR-0023).
            (action::DATA_VALIDATE, Format::Yaml) => {
                let count = bp_data::yaml_validate(&text)
                    .and_then(|()| bp_data::yaml_document_count(&text));
                self.error = Some(match count {
                    Ok(0) => "✓ valid YAML — no documents in this file".to_owned(),
                    Ok(1) => "✓ valid YAML".to_owned(),
                    Ok(n) => format!("✓ valid YAML — {n} documents"),
                    Err(e) => format!("invalid YAML — {e}"),
                });
            }
            (action::DATA_FORMAT, Format::Json) => {
                self.apply_to_active(bp_data::json_format(&text));
            }
            (action::DATA_FORMAT, Format::Toml) => {
                self.apply_to_active(bp_data::toml_format(&text));
            }
            (action::DATA_MINIFY, Format::Json) => {
                self.apply_to_active(bp_data::json_minify(&text));
            }
            (action::DATA_FORMAT, Format::Yaml) => {
                self.apply_yaml(bp_data::yaml_format(&text), "formatted");
            }
            (action::DATA_MINIFY, Format::Yaml) => {
                self.apply_yaml(bp_data::yaml_minify(&text), "rewritten in flow style");
            }
            (action::DATA_YAML_TO_JSON, _) => {
                self.apply_yaml(bp_data::yaml_to_json(&text), "converted to JSON");
            }
            // The one direction that loses nothing, so it is the one that
            // does not go through `apply_yaml`: every JSON value has a YAML
            // spelling, and there are no comments in JSON to drop.
            (action::DATA_JSON_TO_YAML, _) => {
                let converted = bp_data::json_to_yaml(&text);
                let ok = converted.is_ok();
                self.apply_to_active(converted);
                if ok {
                    self.error = Some("✓ converted to YAML".to_owned());
                }
            }
            (action::DATA_TO_JSONL, _) => self.apply_to_active(bp_data::json_to_jsonl(&text)),
            (action::DATA_TO_JSON, _) => self.apply_to_active(bp_data::jsonl_to_json(&text)),
            (action::DATA_REPORT, _) => {
                self.error = Some(match bp_data::delimited_report(&text) {
                    Ok(report) => report.summary(),
                    Err(e) => format!("could not read as a table — {e}"),
                });
            }
            (action::DATA_CSV_TO_JSON, _) => {
                self.apply_to_active(bp_data::delimited_to_json(&text));
            }
            (action::DATA_CSV_TO_JSONL, _) => {
                self.apply_to_active(bp_data::delimited_to_jsonl(&text));
            }
            (action::DATA_COLUMN_TYPES, _) => {
                self.error = Some(match bp_data::column_types(&text) {
                    Ok(columns) if columns.is_empty() => "no columns to report".to_owned(),
                    Ok(columns) => columns
                        .iter()
                        .map(bp_data::ColumnReport::summary)
                        .collect::<Vec<_>>()
                        .join(" │ "),
                    Err(e) => format!("could not read as a table — {e}"),
                });
            }
            // A row that does not apply to this format. The menu should not
            // have offered it; doing nothing is better than guessing.
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::state::now;

    fn csv_state(text: &str) -> AppState {
        let mut state = AppState::new();
        let id = state.workspace.active_id().unwrap();
        // Format detection is extension-first (`bp_formats::detect`), so a
        // `.csv` path is what makes `run_data_action` see `Format::Csv`
        // without needing a real file on disk.
        state
            .workspace
            .get_mut(id)
            .unwrap()
            .set_path(PathBuf::from("table.csv"));
        state.edit(text.to_owned());
        state
    }

    #[test]
    fn converting_csv_to_json_replaces_the_document() {
        let mut state = csv_state("name,age\nAlice,30\nBob,25\n");
        let id = state.workspace.active_id().unwrap();
        // Cleared explicitly, rather than relying on the dirty flag the
        // initial `edit` already set, so the assertion below proves the
        // conversion itself is what marks the document unsaved.
        state.workspace.get_mut(id).unwrap().record_disk_save(now());

        state.run_data_action(action::DATA_CSV_TO_JSON);

        assert!(
            state.error.is_none(),
            "a successful conversion is not an error"
        );
        assert!(state.active_text().contains("Alice"));
        assert!(state.active_text().trim_start().starts_with('['));
        assert!(
            state.is_dirty(id),
            "nothing reaches disk on its own -- the conversion is an ordinary edit"
        );
    }

    #[test]
    fn converting_csv_to_json_lines_replaces_the_document() {
        let mut state = csv_state("name,age\nAlice,30\nBob,25\n");
        state.run_data_action(action::DATA_CSV_TO_JSONL);

        let text = state.active_text();
        assert_eq!(text.lines().count(), 2, "one compact object per data row");
        assert!(text.contains("Alice") && text.contains("Bob"));
    }

    #[test]
    fn column_types_reports_into_the_status_bar_without_changing_the_document() {
        let mut state = csv_state("name,age\nAlice,30\nBob,25\n");
        let before = state.active_text();

        state.run_data_action(action::DATA_COLUMN_TYPES);

        assert_eq!(
            state.active_text(),
            before,
            "a report must not edit the document"
        );
        let message = state.error.expect("column report in the status bar");
        assert!(message.contains("name"), "got {message}");
        assert!(
            message.contains("integer"),
            "age should read as integer; got {message}"
        );
    }

    #[test]
    fn an_action_that_is_not_a_csv_operation_on_a_csv_document_changes_nothing() {
        let mut state = csv_state("name,age\nAlice,30\n");
        let before = state.active_text();
        state.run_data_action(action::NOTE_TITLE);
        assert_eq!(state.active_text(), before);
    }

    /// A document `run_data_action` will see as `Format::Yaml`.
    ///
    /// Detection is extension-first, so the path is what decides it and no
    /// file has to exist -- the same trick `csv_state` uses.
    fn yaml_state(text: &str) -> AppState {
        let mut state = AppState::new();
        let id = state.workspace.active_id().unwrap();
        state
            .workspace
            .get_mut(id)
            .unwrap()
            .set_path(PathBuf::from("config.yaml"));
        state.edit(text.to_owned());
        state
    }

    /// The "billion laughs" document from ADR-0023: ten anchors, each a
    /// sequence of ten references to the one before. Around 200 bytes, and it
    /// names more nodes than the machine has memory for.
    fn billion_laughs() -> String {
        let mut text = String::from("a: &a [x,x,x,x,x,x,x,x,x,x]\n");
        for (name, previous) in [
            ('b', 'a'),
            ('c', 'b'),
            ('d', 'c'),
            ('e', 'd'),
            ('f', 'e'),
            ('g', 'f'),
        ] {
            let refs = std::iter::repeat_n(format!("*{previous}"), 10)
                .collect::<Vec<_>>()
                .join(",");
            text.push_str(&format!("{name}: &{name} [{refs}]\n"));
        }
        text
    }

    #[test]
    fn validating_yaml_says_how_many_documents_the_file_holds() {
        // A YAML file is a stream. Format writes every document back and
        // Convert to JSON refuses more than one, so the refusal must not be
        // where the user first learns there are three.
        let mut state = yaml_state("one: 1\n---\ntwo: 2\n---\nthree: 3\n");
        state.run_data_action(action::DATA_VALIDATE);
        let notice = state.error.clone().expect("a status message");

        assert!(notice.starts_with('✓'), "got {notice}");
        assert!(
            notice.contains('3'),
            "the count belongs in it; got {notice}"
        );
    }

    #[test]
    fn a_single_yaml_document_is_not_counted_out_at_the_user() {
        let mut state = yaml_state("one: 1\n");
        state.run_data_action(action::DATA_VALIDATE);
        assert_eq!(state.error.as_deref(), Some("✓ valid YAML"));
    }

    #[test]
    fn formatting_yaml_says_that_the_comments_did_not_survive() {
        // ADR-0023's one place where "never silently change the user's data"
        // needs a warning rather than an error: `saphyr` parses YAML into
        // data, and a comment is not data.
        let mut state = yaml_state("# why this value matters\nkey:   value\n");
        state.run_data_action(action::DATA_FORMAT);
        let notice = state.error.clone().expect("a status message");

        assert!(
            !state.active_text().contains("why this value matters"),
            "the fixture must actually lose its comment, or this proves nothing"
        );
        assert!(
            notice.contains("comments"),
            "the user has to be told what went; got {notice}"
        );
        assert!(
            notice.contains("Ctrl+Z"),
            "and how to get it back; got {notice}"
        );
    }

    #[test]
    fn minifying_yaml_rewrites_it_in_flow_style_and_says_the_same_thing() {
        let mut state = yaml_state("key:\n  - one\n  - two\n");
        state.run_data_action(action::DATA_MINIFY);
        let notice = state.error.clone().expect("a status message");

        assert!(
            state.active_text().contains('['),
            "got {}",
            state.active_text()
        );
        assert!(notice.contains("comments"), "got {notice}");
    }

    #[test]
    fn yaml_nested_past_the_limit_is_refused_with_the_limit_named() {
        // Not a taste judgement. `saphyr`'s loader recurses one stack frame
        // per level and a stack overflow aborts the process, so this refusal
        // is the only form the answer can take -- and the number has to be in
        // it, because "too deep" is not something a person can act on.
        let deep = format!(
            "{}{}",
            "[".repeat(bp_data::MAX_YAML_NESTING + 1),
            "]".repeat(bp_data::MAX_YAML_NESTING + 1)
        );
        let mut state = yaml_state(&deep);
        state.run_data_action(action::DATA_FORMAT);
        let notice = state.error.clone().expect("a status message");

        assert!(
            notice.contains("left unchanged"),
            "the user has to know the document was not touched; got {notice}"
        );
        assert!(
            notice.contains(&bp_data::MAX_YAML_NESTING.to_string()),
            "the limit belongs in it; got {notice}"
        );
        assert!(
            notice.contains("line 1, column"),
            "and where it was hit; got {notice}"
        );
        assert_eq!(state.active_text(), deep, "nothing may have been rewritten");
    }

    #[test]
    fn yaml_that_expands_past_the_node_budget_is_refused_before_it_is_built() {
        // Around 200 bytes, so neither the file size nor the nesting cap sees
        // it coming -- the document is only ever seven levels deep.
        let laughs = billion_laughs();
        assert!(
            laughs.len() < 400,
            "the fixture must stay small to mean anything"
        );

        let mut state = yaml_state(&laughs);
        state.run_data_action(action::DATA_FORMAT);
        let notice = state.error.clone().expect("a status message");

        assert!(notice.contains("left unchanged"), "got {notice}");
        assert!(
            notice.contains(&bp_data::MAX_YAML_NODES.to_string()),
            "the budget belongs in it; got {notice}"
        );
        assert_eq!(state.active_text(), laughs);
    }

    #[test]
    fn a_duplicate_yaml_key_is_refused_in_words_that_say_why_it_matters() {
        // The whole reason this is an error rather than a merge: `saphyr`
        // drops the earlier value on the way into the map, so formatting the
        // file would delete a line and report success. A message that read
        // like an ordinary parse error would leave the user editing their
        // YAML looking for a missing colon.
        let mut state = yaml_state("name: first\nname: second\n");
        state.run_data_action(action::DATA_FORMAT);
        let notice = state.error.clone().expect("a status message");

        assert!(notice.contains("duplicate key"), "got {notice}");
        assert!(
            notice.contains("replace"),
            "it has to say what would have been lost, not just that it refused; got {notice}"
        );
        assert!(
            notice.contains("line 2"),
            "and which of the two keys to go and look at; got {notice}"
        );
        assert_eq!(state.active_text(), "name: first\nname: second\n");
    }

    #[test]
    fn a_duplicate_key_is_reported_by_validate_as_well_as_by_format() {
        // Validate is where somebody checks a file they are about to hand
        // over, and it is the row that must not answer "fine".
        let mut state = yaml_state("name: first\nname: second\n");
        state.run_data_action(action::DATA_VALIDATE);
        let notice = state.error.clone().expect("a status message");

        assert!(!notice.starts_with('✓'), "got {notice}");
        assert!(notice.contains("duplicate key"), "got {notice}");
    }

    #[test]
    fn converting_a_yaml_stream_to_json_refuses_and_names_the_count() {
        // JSON has one root value. Wrapping three documents in an array would
        // hand back a different shape from the one on screen.
        let mut state = yaml_state("one: 1\n---\ntwo: 2\n");
        state.run_data_action(action::DATA_YAML_TO_JSON);
        let notice = state.error.clone().expect("a status message");

        assert!(notice.contains("left unchanged"), "got {notice}");
        assert!(
            notice.contains('2'),
            "the count belongs in it; got {notice}"
        );
        assert_eq!(state.active_text(), "one: 1\n---\ntwo: 2\n");
    }

    #[test]
    fn converting_one_yaml_document_to_json_replaces_the_document() {
        let mut state = yaml_state("key: value\n");
        state.run_data_action(action::DATA_YAML_TO_JSON);

        assert!(
            state.active_text().contains("\"key\""),
            "got {}",
            state.active_text()
        );
        assert!(state.error.as_deref().is_some_and(|e| e.starts_with('✓')));
    }

    #[test]
    fn converting_json_to_yaml_promises_nothing_was_lost() {
        // The asymmetry ADR-0023 draws: every JSON value has a YAML spelling,
        // so this direction has no warning to carry -- and a warning attached
        // to it anyway would teach the user to ignore the ones that matter.
        let mut state = AppState::new();
        let id = state.workspace.active_id().unwrap();
        state
            .workspace
            .get_mut(id)
            .unwrap()
            .set_path(PathBuf::from("data.json"));
        state.edit("{\"key\": \"value\"}".to_owned());
        state.run_data_action(action::DATA_JSON_TO_YAML);

        assert!(
            state.active_text().contains("key: value"),
            "got {}",
            state.active_text()
        );
        assert_eq!(state.error.as_deref(), Some("✓ converted to YAML"));
        assert!(
            !state.error.as_deref().unwrap().contains("comments"),
            "there are no comments in JSON to lose"
        );
    }
}
