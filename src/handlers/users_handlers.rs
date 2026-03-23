use actix_web::{web, HttpResponse};
use cargo_smith::Db;
use mongodb::bson::doc;

use crate::models::users::Users;

pub async fn create_users(
    db:   web::Data<Db>,
    body: web::Json<Users>,
) -> HttpResponse {
    let col = db.collection::<Users>("users");
    match col.insert(&body.into_inner()).await {
        Ok(_)  => HttpResponse::Created().finish(),
        Err(e) => HttpResponse::InternalServerError().body(e.to_string()),
    }
}

pub async fn get_users(
    db:   web::Data<Db>,
    path: Option<web::Path<String>>,
) -> HttpResponse {
    let col = db.collection::<Users>("users");
    match path {
        Some(id) => match col.find_one(doc! { "id": id.into_inner() }).await {
            Some(user) => HttpResponse::Ok().json(user),
            None       => HttpResponse::NotFound().body("user not found"),
        },
        None => match col.find_all().await {
            Ok(users) => HttpResponse::Ok().json(users),
            Err(e)    => HttpResponse::InternalServerError().body(e.to_string()),
        },
    }
}

pub async fn update_users(
    db:   web::Data<Db>,
    path: web::Path<String>,
    body: web::Json<Users>,
) -> HttpResponse {
    let col = db.collection::<Users>("users");
    let update = mongodb::bson::doc! {
        "$set": {
            "username":      &body.username,
            "email":         &body.email,
            "password_hash": &body.password_hash,
        }
    };
    match col.update_one(doc! { "id": path.into_inner() }, update).await {
        Ok(true)  => HttpResponse::Ok().json(body.into_inner()),
        Ok(false) => HttpResponse::NotFound().body("user not found"),
        Err(e)    => HttpResponse::InternalServerError().body(e.to_string()),
    }
}

pub async fn delete_users(
    db:   web::Data<Db>,
    path: web::Path<String>,
) -> HttpResponse {
    let col = db.collection::<Users>("users");
    match col.delete_one(doc! { "id": path.into_inner() }).await {
        Ok(true)  => HttpResponse::NoContent().finish(),
        Ok(false) => HttpResponse::NotFound().body("user not found"),
        Err(e)    => HttpResponse::InternalServerError().body(e.to_string()),
    }
}