//! BachelorPad+ Lite — Notepad when you want it. More when you need it.

// No console window on Windows for a release build. Kept for debug builds so
// `tracing` output stays visible while developing.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use bp_config::RendererPref;
use bp_theme::ThemeId;

/// What this executable is, for `--version`.
///
/// Read here rather than inside `bp-config` so the answer is the *binary's*
/// -- every crate inherits `[workspace.package]`, so a library reading its
/// own would give the same string today and quietly stop one day.
const PACKAGE: bp_config::cli::Package = bp_config::cli::Package {
    version: env!("CARGO_PKG_VERSION"),
    license: env!("CARGO_PKG_LICENSE"),
};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    // Before configuration, deliberately. `--help` is what somebody reaches
    // for when the product is not behaving, and a config file broken badly
    // enough to be the reason is not a reason to withhold it.
    //
    // Both print to stdout, as `--self-check` already does. On Windows a
    // release build is a GUI-subsystem binary, so this reaches a console
    // when one is attached -- a shell, a pipe, the gate -- and is discarded
    // when there is none. Launching a text editor from Explorer to read its
    // `--help` is not a thing anybody does, and the alternative is
    // `AttachConsole`, which means `unsafe` and a Windows dependency for it.
    if bp_config::cli::asked_for_help(&args) {
        print!("{}", bp_config::cli::help(PACKAGE));
        return Ok(());
    }
    if bp_config::cli::asked_for_version(&args) {
        print!("{}", bp_config::cli::version(PACKAGE));
        return Ok(());
    }

    let loaded = bp_config::load(&args);

    // Logging level comes from configuration, so BACHELORPAD_LOG and the
    // config file agree on one answer. An unparseable filter falls back
    // rather than refusing to start.
    let filter = tracing_subscriber::EnvFilter::try_new(&loaded.config.log)
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    // Report configuration problems before any early exit, so a broken config
    // is visible however the binary was invoked -- not only on the path that
    // opens a window.
    if let Some(source) = &loaded.source {
        tracing::info!("configuration loaded from {}", source.display());
    }
    let mut notices: Vec<String> = loaded.notices.iter().map(ToString::to_string).collect();

    // An unrecognised theme name is a recoverable mistake: warn and fall back
    // rather than refusing to start over a spelling. Resolved here, alongside
    // the other notices, so every invocation reports the same set.
    let theme = loaded.config.theme.as_ref().and_then(|name| {
        let id = ThemeId::from_name(name);
        if id.is_none() {
            notices.push(format!("'{name}' is not a known theme; using the default"));
        }
        id
    });

    // `--line` is not a setting -- nothing remembers it -- so it is read here
    // beside the other per-invocation switches rather than through the
    // precedence chain. What is *not* here is the parsing: `bp_config::cli`
    // owns that, so the rule about what counts as a line number is testable
    // without a process.
    let line = match bp_config::cli::line(&args) {
        Ok(line) => line,
        Err(typed) => {
            notices.push(format!("'{typed}' is not a line number; ignored"));
            None
        }
    };

    for notice in &notices {
        tracing::warn!("config: {notice}");
    }

    // Load-and-exit check for the local CI gate. Neither `cargo build` nor
    // `cargo test` ever loads the linked executable, so a bad link-time
    // feature or manifest passes both and still fails before `main` --
    // exactly what enabling rfd's `common-controls-v6` did. Reaching this
    // line at all is the assertion.
    if args.iter().any(|a| a == "--self-check") {
        println!("BACHELORPAD_SELF_CHECK_OK");
        return Ok(());
    }

    if args.iter().any(|a| a == "--latency-probe") {
        bp_ui::latency_probe();
        return Ok(());
    }

    // Anything that is not a flag is a file to open (specs.md §19).
    // `args[0]` is the executable.
    let files: Vec<std::path::PathBuf> = args
        .iter()
        .skip(1)
        .filter(|a| !a.starts_with('-'))
        .map(std::path::PathBuf::from)
        .collect();

    tracing::info!("starting {}", bp_ui::DISPLAY_NAME);
    bp_ui::run_with(bp_ui::RunOptions {
        files,
        line,
        renderer: match loaded.config.renderer {
            RendererPref::Software => bp_ui::Renderer::Software,
            RendererPref::Platform => bp_ui::Renderer::Platform,
        },
        theme,
        startup_notice: notices.first().map(|first| {
            if notices.len() > 1 {
                format!("{first} (+{} more; see the log)", notices.len() - 1)
            } else {
                first.clone()
            }
        }),
        font_size: Some(loaded.config.font_size),
        measure_exit: args.iter().any(|a| a == "--measure-exit"),
        // Opt-in while the custom editor surface reaches parity with the
        // widget it replaces (word wrap, input-method composition). Parsed
        // here beside the other switches rather than through the config
        // precedence, because it is a thing to try rather than a preference
        // to keep.
        editor_view: args.iter().any(|a| a == "--editor-view"),
    })?;
    Ok(())
}
