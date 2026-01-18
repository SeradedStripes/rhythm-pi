pub mod audio;
pub mod websocket;
pub mod game;
pub mod input;

pub use audio::AudioContext;
pub use websocket::WebSocketClient;
pub use game::{Song, Chart, Note, GameState, HitAccuracy, HitEvent, ChartNote};
pub use input::{InputHandler, KeyBindings, InputEvent};

#[path = "main.rs"]
mod client_main;

use anyhow::Result;

/// Run the client UI by delegating to the `src-client/main.rs` implementation.
pub fn run_client() -> Result<()> {
    client_main::run_client_main()
}
