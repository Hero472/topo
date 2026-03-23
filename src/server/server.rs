use actix_web::{web, App, HttpServer, http};
use actix_cors::Cors;
use cargo_smith::{Db, JwtMiddleware};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::routes::routes::{public_routes, private_routes};
use crate::game::room::RoomRegistry;

pub async fn run() -> std::io::Result<()> {
    dotenvy::dotenv().ok();
    env_logger::init();

    let mongo_uri  = std::env::var("MONGODB_URI").expect("MONGODB_URI must be set");
    let mongo_db   = std::env::var("MONGODB_DB").unwrap_or("cardgame".to_string());
    let jwt_secret = std::env::var("JWT_SECRET").expect("JWT_SECRET must be set");
    let enc_key    = std::env::var("ENCRYPTION_KEY").expect("ENCRYPTION_KEY must be set");

    let db    = Db::connect(&mongo_uri, &mongo_db).await;
    let rooms: RoomRegistry = Arc::new(Mutex::new(HashMap::new()));

    let db_data     = web::Data::new(db);
    let rooms_data  = web::Data::new(rooms);
    let secret_data = web::Data::new(jwt_secret.clone());
    let enc_data    = web::Data::new(enc_key);

    log::info!("Starting server on http://127.0.0.1:8080");

    HttpServer::new(move || {
        let jwt_middleware = JwtMiddleware::new(jwt_secret.clone());

        let cors = Cors::default()
            .allowed_origin("http://localhost:5173")  // Vite dev server
            .allowed_origin("http://127.0.0.1:5173")
            .allowed_origin("http://localhost:5174")  // Vite dev server
            .allowed_origin("http://127.0.0.1:5174")
            .allowed_methods(vec!["GET", "POST", "PUT", "DELETE"])
            .allowed_headers(vec![
                http::header::AUTHORIZATION,
                http::header::CONTENT_TYPE,
            ])
            .max_age(3600);

        App::new()
            .wrap(cors)
            .app_data(db_data.clone())
            .app_data(rooms_data.clone())
            .app_data(secret_data.clone())
            .app_data(enc_data.clone())
            .configure(public_routes)
            .service(
                web::scope("/api/private")
                    .wrap(jwt_middleware)
                    .configure(private_routes)
            )
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await
}