//! Prints live snapshots as JSON lines, for comparison with system tools.
//! Usage: cargo run --example sample -- [count] [interval_ms]
fn main() {
    let mut args = std::env::args().skip(1);
    let count = args.next().and_then(|a| a.parse().ok()).unwrap_or(3);
    let interval = args.next().and_then(|a| a.parse().ok()).unwrap_or(1000);
    for snap in resourcewatch_lib::sample_live(count, std::time::Duration::from_millis(interval)) {
        println!("{snap}");
    }
}
