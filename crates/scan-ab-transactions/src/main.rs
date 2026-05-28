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
use framework::pipeline::Processor;
use framework::pipeline::concurrent::ConcurrentConfig;
use framework::postgres::DbArgs;
use framework::postgres::handler::Handler;
use framework::types::coin_reservation::ParsedDigest;
use framework::types::coin_reservation::ParsedObjectRefWithdrawal;
use framework::types::digests::ChainIdentifier;
use framework::types::digests::get_mainnet_chain_identifier;
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
const DEFAULT_FIRST_CHECKPOINT: u64 = 278_142_335;
const INGEST_CONCURRENCY_INITIAL: usize = 50;
const INGEST_CONCURRENCY_MIN: usize = 1;
const INGEST_CONCURRENCY_MAX: usize = 1_000;

const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

#[derive(Parser, Debug)]
#[command(
    about = "Scan checkpoints for failed early-error txs with non-target address-balance gas payments"
)]
struct Args {
    /// The URL of the database to connect to. Only framework watermarks are stored.
    #[arg(long, default_value = DEFAULT_DATABASE_URL)]
    database_url: Url,

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

struct AbGasFailureScan;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut args = Args::parse();

    args.cluster_args
        .indexer_args
        .first_checkpoint
        .get_or_insert(DEFAULT_FIRST_CHECKPOINT);

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

    cluster
        .concurrent_pipeline(AbGasFailureScan, ConcurrentConfig::default())
        .await?;

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

        let chain_id = get_mainnet_chain_identifier();
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
