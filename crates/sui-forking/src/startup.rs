// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::net::SocketAddr;
use std::num::NonZeroUsize;
use std::sync::Arc;

use anyhow::Result;
use anyhow::bail;
use axum::Router;
use axum::routing::get;
use rand::rngs::OsRng;
use tokio::sync::oneshot;
use tower_http::cors::CorsLayer;
use tracing::info;
use tracing::warn;

use simulacrum::Simulacrum;
use simulacrum::store::in_mem_store::KeyStore;
use sui_swarm_config::network_config_builder::ConfigBuilder;
use sui_types::crypto::AggregateAuthoritySignature;
use sui_types::crypto::AuthorityQuorumSignInfo;
use sui_types::message_envelope::Envelope;
use sui_types::messages_checkpoint::VerifiedCheckpoint;
use sui_types::sui_system_state::SuiSystemState;

use crate::graphql::GraphQLQueryClient;
use crate::network::Network;
use crate::service_store::ServiceStore;
use sui_swarm_config::network_config::NetworkConfig;

mod endpoints {
    pub const HEALTH: &str = "/health";
    pub const STATUS: &str = "/status";
}

/// Start the forking server
pub async fn start_server(
    network: Network,
    startup_checkpoint: Option<u64>,
    host: &str,
    server_port: u16,
    shutdown_receiver: Option<oneshot::Receiver<()>>,
    ready_sender: Option<oneshot::Sender<()>>,
) -> Result<()> {
    let graphql = GraphQLQueryClient::new(network.gql_endpoint())?;
    let checkpoint = fetch_checkpoint(startup_checkpoint, &graphql).await?;
    let service_store = ServiceStore::new(checkpoint.sequence_number);
    let simulacrum = setup_simulacrum(checkpoint, &graphql, service_store).await?;
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
    checkpoint: VerifiedCheckpoint,
    graphql: &GraphQLQueryClient,
    store: ServiceStore,
) -> Result<Simulacrum<OsRng, ServiceStore>, anyhow::Error> {
    let mut rng = OsRng;
    let protocol_version = graphql.fetch_protocol_version().await?;
    let config = ConfigBuilder::new_with_temp_dir()
        .rng(rng)
        .with_chain_start_timestamp_ms(0)
        .deterministic_committee_size(NonZeroUsize::MIN)
        .with_protocol_version(protocol_version.into())
        .build();
    let keystore = KeyStore::from_network_config(&config);
    let system_state = fetch_system_state(&config, &store);

    Ok(Simulacrum::new_from_custom_state(
        keystore,
        checkpoint,
        system_state,
        &config,
        store,
        rng,
    ))
}

async fn fetch_checkpoint(
    checkpoint: Option<u64>,
    graphql: &GraphQLQueryClient,
) -> Result<VerifiedCheckpoint> {
    let checkpoint = graphql.fetch_checkpoint(checkpoint).await?;

    if let Some(checkpoint) = checkpoint {
        let summary = checkpoint.summary;
        let sequence_number = summary.sequence_number;
        // build a dummy AuthorityStrongQuorumSignInfo
        let dummy_sig = AuthorityQuorumSignInfo {
            epoch: summary.epoch.clone(),
            signature: AggregateAuthoritySignature::default(),
            signers_map: roaring::RoaringBitmap::new(),
        };

        // wrap into CertifiedCheckpointSummary (Envelope)
        let certified = Envelope::new_from_data_and_sig(summary.try_into()?, dummy_sig);

        // skip verification because we trust the GraphQL source
        info!("Fetched checkpoint: {}", sequence_number);
        Ok(VerifiedCheckpoint::new_unchecked(certified))
    } else {
        bail!("Failed to fetch checkpoint {checkpoint:?}")
    }
}

/// Fetch the SuiSystemState from the genesis config. This is needed to initialize the Simulacrum.
///
/// Note this will be changed in the future PRs to correctly fetch the SuiSystemState
fn fetch_system_state(config: &NetworkConfig, store: &ServiceStore) -> SuiSystemState {
    config.genesis.sui_system_object()
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
