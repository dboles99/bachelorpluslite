//! BachelorPad+ Phase 1 UI toolkit spike -- Slint shell.
//!
//! Measurement contract, identical in the egui spike:
//!
//! * `BPSPIKE_READY_MS=<f64>` is printed once, after the first frame has
//!   actually been rendered. It measures `main()` entry to first frame, so it
//!   excludes process creation -- the harness measures that as wall clock.
//! * `--measure-exit` quits as soon as that line is printed, which is what
//!   makes the wall-clock run terminate on its own.
//! * Without the flag the window stays up, so the harness can sample RSS.
//!
//! The hook is `RenderingState::AfterRendering` rather than a zero-duration
//! timer. A timer fires when the event loop starts turning, which is before
//! the graphics backend has drawn anything -- it reported ~13 ms against
//! egui's ~890 ms and the two were measuring entirely different events.

use std::time::Instant;

slint::include_modules!();

const SAMPLE_TEXT: &str = "BachelorPad+ toolkit spike.\n\n\
Notepad when you want it. More when you need it.\n\n\
This pane is a live editable buffer, not a static mock: the point of the\n\
spike is to compare real text input, real theming and real startup cost.\n";

fn main() -> Result<(), slint::PlatformError> {
    let start = Instant::now();
    let measure_exit = std::env::args().any(|a| a == "--measure-exit");

    let ui = AppWindow::new()?;
    ui.set_doc_text(SAMPLE_TEXT.into());

    let mut reported = false;
    ui.window()
        .set_rendering_notifier(move |state, _graphics_api| {
            if reported || !matches!(state, slint::RenderingState::AfterRendering) {
                return;
            }
            reported = true;
            println!(
                "BPSPIKE_READY_MS={:.3}",
                start.elapsed().as_secs_f64() * 1000.0
            );
            if measure_exit {
                // Quitting from inside a render callback would re-enter the
                // renderer, so defer it to the next event-loop turn.
                let _ = slint::invoke_from_event_loop(|| {
                    let _ = slint::quit_event_loop();
                });
            }
        })
        .expect("this renderer does not support a rendering notifier");

    ui.run()
}
