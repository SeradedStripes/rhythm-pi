#[path = "../src-client/lib.rs"]
pub mod client;

#[path = "../src-server/lib.rs"]
pub mod server;

// Re-export server modules at crate root so existing `crate::db` and similar paths still work
pub use server::db;
pub use server::handlers;
pub use server::auth;
pub use server::chart_gen;
pub use server::song_watcher;
pub use server::hq;
pub use server::hq_rust;
pub use server::websocket;
