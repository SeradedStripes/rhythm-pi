#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Delegate to the library run function so the server logic can be reused.
    crate::server::run_server().await
}
