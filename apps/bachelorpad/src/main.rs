//! BachelorPad+ — Notepad when you want it. More when you need it.

// No console window on Windows for a release build. Kept for debug builds so
// `tracing` output stays visible while developing.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use bp_config::RendererPref;
use bp_theme::ThemeId;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
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

    tracing::info!("starting BachelorPad+");
    bp_ui::run_with(bp_ui::RunOptions {
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
        measure_exit: args.iter().any(|a| a == "--measure-exit"),
    })?;
    Ok(())
}
