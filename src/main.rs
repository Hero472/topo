// Main entry point for the Actix Web application
use topo::server::server;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    server::run().await
}