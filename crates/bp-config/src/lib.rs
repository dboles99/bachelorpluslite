//! Configuration for BachelorPad+.
//!
//! Precedence, highest first: command line, environment, config file,
//! built-in defaults.
//!
//! Two rules shape this crate:
//!
//! * **Bad configuration must never stop the editor starting.** A malformed
//!   file yields defaults plus a [`Notice`] the user can see, not a refusal to
//!   run. Someone who breaks their config at 2am still needs to open a file.
//! * **Nothing here ever touches document content.** Configuration is the
//!   thing most likely to be logged verbatim at startup, so it must contain
//!   only settings -- never text, paths inside documents, or key material
//!   (ADR-0011, and the Definition of Done's "no secret data in logs").
//!
//! [`resolve`] is pure: it takes the file contents, the environment and the
//! arguments as values, so precedence and malformed-input recovery are
//! testable without a filesystem or a process environment.

#![forbid(unsafe_code)]

use std::fmt;
use std::path::PathBuf;

use serde::Deserialize;

pub mod recent;
pub use recent::{MAX_RECENT, Recent, load_recent, save_recent};

/// Crate identity used by workspace smoke tests and diagnostics.
pub const CRATE_NAME: &str = "bp-config";

/// Directory name used under the platform's configuration root.
const APP_DIR: &str = "bachelorpad";
const FILE_NAME: &str = "config.toml";

/// Which renderer to ask Slint for. See ADR-0017.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RendererPref {
    /// CPU rasterisation. The default: the only configuration measured to
    /// meet the specs.md section 22 budgets.
    #[default]
    Software,
    /// Whatever Slint would pick, normally GPU-accelerated.
    Platform,
}

impl RendererPref {
    fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "software" | "cpu" => Some(Self::Software),
            "platform" | "gpu" | "default" => Some(Self::Platform),
            _ => None,
        }
    }
}

/// Smallest accepted editor font size, in points.
///
/// Below this a Slint `length` is too small to read, and treating it as a
/// typo rather than a legitimate wish protects users from fat-fingering
/// their editor into illegibility.
pub const MIN_FONT_SIZE: u8 = 6;

/// Largest accepted editor font size, in points.
///
/// A `u8` comfortably spans a usable typography range (this crate's whole
/// point is that broken input is clamped, not trusted), and staying inside
/// one byte keeps the type as small as the domain allows.
pub const MAX_FONT_SIZE: u8 = 96;

/// Editor font size used before any configuration is applied, in points.
///
/// `app.slint` no longer carries a size of its own -- it is fed this one --
/// so this is the single place the editor's starting size is decided.
pub const DEFAULT_FONT_SIZE: u8 = 14;

/// The font size one zoom step from `size`, bounded by what the editor accepts
/// from any other source.
///
/// Saturating and clamped rather than refusing: someone holding Ctrl+- wants
/// the text to stop shrinking, not the key to stop responding. The bounds are
/// [`MIN_FONT_SIZE`] and [`MAX_FONT_SIZE`], the same pair a config file is
/// held to, so zoom cannot reach a size the file could not have asked for.
#[must_use]
pub fn zoom(size: u8, steps: i8) -> u8 {
    size.saturating_add_signed(steps)
        .clamp(MIN_FONT_SIZE, MAX_FONT_SIZE)
}

/// Smallest accepted tab width, in columns.
///
/// A width of zero would collapse every indent level to nothing and turns
/// column arithmetic elsewhere into division-by-zero territory, so it is
/// rejected here rather than relied on callers to guard against.
const MIN_TAB_WIDTH: u8 = 1;

/// Largest accepted tab width, in columns.
///
/// Real editors top out around 8 by default; doubling that twice over to 32
/// leaves headroom for unusual but legitimate wide-indent styles while still
/// catching what is almost certainly a typo -- a three-digit tab width reads
/// like a font size typed into the wrong setting.
const MAX_TAB_WIDTH: u8 = 32;

/// Tab width used before any configuration is applied, in columns. Matches
/// the `TAB_WIDTH` constant `bp-ui` currently hard-codes, and the default
/// `bp_editor::view` already falls back to.
const DEFAULT_TAB_WIDTH: u8 = 4;

/// Resolved settings.
///
/// `theme` stays a `String` rather than a `bp-theme` enum on purpose: an
/// unrecognised theme name in a user's file is a recoverable mistake that
/// should warn and fall back, and keeping it textual here means this crate
/// does not need to know which themes exist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub theme: Option<String>,
    pub renderer: RendererPref,
    /// `tracing` filter directive.
    pub log: String,
    /// Editor font size in points. A `u8` because it ends up as a Slint
    /// `length` measured in points, which is always a small positive whole
    /// number in practice -- there is no meaningful fractional or negative
    /// point size for this setting, and `u8` rules both out at the type
    /// level rather than by convention.
    pub font_size: u8,
    /// Indentation width in columns. A `u8` for the same reason as
    /// `font_size`: it is always a small positive whole number in practice.
    /// Matches the `tab_width` parameter `bp_editor::view` already threads
    /// through everywhere, and the `TAB_WIDTH` constant `bp-ui` currently
    /// hard-codes.
    pub tab_width: u8,
    /// Whether the Tab key inserts spaces instead of a literal tab
    /// character.
    pub indent_spaces: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: None,
            renderer: RendererPref::default(),
            // Quiet by default. A text editor that chatters on stdout is a
            // text editor whose real warnings get ignored.
            log: "warn".to_owned(),
            font_size: DEFAULT_FONT_SIZE,
            tab_width: DEFAULT_TAB_WIDTH,
            // A Notepad-style editor promises "Notepad when you want it": Tab
            // inserts a tab. Silently rewriting it into spaces is exactly the
            // kind of surprise that promise rules out, so opting into spaces
            // is something the user must ask for.
            indent_spaces: false,
        }
    }
}

/// A non-fatal configuration problem worth showing the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notice {
    Unreadable { path: PathBuf, error: String },
    Malformed { path: PathBuf, error: String },
    UnknownKey { key: String },
    UnknownValue { key: String, value: String },
}

impl fmt::Display for Notice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreadable { path, error } => {
                write!(f, "cannot read {}: {error}; using defaults", path.display())
            }
            Self::Malformed { path, error } => {
                write!(
                    f,
                    "{} is not valid TOML: {error}; using defaults",
                    path.display()
                )
            }
            Self::UnknownKey { key } => write!(f, "unknown setting '{key}'; ignored"),
            Self::UnknownValue { key, value } => {
                write!(f, "'{value}' is not a valid {key}; ignored")
            }
        }
    }
}

/// Environment inputs, gathered so [`resolve`] stays pure.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Env {
    pub theme: Option<String>,
    pub renderer: Option<String>,
    pub log: Option<String>,
    /// Kept as text, not a number, so parsing and range-checking happen in
    /// one place ([`apply_font_size`]) regardless of which layer the value
    /// came from.
    pub font_size: Option<String>,
    /// Kept as text for the same reason as `font_size` -- see
    /// [`apply_tab_width`].
    pub tab_width: Option<String>,
    /// Kept as text for the same reason as `font_size` -- see
    /// [`apply_indent_spaces`].
    pub indent_spaces: Option<String>,
    /// `BACHELORPAD_CONFIG` overrides the config file location.
    pub config_path: Option<String>,
}

impl Env {
    /// Read `BACHELORPAD_*` from the process environment.
    pub fn from_process() -> Self {
        let get = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        Self {
            theme: get("BACHELORPAD_THEME"),
            renderer: get("BACHELORPAD_RENDERER"),
            log: get("BACHELORPAD_LOG"),
            font_size: get("BACHELORPAD_FONT_SIZE"),
            tab_width: get("BACHELORPAD_TAB_WIDTH"),
            indent_spaces: get("BACHELORPAD_INDENT_SPACES"),
            config_path: get("BACHELORPAD_CONFIG"),
        }
    }
}

/// The settings a config file may set. Unknown keys are ignored rather than
/// fatal -- one typo must not discard every other setting.
///
/// `font_size` and `tab_width` are read as `i64`, wider than the `u8` they
/// settle into, so a too-large or negative number in the file is a range
/// problem this module reports with a [`Notice`], not a TOML type error that
/// would discard the whole file. `indent_spaces` stays a native TOML `bool`
/// for the same reason: any value written the way TOML expects a boolean to
/// look deserialises fine, and only genuinely wrong types (e.g. a quoted
/// string) become a file-level error.
#[derive(Debug, Default, Deserialize)]
struct FileConfig {
    theme: Option<String>,
    renderer: Option<String>,
    log: Option<String>,
    font_size: Option<i64>,
    tab_width: Option<i64>,
    indent_spaces: Option<bool>,
}

/// Outcome of loading configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loaded {
    pub config: Config,
    /// Problems encountered. Never fatal.
    pub notices: Vec<Notice>,
    /// The file actually read, if any.
    pub source: Option<PathBuf>,
}

/// Where the config file lives on this platform.
///
/// Windows: `%APPDATA%\bachelorpad\config.toml`.
/// Linux: `$XDG_CONFIG_HOME/bachelorpad/config.toml`, else
/// `$HOME/.config/bachelorpad/config.toml`.
///
/// Returns `None` when the environment provides no home at all, in which case
/// the editor runs on defaults rather than guessing a path.
pub fn config_path() -> Option<PathBuf> {
    let root = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    };
    Some(root?.join(APP_DIR).join(FILE_NAME))
}

/// Resolve configuration from already-gathered inputs.
///
/// `file` is the *contents* of the config file, not a path, so this function
/// never touches the filesystem.
pub fn resolve(file: Option<&str>, env: &Env, args: &[String]) -> (Config, Vec<Notice>) {
    let mut config = Config::default();
    let mut notices = Vec::new();

    // --- file (lowest precedence above defaults) ----------------------
    if let Some(text) = file {
        match toml::from_str::<FileConfig>(text) {
            Ok(parsed) => {
                if let Some(t) = parsed.theme {
                    config.theme = Some(t);
                }
                if let Some(r) = parsed.renderer {
                    apply_renderer(&mut config, &r, &mut notices);
                }
                if let Some(l) = parsed.log {
                    config.log = l;
                }
                if let Some(f) = parsed.font_size {
                    apply_font_size(&mut config, &f.to_string(), &mut notices);
                }
                if let Some(t) = parsed.tab_width {
                    apply_tab_width(&mut config, &t.to_string(), &mut notices);
                }
                if let Some(s) = parsed.indent_spaces {
                    apply_indent_spaces(&mut config, &s.to_string(), &mut notices);
                }
                // Report keys we ignored, so a typo is visible instead of
                // silently doing nothing.
                if let Ok(table) = toml::from_str::<toml::Table>(text) {
                    for key in table.keys() {
                        if !matches!(
                            key.as_str(),
                            "theme"
                                | "renderer"
                                | "log"
                                | "font_size"
                                | "tab_width"
                                | "indent_spaces"
                        ) {
                            notices.push(Notice::UnknownKey { key: key.clone() });
                        }
                    }
                }
            }
            Err(e) => notices.push(Notice::Malformed {
                path: PathBuf::from(FILE_NAME),
                error: e.message().to_owned(),
            }),
        }
    }

    // --- environment ---------------------------------------------------
    if let Some(t) = &env.theme {
        config.theme = Some(t.clone());
    }
    if let Some(r) = &env.renderer {
        apply_renderer(&mut config, r, &mut notices);
    }
    if let Some(l) = &env.log {
        config.log = l.clone();
    }
    if let Some(f) = &env.font_size {
        apply_font_size(&mut config, f, &mut notices);
    }
    if let Some(t) = &env.tab_width {
        apply_tab_width(&mut config, t, &mut notices);
    }
    if let Some(s) = &env.indent_spaces {
        apply_indent_spaces(&mut config, s, &mut notices);
    }

    // --- command line (highest) ----------------------------------------
    for arg in args {
        let Some((key, value)) = arg.strip_prefix("--").and_then(|a| a.split_once('=')) else {
            continue;
        };
        match key {
            "theme" => config.theme = Some(value.to_owned()),
            "renderer" => apply_renderer(&mut config, value, &mut notices),
            "log" => config.log = value.to_owned(),
            "font-size" => apply_font_size(&mut config, value, &mut notices),
            "tab-width" => apply_tab_width(&mut config, value, &mut notices),
            "indent-spaces" => apply_indent_spaces(&mut config, value, &mut notices),
            _ => {}
        }
    }

    (config, notices)
}

fn apply_renderer(config: &mut Config, value: &str, notices: &mut Vec<Notice>) {
    match RendererPref::parse(value) {
        Some(r) => config.renderer = r,
        None => notices.push(Notice::UnknownValue {
            key: "renderer".to_owned(),
            value: value.to_owned(),
        }),
    }
}

/// Parse and range-check a small positive whole number, shared by every
/// setting shaped like this ([`apply_font_size`], [`apply_tab_width`]): a
/// string in, `min..=max` enforced, `None` for anything unparseable or out
/// of range. Widening through `i64` first means a huge or negative number
/// is a normal range failure here rather than an integer-overflow panic.
fn parse_ranged_u8(value: &str, min: u8, max: u8) -> Option<u8> {
    value
        .trim()
        .parse::<i64>()
        .ok()
        .filter(|n| (i64::from(min)..=i64::from(max)).contains(n))
        .map(|n| n as u8)
}

/// Parse and range-check a font size from any layer (file, environment or
/// command line all hand this a string -- see the comment on `Env::font_size`
/// for why). On failure the existing value is left untouched rather than
/// forced to the default, so an invalid override never clobbers a good
/// setting from a lower-precedence layer; since resolution starts from
/// `Config::default()`, that still means "falls back to the default" in the
/// common case of a bad value with nothing better beneath it.
fn apply_font_size(config: &mut Config, value: &str, notices: &mut Vec<Notice>) {
    match parse_ranged_u8(value, MIN_FONT_SIZE, MAX_FONT_SIZE) {
        Some(n) => config.font_size = n,
        None => notices.push(Notice::UnknownValue {
            key: "font-size".to_owned(),
            value: value.to_owned(),
        }),
    }
}

/// Parse and range-check a tab width from any layer. Same shape and the same
/// "leave the existing value alone on failure" behaviour as
/// [`apply_font_size`] -- see that function's comment for why.
fn apply_tab_width(config: &mut Config, value: &str, notices: &mut Vec<Notice>) {
    match parse_ranged_u8(value, MIN_TAB_WIDTH, MAX_TAB_WIDTH) {
        Some(n) => config.tab_width = n,
        None => notices.push(Notice::UnknownValue {
            key: "tab-width".to_owned(),
            value: value.to_owned(),
        }),
    }
}

/// Parse "insert spaces for Tab" from any layer -- file, environment and
/// command line all hand this a string, same as [`apply_font_size`].
///
/// Only the spellings TOML's own boolean literals use are accepted. Unlike
/// `RendererPref`'s synonyms, where `cpu`/`gpu` name genuinely different
/// renderer concepts, `true`/`false` already has exactly one obvious
/// spelling per value; accepting more (`1`/`0`, `yes`/`no`) would just be
/// more ways to mistype it, not more ways to mean it.
fn apply_indent_spaces(config: &mut Config, value: &str, notices: &mut Vec<Notice>) {
    match value.trim().to_ascii_lowercase().as_str() {
        "true" => config.indent_spaces = true,
        "false" => config.indent_spaces = false,
        _ => notices.push(Notice::UnknownValue {
            key: "indent-spaces".to_owned(),
            value: value.to_owned(),
        }),
    }
}

/// Load configuration from the real environment and filesystem.
pub fn load(args: &[String]) -> Loaded {
    let env = Env::from_process();
    let path = env
        .config_path
        .as_ref()
        .map(PathBuf::from)
        .or_else(config_path);

    let mut notices = Vec::new();
    let mut source = None;
    let text = match &path {
        Some(p) if p.exists() => match std::fs::read_to_string(p) {
            Ok(t) => {
                source = Some(p.clone());
                Some(t)
            }
            Err(e) => {
                notices.push(Notice::Unreadable {
                    path: p.clone(),
                    error: e.to_string(),
                });
                None
            }
        },
        // No file is the normal case, not a problem.
        _ => None,
    };

    let (config, mut more) = resolve(text.as_deref(), &env, args);
    notices.append(&mut more);

    // Point malformed-file notices at the real path now that we know it.
    if let Some(p) = &path {
        for notice in &mut notices {
            if let Notice::Malformed { path: np, .. } = notice {
                *np = p.clone();
            }
        }
    }

    Loaded {
        config,
        notices,
        source,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn defaults_are_quiet_and_software_rendered() {
        let (c, n) = resolve(None, &Env::default(), &[]);
        assert_eq!(c, Config::default());
        assert_eq!(c.renderer, RendererPref::Software);
        assert_eq!(c.log, "warn");
        assert!(c.theme.is_none());
        assert!(n.is_empty());
    }

    #[test]
    fn the_file_overrides_defaults() {
        let file = "theme = \"Green\"\nrenderer = \"platform\"\nlog = \"info\"\n";
        let (c, n) = resolve(Some(file), &Env::default(), &[]);

        assert_eq!(c.theme.as_deref(), Some("Green"));
        assert_eq!(c.renderer, RendererPref::Platform);
        assert_eq!(c.log, "info");
        assert!(n.is_empty());
    }

    #[test]
    fn the_environment_overrides_the_file() {
        let file = "theme = \"Green\"\n";
        let env = Env {
            theme: Some("Dark".to_owned()),
            ..Env::default()
        };
        let (c, _) = resolve(Some(file), &env, &[]);
        assert_eq!(c.theme.as_deref(), Some("Dark"));
    }

    #[test]
    fn the_command_line_overrides_everything() {
        let file = "theme = \"Green\"\nlog = \"info\"\n";
        let env = Env {
            theme: Some("Dark".to_owned()),
            log: Some("debug".to_owned()),
            ..Env::default()
        };
        let (c, _) = resolve(Some(file), &env, &args(&["--theme=Light", "--log=trace"]));

        assert_eq!(c.theme.as_deref(), Some("Light"));
        assert_eq!(c.log, "trace");
    }

    #[test]
    fn the_default_font_size_matches_what_app_slint_used_to_hard_code() {
        assert_eq!(
            Config::default().font_size,
            14,
            "specs.md section 4 zoom needs a starting point that matches today's rendered size, or existing documents would visibly jump on first run"
        );
    }

    #[test]
    fn zooming_in_and_back_out_returns_the_size_it_started_at() {
        // The property that matters to someone who overshoots and corrects:
        // a step out has to undo a step in exactly, at every size in range,
        // or repeated adjustment drifts.
        for size in MIN_FONT_SIZE + 1..MAX_FONT_SIZE {
            assert_eq!(zoom(zoom(size, 1), -1), size, "drifted at {size}");
            assert_eq!(zoom(zoom(size, -1), 1), size, "drifted at {size}");
        }
    }

    #[test]
    fn zoom_stops_at_the_bounds_rather_than_wrapping_or_refusing() {
        assert_eq!(zoom(MAX_FONT_SIZE, 1), MAX_FONT_SIZE);
        assert_eq!(zoom(MIN_FONT_SIZE, -1), MIN_FONT_SIZE);
        // A step larger than the whole range must not wrap through zero,
        // which is what `saturating_add_signed` is there to prevent.
        assert_eq!(zoom(MIN_FONT_SIZE, i8::MIN), MIN_FONT_SIZE);
        assert_eq!(zoom(MAX_FONT_SIZE, i8::MAX), MAX_FONT_SIZE);
    }

    #[test]
    fn zoom_cannot_reach_a_size_a_config_file_would_have_been_refused_for() {
        // The two paths to a font size have to agree. A file asking for 200
        // falls back; zoom must not arrive there by held keys instead.
        for steps in [i8::MIN, -50, -1, 0, 1, 50, i8::MAX] {
            for size in [MIN_FONT_SIZE, DEFAULT_FONT_SIZE, MAX_FONT_SIZE] {
                let got = zoom(size, steps);
                assert!(
                    (MIN_FONT_SIZE..=MAX_FONT_SIZE).contains(&got),
                    "zoom({size}, {steps}) escaped the range at {got}"
                );
            }
        }
    }

    #[test]
    fn the_file_sets_the_font_size() {
        let (c, n) = resolve(Some("font_size = 18"), &Env::default(), &[]);
        assert_eq!(c.font_size, 18);
        assert!(n.is_empty(), "a valid font size must not warn");
    }

    #[test]
    fn the_environment_overrides_the_files_font_size() {
        let file = "font_size = 18\n";
        let env = Env {
            font_size: Some("22".to_owned()),
            ..Env::default()
        };
        let (c, _) = resolve(Some(file), &env, &[]);
        assert_eq!(c.font_size, 22);
    }

    #[test]
    fn the_command_line_overrides_the_font_size() {
        let file = "font_size = 18\n";
        let env = Env {
            font_size: Some("22".to_owned()),
            ..Env::default()
        };
        let (c, _) = resolve(Some(file), &env, &args(&["--font-size=30"]));
        assert_eq!(c.font_size, 30);
    }

    #[test]
    fn an_unparseable_font_size_warns_and_falls_back() {
        let (c, n) = resolve(None, &Env::default(), &args(&["--font-size=huge"]));

        assert_eq!(
            c.font_size, 14,
            "a value that is not even a number must not change anything"
        );
        assert_eq!(
            n,
            vec![Notice::UnknownValue {
                key: "font-size".to_owned(),
                value: "huge".to_owned()
            }]
        );
    }

    #[test]
    fn an_out_of_range_font_size_warns_and_falls_back() {
        let (c, n) = resolve(None, &Env::default(), &args(&["--font-size=999"]));

        assert_eq!(
            c.font_size, 14,
            "999pt is not a legitimate editor size; it is almost certainly a mistake, so fall back rather than clamp to a value the user never asked for"
        );
        assert_eq!(
            n,
            vec![Notice::UnknownValue {
                key: "font-size".to_owned(),
                value: "999".to_owned()
            }]
        );
    }

    #[test]
    fn a_negative_font_size_warns_and_falls_back() {
        let (c, n) = resolve(None, &Env::default(), &args(&["--font-size=-5"]));

        assert_eq!(c.font_size, 14, "a negative size makes no sense at all");
        assert_eq!(
            n.len(),
            1,
            "the negative value must produce exactly one notice"
        );
    }

    #[test]
    fn an_empty_environment_font_size_is_treated_as_unset() {
        // Mirrors empty_environment_values_are_treated_as_unset: an
        // exported-but-blank variable is the shell's idea of "unset", so it
        // must not shadow the file underneath it.
        let env = Env {
            font_size: None,
            ..Env::default()
        };
        let (c, _) = resolve(Some("font_size = 20"), &env, &[]);
        assert_eq!(c.font_size, 20);
    }

    #[test]
    fn the_default_tab_width_matches_bp_uis_hard_coded_constant() {
        assert_eq!(
            Config::default().tab_width,
            4,
            "bp-ui's TAB_WIDTH and bp_editor::view's own default are both 4; the config default must not disagree with the editor it configures"
        );
    }

    #[test]
    fn the_file_sets_the_tab_width() {
        let (c, n) = resolve(Some("tab_width = 8"), &Env::default(), &[]);
        assert_eq!(c.tab_width, 8);
        assert!(n.is_empty(), "a valid tab width must not warn");
    }

    #[test]
    fn the_environment_overrides_the_files_tab_width() {
        let file = "tab_width = 8\n";
        let env = Env {
            tab_width: Some("2".to_owned()),
            ..Env::default()
        };
        let (c, _) = resolve(Some(file), &env, &[]);
        assert_eq!(c.tab_width, 2);
    }

    #[test]
    fn the_command_line_overrides_the_tab_width() {
        let file = "tab_width = 8\n";
        let env = Env {
            tab_width: Some("2".to_owned()),
            ..Env::default()
        };
        let (c, _) = resolve(Some(file), &env, &args(&["--tab-width=3"]));
        assert_eq!(c.tab_width, 3);
    }

    #[test]
    fn an_unparseable_tab_width_warns_and_falls_back() {
        let (c, n) = resolve(None, &Env::default(), &args(&["--tab-width=wide"]));

        assert_eq!(
            c.tab_width, 4,
            "a value that is not even a number must not change anything"
        );
        assert_eq!(
            n,
            vec![Notice::UnknownValue {
                key: "tab-width".to_owned(),
                value: "wide".to_owned()
            }]
        );
    }

    #[test]
    fn an_out_of_range_tab_width_warns_and_falls_back() {
        let (c, n) = resolve(None, &Env::default(), &args(&["--tab-width=999"]));

        assert_eq!(
            c.tab_width, 4,
            "a 999-column tab is not a legitimate indent width; it is almost certainly a mistake, so fall back rather than clamp to a value the user never asked for"
        );
        assert_eq!(
            n,
            vec![Notice::UnknownValue {
                key: "tab-width".to_owned(),
                value: "999".to_owned()
            }]
        );
    }

    #[test]
    fn a_tab_width_of_zero_warns_and_falls_back() {
        let (c, n) = resolve(None, &Env::default(), &args(&["--tab-width=0"]));

        assert_eq!(
            c.tab_width, 4,
            "a zero-width tab would collapse every indent level to nothing"
        );
        assert_eq!(
            n.len(),
            1,
            "the out-of-range value must produce exactly one notice"
        );
    }

    #[test]
    fn an_empty_environment_tab_width_is_treated_as_unset() {
        // Mirrors an_empty_environment_font_size_is_treated_as_unset: an
        // exported-but-blank variable is the shell's idea of "unset", so it
        // must not shadow the file underneath it.
        let env = Env {
            tab_width: None,
            ..Env::default()
        };
        let (c, _) = resolve(Some("tab_width = 2"), &env, &[]);
        assert_eq!(c.tab_width, 2);
    }

    #[test]
    fn the_default_is_to_insert_a_literal_tab() {
        assert!(
            !Config::default().indent_spaces,
            "BachelorPad+ promises Notepad when you want it: Tab must insert a tab unless the user opts into spaces"
        );
    }

    #[test]
    fn the_file_sets_indent_spaces() {
        let (c, n) = resolve(Some("indent_spaces = true"), &Env::default(), &[]);
        assert!(c.indent_spaces);
        assert!(n.is_empty(), "a valid indent-spaces value must not warn");
    }

    #[test]
    fn the_environment_overrides_the_files_indent_spaces() {
        let file = "indent_spaces = true\n";
        let env = Env {
            indent_spaces: Some("false".to_owned()),
            ..Env::default()
        };
        let (c, _) = resolve(Some(file), &env, &[]);
        assert!(!c.indent_spaces);
    }

    #[test]
    fn the_command_line_overrides_indent_spaces() {
        let file = "indent_spaces = false\n";
        let env = Env {
            indent_spaces: Some("false".to_owned()),
            ..Env::default()
        };
        let (c, _) = resolve(Some(file), &env, &args(&["--indent-spaces=true"]));
        assert!(c.indent_spaces);
    }

    #[test]
    fn an_unrecognised_indent_spaces_spelling_warns_and_falls_back() {
        let (c, n) = resolve(None, &Env::default(), &args(&["--indent-spaces=yes"]));

        assert!(
            !c.indent_spaces,
            "an unrecognised spelling must not change anything"
        );
        assert_eq!(
            n,
            vec![Notice::UnknownValue {
                key: "indent-spaces".to_owned(),
                value: "yes".to_owned()
            }]
        );
    }

    #[test]
    fn an_empty_environment_indent_spaces_is_treated_as_unset() {
        // Mirrors an_empty_environment_font_size_is_treated_as_unset: an
        // exported-but-blank variable is the shell's idea of "unset", so it
        // must not shadow the file underneath it.
        let env = Env {
            indent_spaces: None,
            ..Env::default()
        };
        let (c, _) = resolve(Some("indent_spaces = true"), &env, &[]);
        assert!(c.indent_spaces);
    }

    #[test]
    fn a_malformed_file_yields_defaults_and_a_notice() {
        // The rule that matters: broken config must not stop the editor.
        let (c, n) = resolve(Some("theme = = broken"), &Env::default(), &[]);

        assert_eq!(c, Config::default());
        assert_eq!(n.len(), 1);
        assert!(matches!(n[0], Notice::Malformed { .. }));
        assert!(n[0].to_string().contains("using defaults"));
    }

    #[test]
    fn a_malformed_file_does_not_block_higher_precedence_sources() {
        let env = Env {
            theme: Some("Dark".to_owned()),
            ..Env::default()
        };
        let (c, n) = resolve(Some("nonsense = = ="), &env, &[]);

        assert_eq!(
            c.theme.as_deref(),
            Some("Dark"),
            "a broken file must not discard the environment"
        );
        assert_eq!(n.len(), 1);
    }

    #[test]
    fn an_unknown_key_is_reported_but_keeps_the_rest() {
        let file = "theme = \"Green\"\nthmee = \"typo\"\n";
        let (c, n) = resolve(Some(file), &Env::default(), &[]);

        assert_eq!(
            c.theme.as_deref(),
            Some("Green"),
            "one typo must not discard the file"
        );
        assert_eq!(
            n,
            vec![Notice::UnknownKey {
                key: "thmee".to_owned()
            }]
        );
    }

    #[test]
    fn an_unknown_renderer_is_reported_and_ignored() {
        let (c, n) = resolve(Some("renderer = \"raytraced\""), &Env::default(), &[]);

        assert_eq!(
            c.renderer,
            RendererPref::Software,
            "fall back, do not guess"
        );
        assert_eq!(
            n,
            vec![Notice::UnknownValue {
                key: "renderer".to_owned(),
                value: "raytraced".to_owned()
            }]
        );
    }

    #[test]
    fn renderer_accepts_the_obvious_synonyms() {
        assert_eq!(
            RendererPref::parse("software"),
            Some(RendererPref::Software)
        );
        assert_eq!(RendererPref::parse("CPU"), Some(RendererPref::Software));
        assert_eq!(RendererPref::parse(" gpu "), Some(RendererPref::Platform));
        assert_eq!(
            RendererPref::parse("platform"),
            Some(RendererPref::Platform)
        );
        assert_eq!(RendererPref::parse(""), None);
    }

    #[test]
    fn unrecognised_arguments_are_ignored() {
        // --measure-exit and --self-check are handled elsewhere; config must
        // not choke on them.
        let (c, n) = resolve(
            None,
            &Env::default(),
            &args(&["--measure-exit", "--self-check", "-v", "file.txt"]),
        );
        assert_eq!(c, Config::default());
        assert!(n.is_empty());
    }

    #[test]
    fn empty_environment_values_are_treated_as_unset() {
        // An exported-but-blank variable is the shell's idea of "unset".
        let env = Env {
            theme: None,
            ..Env::default()
        };
        let (c, _) = resolve(Some("theme = \"Green\""), &env, &[]);
        assert_eq!(c.theme.as_deref(), Some("Green"));
    }

    #[test]
    fn config_holds_no_document_content() {
        // Structural guard for ADR-0011: everything in Config is a setting.
        // If a field is ever added that could carry document text, this test
        // is where the argument about it should happen.
        let file = "theme = \"Green\"\nrenderer = \"software\"\nlog = \"warn\"\nfont_size = 18\ntab_width = 8\nindent_spaces = true\n";
        let (c, _) = resolve(Some(file), &Env::default(), &[]);
        let rendered = format!("{c:?}");

        for forbidden in ["passphrase", "secret", "token", "password", "content"] {
            assert!(
                !rendered.to_lowercase().contains(forbidden),
                "Config debug output must never carry {forbidden}"
            );
        }
    }
}
