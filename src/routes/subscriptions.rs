use actix_web::{web, HttpResponse};
use chrono::Utc;
use serde::Deserialize;
use sqlx::PgPool;

#[allow(unused_imports)]
use tracing::{self, Instrument};

use uuid::Uuid;

#[derive(Deserialize)]
pub struct FormData {
    email: String,
    name: String,
}

// Old working code
// #[tracing::instrument(
//     name = "Adding a new subscriber",
//     skip(form, pool),
//     fields(
//         request_id = %Uuid::new_v4(),
//         subscriber_email = %form.email,
//         subscriber_name = %form.name
//     )
// )]
// pub async fn subscribe(
//     form: web::Form<FormData>,
//     // Retrieving a connection from the application state!
//     pool: web::Data<PgPool>,
// ) -> HttpResponse {
//     // let request_id = Uuid::new_v4();
//     // let request_span = tracing::info_span!(
//     //     "Adding a new subscriber.",
//     //     %request_id,
//     //     subscriber_email = %form.email,
//     //     subscriber_name = %form.name
//     // );
//     // let _request_span_guard = request_span.enter();
//     // tracing::info!(
//     //     "request_id {} - Adding '{}' '{}' as a new subscriber.",
//     //     request_id,
//     //     form.email,
//     //     form.name
//     // );
//     // tracing::info!(
//     //     "request_id {} - Saving new subscriber details in the database",
//     //     request_id
//     // );

//     // We do not call `.enter` on query_span!
//     // `.instrument` takes care of it at the right moments
//     // in the query future lifetime
//     let query_span = tracing::info_span!("Saving new subscriber details in the database");
//     match sqlx::query!(
//         r#"
//         INSERT INTO subscriptions (id, email, name, subscribed_at)
//         VALUES ($1, $2, $3, $4)
//         "#,
//         Uuid::new_v4(),
//         form.email,
//         form.name,
//         Utc::now()
//     )
//     // We use `get_ref` to get an immutable reference to the `PgConnection`
//     // wrapped by `web::Data`.
//     .execute(pool.get_ref())
//     .instrument(query_span)
//     .await
//     {
//         Ok(_) => {
//             // tracing::info!(
//             //     "request_id {} - New subscriber details have been saved",
//             //     request_id
//             // );
//             HttpResponse::Ok().finish()
//         }
//         Err(e) => {
//             tracing::error!(
//                 "Failed to execute query: {:?}",
//                 e
//             );
//             HttpResponse::InternalServerError().finish()
//         }
//     }
// }

#[tracing::instrument(
    name = "Saving new subscriber details in the database",
    skip(form, pool)
)]
pub async fn insert_subscriber(pool: &PgPool, form: &FormData) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
        INSERT INTO subscriptions (id, email, name, subscribed_at)
        VALUES ($1, $2, $3, $4)
        "#,
        Uuid::new_v4(),
        form.email,
        form.name,
        Utc::now()
    )
    .execute(pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to execute query: {:?}", e);
        e
        // Using the `?` operator to return early
        // if the function failed, returning a sqlx::Error
        // We will talk about error handling in depth later!
    })?;
    Ok(())
}

#[tracing::instrument(
    name = "Adding a new subscriber",
    skip(form, pool),
    fields(
    // request_id = %Uuid::new_v4(),
    subscriber_email = %form.email,
    subscriber_name = %form.name
    )
    )]
pub async fn subscribe(form: web::Form<FormData>, pool: web::Data<PgPool>) -> HttpResponse {
    match insert_subscriber(&pool, &form).await {
        Ok(_) => HttpResponse::Ok().finish(),
        Err(_) => HttpResponse::InternalServerError().finish(),
    }
}
