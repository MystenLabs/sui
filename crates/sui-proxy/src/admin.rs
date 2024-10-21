// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::config::{DynamicPeerValidationConfig, RemoteWriteConfig, StaticPeerValidationConfig};
use crate::handlers::publish_metrics;
use crate::histogram_relay::HistogramRelay;
use crate::middleware::{
    expect_content_length, expect_mysten_proxy_header, expect_valid_public_key,
};
use crate::peers::{AllowedPeer, SuiNodeProvider};
use crate::var;
use anyhow::Error;
use anyhow::Result;
use axum::{extract::DefaultBodyLimit, middleware, routing::post, Extension, Router};
use fastcrypto::ed25519::{Ed25519KeyPair, Ed25519PublicKey};
use fastcrypto::traits::{KeyPair, ToFromBytes};
use std::fs;
use std::io::BufReader;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use sui_tls::SUI_VALIDATOR_SERVER_NAME;
use sui_tls::{
    rustls::ServerConfig, AllowAll, ClientCertVerifier, SelfSignedCertificate, TlsAcceptor,
};
use tokio::signal;
use tower::ServiceBuilder;
use tower_http::{
    timeout::TimeoutLayer,
    trace::{DefaultOnFailure, DefaultOnResponse, TraceLayer},
    LatencyUnit,
};
use tracing::{info, Level};

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

/// Reqwest client holds the global client for remote_push api calls
/// it also holds the username and password.  The client has an underlying
/// connection pool.  See reqwest documentation for details
#[derive(Clone)]
pub struct ReqwestClient {
    pub client: reqwest::Client,
    pub settings: RemoteWriteConfig,
}

pub fn make_reqwest_client(settings: RemoteWriteConfig, user_agent: &str) -> ReqwestClient {
    ReqwestClient {
        client: reqwest::Client::builder()
            .user_agent(user_agent)
            .pool_max_idle_per_host(settings.pool_max_idle_per_host)
            .timeout(Duration::from_secs(var!("MIMIR_CLIENT_TIMEOUT", 30)))
            .build()
            .expect("cannot create reqwest client"),
        settings,
    }
}

// Labels are adhoc labels we will inject per our config
#[derive(Clone)]
pub struct Labels {
    pub network: String,
    pub inventory_hostname: String,
}

/// App will configure our routes. This fn is also used to instrument our tests
pub fn app(
    labels: Labels,
    client: ReqwestClient,
    relay: HistogramRelay,
    allower: Option<SuiNodeProvider>,
) -> Router {
    // build our application with a route and our sender mpsc
    let mut router = Router::new()
        .route("/publish/metrics", post(publish_metrics))
        .route_layer(DefaultBodyLimit::max(var!(
            "MAX_BODY_SIZE",
            1024 * 1024 * 5
        )))
        .route_layer(middleware::from_fn(expect_mysten_proxy_header))
        .route_layer(middleware::from_fn(expect_content_length));
    if let Some(allower) = allower {
        router = router
            .route_layer(middleware::from_fn(expect_valid_public_key))
            .layer(Extension(Arc::new(allower)));
    }
    router
        // Enforce on all routes.
        // If the request does not complete within the specified timeout it will be aborted
        // and a 408 Request Timeout response will be sent.
        .layer(TimeoutLayer::new(Duration::from_secs(var!(
            "NODE_CLIENT_TIMEOUT",
            20
        ))))
        .layer(Extension(relay))
        .layer(Extension(labels))
        .layer(Extension(client))
        .layer(
            ServiceBuilder::new().layer(
                TraceLayer::new_for_http()
                    .on_response(
                        DefaultOnResponse::new()
                            .level(Level::INFO)
                            .latency_unit(LatencyUnit::Seconds),
                    )
                    .on_failure(
                        DefaultOnFailure::new()
                            .level(Level::ERROR)
                            .latency_unit(LatencyUnit::Seconds),
                    ),
            ),
        )
}

/// Server creates our http/https server
pub async fn server(
    listener: std::net::TcpListener,
    app: Router,
    acceptor: Option<TlsAcceptor>,
) -> std::io::Result<()> {
    // setup our graceful shutdown
    let handle = axum_server::Handle::new();
    // Spawn a task to gracefully shutdown server.
    tokio::spawn(shutdown_signal(handle.clone()));

    if let Some(verify_peers) = acceptor {
        axum_server::Server::from_tcp(listener)
            .acceptor(verify_peers)
            .handle(handle)
            .serve(app.into_make_service_with_connect_info::<SocketAddr>())
            .await
    } else {
        axum_server::Server::from_tcp(listener)
            .handle(handle)
            .serve(app.into_make_service_with_connect_info::<SocketAddr>())
            .await
    }
}

/// CertKeyPair wraps a self signed certificate and the corresponding public key
pub struct CertKeyPair(pub SelfSignedCertificate, pub Ed25519PublicKey);

/// Generate server certs for use with peer verification
pub fn generate_self_cert(hostname: String) -> CertKeyPair {
    let mut rng = rand::thread_rng();
    let keypair = Ed25519KeyPair::generate(&mut rng);
    CertKeyPair(
        SelfSignedCertificate::new(keypair.copy().private(), &hostname),
        keypair.public().to_owned(),
    )
}

/// Load a certificate for use by the listening service
fn load_certs(filename: &str) -> Vec<rustls::pki_types::CertificateDer<'static>> {
    let certfile = fs::File::open(filename)
        .unwrap_or_else(|e| panic!("cannot open certificate file: {}; {}", filename, e));
    let mut reader = BufReader::new(certfile);
    rustls_pemfile::certs(&mut reader)
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}

/// Load a private key
fn load_private_key(filename: &str) -> rustls::pki_types::PrivateKeyDer<'static> {
    let keyfile = fs::File::open(filename)
        .unwrap_or_else(|e| panic!("cannot open private key file {}; {}", filename, e));
    let mut reader = BufReader::new(keyfile);

    loop {
        match rustls_pemfile::read_one(&mut reader).expect("cannot parse private key .pem file") {
            Some(rustls_pemfile::Item::Pkcs1Key(key)) => return key.into(),
            Some(rustls_pemfile::Item::Pkcs8Key(key)) => return key.into(),
            Some(rustls_pemfile::Item::Sec1Key(key)) => return key.into(),
            None => break,
            _ => {}
        }
    }

    panic!(
        "no keys found in {:?} (encrypted keys not supported)",
        filename
    );
}

/// load the static keys we'll use to allow external non-validator nodes to push metrics
fn load_static_peers(
    static_peers: Option<StaticPeerValidationConfig>,
) -> Result<Vec<AllowedPeer>, Error> {
    let Some(static_peers) = static_peers else {
        return Ok(vec![]);
    };
    let static_keys = static_peers
        .pub_keys
        .into_iter()
        .map(|spk| {
            let peer_id = hex::decode(spk.peer_id).unwrap();
            let public_key = Ed25519PublicKey::from_bytes(peer_id.as_ref()).unwrap();
            let s = AllowedPeer {
                name: spk.name.clone(),
                public_key,
            };
            info!(
                "loaded static peer: {} public key: {}",
                &s.name, &s.public_key,
            );
            s
        })
        .collect();
    Ok(static_keys)
}

/// Default allow mode for server, we don't verify clients, everything is accepted
pub fn create_server_cert_default_allow(
    hostname: String,
) -> Result<ServerConfig, sui_tls::rustls::Error> {
    let CertKeyPair(server_certificate, _) = generate_self_cert(hostname);

    ClientCertVerifier::new(AllowAll, SUI_VALIDATOR_SERVER_NAME.to_string()).rustls_server_config(
        vec![server_certificate.rustls_certificate()],
        server_certificate.rustls_private_key(),
    )
}

/// Verify clients against sui blockchain, clients that are not found in sui_getValidators
/// will be rejected
pub fn create_server_cert_enforce_peer(
    dynamic_peers: DynamicPeerValidationConfig,
    static_peers: Option<StaticPeerValidationConfig>,
) -> Result<(ServerConfig, Option<SuiNodeProvider>), sui_tls::rustls::Error> {
    let (Some(certificate_path), Some(private_key_path)) =
        (dynamic_peers.certificate_file, dynamic_peers.private_key)
    else {
        return Err(sui_tls::rustls::Error::General(
            "missing certs to initialize server".into(),
        ));
    };
    let static_peers = load_static_peers(static_peers).map_err(|e| {
        sui_tls::rustls::Error::General(format!("unable to load static pub keys: {}", e))
    })?;
    let allower = SuiNodeProvider::new(dynamic_peers.url, dynamic_peers.interval, static_peers);
    allower.poll_peer_list();
    let c = ClientCertVerifier::new(allower.clone(), SUI_VALIDATOR_SERVER_NAME.to_string())
        .rustls_server_config(
            load_certs(&certificate_path),
            load_private_key(&private_key_path),
        )?;
    Ok((c, Some(allower)))
}
