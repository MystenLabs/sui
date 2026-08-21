// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Measures the per-transaction execution-latency difference between two full nodes.
//!
//! Each transaction is submitted once to the validators, then `WaitForLocalEffects` is called on
//! both a "primary" and a "baseline" full node. The delta between when each node reports local
//! execution isolates how much sooner one node (e.g. a consensus-observer node) executes than the
//! other (e.g. a state-sync node) — independent of checkpointing or indexing.

use std::sync::Arc;
use std::time::Instant;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use clap::Parser;
use comfy_table::Table;
use prometheus::Registry;
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
use tonic::transport::Channel;

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
    /// Number of concurrent in-flight transactions (also the number of gas coins used).
    #[arg(long, default_value_t = 8)]
    concurrency: usize,
    /// Gas price to use. Defaults to the network reference gas price.
    #[arg(long)]
    gas_price: Option<u64>,
    /// Per-node wait timeout for a transaction's local effects, in milliseconds.
    #[arg(long, default_value_t = 30_000)]
    wait_timeout_ms: u64,
    /// Optional path to write the raw per-transaction samples as JSON.
    #[arg(long)]
    output_json: Option<String>,
}

/// One measured transaction: milliseconds from submission to each node reporting local effects.
#[derive(Clone, Copy, serde::Serialize)]
struct Sample {
    primary_ms: f64,
    baseline_ms: f64,
    /// `baseline_ms - primary_ms`: positive means the primary node executed sooner.
    delta_ms: f64,
}

#[tokio::main]
async fn main() -> Result<()> {
    let opts = Opts::parse();

    let genesis = Genesis::new_from_file(&opts.genesis_blob_path);
    let genesis = genesis.genesis()?;
    let registry = Registry::new();
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
        None => proxy.get_latest_system_state_object().await?.reference_gas_price,
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

    let per_worker = opts.num_transactions.div_ceil(opts.concurrency);
    let mut handles = Vec::new();
    for gas in gas_coins {
        let worker = Worker {
            proxy: proxy.clone(),
            keypair: keypair.clone(),
            sender,
            gas_price,
            wait_timeout_ms: opts.wait_timeout_ms,
            primary_client: primary_client.clone(),
            baseline_client: baseline_client.clone(),
        };
        handles.push(tokio::spawn(async move { worker.run(gas, per_worker).await }));
    }

    let mut samples = Vec::new();
    let mut failures = 0usize;
    for handle in handles {
        let (worker_samples, worker_failures) = handle.await.expect("worker task panicked");
        samples.extend(worker_samples);
        failures += worker_failures;
    }

    report(&samples, failures);
    if let Some(path) = opts.output_json {
        std::fs::write(&path, serde_json::to_vec_pretty(&samples)?)?;
        println!("wrote {} samples to {path}", samples.len());
    }
    Ok(())
}

struct Worker {
    proxy: Arc<dyn ValidatorProxy + Send + Sync>,
    keypair: Arc<AccountKeyPair>,
    sender: SuiAddress,
    gas_price: u64,
    wait_timeout_ms: u64,
    primary_client: LocalExecutionServiceClient<Channel>,
    baseline_client: LocalExecutionServiceClient<Channel>,
}

impl Worker {
    async fn run(self, mut gas: ObjectRef, count: usize) -> (Vec<Sample>, usize) {
        let mut samples = Vec::with_capacity(count);
        let mut failures = 0;
        for _ in 0..count {
            // Transfer the whole coin to self: recycles the gas coin (new version, same owner).
            let tx = make_transfer_sui_transaction(
                gas,
                self.sender,
                None,
                self.sender,
                &self.keypair,
                self.gas_price,
            );
            let digest = *tx.digest();

            let submit = self.proxy.execute_transaction_block(tx);
            let primary = wait_for_effects(&self.primary_client, digest, self.wait_timeout_ms);
            let baseline = wait_for_effects(&self.baseline_client, digest, self.wait_timeout_ms);
            let (submit_res, primary_res, baseline_res) = tokio::join!(submit, primary, baseline);

            // Recycle the gas coin from the finalized effects so the next iteration can proceed.
            match &submit_res {
                Ok(effects) if effects.is_ok() => gas = effects.gas_object().0,
                Ok(effects) => {
                    eprintln!("transaction {digest} failed on-chain: {:?}", effects.status());
                    failures += 1;
                    continue;
                }
                Err(e) => {
                    eprintln!("submitting transaction {digest} failed: {e}");
                    failures += 1;
                    continue;
                }
            }

            match (primary_res, baseline_res) {
                (Some(primary_ms), Some(baseline_ms)) => samples.push(Sample {
                    primary_ms,
                    baseline_ms,
                    delta_ms: baseline_ms - primary_ms,
                }),
                _ => failures += 1,
            }
        }
        (samples, failures)
    }
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
            eprintln!("wait_for_local_effects for {digest} errored: {status}");
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
    table.set_header(vec!["metric (ms)", "mean", "p50", "p90", "p99", "min", "max"]);
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
    println!(
        "samples: {} succeeded, {} failed",
        samples.len(),
        failures
    );
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
