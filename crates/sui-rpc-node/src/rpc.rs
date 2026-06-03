// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Wires [`sui_rpc_api::RpcService`] over an [`RpcStoreReader`] and
//! exposes it as a [`sui_futures::Service`] so the orchestrator can
//! compose it with the indexer and metrics services. Optional
//! HTTPS support is honoured when
//! [`sui_rpc_api::Config::tls_config`] is set, mirroring how
//! `sui-node` boots its RPC stack.

use std::sync::Arc;

use anyhow::Context as _;
use axum_server::Handle;
use axum_server::tls_rustls::RustlsConfig;
use prometheus::Registry;
use sui_consistent_store::Db;
use sui_futures::service::Service;
use sui_rpc_api::RpcMetrics;
use sui_rpc_api::RpcService;
use sui_rpc_api::ServerVersion;
use sui_rpc_store::RpcStoreReader;
use sui_rpc_store::RpcStoreSchema;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tracing::info;

use crate::config::RpcConfig;

/// Build an HTTP (and optionally HTTPS) RPC server backed by
/// [`RpcStoreReader`] over the live [`Db`] and return a
/// [`sui_futures::Service`] that supervises it.
///
/// `version` ends up as the `X-Sui-Rpc-Version` header on every
/// response (built by `main.rs`); `bin_name` names this binary in
/// the `Server` header.
pub async fn build_rpc_service(
    db: Db,
    schema: Arc<RpcStoreSchema>,
    config: RpcConfig,
    bin_name: &'static str,
    version: &'static str,
    registry: &Registry,
) -> anyhow::Result<Service> {
    config
        .config
        .validate()
        .context("sui-rpc-api configuration failed validation")?;

    let reader = RpcStoreReader::new(db, schema);
    let mut rpc_service = RpcService::new(Arc::new(reader));
    rpc_service.with_server_version(ServerVersion::new(bin_name, version));
    rpc_service.with_metrics(RpcMetrics::new(registry));
    rpc_service.with_config(config.config.clone());

    let router = rpc_service.into_router().await;

    let mut service = Service::new();

    // HTTPS listener — only spawned when both a listen address and
    // a TLS keypair are configured. Mirrors the `sui-node` behaviour
    // (HTTPS is opt-in, plain HTTP is always served).
    if let Some(tls) = config.config.tls_config() {
        let cert = tls.cert().to_owned();
        let key = tls.key().to_owned();
        let https_addr = config.config.https_address();
        let tls_router = router.clone();
        let tls_cfg = RustlsConfig::from_pem_file(cert, key)
            .await
            .context("Failed to load TLS keypair for HTTPS RPC")?;
        let handle = Handle::new();
        service = service
            .with_shutdown_signal({
                let handle = handle.clone();
                async move {
                    handle.graceful_shutdown(None);
                }
            })
            .spawn(async move {
                info!("Starting HTTPS RPC service on {https_addr}");
                axum_server::bind_rustls(https_addr, tls_cfg)
                    .handle(handle)
                    .serve(tls_router.into_make_service())
                    .await
                    .context("HTTPS RPC service failed")?;
                Ok(())
            });
    }

    // Plain HTTP listener — always started.
    let http_addr = config.listen_address;
    let listener = TcpListener::bind(http_addr)
        .await
        .with_context(|| format!("Failed to bind HTTP RPC at {http_addr}"))?;
    let (stx, srx) = oneshot::channel::<()>();
    service = service
        .with_shutdown_signal(async move {
            let _ = stx.send(());
        })
        .spawn(async move {
            info!("Starting HTTP RPC service on {http_addr}");
            axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    let _ = srx.await;
                })
                .await
                .context("HTTP RPC service failed")
        });

    Ok(service)
}
