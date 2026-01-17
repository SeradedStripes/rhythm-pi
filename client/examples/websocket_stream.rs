// Example: Using WebSocket Audio Streaming in Client
//
// This example demonstrates how to connect to the server's WebSocket
// audio streaming endpoint and receive audio data.

use rhythm_pi_client::websocket::AudioStreamClient;
use log::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    env_logger::init();
    
    // Configuration
    let server_url = "ws://localhost:8080";
    let song_id = "Paradisus-Paradoxum";
    
    // Connect to audio stream
    let mut stream_client = AudioStreamClient::connect_audio_stream(server_url, song_id).await?;
    info!("Connected to audio stream");
    
    // Start the stream
    stream_client.start_stream()?;
    info!("Stream started, receiving audio chunks...");
    
    // Collect all audio chunks
    let mut all_audio_data = Vec::new();
    let mut chunk_count = 0;
    
    while let Some(chunk) = stream_client.recv_audio_chunk_async().await {
        chunk_count += 1;
        all_audio_data.extend_from_slice(&chunk.data);
        info!("Received chunk #{}: {} bytes (total: {} bytes)", 
              chunk_count, chunk.data.len(), all_audio_data.len());
    }
    
    info!("Stream complete! Received {} chunks, {} total bytes", 
          chunk_count, all_audio_data.len());
    
    // At this point, all_audio_data contains the complete audio file
    // You can now:
    // 1. Write it to a file
    // 2. Pass it to the audio player (cpal/rodio)
    // 3. Process it for visualization
    
    // Example: Save to file
    std::fs::write(format!("{}.mp3", song_id), &all_audio_data)?;
    info!("Audio saved to {}.mp3", song_id);
    
    Ok(())
}
