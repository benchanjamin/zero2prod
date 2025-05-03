use crate::authentication::{validate_credentials, AuthError, Credentials};
use crate::routes::error_chain_fmt;
use crate::session_state::TypedSession;
use crate::startup::HmacSecret;
use actix_session::Session;
use actix_web::cookie::Cookie;
use actix_web::error::InternalError;
use actix_web::http::header::LOCATION;
use actix_web::web;
use actix_web::HttpResponse;
use actix_web_flash_messages::FlashMessage;
use hmac::{Hmac, Mac};
use secrecy::ExposeSecret;
use secrecy::Secret;
use sqlx::PgPool;

#[derive(serde::Deserialize)]
pub struct FormData {
    username: String,
    password: Secret<String>,
}
#[tracing::instrument(
    skip(form, pool, session),
    fields(username=tracing::field::Empty, user_id=tracing::field::Empty)
)]
// We are now injecting `PgPool` to retrieve stored credentials from the database
pub async fn login(
    form: web::Form<FormData>,
    pool: web::Data<PgPool>,
    // Injecting the secret as a secret string for the time being.
    // secret: web::Data<HmacSecret>,
    session: TypedSession,
) -> Result<HttpResponse, InternalError<LoginError>> {
    let credentials = Credentials {
        username: form.0.username,
        password: form.0.password,
    };
    tracing::Span::current().record("username", &tracing::field::display(&credentials.username));
    match validate_credentials(credentials, &pool).await {
        Ok(user_id) => {
            tracing::Span::current().record("user_id", &tracing::field::display(&user_id));
            session.renew();
            session
                // OLD WAY: using the old method of setting session data using Session type
                // .insert("user_id", user_id)
                // NEW WAY: using the new method of setting session data using TypedSession type
                .insert_user_id(user_id)
                .map_err(|e| login_redirect(LoginError::UnexpectedError(e.into())))?;
            Ok(HttpResponse::SeeOther()
                .insert_header((LOCATION, "/admin/dashboard"))
                .finish())
        }
        Err(e) => {
            let e = match e {
                AuthError::InvalidCredentials(_) => LoginError::AuthError(e.into()),
                AuthError::UnexpectedError(_) => LoginError::UnexpectedError(e.into()),
            };
            // OLD WAY: these were needed when we used the old method of setting query params
            //          to get the error message back at /login
            // let query_string = format!("error={}", urlencoding::Encoded::new(e.to_string()));
            // let hmac_tag = {
            //     let mut mac =
            //         Hmac::<sha2::Sha256>::new_from_slice(secret.0.expose_secret().as_bytes())
            //             .unwrap();
            //     mac.update(query_string.as_bytes());
            //     mac.finalize().into_bytes()
            // };
            FlashMessage::error(e.to_string()).send();
            let response = HttpResponse::SeeOther()
                .insert_header((LOCATION, "/login"))
                // .cookie(Cookie::new("_flash", e.to_string()))
                .finish();
            Err(InternalError::from_response(e, response))
        }
    }
}

// Redirect to the login page with an error message.
fn login_redirect(e: LoginError) -> InternalError<LoginError> {
    FlashMessage::error(e.to_string()).send();
    let response = HttpResponse::SeeOther()
        .insert_header((LOCATION, "/login"))
        .finish();
    InternalError::from_response(e, response)
}

#[derive(thiserror::Error)]
pub enum LoginError {
    #[error("Authentication failed")]
    AuthError(#[source] anyhow::Error),
    #[error("Something went wrong")]
    UnexpectedError(#[from] anyhow::Error),
}
impl std::fmt::Debug for LoginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}
// impl ResponseError for LoginError {
//     fn status_code(&self) -> StatusCode {
//         StatusCode::SEE_OTHER
//     }
//     fn error_response(&self) -> HttpResponse {
//         let query_string = format!(
//             "error={}",
//             urlencoding::Encoded::new(self.to_string())
//         );
//         // We need the secret here - how do we get it?
//         let secret: &[u8] = todo!();
//         let hmac_tag = {
//             let mut mac = Hmac::<sha2::Sha256>::new_from_slice(secret).unwrap();
//             mac.update(query_string.as_bytes());
//             mac.finalize().into_bytes()
//         };
//         HttpResponse::build(self.status_code())
//             // Appending the hexadecimal representation of the HMAC tag to the
//             // query string as an additional query parameter.
//             .insert_header((
//                 LOCATION,
//                 format!("/login?{query_string}&tag={hmac_tag:x}")
//             ))
//             .finish()
//     }
// }
