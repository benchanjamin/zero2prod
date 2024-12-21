use std::net::TcpListener;

use crate::routes::{health_check, subscribe};
#[allow(unused_imports)]
use actix_web::{dev::Server, middleware::Logger, web, App, HttpServer};
// use env_logger::Env;
use sqlx::PgPool;
use tracing_actix_web::TracingLogger;

pub fn run(listener: TcpListener, connection: PgPool) -> Result<Server, std::io::Error> {
    let pg_pool = web::Data::new(connection);
    let server = HttpServer::new(move || {
        App::new()
            .wrap(TracingLogger::default())
            .route("/health_check", web::get().to(health_check))
            .route("/subscriptions", web::post().to(subscribe))
            .app_data(pg_pool.clone())
    })
    .listen(listener)?
    .run();
    // No .await here!

    Ok(server)
}
