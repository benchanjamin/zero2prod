use actix_web::http::StatusCode;
use actix_web::{web, HttpResponse, ResponseError};
use chrono::Utc;
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use serde::Deserialize;
use sqlx::{Executor, PgPool, Postgres, Transaction};
#[allow(unused_imports)]
use tracing::{self, Instrument};

use uuid::Uuid;

use crate::{
    domain::{NewSubscriber, SubscriberEmail, SubscriberName},
    email_client::EmailClient,
    startup::ApplicationBaseUrl,
};

#[derive(Deserialize)]
pub struct FormData {
    email: String,
    name: String,
}

pub fn error_chain_fmt(
    e: &impl std::error::Error,
    f: &mut std::fmt::Formatter<'_>,
) -> std::fmt::Result {
    writeln!(f, "{}\n", e)?;
    let mut current = e.source();
    while let Some(cause) = current {
        writeln!(f, "Caused by:\n\t{}", cause)?;
        current = cause.source();
    }
    Ok(())
}

// BEGIN -- Old way for error handling the store token error

pub struct StoreTokenError(sqlx::Error);

impl std::error::Error for StoreTokenError {
    // source() allows us to chain errors using the error_chain_fmt function
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        // The compiler transparently casts `&sqlx::Error` into a `&dyn Error`
        Some(&self.0)
    }
}

impl std::fmt::Debug for StoreTokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl std::fmt::Display for StoreTokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "A database error occurred while \
            trying to store a subscription token."
        )
    }
}

impl ResponseError for StoreTokenError {}

// END -- Old way for error handling the store token error

#[derive(thiserror::Error)]
pub enum SubscribeError {
    #[error("{0}")]
    ValidationError(String),
    #[error("Failed to store the confirmation token for a new subscriber.")]
    StoreTokenError(#[from] StoreTokenError),
    #[error("Failed to send a confirmation email.")]
    SendEmailError(#[from] reqwest::Error),
    // extra errors for Display trait
    #[error("Failed to acquire a Postgres connection from the pool")]
    // source() allows us to chain errors using the error_chain_fmt function
    PoolError(#[source] sqlx::Error),
    #[error("Failed to insert new subscriber in the database.")]
    InsertSubscriberError(#[source] sqlx::Error),
    #[error("Failed to commit SQL transaction to store a new subscriber.")]
    TransactionCommitError(#[source] sqlx::Error),
    // Transparent delegates both `Display`'s and `source`'s implementation
    // to the type wrapped by `UnexpectedError`.
    // #[error(transparent)]
    // UnexpectedError(#[from] Box<dyn std::error::Error>),
}

// simpl From<reqwest::Error> for SubscribeError {
//     fn from(e: reqwest::Error) -> Self {
//         Self::SendEmailError(e)
//     }
// }

// impl From<StoreTokenError> for SubscribeError {
//     fn from(e: StoreTokenError) -> Self {
//         Self::StoreTokenError(e)
//     }
// }

// impl From<String> for SubscribeError {
//     fn from(e: String) -> Self {
//         Self::ValidationError(e)
//     }
// }

impl std::fmt::Debug for SubscribeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

// OLD WAY v1
// impl std::fmt::Display for SubscribeError {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         write!(f, "Failed to create a new subscriber.")
//     }
// }

// OLD WAY v2
// impl std::fmt::Display for SubscribeError {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         match self {
//             SubscribeError::ValidationError(e) => write!(f, "{}", e),
//             // What should we do here? ALREADY HANDLED with PoolError, InsertSubscriberError, and TransactionCommitError
//             // SubscribeError::DatabaseError(_) => write!(f, "???"),
//             SubscribeError::StoreTokenError(_) => write!(
//                 f,
//                 "Failed to store the confirmation token for a new subscriber."
//             ),
//             SubscribeError::SendEmailError(_) => {
//                 write!(f, "Failed to send a confirmation email.")
//             }
//             SubscribeError::PoolError(_) => {
//                 write!(f,
//                     "Failed to acquire a Postgres connection from the pool")
//             }
//             SubscribeError::InsertSubscriberError(_) => {
//                 write!(f, "Failed to insert new subscriber in the database.")
//             }
//             SubscribeError::TransactionCommitError(_) => {
//                 write!(f,
//                     "Failed to commit SQL transaction to store a new subscriber.")
//             }
//         }
//     }
// }

// Note - Not needed when using thiserror
// impl std::error::Error for SubscribeError {
//     fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
//         match self {
//             // &str does not implement `Error` - we consider it the root cause
//             SubscribeError::ValidationError(_) => None,
//             SubscribeError::DatabaseError(e) => Some(e),
//             SubscribeError::StoreTokenError(e) => Some(e),
//             SubscribeError::SendEmailError(e) => Some(e),
//             SubscribeError::PoolError(e) => Some(e),
//             SubscribeError::InsertSubscriberError(e) => Some(e),
//             SubscribeError::TransactionCommitError(e) => Some(e),
//         }
//     }
// }

impl ResponseError for SubscribeError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::ValidationError(_) => StatusCode::BAD_REQUEST,
            Self::StoreTokenError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::SendEmailError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::PoolError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::InsertSubscriberError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::TransactionCommitError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            // Self::UnexpectedError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
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
    name = "Adding a new subscriber",
    skip(form, pool, email_client, base_url),
    fields(
    // request_id = %Uuid::new_v4(),
        subscriber_email = %form.email,
    subscriber_name = %form.name
    )
    )]
pub async fn subscribe(
    form: web::Form<FormData>,
    pool: web::Data<PgPool>,
    email_client: web::Data<EmailClient>,
    base_url: web::Data<ApplicationBaseUrl>,
) -> Result<HttpResponse, SubscribeError> {
    // `web::Form` is a wrapper around `FormData`
    // `form.0` gives us access to the underlying `FormData`
    let new_subscriber = form.0.try_into().map_err(SubscribeError::ValidationError)?;

    let mut transaction = pool.begin().await.map_err(SubscribeError::PoolError)?;

    let subscriber_id = insert_subscriber(&mut transaction, &new_subscriber)
        .await
        .map_err(SubscribeError::InsertSubscriberError)?;

    let subscription_token = generate_subscription_token();
    store_token(&mut transaction, subscriber_id, &subscription_token).await?;

    transaction
        .commit()
        .await
        .map_err(SubscribeError::TransactionCommitError)?;

    // Send a (useless) email to the new subscriber.
    // We are ignoring email delivery errors for now.
    send_confirmation_email(
        &email_client,
        new_subscriber,
        &base_url.0,
        &subscription_token,
    )
    .await?;

    Ok(HttpResponse::Ok().finish())
}

#[tracing::instrument(
    name = "Saving new subscriber details in the database",
    skip(transaction, new_subscriber)
)]
pub async fn insert_subscriber(
    transaction: &mut Transaction<'_, Postgres>,
    new_subscriber: &NewSubscriber,
) -> Result<Uuid, sqlx::Error> {
    let subscriber_id = Uuid::new_v4();
    let query = sqlx::query!(
        r#"
        INSERT INTO subscriptions (id, email, name, subscribed_at, status)
        VALUES ($1, $2, $3, $4, 'pending_confirmation')
        "#,
        subscriber_id,
        new_subscriber.email.as_ref(),
        new_subscriber.name.as_ref(),
        Utc::now(),
    );
    transaction.execute(query).await.map_err(|e| {
        // tracing::error!("Failed to execute query: {:?}", e);
        e
        // Using the `?` operator to return early
        // if the function failed, returning a sqlx::Error
        // We will talk about error handling in depth later!
    })?;
    Ok(subscriber_id)
}

impl TryFrom<FormData> for NewSubscriber {
    type Error = String;

    fn try_from(form: FormData) -> Result<Self, Self::Error> {
        let name = SubscriberName::parse(form.name)?;
        let email = SubscriberEmail::parse(form.email)?;
        Ok(NewSubscriber { email, name })
    }
}

#[tracing::instrument(
    name = "Store token in the database",
    skip(transaction, subscriber_id, subscription_token)
)]
async fn store_token(
    transaction: &mut Transaction<'_, Postgres>,
    subscriber_id: Uuid,
    subscription_token: &str,
) -> Result<(), StoreTokenError> {
    let query = sqlx::query!(
        r#"
        INSERT INTO subscription_tokens (subscription_token, subscriber_id)
        VALUES ($1, $2)
        "#,
        subscription_token,
        subscriber_id
    );
    transaction.execute(query).await.map_err(|e| {
        // tracing::error!("Failed to execute query: {:?}", e);
        StoreTokenError(e)
    })?;
    Ok(())
}

#[tracing::instrument(
    name = "Send a confirmation email to a new subscriber",
    skip(new_subscriber, email_client)
)]
pub async fn send_confirmation_email(
    email_client: &EmailClient,
    new_subscriber: NewSubscriber,
    base_url: &str,
    subscription_token: &str,
) -> Result<(), reqwest::Error> {
    let confirmation_link = format!(
        "{}/subscriptions/confirm?subscription_token={}",
        base_url, subscription_token
    );
    let plain_body = format!(
        "Welcome to our newsletter!\nVisit {} to confirm your subscription.",
        confirmation_link
    );
    let html_body = format!(
        "Welcome to our newsletter!<br />\
        Click <a href=\"{}\">here</a> to confirm your subscription.",
        confirmation_link
    );
    // get email client's base url
    dbg!(&email_client.base_url);
    email_client
        .send_email(&new_subscriber.email, "Welcome!", &html_body, &plain_body)
        .await
}

/// Generate a random 25-characters-long case-sensitive subscription token.
fn generate_subscription_token() -> String {
    let mut rng = thread_rng();
    std::iter::repeat_with(|| rng.sample(Alphanumeric))
        .map(char::from)
        .take(25)
        .collect()
}
