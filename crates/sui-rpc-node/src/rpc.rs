// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Wires [`sui_rpc_api::RpcService`] over an [`RpcStoreReader`] and
//! exposes it as a [`sui_futures::service::Service`] so the orchestrator can
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
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha as consistent;
use sui_indexer_alt_consistent_api::proto::rpc::consistent::v1alpha::consistent_service_server::ConsistentServiceServer;
use sui_rpc_api::RpcMetrics;
use sui_rpc_api::RpcService;
use sui_rpc_api::ServerVersion;
use sui_rpc_api::subscription::SubscriptionServiceHandle;
use sui_rpc_store::ConsistencyConfig;
use sui_rpc_store::RpcStoreReader;
use sui_rpc_store::RpcStoreSchema;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tracing::info;

use crate::config::RpcConfig;
use crate::consistent_service::State as ConsistentState;

/// Build an HTTP (and optionally HTTPS) RPC server backed by
/// [`RpcStoreReader`] over the live [`Db`] and return a
/// [`sui_futures::service::Service`] that supervises it.
///
/// `version` ends up as the `X-Sui-Rpc-Version` header on every
/// response (built by `main.rs`); `bin_name` names this binary in
/// the `Server` header.
#[allow(clippy::too_many_arguments)]
pub async fn build_rpc_service(
    db: Db,
    schema: Arc<RpcStoreSchema>,
    consistency: ConsistencyConfig,
    config: RpcConfig,
    subscription_handle: Option<SubscriptionServiceHandle>,
    bin_name: &'static str,
    version: &'static str,
    registry: &Registry,
) -> anyhow::Result<Service> {
    config
        .config
        .validate()
        .context("sui-rpc-api configuration failed validation")?;

    let reader = RpcStoreReader::new(db.clone(), schema);
    let mut rpc_service = RpcService::new(Arc::new(reader));
    rpc_service.with_server_version(ServerVersion::new(bin_name, version));
    rpc_service.with_metrics(RpcMetrics::new(registry));
    rpc_service.with_config(config.config.clone());

    // Mount the checkpoint-subscription service when a handle is
    // supplied (the `run` path, which drives the indexer and feeds the
    // broadcast). Without it the v2 `SubscriptionService` stays
    // unregistered and `subscribe_checkpoints` returns `Unimplemented`.
    if let Some(subscription_handle) = subscription_handle {
        rpc_service.with_subscription_service(subscription_handle);
    }

    // Mount the v1alpha `ConsistentService` over the same `Db`
    // / schema. The service hands out paginated, checkpoint-
    // consistent reads through snapshots taken by the
    // synchronizer.
    let consistent_state = ConsistentState::new(db.clone(), config.pagination.clone(), consistency);
    rpc_service.with_custom_service(ConsistentServiceServer::new(consistent_state));
    rpc_service.with_file_descriptor_set(consistent::FILE_DESCRIPTOR_SET);

    // TODO: wire a `TransactionExecutor` impl via
    // `rpc_service.with_executor(...)`. Without one,
    // `TransactionExecutionService::{execute_transaction,
    // simulate_transaction}` reject every request with
    // `Unimplemented`. Three candidate impls:
    //
    // - A forwarding executor over `sui_rpc_api::Client`
    //   that proxies execute / simulate to an upstream
    //   fullnode or validator (thin read-tier model).
    // - A local simulate-only executor built over
    //   `sui-execution` + `RpcStoreReader` (reuses the
    //   `BackingPackageStore` / `ProtocolConfig` plumbing
    //   `reader/layout.rs` already exercises) — covers
    //   `simulate_transaction` only.
    // - A Simulacrum-backed executor for tests.

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
