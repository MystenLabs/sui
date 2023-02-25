// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::channels::UpstreamConsumer;
use crate::handlers::publish_metrics;
use anyhow::{Context, Result};
use axum::{routing::post as axum_post, Router};
use fastcrypto::ed25519::Ed25519KeyPair;
use fastcrypto::ed25519::Ed25519PublicKey;
use fastcrypto::traits::KeyPair;
use fastcrypto::traits::ToFromBytes;
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;
use sui_tls::{
    rustls::ServerConfig, SelfSignedCertificate, TlsAcceptor, ValidatorAllowlist,
    ValidatorCertVerifier,
};
use sui_types::sui_system_state::ValidatorMetadata;
use tokio::{signal, sync::mpsc};
use tracing::{error, info};

/// Configure our graceful shutdown scenarios
pub async fn shutdown_signal(h: axum_server::Handle) {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    let grace = 30;
    info!(
        "signal received, starting graceful shutdown, grace period {} seconds, if needed",
        &grace
    );
    h.graceful_shutdown(Some(Duration::from_secs(grace)))
}

/// App will configure our routes and create our mpsc channels.  This fn is also used to instrument
/// our tests
pub fn app(buffer_size: usize, network: String) -> Router {
    // we accept data on our UpstreamConsumer up to our buffer size.
    let (sender, receiver) = mpsc::channel(buffer_size);
    let mut consumer = UpstreamConsumer::new(network, receiver);

    tokio::spawn(async move { consumer.run().await });

    // build our application with a route and our sender mpsc
    Router::new()
        .route("/publish/metrics", axum_post(publish_metrics))
        .with_state(Arc::new(sender))
}

pub async fn server(
    listener: std::net::TcpListener,
    _acceptor: TlsAcceptor, // TODO enable for tls
    app: Router,
) -> std::io::Result<()> {
    // setup our graceful shutdown
    let handle = axum_server::Handle::new();
    // Spawn a task to gracefully shutdown server.
    tokio::spawn(shutdown_signal(handle.clone()));

    axum_server::Server::from_tcp(listener)
        // .acceptor(acceptor) // TODO enable for tls
        .handle(handle)
        .serve(app.into_make_service())
        .await
}

pub fn create_server_cert(
    hostname: &str,
) -> Result<(ServerConfig, ValidatorAllowlist), sui_tls::rustls::Error> {
    let mut rng = rand::thread_rng();
    let server_keypair = Ed25519KeyPair::generate(&mut rng);
    let server_certificate = SelfSignedCertificate::new(server_keypair.private(), hostname);

    ValidatorCertVerifier::rustls_server_config(
        vec![server_certificate.rustls_certificate()],
        server_certificate.rustls_private_key(),
    )
}

async fn get_validators(url: String) -> Result<Vec<ValidatorMetadata>> {
    let client = reqwest::Client::builder().build().unwrap();
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "method":"sui_getValidators",
        "id":1,
    });
    let response = client
        .post(url)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(request.to_string())
        .send()
        .await
        .context("unable to perform rpc")?;

    #[derive(Debug, Deserialize)]
    struct ResponseBody {
        result: Vec<ValidatorMetadata>,
    }
    let body = response
        .json::<ResponseBody>()
        .await
        .context("unable to deserialize validator peer list")?;

    Ok(body.result)
}

pub fn manage_validators(url: String, period: Duration, allowlist: ValidatorAllowlist) {
    info!("Started polling for peers using rpc: {}", url);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(period);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            interval.tick().await;

            match get_validators(url.clone()).await {
                Ok(peers) => {
                    // obtain rwlock; nb allowed is an raii object
                    let mut allowed = allowlist.write().unwrap();
                    let latest_peers = peers.iter().filter_map(|v| {
                        info!("name: {:?} sui_address: {:?}", v.name, v.sui_address);
                        match Ed25519PublicKey::from_bytes(&v.network_pubkey_bytes) {
                            Ok(client_public_key) => Some(client_public_key),
                            Err(error) => {
                                error!(
                                "unable to decode public key for name: {:?} sui_address: {:?} error: {error}",
                                v.name, v.sui_address);
                                return None;
                            }
                        }
                    });
                    allowed.clear();
                    allowed.extend(latest_peers);
                }
                Err(error) => error!("unable to refresh peer list: {error}"),
            }
        }
    });
}
