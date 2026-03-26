// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::SocketAddr;
use std::num::NonZeroUsize;
use std::sync::Arc;

use anyhow::Result;
use axum::Router;
use axum::routing::get;
use rand::rngs::OsRng;
use tokio::sync::oneshot;
use tower_http::cors::CorsLayer;
use tracing::info;
use tracing::warn;

use crate::service_store::ServiceStore;
use simulacrum::Simulacrum;
use simulacrum::store::in_mem_store::KeyStore;
use sui_swarm_config::network_config_builder::ConfigBuilder;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use sui_types::sui_system_state::SuiSystemState;

mod endpoints {
    pub const HEALTH: &str = "/health";
    pub const STATUS: &str = "/status";
}

struct AppState {
    simulacrum: Arc<Simulacrum>,
}

/// Start the forking server
pub(crate) async fn start_server(
    startup_checkpoint: Option<u64>,
    host: &str,
    server_port: u16,
    shutdown_receiver: Option<oneshot::Receiver<()>>,
    ready_sender: Option<oneshot::Sender<()>>,
) -> Result<()> {
    let context = Context::new(startup_checkpoint, network: Network)
    let app = Router::new()
        .route(endpoints::HEALTH, get(health))
        .route(endpoints::STATUS, get(get_status))
        .layer(CorsLayer::permissive());

    let addr: SocketAddr = format!("{}:{}", host, server_port).parse()?;
    println!("Forking server listening on {}", addr);
    println!("Ready to accept requests");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    if let Some(ready_sender) = ready_sender {
        let _ = ready_sender.send(());
    }

    let shutdown_signal = async move {
        match shutdown_receiver {
            Some(receiver) => {
                let ctrl_c = async {
                    match tokio::signal::ctrl_c().await {
                        Ok(()) => {
                            info!("\nReceived CTRL+C, shutting down gracefully...");
                        }
                        Err(err) => {
                            warn!("Failed to install CTRL+C signal handler: {err}");
                            std::future::pending::<()>().await;
                        }
                    }
                };

                tokio::select! {
                    _ = receiver => {
                        info!("received programmatic shutdown signal");
                    }
                    _ = ctrl_c => {
                    }
                }
            }
            None => {
                if let Err(err) = tokio::signal::ctrl_c().await {
                    warn!("Failed to install CTRL+C signal handler: {err}");
                    return;
                }
                info!("\nReceived CTRL+C, shutting down gracefully...");
            }
        }
    };

    // Serve with graceful shutdown
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal)
        .await?;

    info!("Server shutdown complete");

    Ok(())
}

async fn setup_simulacrum(
    startup_checkpoint: Option<u64>,
    protocol_version: u64,
    initial_sui_system_state: SuiSystemState,
) -> Result<Simulacrum<OsRng, ServiceStore>, anyhow::Error> {
    let mut rng = OsRng;
    let config = ConfigBuilder::new_with_temp_dir()
        .rng(rng)
        .with_chain_start_timestamp_ms(0)
        .deterministic_committee_size(NonZeroUsize::MIN)
        .with_protocol_version(protocol_version.into())
        .build();
    let keystore = KeyStore::from_network_config(&config);
    let checkpoint = fetch_checkpoint(startup_checkpoint).await;
    let system_state = fetch_system_state().await;
    let store = ServiceStore::new(checkpoint.sequence_number);

    Ok(Simulacrum::new_from_custom_state(
        keystore,
        checkpoint,
        system_state,
        &config,
        store,
        rng,
    ))
}

async fn fetch_checkpoint(checkpoint: Option<u64>) -> VerifiedCheckpoint {}

async fn fetch_system_state() -> SuiSystemState {
    todo!()
}

async fn health() -> &'static str {
    "OK"
}

async fn get_status() -> &'static str {
    "OK"
}

mod tests {
    use crate::startup::{get_status, health};

    #[tokio::test]
    async fn test_health_endpoint() {
        let response = health().await;
        assert_eq!(response, "OK");
    }

    #[tokio::test]
    async fn test_status_endpoint() {
        let response = get_status().await;
        assert_eq!(response, "OK");
    }
}
