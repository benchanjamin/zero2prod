use crate::telemetry::spawn_blocking_with_tracing;
use anyhow::Context;
use argon2::{Algorithm, Argon2, Params, PasswordHash, PasswordHasher, PasswordVerifier, Version};
use argon2::password_hash::SaltString;
use secrecy::{ExposeSecret, Secret};
use sqlx::PgPool;

// #[source] (on InvalidCredentials)
// Marks the field as the error source for error chains
// Does NOT implement automatic conversion
// Preserves the error chain for diagnostics
// Requires explicitly constructing this variant
// #[from] (on UnexpectedError)
// Does everything #[source] does
// Additionally implements From<anyhow::Error> for AuthError
// Enables automatic conversion with the ? operator
// Only one variant can have From<T> for a given type T
#[derive(thiserror::Error, Debug)]
pub enum AuthError {
    /* If InvalidCredentials used #[from], any anyhow::Error would automatically
    convert to InvalidCredentials when using ?, which would conflict with the UnexpectedError
     variant that already implements From<anyhow::Error>. */
    #[error("Invalid credentials.")]
    InvalidCredentials(#[source] anyhow::Error),
    #[error(transparent)]
    UnexpectedError(#[from] anyhow::Error),
}

pub struct Credentials {
    // These two fields were not marked as `pub` before!
    pub username: String,
    pub password: Secret<String>,
}

#[tracing::instrument(name = "Validate credentials", skip(credentials, pool))]
pub async fn validate_credentials(
    credentials: Credentials,
    pool: &PgPool,
) -> Result<uuid::Uuid, AuthError> {
    let mut user_id = None;
    let mut expected_password_hash = Secret::new(
        "$argon2id$v=19$m=15000,t=2,p=1$\
gZiV/M1gPc22ElAH/Jh1Hw$\
CWOrkoo7oJBQ/iyh7uJ0LO2aLEfrHwTWllSAxT0zRno"
            .to_string(),
    );

    if let Some((stored_user_id, stored_password_hash)) =
        get_stored_credentials(&credentials.username, &pool).await?
    {
        user_id = Some(stored_user_id);
        expected_password_hash = stored_password_hash;
    }

    // let (user_id, expected_password_hash) = get_stored_credentials(&credentials.username, pool)
    //     .await
    //     .map_err(PublishError::UnexpectedError)?
    //     .ok_or_else(|| PublishError::AuthError(anyhow::anyhow!("Unknown username.")))?;

    // let (expected_password_hash, user_id) = match row {
    //     Some(row) => (row.0, row.1),
    //     None => {
    //         return Err(PublishError::AuthError(anyhow::anyhow!(
    //             "Invalid username or password."
    //         )))
    //     }
    // };

    //  Old way:: needed to move ownership of the password into the closure
    // let expected_password_hash = PasswordHash::new(&expected_password_hash.expose_secret())
    //     .context("Failed to parse hash in PHC string format.")
    //     .map_err(PublishError::UnexpectedError)?;

    spawn_blocking_with_tracing(move || {
        verify_password_hash(expected_password_hash, credentials.password)
    })
        .await
        .context("Failed to spawn blocking task.")??;

    // This is only set to `Some` if we found credentials in the store
    // So, even if the default password ends up matching (somehow)
    // with the provided password,
    // we never authenticate a non-existing user.
    // You can easily add a unit test for that precise scenario.
    user_id.ok_or_else(|| AuthError::InvalidCredentials(anyhow::anyhow!("Unknown username.")))

    //  Old way:: needed to move ownership of the password into the closure
    // tokio::task::spawn_blocking(move || {
    //     tracing::info_span!("Verify password hash").in_scope(|| {
    //         Argon2::default().verify_password(
    //             credentials.password.expose_secret().as_bytes(),
    //             &expected_password_hash,
    //         )
    //     })
    // })
    // .await
    // // spawn_blocking is fallible - we have a nested Result here!
    // .context("Failed to spawn blocking task.")
    // .map_err(PublishError::UnexpectedError)?
    // .context("Invalid password.")
    // .map_err(PublishError::AuthError)?;
    //
    // Ok(user_id)
}

#[tracing::instrument(
    name = "Verify password hash",
    skip(expected_password_hash, password_candidate)
)]
fn verify_password_hash(
    expected_password_hash: Secret<String>,
    password_candidate: Secret<String>,
) -> Result<(), AuthError> {
    let expected_password_hash = PasswordHash::new(expected_password_hash.expose_secret())
        .context("Failed to parse hash in PHC string format.")?;

    Argon2::default()
        .verify_password(
            password_candidate.expose_secret().as_bytes(),
            &expected_password_hash,
        )
        .context("Invalid password.")
        .map_err(AuthError::InvalidCredentials)
}

// We extracted the db-querying logic in its own function with its own span.
#[tracing::instrument(name = "Get stored credentials", skip(username, pool))]
async fn get_stored_credentials(
    username: &str,
    pool: &PgPool,
) -> Result<Option<(uuid::Uuid, Secret<String>)>, anyhow::Error> {
    let row: Option<_> = sqlx::query!(
        r#"
            SELECT user_id, password_hash
            FROM users
            WHERE username = $1
        "#,
        username,
    )
        .fetch_optional(pool)
        .await
        .context("Failed to perform a query to retrieve stored credentials.")?
        .map(|row| (row.user_id, Secret::new(row.password_hash)));

    Ok(row)
}

#[tracing::instrument(name = "Change password", skip(password, pool))]
pub async fn change_password(
    user_id: uuid::Uuid,
    password: Secret<String>,
    pool: &PgPool,
) -> Result<(), anyhow::Error> {
    let password_hash = spawn_blocking_with_tracing(move || compute_password_hash(password))
        .await?
        .context("Failed to hash password")?;
    sqlx::query!(
        r#"
        UPDATE users
        SET password_hash = $1
        WHERE user_id = $2
        "#,
        password_hash.expose_secret(),
        user_id
    )
        .execute(pool)
        .await
        .context("Failed to change user's password in the database.")?;
    Ok(())
}

fn compute_password_hash(password: Secret<String>) -> Result<Secret<String>, anyhow::Error> {
    let salt = SaltString::generate(&mut rand::thread_rng());
    let password_hash = Argon2::new(
        Algorithm::Argon2id,
        Version::V0x13,
        Params::new(15000, 2, 1, None).unwrap(),
    )
        .hash_password(password.expose_secret().as_bytes(), &salt)?
        .to_string();
    Ok(Secret::new(password_hash))
}
