fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    println!("BachelorPad+ scaffold");
    println!("Core crate: {}", bp_core::CRATE_NAME);
    Ok(())
}
