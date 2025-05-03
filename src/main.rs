use std::fmt::{Debug, Display};
use tokio::task::JoinError;
use zero2prod::issue_delivery_worker::run_worker_until_stopped;
use zero2prod::{
    configuration::get_configuration,
    startup::Application,
    telemetry::{get_subscriber, init_subscriber},
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // We removed the `env_logger` line we had before!
    // env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    let subscriber = get_subscriber("zero2prod".into(), "info".into(), std::io::stdout);
    init_subscriber(subscriber);

    // Bubble up the io::Error if we failed to bind the address
    // Otherwise call .await on our Server
    // Panic if we can't read configuration
    let configuration = get_configuration().expect("Failed to read configuration.");
    // let application = Application::build(configuration.clone())
    //     .await?
    //     .run_until_stopped();
    // let worker = run_worker_until_stopped(configuration);

    let application = Application::build(configuration.clone()).await?;
    let application_task = tokio::spawn(application.run_until_stopped());
    let worker_task = tokio::spawn(run_worker_until_stopped(configuration));

    // Wait for either future to finish
    tokio::select! {
        o = application_task => report_exit("API", o),
        o = worker_task => report_exit("Background worker", o),
    };

    Ok(())

    // Option #1: hard-coded connection string
    // let connection_pool =
    //     PgPool::connect_lazy(&configuration.database.connection_string().expose_secret())
    //         // .await
    //         .expect("Failed to connect to Postgres.");

    // Option #2: makes it easier to manage all moving parts
    // let application = Application::build(configuration).await?;
    // application.run_until_stopped().await?;
    // Ok(())
}

fn report_exit(task_name: &str, outcome: Result<Result<(), impl Debug + Display>, JoinError>) {
    match outcome {
        Ok(Ok(())) => {
            tracing::info!("{} has exited", task_name)
        }
        Ok(Err(e)) => {
            tracing::error!(
            error.cause_chain = ?e,
            error.message = %e,
            "{} failed",
            task_name
            )
        }
        Err(e) => {
            tracing::error!(
            error.cause_chain = ?e,
            error.message = %e,
            "{}' task failed to complete",
            task_name
            )
        }
    }
}
