// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Measures the per-transaction execution-latency difference between two full nodes.
//!
//! Each transaction is submitted once to the validators, then `WaitForLocalEffects` is called on
//! both a "primary" and a "baseline" full node. The delta between when each node reports local
//! execution isolates how much sooner one node (e.g. a consensus-observer node) executes than the
//! other (e.g. a state-sync node) — independent of checkpointing or indexing.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use clap::Parser;
use comfy_table::Table;
use prometheus::HistogramVec;
use prometheus::IntCounter;
use prometheus::IntGauge;
use prometheus::Registry;
use prometheus::register_histogram_vec_with_registry;
use prometheus::register_int_counter_with_registry;
use prometheus::register_int_gauge_with_registry;
use sui_benchmark::BenchmarkProxyMetrics;
use sui_benchmark::LocalValidatorAggregatorProxy;
use sui_benchmark::ValidatorProxy;
use sui_benchmark::util::get_ed25519_keypair_from_keystore;
use sui_config::node::Genesis;
use sui_core::test_utils::make_transfer_sui_transaction;
use sui_rpc_api::grpc::local_execution::LocalExecutionServiceClient;
use sui_types::base_types::ObjectRef;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::AccountKeyPair;
use sui_types::gas_coin::GasCoin;
use sui_types::local_execution::WaitForLocalEffectsRequest;
use sui_types::local_execution::WaitForLocalEffectsResponse;
use sui_types::object::Owner;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::TransactionData;
use sui_types::utils::to_sender_signed_transaction;
use tokio::sync::Mutex;
use tokio::task::JoinSet;
use tokio::time::MissedTickBehavior;
use tonic::transport::Channel;
use tracing::info;

#[derive(Parser, Debug)]
#[command(
    name = "blockstream-effects-latency",
    about = "Compare per-transaction local-execution latency between two full nodes"
)]
struct Opts {
    /// Path to the network genesis blob (for the validator committee).
    #[arg(long)]
    genesis_blob_path: String,
    /// Path to the Sui keystore holding the primary gas owner's key.
    #[arg(long)]
    keystore_path: String,
    /// Address that owns the primary gas coin used to fund the run.
    #[arg(long)]
    primary_gas_owner: SuiAddress,
    /// gRPC URL of the primary full node (e.g. the consensus-observer node).
    #[arg(long)]
    primary_node_url: String,
    /// gRPC URL of the baseline full node (e.g. the state-sync node).
    #[arg(long)]
    baseline_node_url: String,
    /// Total number of transactions to measure.
    #[arg(long, default_value_t = 200)]
    num_transactions: usize,
    /// Number of gas coins, which bounds the maximum in-flight transactions.
    #[arg(long, default_value_t = 8)]
    concurrency: usize,
    /// Open-loop offered load in transactions/second. When unset (or 0), runs closed-loop: each of
    /// `concurrency` workers submits, waits for completion, then submits the next. To sustain a
    /// target without saturating, size `concurrency` >= target-qps * average latency (seconds).
    #[arg(long)]
    target_qps: Option<u64>,
    /// Gas price to use. Defaults to the network reference gas price.
    #[arg(long)]
    gas_price: Option<u64>,
    /// Per-node wait timeout for a transaction's local effects, in milliseconds.
    #[arg(long, default_value_t = 30_000)]
    wait_timeout_ms: u64,
    /// Host for the Prometheus metrics scrape endpoint.
    #[arg(long, default_value = "127.0.0.1")]
    client_metric_host: String,
    /// Port for the Prometheus metrics scrape endpoint.
    #[arg(long, default_value_t = 8081)]
    client_metric_port: u16,
}

/// One measured transaction: milliseconds from submission to each node reporting local effects.
#[derive(Clone, Copy)]
struct Sample {
    primary_ms: f64,
    baseline_ms: f64,
    /// `baseline_ms - primary_ms`: positive means the primary node executed sooner.
    delta_ms: f64,
}

/// Prometheus metrics exposed on the scrape endpoint for live Grafana dashboards.
struct Metrics {
    /// Submit-to-local-effects latency per node (label `node` = primary|baseline).
    latency_seconds: HistogramVec,
    submitted: IntCounter,
    succeeded: IntCounter,
    failed: IntCounter,
    in_flight: IntGauge,
    /// Configured open-loop target rate (0 in closed-loop mode).
    target_qps: IntGauge,
    /// Ticks skipped because no gas coin was free — the offered load exceeded capacity.
    saturated: IntCounter,
}

impl Metrics {
    fn new(registry: &Registry) -> Self {
        Self {
            latency_seconds: register_histogram_vec_with_registry!(
                "blockstream_effects_latency_seconds",
                "Submit-to-local-effects latency per full node",
                &["node"],
                mysten_metrics::LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            submitted: register_int_counter_with_registry!(
                "blockstream_effects_submitted_total",
                "Transactions submitted for measurement",
                registry,
            )
            .unwrap(),
            succeeded: register_int_counter_with_registry!(
                "blockstream_effects_succeeded_total",
                "Transactions whose effects were observed on both nodes",
                registry,
            )
            .unwrap(),
            failed: register_int_counter_with_registry!(
                "blockstream_effects_failed_total",
                "Transactions that failed to submit or timed out on a node",
                registry,
            )
            .unwrap(),
            in_flight: register_int_gauge_with_registry!(
                "blockstream_effects_in_flight",
                "Measured transactions currently in flight",
                registry,
            )
            .unwrap(),
            target_qps: register_int_gauge_with_registry!(
                "blockstream_effects_target_qps",
                "Configured open-loop target rate (0 = closed-loop)",
                registry,
            )
            .unwrap(),
            saturated: register_int_counter_with_registry!(
                "blockstream_effects_saturated_total",
                "Dispatch ticks skipped because no gas coin was free (offered load exceeded capacity)",
                registry,
            )
            .unwrap(),
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let opts = Opts::parse();

    let mut telemetry = telemetry_subscribers::TelemetryConfig::new();
    telemetry.log_string = Some("info".to_string());
    let _guard = telemetry.with_env().init();

    let registry_service = mysten_metrics::start_prometheus_server(
        format!("{}:{}", opts.client_metric_host, opts.client_metric_port)
            .parse::<SocketAddr>()
            .context("parsing metrics address")?,
    );
    let registry = registry_service.default_registry();
    mysten_metrics::init_metrics(&registry);
    let metric = Arc::new(Metrics::new(&registry));

    let genesis = Genesis::new_from_file(&opts.genesis_blob_path);
    let genesis = genesis.genesis()?;

    // Standard uptime gauge (self-updating), so Grafana can tell the tool is alive and for how long.
    let chain_identifier =
        sui_types::digests::ChainIdentifier::from(*genesis.checkpoint().digest()).to_string();
    registry
        .register(mysten_metrics::uptime_metric(
            "blockstream-effects-latency",
            env!("CARGO_PKG_VERSION"),
            &chain_identifier,
        ))
        .unwrap();

    let metrics = BenchmarkProxyMetrics::new(&registry);

    // Submit through the validators directly; the baseline node URL is used only for the
    // reconfiguration observer.
    let proxy: Arc<dyn ValidatorProxy + Send + Sync> = Arc::new(
        LocalValidatorAggregatorProxy::from_genesis(genesis, &opts.baseline_node_url, &metrics)
            .await,
    );

    let keypair = Arc::new(get_ed25519_keypair_from_keystore(
        opts.keystore_path.clone().into(),
        &opts.primary_gas_owner,
    )?);
    let sender = opts.primary_gas_owner;

    let gas_price = match opts.gas_price {
        Some(p) => p,
        None => {
            proxy
                .get_latest_system_state_object()
                .await?
                .reference_gas_price
        }
    };

    let gas_coins = split_gas_coins(&proxy, sender, &keypair, gas_price, opts.concurrency).await?;
    println!(
        "funded {} gas coin(s); measuring {} transactions at gas price {}",
        gas_coins.len(),
        opts.num_transactions,
        gas_price
    );

    let primary_client = LocalExecutionServiceClient::connect(opts.primary_node_url.clone())
        .await
        .context("connecting to primary node")?;
    let baseline_client = LocalExecutionServiceClient::connect(opts.baseline_node_url.clone())
        .await
        .context("connecting to baseline node")?;

    let ctx = Arc::new(Ctx {
        proxy,
        keypair,
        sender,
        gas_price,
        wait_timeout_ms: opts.wait_timeout_ms,
        primary_client,
        baseline_client,
        metric: metric.clone(),
    });

    metric.target_qps.set(opts.target_qps.unwrap_or(0) as i64);
    let (samples, failures) = match opts.target_qps {
        Some(qps) if qps > 0 => run_open_loop(ctx, gas_coins, opts.num_transactions, qps).await,
        _ => run_closed_loop(ctx, gas_coins, opts.num_transactions).await,
    };

    report(&samples, failures);
    Ok(())
}

struct Ctx {
    proxy: Arc<dyn ValidatorProxy + Send + Sync>,
    keypair: Arc<AccountKeyPair>,
    sender: SuiAddress,
    gas_price: u64,
    wait_timeout_ms: u64,
    primary_client: LocalExecutionServiceClient<Channel>,
    baseline_client: LocalExecutionServiceClient<Channel>,
    metric: Arc<Metrics>,
}

/// Submit one transaction and measure its local-effects latency on both nodes. Returns the sample
/// (`None` on failure) and the gas coin reference to use next — recycled from the finalized effects,
/// or unchanged if submission failed.
async fn measure_transaction(ctx: &Ctx, gas: ObjectRef) -> (Option<Sample>, ObjectRef) {
    // Transfer the whole coin to self: recycles the gas coin (new version, same owner).
    let tx = make_transfer_sui_transaction(
        gas,
        ctx.sender,
        None,
        ctx.sender,
        &ctx.keypair,
        ctx.gas_price,
    );
    let digest = *tx.digest();

    ctx.metric.submitted.inc();
    ctx.metric.in_flight.inc();

    let submit = ctx.proxy.execute_transaction_block(tx);
    let primary = wait_for_effects(&ctx.primary_client, digest, ctx.wait_timeout_ms);
    let baseline = wait_for_effects(&ctx.baseline_client, digest, ctx.wait_timeout_ms);
    let (submit_res, primary_res, baseline_res) = tokio::join!(submit, primary, baseline);
    ctx.metric.in_flight.dec();

    let next_gas = match &submit_res {
        Ok(effects) if effects.is_ok() => effects.gas_object().0,
        Ok(effects) => {
            ctx.metric.failed.inc();
            tracing::warn!(
                "transaction {digest} failed on-chain: {:?}",
                effects.status()
            );
            return (None, gas);
        }
        Err(e) => {
            ctx.metric.failed.inc();
            tracing::warn!("submitting transaction {digest} failed: {e}");
            return (None, gas);
        }
    };

    match (primary_res, baseline_res) {
        (Some(primary_ms), Some(baseline_ms)) => {
            let delta_ms = baseline_ms - primary_ms;
            ctx.metric
                .latency_seconds
                .with_label_values(&["primary"])
                .observe(primary_ms / 1000.0);
            ctx.metric
                .latency_seconds
                .with_label_values(&["baseline"])
                .observe(baseline_ms / 1000.0);
            ctx.metric.succeeded.inc();
            info!(
                digest = %digest,
                primary_ms,
                baseline_ms,
                delta_ms,
                "measured transaction"
            );
            (
                Some(Sample {
                    primary_ms,
                    baseline_ms,
                    delta_ms,
                }),
                next_gas,
            )
        }
        _ => {
            ctx.metric.failed.inc();
            (None, next_gas)
        }
    }
}

/// Closed-loop: one worker per gas coin, each submitting sequentially until the total is reached.
async fn run_closed_loop(
    ctx: Arc<Ctx>,
    gas_coins: Vec<ObjectRef>,
    num: usize,
) -> (Vec<Sample>, usize) {
    let workers = gas_coins.len();
    let mut set = JoinSet::new();
    for (i, gas) in gas_coins.into_iter().enumerate() {
        let ctx = ctx.clone();
        let count = num / workers + usize::from(i < num % workers);
        set.spawn(async move {
            let mut gas = gas;
            let mut samples = Vec::new();
            let mut failures = 0;
            for _ in 0..count {
                let (sample, next) = measure_transaction(&ctx, gas).await;
                gas = next;
                match sample {
                    Some(s) => samples.push(s),
                    None => failures += 1,
                }
            }
            (samples, failures)
        });
    }
    let mut samples = Vec::new();
    let mut failures = 0;
    while let Some(res) = set.join_next().await {
        let (worker_samples, worker_failures) = res.expect("worker task panicked");
        samples.extend(worker_samples);
        failures += worker_failures;
    }
    (samples, failures)
}

/// Open-loop: dispatch at a fixed rate, bounded by the gas-coin pool. When no coin is free at a
/// tick, the offered load has exceeded capacity — the tick is counted as saturated and skipped.
async fn run_open_loop(
    ctx: Arc<Ctx>,
    gas_coins: Vec<ObjectRef>,
    num: usize,
    qps: u64,
) -> (Vec<Sample>, usize) {
    let pool = Arc::new(Mutex::new(gas_coins));
    let mut ticker = tokio::time::interval(Duration::from_micros(1_000_000 / qps.max(1)));
    ticker.set_missed_tick_behavior(MissedTickBehavior::Burst);

    let mut set = JoinSet::new();
    let mut submitted = 0;
    while submitted < num {
        ticker.tick().await;
        let gas = pool.lock().await.pop();
        let Some(gas) = gas else {
            ctx.metric.saturated.inc();
            continue;
        };
        submitted += 1;
        let ctx = ctx.clone();
        let pool = pool.clone();
        set.spawn(async move {
            let (sample, next) = measure_transaction(&ctx, gas).await;
            pool.lock().await.push(next);
            sample
        });
    }

    let mut samples = Vec::new();
    let mut failures = 0;
    while let Some(res) = set.join_next().await {
        match res.expect("measurement task panicked") {
            Some(s) => samples.push(s),
            None => failures += 1,
        }
    }
    (samples, failures)
}

/// Call `WaitForLocalEffects` and return the elapsed milliseconds since `start`, or `None` if the
/// node did not report local execution before the timeout.
async fn wait_for_effects(
    client: &LocalExecutionServiceClient<Channel>,
    digest: sui_types::digests::TransactionDigest,
    timeout_ms: u64,
) -> Option<f64> {
    let start = Instant::now();
    let response = client
        .clone()
        .wait_for_local_effects(WaitForLocalEffectsRequest {
            transaction_digest: digest,
            timeout_ms: Some(timeout_ms),
            include_details: false,
        })
        .await;
    match response.map(|r| r.into_inner()) {
        Ok(WaitForLocalEffectsResponse::Executed { .. }) => {
            Some(start.elapsed().as_secs_f64() * 1000.0)
        }
        Ok(WaitForLocalEffectsResponse::TimedOut) => None,
        Err(status) => {
            tracing::warn!("wait_for_local_effects for {digest} errored: {status}");
            None
        }
    }
}

/// Split the primary gas owner's largest gas coin into `count` roughly equal coins owned by the
/// same address, returning their object references.
async fn split_gas_coins(
    proxy: &Arc<dyn ValidatorProxy + Send + Sync>,
    sender: SuiAddress,
    keypair: &AccountKeyPair,
    gas_price: u64,
    count: usize,
) -> Result<Vec<ObjectRef>> {
    let mut coins: Vec<(u64, ObjectRef)> = proxy
        .get_owned_objects(sender)
        .await?
        .into_iter()
        .filter_map(|(_, object)| {
            let value = GasCoin::try_from(&object).ok()?.value();
            Some((value, object.compute_object_reference()))
        })
        .collect();
    coins.sort_by_key(|(value, _)| std::cmp::Reverse(*value));
    let (primary_value, primary) = *coins
        .first()
        .context("primary gas owner has no gas coins")?;

    // Leave headroom for gas of the split tx and the per-coin transfers.
    let per_coin = primary_value / (count as u64 + 4);
    if per_coin == 0 {
        bail!("primary gas coin balance {primary_value} too small to split into {count} coins");
    }

    let mut builder = ProgrammableTransactionBuilder::new();
    builder.pay_sui(vec![sender; count], vec![per_coin; count])?;
    let data = TransactionData::new_programmable(
        sender,
        vec![primary],
        builder.finish(),
        per_coin * count as u64 / 10 + 10_000_000,
        gas_price,
    );
    let effects = proxy
        .execute_transaction_block(to_sender_signed_transaction(data, keypair))
        .await?;
    if !effects.is_ok() {
        bail!("gas split transaction failed: {:?}", effects.status());
    }

    let new_coins: Vec<ObjectRef> = effects
        .created()
        .into_iter()
        .filter(|(_, owner)| matches!(owner, Owner::AddressOwner(a) if *a == sender))
        .map(|(reference, _)| reference)
        .collect();
    if new_coins.len() != count {
        bail!("expected {count} new gas coins, got {}", new_coins.len());
    }
    Ok(new_coins)
}

fn report(samples: &[Sample], failures: usize) {
    if samples.is_empty() {
        println!("no successful samples ({failures} failures)");
        return;
    }
    let primary: Vec<f64> = samples.iter().map(|s| s.primary_ms).collect();
    let baseline: Vec<f64> = samples.iter().map(|s| s.baseline_ms).collect();
    let delta: Vec<f64> = samples.iter().map(|s| s.delta_ms).collect();

    let mut table = Table::new();
    table.set_header(vec![
        "metric (ms)",
        "mean",
        "p50",
        "p90",
        "p99",
        "min",
        "max",
    ]);
    for (name, series) in [
        ("primary node", primary),
        ("baseline node", baseline),
        ("delta (base - prim)", delta),
    ] {
        let stats = Stats::from(&series);
        table.add_row(vec![
            name.to_string(),
            fmt(stats.mean),
            fmt(stats.p50),
            fmt(stats.p90),
            fmt(stats.p99),
            fmt(stats.min),
            fmt(stats.max),
        ]);
    }
    println!("{table}");
    println!("samples: {} succeeded, {} failed", samples.len(), failures);
}

fn fmt(v: f64) -> String {
    format!("{v:.2}")
}

struct Stats {
    mean: f64,
    p50: f64,
    p90: f64,
    p99: f64,
    min: f64,
    max: f64,
}

impl Stats {
    fn from(series: &[f64]) -> Self {
        let mut sorted = series.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mean = series.iter().sum::<f64>() / series.len() as f64;
        Self {
            mean,
            p50: percentile(&sorted, 0.50),
            p90: percentile(&sorted, 0.90),
            p99: percentile(&sorted, 0.99),
            min: sorted[0],
            max: sorted[sorted.len() - 1],
        }
    }
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    let rank = (p * (sorted.len() as f64 - 1.0)).round() as usize;
    sorted[rank.min(sorted.len() - 1)]
}
