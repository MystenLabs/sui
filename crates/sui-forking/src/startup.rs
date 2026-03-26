// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::SocketAddr;
use std::num::NonZeroUsize;

use anyhow::Result;
use axum::Router;
use axum::routing::get;
use rand::rngs::OsRng;
use simulacrum::Simulacrum;
use simulacrum::store::SimulatorStore;
use simulacrum::store::in_mem_store::KeyStore;
use sui_swarm_config::network_config::NetworkConfig;
use sui_swarm_config::network_config_builder::ConfigBuilder;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use sui_types::sui_system_state::SuiSystemState;
use tokio::sync::oneshot;
use tower_http::cors::CorsLayer;
use tracing::{info, warn};

use crate::graphql::NetworkDataClient;

mod endpoints {
    pub const HEALTH: &str = "/health";
    pub const STATUS: &str = "/status";
}

/// Start the forking server.
///
/// `store_factory` receives the forked checkpoint sequence number and returns a store
/// implementation.
pub async fn start_server<S: SimulatorStore>(
    client: &dyn NetworkDataClient,
    startup_checkpoint: Option<u64>,
    store_factory: impl FnOnce(u64) -> S,
    host: &str,
    server_port: u16,
    shutdown_receiver: Option<oneshot::Receiver<()>>,
    ready_sender: Option<oneshot::Sender<SocketAddr>>,
) -> Result<()> {
    let checkpoint = client.fetch_checkpoint(startup_checkpoint).await?;
    let protocol_version = client.fetch_protocol_version().await?;
    let store = store_factory(checkpoint.sequence_number);
    let _simulacrum = setup_simulacrum(checkpoint, protocol_version, store)?;
    let app = build_router();
    serve(app, host, server_port, shutdown_receiver, ready_sender).await
}

fn setup_simulacrum<S: SimulatorStore>(
    checkpoint: VerifiedCheckpoint,
    protocol_version: u64,
    store: S,
) -> Result<Simulacrum<OsRng, S>> {
    let rng = OsRng;
    let config = ConfigBuilder::new_with_temp_dir()
        .rng(rng)
        .with_chain_start_timestamp_ms(0)
        .deterministic_committee_size(NonZeroUsize::MIN)
        .with_protocol_version(protocol_version.into())
        .build();
    let keystore = KeyStore::from_network_config(&config);
    let system_state = fetch_system_state(&config);

    println!(
        "Starting forking server from checkpoint {} with protocol version {}",
        checkpoint.sequence_number, protocol_version
    );

    Ok(Simulacrum::new_from_custom_state(
        keystore,
        checkpoint,
        system_state,
        &config,
        store,
        rng,
    ))
}

/// Fetch the SuiSystemState from the genesis config. This is needed to initialize the Simulacrum.
///
/// Note this will be changed in future PRs to correctly fetch the SuiSystemState from the network.
fn fetch_system_state(config: &NetworkConfig) -> SuiSystemState {
    config.genesis.sui_system_object()
}

pub(crate) fn build_router() -> Router {
    Router::new()
        .route(endpoints::HEALTH, get(health))
        .route(endpoints::STATUS, get(get_status))
        .layer(CorsLayer::permissive())
}

async fn serve(
    app: Router,
    host: &str,
    server_port: u16,
    shutdown_receiver: Option<oneshot::Receiver<()>>,
    ready_sender: Option<oneshot::Sender<SocketAddr>>,
) -> Result<()> {
    let addr: SocketAddr = format!("{}:{}", host, server_port).parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let local_addr = listener.local_addr()?;

    println!("Forking server listening on {}", local_addr);

    if let Some(ready_sender) = ready_sender {
        let _ = ready_sender.send(local_addr);
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

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal)
        .await?;

    info!("Server shutdown complete");

    Ok(())
}

async fn health() -> &'static str {
    "OK"
}

async fn get_status() -> &'static str {
    "OK"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graphql::MockNetworkDataClient;
    use simulacrum::store::in_mem_store::InMemoryStore;

    use sui_swarm_config::network_config_builder::ConfigBuilder;
    use sui_types::sui_system_state::SuiSystemStateTrait;

    fn mock_setup() -> (
        MockNetworkDataClient,
        sui_swarm_config::network_config::NetworkConfig,
    ) {
        let config = ConfigBuilder::new_with_temp_dir()
            .with_chain_start_timestamp_ms(1)
            .deterministic_committee_size(NonZeroUsize::new(1).unwrap())
            .build();
        let store = InMemoryStore::new(&config.genesis);
        let checkpoint = store
            .get_checkpoint_by_sequence_number(0)
            .expect("genesis checkpoint must exist")
            .clone();
        let protocol_version = config.genesis.sui_system_object().protocol_version();

        let mock_client = MockNetworkDataClient {
            checkpoint,
            protocol_version,
        };
        (mock_client, config)
    }

    #[tokio::test]
    async fn test_server_health_and_status() {
        let (mock_client, config) = mock_setup();
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let (ready_tx, ready_rx) = oneshot::channel::<SocketAddr>();

        let server = tokio::spawn(async move {
            start_server(
                &mock_client,
                None,
                |_| InMemoryStore::new(&config.genesis),
                "127.0.0.1",
                9001,
                Some(shutdown_rx),
                Some(ready_tx),
            )
            .await
            .unwrap();
        });

        let addr = ready_rx.await.expect("server should signal ready");
        let client = reqwest::Client::new();

        let resp = client
            .get(format!("http://{}/health", addr))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(resp.text().await.unwrap(), "OK");

        let resp = client
            .get(format!("http://{}/status", addr))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(resp.text().await.unwrap(), "OK");

        shutdown_tx.send(()).unwrap();
        server.await.unwrap();
    }
}
