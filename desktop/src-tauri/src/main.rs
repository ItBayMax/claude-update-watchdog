// Prevent additional console window on Windows in release. The CLI modes
// (--check / --repair / --repair-from-event) re-attach to the parent console
// themselves, see cli.rs.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    claude_watchdog_lib::run();
}
