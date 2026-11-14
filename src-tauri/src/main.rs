// Prevents an additional console window on Windows in release; harmless on Linux.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Before any thread is created (environment variables are process-wide).
    resourcewatch_lib::prepare_environment();
    resourcewatch_lib::run();
}
