#![cfg_attr(
    not(debug_assertions),
    windows_subsystem = "windows"
)]

fn main() {
    orion_player_lib::run();
}