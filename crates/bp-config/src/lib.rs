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
const MIN_FONT_SIZE: u8 = 6;

/// Largest accepted editor font size, in points.
///
/// A `u8` comfortably spans a usable typography range (this crate's whole
/// point is that broken input is clamped, not trusted), and staying inside
/// one byte keeps the type as small as the domain allows.
const MAX_FONT_SIZE: u8 = 96;

/// Editor font size used before any configuration is applied, in points.
/// Matches the size `app.slint` used to hard-code.
const DEFAULT_FONT_SIZE: u8 = 14;

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
            config_path: get("BACHELORPAD_CONFIG"),
        }
    }
}

/// The settings a config file may set. Unknown keys are ignored rather than
/// fatal -- one typo must not discard every other setting.
///
/// `font_size` is read as `i64`, wider than the `u8` it settles into, so a
/// too-large or negative number in the file is a range problem this module
/// reports with a [`Notice`], not a TOML type error that would discard the
/// whole file.
#[derive(Debug, Default, Deserialize)]
struct FileConfig {
    theme: Option<String>,
    renderer: Option<String>,
    log: Option<String>,
    font_size: Option<i64>,
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
                // Report keys we ignored, so a typo is visible instead of
                // silently doing nothing.
                if let Ok(table) = toml::from_str::<toml::Table>(text) {
                    for key in table.keys() {
                        if !matches!(key.as_str(), "theme" | "renderer" | "log" | "font_size") {
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

/// Parse and range-check a font size from any layer (file, environment or
/// command line all hand this a string -- see the comment on `Env::font_size`
/// for why). On failure the existing value is left untouched rather than
/// forced to the default, so an invalid override never clobbers a good
/// setting from a lower-precedence layer; since resolution starts from
/// `Config::default()`, that still means "falls back to the default" in the
/// common case of a bad value with nothing better beneath it.
fn apply_font_size(config: &mut Config, value: &str, notices: &mut Vec<Notice>) {
    let in_range = value
        .trim()
        .parse::<i64>()
        .ok()
        .filter(|n| (i64::from(MIN_FONT_SIZE)..=i64::from(MAX_FONT_SIZE)).contains(n));
    match in_range {
        Some(n) => config.font_size = n as u8,
        None => notices.push(Notice::UnknownValue {
            key: "font-size".to_owned(),
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
        let file = "theme = \"Green\"\nrenderer = \"software\"\nlog = \"warn\"\nfont_size = 18\n";
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
