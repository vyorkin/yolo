mod server_config;
mod server_env;

use std::time::Duration;

use axum::{Router, error_handling::HandleErrorLayer, http::StatusCode, routing::get};
use server_config::ServerConfig;
use tokio::net::TcpListener;
use tower::{BoxError, ServiceBuilder};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

async fn handler() -> &'static str {
    "hello world"
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let server_config = ServerConfig::read()?;

    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| format!("{}=debug,tower_http=debug", env!("CARGO_CRATE_NAME")).into());

    tracing_subscriber::registry()
        .with(env_filter)
        .with(tracing_subscriber::fmt::layer())
        .init();

    let error_handling_layer = HandleErrorLayer::new(|error: BoxError| async move {
        if error.is::<tower::timeout::error::Elapsed>() {
            Ok(StatusCode::REQUEST_TIMEOUT)
        } else {
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("unhandled internal error: {error}"),
            ))
        }
    });

    let service_stack = ServiceBuilder::new()
        .layer(error_handling_layer)
        .timeout(Duration::from_secs(10))
        .layer(TraceLayer::new_for_http())
        .into_inner();

    let app = Router::new().route("/", get(handler)).layer(service_stack);

    let address = format!("{}:{}", server_config.host, server_config.port);
    let listener = TcpListener::bind(address).await?;
    tracing::debug!("listening on {}", listener.local_addr().unwrap());

    axum::serve(listener, app).await?;

    Ok(())
}
