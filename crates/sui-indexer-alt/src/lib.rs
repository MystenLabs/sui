// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context;
use bootstrap::bootstrap;
use config::{IndexerConfig, PipelineLayer};
use handlers::{
    coin_balance_buckets::CoinBalanceBuckets, cp_sequence_numbers::CpSequenceNumbers,
    ev_emit_mod::EvEmitMod, ev_struct_inst::EvStructInst, kv_checkpoints::KvCheckpoints,
    kv_epoch_ends::KvEpochEnds, kv_epoch_starts::KvEpochStarts, kv_feature_flags::KvFeatureFlags,
    kv_objects::KvObjects, kv_packages::KvPackages, kv_protocol_configs::KvProtocolConfigs,
    kv_transactions::KvTransactions, obj_info::ObjInfo, obj_versions::ObjVersions,
    sum_displays::SumDisplays, tx_affected_addresses::TxAffectedAddresses,
    tx_affected_objects::TxAffectedObjects, tx_balance_changes::TxBalanceChanges,
    tx_calls::TxCalls, tx_digests::TxDigests, tx_kinds::TxKinds,
};
use prometheus::Registry;
use sui_indexer_alt_framework::{
    ingestion::{ClientArgs, IngestionConfig},
    pipeline::{
        concurrent::{ConcurrentConfig, PrunerConfig},
        sequential::SequentialConfig,
        CommitterConfig,
    },
    postgres::{Db, DbArgs},
    Indexer, IndexerArgs,
};
use sui_indexer_alt_metrics::db::DbConnectionStatsCollector;
use sui_indexer_alt_schema::MIGRATIONS;
use tokio_util::sync::CancellationToken;
use url::Url;

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
    // If true, the indexer will bootstrap from genesis.
    // Otherwise it will skip the pipelines that rely on genesis data.
    // TODO: There is probably a better way to handle this.
    // For instance, we could also pass in dummy genesis data in the benchmark mode.
    with_genesis: bool,
    registry: &Registry,
    cancel: CancellationToken,
) -> anyhow::Result<Indexer<Db>> {
    let IndexerConfig {
        ingestion,
        committer,
        pruner,
        pipeline,
        extra: _,
    } = indexer_config.finish();

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
        extra: _,
    } = pipeline.finish();

    let ingestion = ingestion.finish(IngestionConfig::default());
    let committer = committer.finish(CommitterConfig::default());
    let pruner = pruner.finish(PrunerConfig::default());

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

    let mut indexer = Indexer::new(
        store,
        indexer_args,
        client_args,
        ingestion,
        registry,
        cancel.clone(),
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
                        }),
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
                        }),
                    )
                    .await?
            }
        };
    }

    if with_genesis {
        let genesis = bootstrap(&indexer, retry_interval, cancel.clone()).await?;

        // Pipelines that rely on genesis information
        add_concurrent!(KvFeatureFlags(genesis.clone()), kv_feature_flags);
        add_concurrent!(KvProtocolConfigs(genesis.clone()), kv_protocol_configs);
    }

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
