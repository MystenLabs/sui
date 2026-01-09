// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context;
use prometheus::Registry;
use sui_indexer_alt_framework::Indexer;
use sui_indexer_alt_framework::IndexerArgs;
use sui_indexer_alt_framework::ingestion::ClientArgs;
use sui_indexer_alt_framework::ingestion::IngestionConfig;
use sui_indexer_alt_framework::pipeline::CommitterConfig;
use sui_indexer_alt_framework::pipeline::concurrent::ConcurrentConfig;
use sui_indexer_alt_framework::pipeline::concurrent::PrunerConfig;
use sui_indexer_alt_framework::pipeline::sequential::SequentialConfig;
use sui_indexer_alt_framework::postgres::Db;
use sui_indexer_alt_framework::postgres::DbArgs;
use sui_indexer_alt_metrics::db::DbConnectionStatsCollector;
use sui_indexer_alt_schema::MIGRATIONS;
use url::Url;

use crate::bootstrap::bootstrap;
use crate::config::IndexerConfig;
use crate::config::PipelineLayer;
use crate::handlers::coin_balance_buckets::CoinBalanceBuckets;
use crate::handlers::cp_sequence_numbers::CpSequenceNumbers;
use crate::handlers::ev_emit_mod::EvEmitMod;
use crate::handlers::ev_struct_inst::EvStructInst;
use crate::handlers::kv_checkpoints::KvCheckpoints;
use crate::handlers::kv_epoch_ends::KvEpochEnds;
use crate::handlers::kv_epoch_starts::KvEpochStarts;
use crate::handlers::kv_feature_flags::KvFeatureFlags;
use crate::handlers::kv_objects::KvObjects;
use crate::handlers::kv_packages::KvPackages;
use crate::handlers::kv_protocol_configs::KvProtocolConfigs;
use crate::handlers::kv_transactions::KvTransactions;
use crate::handlers::obj_info::ObjInfo;
use crate::handlers::obj_versions::ObjVersions;
use crate::handlers::sum_displays::SumDisplays;
use crate::handlers::tx_affected_addresses::TxAffectedAddresses;
use crate::handlers::tx_affected_objects::TxAffectedObjects;
use crate::handlers::tx_balance_changes::TxBalanceChanges;
use crate::handlers::tx_calls::TxCalls;
use crate::handlers::tx_digests::TxDigests;
use crate::handlers::tx_kinds::TxKinds;

pub use crate::bootstrap::BootstrapGenesis;

pub mod args;
#[cfg(feature = "benchmark")]
pub mod benchmark;
pub(crate) mod bootstrap;
pub mod config;
pub(crate) mod handlers;

pub async fn setup_indexer(
    database_url: Url,
    db_args: DbArgs,
    indexer_args: IndexerArgs,
    client_args: ClientArgs,
    indexer_config: IndexerConfig,
    bootstrap_genesis: Option<BootstrapGenesis>,
    registry: &Registry,
) -> anyhow::Result<Indexer<Db>> {
    let IndexerConfig {
        ingestion,
        committer,
        pruner,
        pipeline,
    } = indexer_config;

    let PipelineLayer {
        sum_displays,
        coin_balance_buckets,
        cp_sequence_numbers,
        ev_emit_mod,
        ev_struct_inst,
        kv_checkpoints,
        kv_epoch_ends,
        kv_epoch_starts,
        kv_feature_flags,
        kv_objects,
        kv_packages,
        kv_protocol_configs,
        kv_transactions,
        obj_info,
        obj_versions,
        tx_affected_addresses,
        tx_affected_objects,
        tx_balance_changes,
        tx_calls,
        tx_digests,
        tx_kinds,
    } = pipeline;

    let ingestion = ingestion.finish(IngestionConfig::default())?;
    let committer = committer.finish(CommitterConfig::default())?;
    let pruner = pruner.finish(PrunerConfig::default())?;

    let retry_interval = ingestion.retry_interval();

    // Prepare the store for the indexer
    let store = Db::for_write(database_url, db_args)
        .await
        .context("Failed to connect to database")?;

    // we want to merge &MIGRATIONS with the migrations from the store
    store
        .run_migrations(Some(&MIGRATIONS))
        .await
        .context("Failed to run pending migrations")?;

    registry.register(Box::new(DbConnectionStatsCollector::new(
        Some("indexer_db"),
        store.clone(),
    )))?;

    let metrics_prefix = None;
    let mut indexer = Indexer::new(
        store,
        indexer_args,
        client_args,
        ingestion,
        metrics_prefix,
        registry,
    )
    .await?;

    // These macros are responsible for registering pipelines with the indexer. It is responsible
    // for:
    //
    //  - Checking whether the pipeline is enabled in the file-based configuration.
    //  - Checking for unexpected parameters in the config.
    //  - Combining shared and per-pipeline configurations.
    //  - Registering the pipeline with the indexer.
    //
    // There are two kinds of pipelines, each with their own macro: `add_concurrent` and
    // `add_sequential`. They map directly to `Indexer::concurrent_pipeline` and
    // `Indexer::sequential_pipeline` respectively.

    macro_rules! add_concurrent {
        ($handler:expr, $config:expr) => {
            if let Some(layer) = $config {
                indexer
                    .concurrent_pipeline(
                        $handler,
                        layer.finish(ConcurrentConfig {
                            committer: committer.clone(),
                            pruner: Some(pruner.clone()),
                        })?,
                    )
                    .await?
            }
        };
    }

    macro_rules! add_sequential {
        ($handler:expr, $config:expr) => {
            if let Some(layer) = $config {
                indexer
                    .sequential_pipeline(
                        $handler,
                        layer.finish(SequentialConfig {
                            committer: committer.clone(),
                            ..Default::default()
                        })?,
                    )
                    .await?
            }
        };
    }

    let genesis = bootstrap(&indexer, retry_interval, bootstrap_genesis).await?;

    // Pipelines that rely on genesis information
    add_concurrent!(KvFeatureFlags(genesis.clone()), kv_feature_flags);
    add_concurrent!(KvProtocolConfigs(genesis.clone()), kv_protocol_configs);

    // Summary tables (without write-ahead log)
    add_sequential!(SumDisplays, sum_displays);

    // Concurrent pipelines with retention
    add_concurrent!(CoinBalanceBuckets, coin_balance_buckets);
    add_concurrent!(ObjInfo, obj_info);

    // Unpruned concurrent pipelines
    add_concurrent!(CpSequenceNumbers, cp_sequence_numbers);
    add_concurrent!(EvEmitMod, ev_emit_mod);
    add_concurrent!(EvStructInst, ev_struct_inst);
    add_concurrent!(KvCheckpoints, kv_checkpoints);
    add_concurrent!(KvEpochEnds, kv_epoch_ends);
    add_concurrent!(KvEpochStarts, kv_epoch_starts);
    add_concurrent!(KvObjects, kv_objects);
    add_concurrent!(KvPackages, kv_packages);
    add_concurrent!(KvTransactions, kv_transactions);
    add_concurrent!(ObjVersions, obj_versions);
    add_concurrent!(TxAffectedAddresses, tx_affected_addresses);
    add_concurrent!(TxAffectedObjects, tx_affected_objects);
    add_concurrent!(TxBalanceChanges, tx_balance_changes);
    add_concurrent!(TxCalls, tx_calls);
    add_concurrent!(TxDigests, tx_digests);
    add_concurrent!(TxKinds, tx_kinds);

    Ok(indexer)
}
