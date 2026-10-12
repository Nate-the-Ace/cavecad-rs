// Keeps a console window from opening behind the app on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    cavecad_desktop_lib::run();
}
