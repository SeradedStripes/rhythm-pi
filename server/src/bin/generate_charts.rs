use anyhow::Result;
use std::env;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: generate_charts <song_id>");
        std::process::exit(2);
    }
    let song_id = &args[1];
    let songs_dir = env::var("SONGS_DIR").unwrap_or_else(|_| "server/assets/songs".to_string());
    let charts_dir = env::var("CHARTS_DIR").unwrap_or_else(|_| "server/assets/charts".to_string());

    let written = rhythm_pi_server::chart_gen::generate_charts_for_song(song_id, &std::path::Path::new(&songs_dir), &std::path::Path::new(&charts_dir))?;
    println!("generated {} charts for {}", written.len(), song_id);
    Ok(())
}
