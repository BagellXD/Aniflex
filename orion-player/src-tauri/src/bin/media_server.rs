fn main() {
    orion_player_lib::video_server::start();

    println!(
        "Orion media server process is running."
    );

    std::thread::park();
}