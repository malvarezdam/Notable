// Sin ventana de consola en Windows al compilar en modo release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    notable_app_lib::run()
}
