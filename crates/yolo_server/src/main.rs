mod api;
mod models;
mod server_config;
mod server_env;
mod server_state;

use std::time::Duration;

use api::order_book_index;
use axum::{Router, error_handling::HandleErrorLayer, http::StatusCode, routing::get};
use server_config::ServerConfig;
use server_state::SharedServerState;
use tokio::{
    net::TcpListener,
    signal::{self, unix::SignalKind},
};
use tower::{BoxError, ServiceBuilder, timeout::TimeoutLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install ctrl+c handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            tracing::debug!("ctrl+c received");
            tracing::debug!("waiting for outstanding requests to complete...")
        },
        _ = terminate => {
            tracing::debug!("SIGTERM received");
            tracing::debug!("waiting for a few seconds to complete outstanding requests...")
        },
    }
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
        .layer((
            TraceLayer::new_for_http(),
            // graceful shutdown:
            // wait for outstanding requests to complete
            TimeoutLayer::new(Duration::from_secs(3)),
        ))
        .into_inner();

    let server_state = SharedServerState::default();

    let app = Router::new()
        .route("/order-book/{pair}", get(order_book_index))
        .layer(service_stack)
        .with_state(server_state);

    let address = format!("{}:{}", server_config.host, server_config.port);
    let listener = TcpListener::bind(address).await?;
    tracing::debug!("listening on {}", listener.local_addr().unwrap());

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}
