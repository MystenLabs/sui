// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Standalone DB-less GraphQL subscription server pointed at testnet's fullnode gRPC (checkpoint
//! stream + LedgerService). No validator, indexer, or postgres, just the real `start_rpc`. A separate
//! client (see `examples/subscribe_bench.rs`) subscribes to it over SSE.
//!
//! ```text
//! PORT=8000 cargo run --release --example serve_testnet -p sui-indexer-alt-graphql --features staging
//! ```

use std::alloc::GlobalAlloc;
use std::alloc::Layout;
use std::alloc::System;
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::SocketAddr;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use prometheus::Registry;

/// Bench-only capping allocator: once live heap exceeds the cap from `GRAPHQL_BENCH_ALLOC_CAP_MB`,
/// `alloc` returns null, driving Rust's real allocation-failure path (`handle_alloc_error` -> abort).
/// Lets a macOS run observe genuine OOM crash behavior (where `ulimit -v` does not bite). Uncapped by
/// default (cap = usize::MAX), adding only two relaxed atomic ops per allocation.
struct CappingAlloc;

static LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);
static CAP_BYTES: AtomicUsize = AtomicUsize::new(usize::MAX);

unsafe impl GlobalAlloc for CappingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let after = LIVE_BYTES.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
        if after > CAP_BYTES.load(Ordering::Relaxed) {
            LIVE_BYTES.fetch_sub(layout.size(), Ordering::Relaxed);
            return std::ptr::null_mut();
        }
        System.alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout);
        LIVE_BYTES.fetch_sub(layout.size(), Ordering::Relaxed);
    }
}

#[global_allocator]
static GLOBAL: CappingAlloc = CappingAlloc;
use sui_indexer_alt_graphql::RpcArgs;
use sui_indexer_alt_graphql::args::SubscriptionArgs;
use sui_indexer_alt_graphql::config::RpcConfig;
use sui_indexer_alt_graphql::start_rpc;
use sui_indexer_alt_metrics::MetricsArgs;
use sui_indexer_alt_metrics::MetricsService;
use sui_indexer_alt_reader::consistent_reader::ConsistentReaderArgs;
use sui_indexer_alt_reader::fullnode_client::FullnodeArgs;
use sui_indexer_alt_reader::kv_loader::KvArgs;
use sui_indexer_alt_reader::pg_reader::db::DbArgs;
use sui_indexer_alt_reader::system_package_task::SystemPackageTaskArgs;

const TESTNET: &str = "https://fullnode.testnet.sui.io:443";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _telemetry = telemetry_subscribers::TelemetryConfig::new()
        .with_env()
        .init();

    // Required before any TLS gRPC connection (testnet is https).
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("install rustls provider");

    // BENCH-ONLY: cap live heap so an OOM crash can be observed on a bounded budget.
    if let Some(mb) = std::env::var("GRAPHQL_BENCH_ALLOC_CAP_MB")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
    {
        CAP_BYTES.store(mb * 1024 * 1024, Ordering::Relaxed);
        eprintln!("[serve] BENCH: heap capped at {mb} MB (abort on exceed)");
    }

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8000);
    let listen = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);

    let kv_args = KvArgs {
        ledger_grpc_url: Some(TESTNET.parse().unwrap()),
        enable_list_apis: Some(true),
        ..Default::default()
    };

    let metrics_port: u16 = std::env::var("METRICS_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(9184);
    let metrics_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), metrics_port);
    let metrics = MetricsService::new(
        MetricsArgs {
            metrics_address: metrics_addr,
        },
        Registry::new(),
    );

    eprintln!("[serve] subscriptions on http://{listen}/graphql/subscriptions");
    eprintln!("[serve] metrics on       http://{metrics_addr}/metrics");
    eprintln!("[serve] upstream (stream + ledger): {TESTNET}");

    let service = start_rpc(
        None, // DB-less
        FullnodeArgs::new(TESTNET.parse().unwrap()),
        DbArgs::default(),
        kv_args,
        ConsistentReaderArgs::default(),
        RpcArgs {
            rpc_listen_address: listen,
            no_ide: true,
        },
        SystemPackageTaskArgs::default(),
        SubscriptionArgs {
            checkpoint_stream_url: Some(TESTNET.parse().unwrap()),
        },
        "0.0.0",
        RpcConfig::default(),
        vec![], // no pg pipelines
        metrics.registry(),
    )
    .await?;
    let s_metrics = metrics.run().await?;

    eprintln!("[serve] up. Ctrl-C to stop.");
    tokio::signal::ctrl_c().await.ok();
    drop(service);
    drop(s_metrics);
    Ok(())
}
