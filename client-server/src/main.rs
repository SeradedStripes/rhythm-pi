use anyhow::Result;

fn main() -> Result<()> {
    // initialize logging once for the process (idempotent)
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).try_init();

    // Spawn server on a background thread with its own tokio runtime
    let server_thread = std::thread::spawn(|| {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("failed to create tokio runtime");

        rt.block_on(async {
            if let Err(e) = client_server::server::run_server().await {
                eprintln!("Server error: {}", e);
            }
        });
    });

    // Run the client UI on the main thread
    if let Err(e) = client_server::client::run_client() {
        eprintln!("Client error: {}", e);
    }

    // Wait for server thread to exit (this will usually only happen if the server fails)
    let _ = server_thread.join();

    Ok(())
}
