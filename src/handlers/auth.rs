use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};
use cargo_smith::auth::{AuthService};
use cargo_smith::Db;
use mongodb::bson::doc;

use crate::models::users::{Tokens, Users};

#[derive(Deserialize)] pub struct RegisterBody { pub username: String, pub email: String, pub password: String }
#[derive(Deserialize)] pub struct LoginBody    { pub username: String, pub password: String }
#[derive(Deserialize)] pub struct RefreshBody  { pub refresh_token: String }
#[derive(Serialize)] pub struct AuthResponse {
    pub tokens:   Tokens,
    pub username: String,
}

pub async fn register(
    body:   web::Json<RegisterBody>,
    db:     web::Data<Db>,
    secret: web::Data<String>,
) -> HttpResponse {
    let hash = AuthService::hash_password(&body.password).unwrap();
    let mut user = Users::new(body.username.clone(), body.email.clone(), hash);
    let col  = db.collection::<Users>("users");

    if col.find_one(doc! { "username": &user.username }).await.is_some() {
        return HttpResponse::Conflict().body("username taken");
    }

    let auth = AuthService::new(secret.get_ref().clone(), secret.get_ref().clone());
    let (access, refresh) = auth.token_pair(&user.id, &user.username);
    user.refresh_token = Some(refresh.clone());

    if col.insert(&user).await.is_err() {
        return HttpResponse::InternalServerError().body("failed to create user");
    }

    HttpResponse::Ok().json(AuthResponse {
        tokens:   Tokens { access_token: access, refresh_token: refresh },
        username: user.username,
    })
}

pub async fn login(
    body:   web::Json<LoginBody>,
    db:     web::Data<Db>,
    secret: web::Data<String>,
) -> HttpResponse {
    let col = db.collection::<Users>("users");

    let Some(mut user) = col.find_one(doc! { "username": &body.username }).await else {
        return HttpResponse::Unauthorized().body("invalid credentials");
    };

    if !AuthService::verify_password(&body.password, &user.password_hash).unwrap_or(false) {
        return HttpResponse::Unauthorized().body("invalid credentials");
    }

    let auth = AuthService::new(secret.get_ref().clone(), secret.get_ref().clone());
    let (access, refresh) = auth.token_pair(&user.id, &user.username);

    // Rotate refresh token in DB
    let _ = col.update_one(
        doc! { "id": &user.id },
        doc! { "$set": { "refresh_token": &refresh } },
    ).await;

    HttpResponse::Ok().json(AuthResponse {
        tokens:   Tokens { access_token: access, refresh_token: refresh },
        username: user.username,
    })
}

pub async fn refresh(
    body:   web::Json<RefreshBody>,
    db:     web::Data<Db>,
    secret: web::Data<String>,
) -> HttpResponse {
    let auth = AuthService::new(secret.get_ref().clone(), secret.get_ref().clone());

    // Validate the refresh token
    if !auth.verify_token::<serde_json::Value>(&body.refresh_token) {
        return HttpResponse::Unauthorized().body("invalid refresh token");
    }

    let col = db.collection::<Users>("users");

    // Find user by stored refresh token
    let Some(user) = col.find_one(doc! { "refresh_token": &body.refresh_token }).await else {
        return HttpResponse::Unauthorized().body("refresh token not recognised");
    };

    let (access, new_refresh) = auth.token_pair(&user.id, &user.username);

    // Rotate — old refresh token is now invalid
    let _ = col.update_one(
        doc! { "id": &user.id },
        doc! { "$set": { "refresh_token": &new_refresh } },
    ).await;

    HttpResponse::Ok().json(AuthResponse {
        tokens:   Tokens { access_token: access, refresh_token: new_refresh },
        username: user.username,
    })
}

pub async fn guest(secret: web::Data<String>) -> HttpResponse {
    let user = Users::new_guest();
    let auth = AuthService::new(secret.get_ref().clone(), secret.get_ref().clone());
    let (access, refresh) = auth.guest_token_pair(&user.id, &user.username);

    // Guests aren't saved to DB — stateless
    HttpResponse::Ok().json(AuthResponse {
        tokens:   Tokens { access_token: access, refresh_token: refresh },
        username: user.username,
    })
}