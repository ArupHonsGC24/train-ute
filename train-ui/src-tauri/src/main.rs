// Prevents additional console window on Windows in release, DO NOT REMOVE!!
// I have removed this because I want to see the console output even in release mode (for now).
//#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    app_lib::run();
}
