// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Context as _;
use anyhow::anyhow;
use async_trait::async_trait;
use clap::Parser;
use diesel::prelude::*;
use diesel_async::RunQueryDsl;
use diesel_migrations::EmbeddedMigrations;
use diesel_migrations::embed_migrations;
use framework::FieldCount;
use framework::cluster::IndexerCluster;
use framework::ingestion::IngestConcurrencyConfig;
use framework::ingestion::IngestionConfig;
use framework::pipeline::CommitterConfig;
use framework::pipeline::Processor;
use framework::pipeline::concurrent::ConcurrentConfig;
use framework::postgres::DbArgs;
use framework::postgres::handler::Handler;
use framework::types::coin_reservation::ParsedDigest;
use framework::types::coin_reservation::ParsedObjectRefWithdrawal;
use framework::types::digests::ChainIdentifier;
use framework::types::digests::get_mainnet_chain_identifier;
use framework::types::digests::get_testnet_chain_identifier;
use framework::types::effects::TransactionEffectsAPI;
use framework::types::execution_status::ExecutionErrorKind;
use framework::types::execution_status::ExecutionStatus;
use framework::types::full_checkpoint_content::Checkpoint;
use framework::types::full_checkpoint_content::ExecutedTransaction;
use framework::types::transaction::TransactionDataAPI;
use sui_indexer_alt_framework as framework;
use tracing::warn;
use url::Url;

mod schema {
    diesel::table! {
        scan_ab_transaction_matches (tx_digest) {
            tx_sequence_number -> Int8,
            checkpoint_sequence_number -> Int8,
            tx_digest -> Text,
        }
    }
}

use schema::scan_ab_transaction_matches;

const DEFAULT_DATABASE_URL: &str =
    "postgres://postgres:postgrespw@localhost:5432/scan_ab_transactions";
/// Mainnet checkpoint at which address-balance gas payments became possible —
/// used as the default start checkpoint when scanning mainnet. Other chains
/// must supply `--first-checkpoint` explicitly.
const DEFAULT_MAINNET_FIRST_CHECKPOINT: u64 = 278_142_335;
const DEFAULT_WRITE_CONCURRENCY: usize = 5;
const INGEST_CONCURRENCY_INITIAL: usize = 50;
const INGEST_CONCURRENCY_MIN: usize = 1;
const INGEST_CONCURRENCY_MAX: usize = 1_000;

const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

/// The chain whose data is being scanned. Selects the chain identifier used to
/// unmask address-balance withdrawal object refs (they are XOR-masked with the
/// genesis digest for cross-chain replay protection), so it must match the
/// checkpoints being ingested or no withdrawals will be recognised.
#[derive(clap::ValueEnum, Clone, Copy, Debug)]
enum Chain {
    Mainnet,
    Testnet,
}

impl Chain {
    fn identifier(self) -> ChainIdentifier {
        match self {
            Chain::Mainnet => get_mainnet_chain_identifier(),
            Chain::Testnet => get_testnet_chain_identifier(),
        }
    }

    /// Default start checkpoint when `--first-checkpoint` is not supplied. Only
    /// mainnet has a known address-balance activation height baked in.
    fn default_first_checkpoint(self) -> Option<u64> {
        match self {
            Chain::Mainnet => Some(DEFAULT_MAINNET_FIRST_CHECKPOINT),
            Chain::Testnet => None,
        }
    }
}

#[derive(Parser, Debug)]
#[command(
    about = "Scan checkpoints for failed early-error txs with non-target address-balance gas payments"
)]
struct Args {
    /// The URL of the database to connect to. Only framework watermarks are stored.
    #[arg(long, default_value = DEFAULT_DATABASE_URL)]
    database_url: Url,

    /// Which chain's checkpoints are being scanned. Must match the ingested
    /// checkpoint source, otherwise address-balance withdrawals won't be recognised.
    #[arg(long, value_enum, default_value_t = Chain::Mainnet)]
    chain: Chain,

    /// Number of concurrent committers writing matches to the database.
    #[arg(long, default_value_t = DEFAULT_WRITE_CONCURRENCY)]
    write_concurrency: usize,

    #[command(flatten)]
    db_args: DbArgs,

    #[command(flatten)]
    cluster_args: framework::cluster::Args,
}

#[derive(Debug, FieldCount, Insertable)]
#[diesel(table_name = scan_ab_transaction_matches)]
struct MatchRecord {
    checkpoint_sequence_number: i64,
    tx_sequence_number: i64,
    tx_digest: String,
}

struct AbGasFailureScan {
    chain_id: ChainIdentifier,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = Args::parse();

    if let Some(default_first_checkpoint) = args.chain.default_first_checkpoint() {
        args.cluster_args
            .indexer_args
            .first_checkpoint
            .get_or_insert(default_first_checkpoint);
    }

    let mut cluster = IndexerCluster::builder()
        .with_database_url(args.database_url)
        .with_db_args(args.db_args)
        .with_args(args.cluster_args)
        .with_migrations(&MIGRATIONS)
        .with_ingestion_config(IngestionConfig {
            ingest_concurrency: IngestConcurrencyConfig::Adaptive {
                initial: INGEST_CONCURRENCY_INITIAL,
                min: INGEST_CONCURRENCY_MIN,
                max: INGEST_CONCURRENCY_MAX,
                dead_band: None,
            },
            ..Default::default()
        })
        .with_metrics_prefix("scan_ab_transactions")
        .build()
        .await?;

    let processor = AbGasFailureScan {
        chain_id: args.chain.identifier(),
    };
    let config = ConcurrentConfig {
        committer: CommitterConfig {
            write_concurrency: args.write_concurrency,
            ..Default::default()
        },
        ..Default::default()
    };

    cluster.concurrent_pipeline(processor, config).await?;

    let service = cluster.run().await?;
    match service.main().await {
        Ok(()) | Err(framework::service::Error::Terminated) => Ok(()),
        Err(error) => Err(anyhow!("indexer failed: {error:?}")),
    }
}

#[async_trait]
impl Processor for AbGasFailureScan {
    const NAME: &'static str = "ab_gas_failure_scan";

    type Value = MatchRecord;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Self::Value>> {
        let checkpoint_sequence_number = checkpoint.summary.sequence_number;
        let first_tx_sequence_number = checkpoint
            .summary
            .network_total_transactions
            .checked_sub(checkpoint.transactions.len() as u64)
            .with_context(|| {
                format!(
                    "checkpoint {} transaction count exceeds network total transactions",
                    checkpoint_sequence_number,
                )
            })?;

        let chain_id = self.chain_id;
        let mut records = Vec::new();
        for (tx_index, tx) in checkpoint.transactions.iter().enumerate() {
            if !has_insufficient_funds_error(tx) {
                continue;
            }

            if !has_non_target_address_balance_gas_payment(tx, chain_id) {
                continue;
            }

            records.push(MatchRecord {
                checkpoint_sequence_number: checkpoint_sequence_number as i64,
                tx_sequence_number: (first_tx_sequence_number + tx_index as u64) as i64,
                tx_digest: tx.effects.transaction_digest().to_string(),
            });
        }

        Ok(records)
    }
}

#[async_trait]
impl Handler for AbGasFailureScan {
    async fn commit<'a>(
        batch: &[MatchRecord],
        conn: &mut framework::postgres::Connection<'a>,
    ) -> anyhow::Result<usize> {
        let written = diesel::insert_into(scan_ab_transaction_matches::table)
            .values(batch)
            .on_conflict_do_nothing()
            .execute(conn)
            .await?;

        for record in batch {
            warn!(
                checkpoint_sequence_number = record.checkpoint_sequence_number,
                tx_sequence_number = record.tx_sequence_number,
                tx_digest = %record.tx_digest,
                "matched transaction",
            );
        }

        Ok(written)
    }
}

fn has_insufficient_funds_error(tx: &ExecutedTransaction) -> bool {
    let ExecutionStatus::Failure(failure) = tx.effects.status() else {
        return false;
    };

    matches!(
        failure.error,
        ExecutionErrorKind::InsufficientFundsForWithdraw
    )
}

/// The first gas payment is a legitimate coin and there are some address balance reservations
/// being smashed into it.
fn has_non_target_address_balance_gas_payment(
    tx: &ExecutedTransaction,
    chain_id: ChainIdentifier,
) -> bool {
    let gas_payments = tx.transaction.gas();
    let Some(((_, _, digest), smashed)) = gas_payments.split_first() else {
        return false;
    };

    ParsedDigest::try_from(*digest).is_err()
        && smashed
            .iter()
            .any(|payment| ParsedObjectRefWithdrawal::parse(payment, chain_id).is_some())
}
