// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Backtest the execution layer against historical mainnet data: re-execute past transactions under
//! the current execution rules and report where the recomputed result diverges from what was
//! recorded on chain. Useful for measuring the behavioral impact of an execution/protocol change
//! before it ships.
//!
//! Runs as a `sui-indexer-alt-framework` concurrent pipeline (see [`handler`]): the framework's
//! `Indexer` ingests the resolved checkpoint range with adaptive concurrency and runs the
//! per-checkpoint processor with a configurable fanout. Each transaction matching `--status` is
//! re-executed against reconstructed checkpoint state; any whose recomputed success/failure status
//! disagrees with its on-chain status is recorded as a divergence. Output goes to a swappable sink
//! (`--store`): postgres (durable + queryable) or an ndjson file (zero-setup).

mod context;
mod execute;
mod grpc;
mod handler;
mod ingestion;
mod ndjson_store;
mod rows;
mod schema;
mod store;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context as _, Result, bail};
use clap::{Parser, ValueEnum};
use diesel_migrations::EmbeddedMigrations;
use diesel_migrations::embed_migrations;
use prometheus::Registry;
use sui_indexer_alt_framework::Indexer;
use sui_indexer_alt_framework::IndexerArgs;
use sui_indexer_alt_framework::TaskArgs;
use sui_indexer_alt_framework::ingestion::IngestionConfig;
use sui_indexer_alt_framework::ingestion::IngestionService;
use sui_indexer_alt_framework::ingestion::ingestion_client::IngestionClient;
use sui_indexer_alt_framework::ingestion::ingestion_client::IngestionClientArgs;
use sui_indexer_alt_framework::metrics::IngestionMetrics;
use sui_indexer_alt_framework::pipeline::ConcurrencyConfig;
use sui_indexer_alt_framework::pipeline::concurrent::ConcurrentConfig;
use sui_indexer_alt_framework::postgres::{Db, DbArgs};
use sui_protocol_config::Chain;
use sui_types::digests::ChainIdentifier;
use sui_types::metrics::ExecutionMetrics;
use tracing::info;
use url::Url;

use crate::context::{EpochCtx, resolve_epoch_work};
use crate::grpc::RpcClient;
use crate::handler::{Backtest, CommitRows};
use crate::ndjson_store::NdjsonStore;
use crate::store::PackageCache;

/// Backtest-specific migrations (the `divergence` and `run_stats` tables). The framework's
/// watermark tables come from `sui-pg-db`'s own migrations, which `Db::run_migrations` applies
/// alongside these.
pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

/// Which on-chain transaction statuses to re-execute.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub(crate) enum StatusFilter {
    /// Only transactions that succeeded on-chain (strict differential baseline).
    Success,
    /// Only transactions that failed on-chain.
    Failed,
    /// Both; divergence records carry their on-chain status/failure kind for downstream filtering.
    All,
}

/// Where divergence (and stats) rows are written.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum StoreKind {
    /// Newline-delimited JSON file (zero-setup; no resumption across restarts).
    Ndjson,
    /// Postgres (durable, queryable, resumable).
    Postgres,
}

#[derive(Parser)]
#[clap(
    name = "sui-execution-backtest",
    about = "Re-execute historical transactions and report divergences from their on-chain effects."
)]
struct Args {
    #[clap(flatten)]
    client: IngestionClientArgs,

    /// Sui fullnode gRPC url used to resolve epochs and fetch packages. Defaults to `--rpc-api-url`
    /// when that is provided as the checkpoint source.
    #[clap(long)]
    fullnode_url: Option<Url>,

    /// First epoch to scan (inclusive).
    #[clap(long)]
    start_epoch: u64,

    /// Last epoch to scan (inclusive).
    #[clap(long)]
    end_epoch: u64,

    /// Optional cap on the number of checkpoints to process per epoch (for bounded samples).
    #[clap(long)]
    max_checkpoints_per_epoch: Option<u64>,

    /// Which on-chain transaction statuses to re-execute. `success` keeps the strict baseline (only
    /// txns that succeeded on-chain, so any divergence is a clear regression); `failed` runs only
    /// failed txns; `all` runs both and tags each divergence record with its `original_status`/
    /// failure kind so the differential can be applied in the analysis layer.
    #[clap(long, value_enum, default_value_t = StatusFilter::All)]
    status: StatusFilter,

    /// CPU width: the number of checkpoints processed (and thus re-executed) concurrently by the
    /// pipeline's processor fanout. Defaults to the framework's adaptive scaling (up to the number
    /// of CPUs). Each checkpoint's transactions are executed serially on one blocking worker.
    #[clap(long)]
    execute_concurrency: Option<usize>,

    /// Optional directory for an on-disk package cache (speeds up re-scans).
    #[clap(long)]
    cache: Option<PathBuf>,

    /// Where to write divergence (and stats) rows.
    #[clap(long, value_enum, default_value_t = StoreKind::Ndjson)]
    store: StoreKind,

    /// Postgres connection url (required for `--store postgres`).
    #[clap(long)]
    database_url: Option<Url>,

    /// Output file for `--store ndjson` (required for that store).
    #[clap(long)]
    output: Option<PathBuf>,

    /// Run identifier. Namespaces both the output rows (the `task` column, part of the primary key)
    /// and the postgres watermark, so re-running under changed execution rules starts fresh instead
    /// of resuming the previous run's watermark and skipping already-processed checkpoints. Omit to
    /// derive it from the git revision the binary was built from — the HEAD commit, plus a sha of
    /// uncommitted changes when the working tree is dirty — so a commit or local edit gets a fresh
    /// namespace while re-running unchanged code resumes where it left off.
    #[clap(long)]
    task: Option<String>,

    /// Skip emitting per-checkpoint `run_stats` rows (divergences are still recorded).
    #[clap(long)]
    no_stats: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    if args.start_epoch > args.end_epoch {
        bail!(
            "start_epoch ({}) must be <= end_epoch ({})",
            args.start_epoch,
            args.end_epoch
        );
    }

    let fullnode_url: Url = args
        .fullnode_url
        .clone()
        .or_else(|| args.client.rpc_api_url.clone())
        .context("provide --fullnode-url (or --rpc-api-url) for epoch and package resolution")?;
    let rpc = RpcClient::new(fullnode_url)?;

    let registry = Registry::new();
    let execution_metrics = Arc::new(ExecutionMetrics::new(&registry));
    let ingestion_metrics = IngestionMetrics::new(None, &registry);
    let packages = Arc::new(PackageCache::new(
        rpc.clone(),
        tokio::runtime::Handle::current(),
        args.cache.clone(),
    )?);

    // The chain identifier comes from the fullnode's GetServiceInfo (cheap), not by fetching
    // genesis. For a remote object store we wrap the ingestion client so it never derives the chain
    // id from genesis (see [`ingestion`]); the gRPC source already uses GetServiceInfo and a local
    // store is fast. This one client serves both the upfront epoch resolution and the indexer, so
    // genesis is never fetched.
    let chain_id = rpc.chain_id().await.context("fetching chain id")?;
    let chain: Chain = chain_id.chain();
    let ingestion_client = match &args.client.remote_store_url {
        Some(url) => ingestion::remote_store_client(url, chain_id, ingestion_metrics.clone())?,
        None => IngestionClient::new(args.client.clone(), ingestion_metrics.clone())?,
    };

    let first_bounds = rpc
        .epoch_bounds(args.start_epoch)
        .await
        .with_context(|| format!("resolving epoch {}", args.start_epoch))?;
    let (epochs, first_checkpoint, last_checkpoint) = resolve_epoch_work(
        &rpc,
        chain,
        args.start_epoch..=args.end_epoch,
        args.max_checkpoints_per_epoch,
        first_bounds,
        &execution_metrics,
    )
    .await?;
    let epochs = Arc::new(epochs);

    // The framework watermark is keyed by `{pipeline}@{task}`, so namespacing per run-id keeps a
    // re-run under changed rules from resuming the previous run's watermark. When no `--task` is
    // given, derive it from the git revision (HEAD commit, plus an uncommitted-changes sha when the
    // tree is dirty): a commit or local edit to the execution rules gets a fresh namespace, while
    // re-running unchanged code resumes. The reader interval is irrelevant here (this pipeline does
    // no pruning and has no main pipeline), so it is set high.
    const READER_INTERVAL_MS: u64 = 3_600_000;
    let run_id = match args.task {
        Some(run_id) => run_id,
        None => {
            let derived = derive_task_from_git().context("deriving task id from git revision")?;
            info!(task = %derived, "no --task given; derived run identifier from git revision");
            derived
        }
    };
    let indexer_task = TaskArgs::tasked(run_id.clone(), READER_INTERVAL_MS);
    let row_task = run_id;

    let plan = BacktestPlan {
        epochs,
        packages,
        chain_id,
        status: args.status,
        task: row_task,
        indexer_task,
        stats_enabled: !args.no_stats,
        first_checkpoint,
        last_checkpoint,
        fanout: args
            .execute_concurrency
            .map(|value| ConcurrencyConfig::Fixed { value }),
    };

    // We hand-build the ingestion service (rather than the simpler `Indexer::new`, which constructs
    // one internally from `ClientArgs`) for two reasons, both of which would regress if we switched:
    //   1. The remote-store source needs our `FixedChainId` wrapper (see [`ingestion`]) to avoid
    //      deriving the chain id from genesis. `Indexer::new` builds the store client without it, so
    //      every concurrent fetch `try_join`s a slow, repeatedly-retried genesis download — measured
    //      as a sustained ~4x throughput drop (not just a startup cost), worst on bounded samples.
    //   2. `Indexer::new` takes `ClientArgs` (= ingestion + streaming args), which would re-add the
    //      streaming flags to the CLI that we deliberately dropped; we only flatten
    //      `IngestionClientArgs`.
    // If the remote-store source is ever retired (leaving only gRPC, which already gets the chain id
    // from `GetServiceInfo`), this wiring — and the `ingestion` module — could collapse into
    // `Indexer::new`.
    let ingestion_service = IngestionService::with_clients(
        ingestion_client,
        None,
        IngestionConfig::default(),
        ingestion_metrics,
    );

    match args.store {
        StoreKind::Ndjson => {
            let output = args
                .output
                .context("--output is required for --store ndjson")?;
            let store = NdjsonStore::create(&output)?;
            run_backtest(store, ingestion_service, registry, plan).await
        }
        StoreKind::Postgres => {
            let database_url = args
                .database_url
                .context("--database-url is required for --store postgres")?;
            let store = Db::for_write(database_url, DbArgs::default())
                .await
                .context("connecting to postgres")?;
            store
                .run_migrations(Some(&MIGRATIONS))
                .await
                .context("running migrations")?;
            run_backtest(store, ingestion_service, registry, plan).await
        }
    }
}

/// The default run identifier: the source revision the binary was built from. This is the HEAD
/// commit's short sha, plus — when the working tree is dirty — a second sha fingerprinting the
/// uncommitted (tracked) changes. So committed code re-runs under a stable id (resuming its
/// watermark), while each distinct set of edits-under-test gets a fresh namespace.
///
/// Git runs against the build-time source tree (`CARGO_MANIFEST_DIR`), so the id reflects the code
/// the binary came from regardless of the process's working directory. The dirty fingerprint is the
/// diff against HEAD hashed into a git blob sha (`git diff HEAD | git hash-object --stdin`), which —
/// unlike `git stash create` — carries no commit timestamp, so re-running the same edits yields the
/// same id. It is empty when the tree is clean; untracked files are not captured. Errors out if git
/// is unavailable or the source tree is gone — pass `--task` explicitly in that case.
fn derive_task_from_git() -> Result<String> {
    let dir = env!("CARGO_MANIFEST_DIR");

    let head = run_git(dir, &["rev-parse", "--short=12", "HEAD"], None)
        .context("resolving HEAD (pass --task to set a run id explicitly)")?;
    let head = String::from_utf8(head).context("git HEAD not utf-8")?;
    let head = head.trim();

    let diff = run_git(dir, &["diff", "HEAD"], None).context("diffing working tree")?;
    if diff.is_empty() {
        return Ok(format!("git-{head}"));
    }
    let wip = run_git(dir, &["hash-object", "--stdin"], Some(&diff))
        .context("hashing working-tree diff")?;
    let wip = String::from_utf8(wip).context("git hash-object output not utf-8")?;
    let wip = wip.trim();
    Ok(format!("git-{head}-{}", &wip[..wip.len().min(12)]))
}

/// Run `git -C <dir> <args>`, optionally feeding `stdin`, and return raw stdout. Errors on a
/// non-zero exit (carrying git's stderr). Output is bytes, not text, because `git diff` can contain
/// non-UTF-8 content from binary files.
fn run_git(dir: &str, args: &[&str], stdin: Option<&[u8]>) -> Result<Vec<u8>> {
    use std::io::Write as _;
    use std::process::{Command, Stdio};

    let mut cmd = Command::new("git");
    cmd.arg("-C")
        .arg(dir)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if stdin.is_some() {
        cmd.stdin(Stdio::piped());
    }
    let mut child = cmd.spawn().context("spawning git")?;
    if let Some(bytes) = stdin {
        child
            .stdin
            .take()
            .expect("stdin piped")
            .write_all(bytes)
            .context("writing git stdin")?;
    }
    let out = child.wait_with_output().context("waiting on git")?;
    if !out.status.success() {
        anyhow::bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(out.stdout)
}

/// Everything needed to run the pipeline, independent of the sink. Grouped so [`run_backtest`] can
/// be generic over the store with a small argument list.
struct BacktestPlan {
    epochs: Arc<BTreeMap<u64, Arc<EpochCtx>>>,
    packages: Arc<PackageCache>,
    chain_id: ChainIdentifier,
    status: StatusFilter,
    /// Run identifier written to the `task` row column.
    task: String,
    /// Framework task config that namespaces the watermark (matches `task` when set via `--task`).
    indexer_task: TaskArgs,
    stats_enabled: bool,
    first_checkpoint: u64,
    last_checkpoint: u64,
    fanout: Option<ConcurrencyConfig>,
}

/// Build and run the indexer for a given sink. Generic over the store so the postgres and ndjson
/// paths share all the wiring.
async fn run_backtest<S: CommitRows>(
    store: S,
    ingestion_service: IngestionService,
    registry: Registry,
    plan: BacktestPlan,
) -> Result<()> {
    let indexer_args = IndexerArgs {
        first_checkpoint: Some(plan.first_checkpoint),
        last_checkpoint: Some(plan.last_checkpoint),
        pipeline: Vec::new(),
        task: plan.indexer_task,
    };

    let mut indexer =
        Indexer::with_ingestion_service(store, indexer_args, ingestion_service, None, &registry)
            .await?;

    let handler = Backtest::new(
        plan.epochs,
        plan.packages,
        plan.chain_id,
        plan.status,
        plan.task,
        plan.stats_enabled,
    );
    let config = ConcurrentConfig {
        fanout: plan.fanout,
        ..Default::default()
    };
    indexer.concurrent_pipeline(handler, config).await?;

    let service = indexer.run().await?;
    service
        .main()
        .await
        .map_err(|e| anyhow::anyhow!("indexer terminated: {e:?}"))?;
    Ok(())
}
