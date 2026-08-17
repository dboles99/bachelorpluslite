//! BachelorPad+ — Notepad when you want it. More when you need it.

// No console window on Windows for a release build. Kept for debug builds so
// `tracing` output stays visible while developing.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("BACHELORPAD_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    tracing::info!("starting BachelorPad+");
    let options = bp_ui::RunOptions {
        measure_exit: std::env::args().any(|a| a == "--measure-exit"),
        ..Default::default()
    };
    bp_ui::run_with(options)?;
    Ok(())
}
