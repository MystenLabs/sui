// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context;
use bootstrap::bootstrap;
use config::{ConsistencyConfig, IndexerConfig, PipelineLayer};
use handlers::{
    ev_emit_mod::EvEmitMod, ev_struct_inst::EvStructInst, kv_checkpoints::KvCheckpoints,
    kv_epoch_ends::KvEpochEnds, kv_epoch_starts::KvEpochStarts, kv_feature_flags::KvFeatureFlags,
    kv_objects::KvObjects, kv_protocol_configs::KvProtocolConfigs, kv_transactions::KvTransactions,
    obj_info::ObjInfo, obj_versions::ObjVersions, sum_coin_balances::SumCoinBalances,
    sum_displays::SumDisplays, sum_obj_types::SumObjTypes, sum_packages::SumPackages,
    tx_affected_addresses::TxAffectedAddresses, tx_affected_objects::TxAffectedObjects,
    tx_balance_changes::TxBalanceChanges, tx_calls::TxCalls, tx_digests::TxDigests,
    tx_kinds::TxKinds, wal_coin_balances::WalCoinBalances, wal_obj_types::WalObjTypes,
};
use models::MIGRATIONS;
use sui_indexer_alt_framework::db::DbArgs;
use sui_indexer_alt_framework::ingestion::{ClientArgs, IngestionConfig};
use sui_indexer_alt_framework::pipeline::{
    concurrent::{ConcurrentConfig, PrunerConfig},
    sequential::SequentialConfig,
    CommitterConfig,
};
use sui_indexer_alt_framework::{Indexer, IndexerArgs};
use tokio_util::sync::CancellationToken;

pub mod args;
pub(crate) mod bootstrap;
pub mod config;
pub(crate) mod handlers;
pub mod models;
pub mod schema;

#[cfg(feature = "benchmark")]
pub mod benchmark;

pub async fn start_indexer(
    db_args: DbArgs,
    indexer_args: IndexerArgs,
    client_args: ClientArgs,
    indexer_config: IndexerConfig,
    // If true, the indexer will bootstrap from genesis.
    // Otherwise it will skip the pipelines that rely on genesis data.
    // TODO: There is probably a better way to handle this.
    // For instance, we could also pass in dummy genesis data in the benchmark mode.
    with_genesis: bool,
) -> anyhow::Result<()> {
    let IndexerConfig {
        ingestion,
        consistency,
        committer,
        pruner,
        pipeline,
        extra: _,
    } = indexer_config.finish();

    let PipelineLayer {
        sum_coin_balances,
        wal_coin_balances,
        sum_obj_types,
        wal_obj_types,
        sum_displays,
        sum_packages,
        ev_emit_mod,
        ev_struct_inst,
        kv_checkpoints,
        kv_epoch_ends,
        kv_epoch_starts,
        kv_feature_flags,
        kv_objects,
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

    let ConsistencyConfig {
        consistent_pruning_interval_ms,
        pruner_delay_ms,
        consistent_range,
    } = consistency.finish(ConsistencyConfig::default());

    let committer = committer.finish(CommitterConfig::default());
    let pruner = pruner.finish(PrunerConfig::default());

    // Pipelines that are split up into a summary table, and a write-ahead log prune their
    // write-ahead log so it contains just enough information to overlap with the summary table.
    let consistent_range = consistent_range.unwrap_or_default();
    let pruner_config = (consistent_range != 0).then(|| PrunerConfig {
        interval_ms: consistent_pruning_interval_ms,
        delay_ms: pruner_delay_ms,
        // Retain at least twice as much data as the lag, to guarantee overlap between the
        // summary table and the write-ahead log.
        retention: consistent_range * 2,
        // Prune roughly five minutes of data in one go.
        max_chunk_size: 5 * 300,
    });

    let cancel = CancellationToken::new();
    let retry_interval = ingestion.retry_interval();

    let mut indexer = Indexer::new(
        db_args,
        indexer_args,
        client_args,
        ingestion,
        &MIGRATIONS,
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
    // There are three kinds of pipeline, each with their own macro: `add_concurrent`,
    // `add_sequential`, and `add_consistent`. `add_concurrent` and `add_sequential` map directly
    // to `Indexer::concurrent_pipeline` and `Indexer::sequential_pipeline` respectively while
    // `add_consistent` is a special case that generates both a sequential "summary" pipeline and a
    // `concurrent` "write-ahead log" pipeline, with their configuration based on the supplied
    // ConsistencyConfig.

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

    macro_rules! add_consistent {
        ($sum_handler:expr, $sum_config:expr; $wal_handler:expr, $wal_config:expr) => {
            if let Some(sum_layer) = $sum_config {
                indexer
                    .sequential_pipeline(
                        $sum_handler,
                        SequentialConfig {
                            committer: sum_layer.finish(committer.clone()),
                            checkpoint_lag: consistent_range,
                        },
                    )
                    .await?;

                if let Some(pruner_config) = pruner_config.clone() {
                    indexer
                        .concurrent_pipeline(
                            $wal_handler,
                            ConcurrentConfig {
                                committer: $wal_config
                                    .unwrap_or_default()
                                    .finish(committer.clone()),
                                pruner: Some(pruner_config),
                            },
                        )
                        .await?;
                }
            }
        };
    }

    if with_genesis {
        let genesis = bootstrap(&indexer, retry_interval, cancel.clone()).await?;

        // Pipelines that rely on genesis information
        add_concurrent!(KvFeatureFlags(genesis.clone()), kv_feature_flags);
        add_concurrent!(KvProtocolConfigs(genesis.clone()), kv_protocol_configs);
    }

    add_consistent!(
        SumCoinBalances, sum_coin_balances;
        WalCoinBalances, wal_coin_balances
    );

    add_consistent!(
        SumObjTypes, sum_obj_types;
        WalObjTypes, wal_obj_types
    );

    // Other summary tables (without write-ahead log)
    add_sequential!(SumDisplays, sum_displays);
    add_sequential!(SumPackages, sum_packages);

    // Unpruned concurrent pipelines
    add_concurrent!(EvEmitMod, ev_emit_mod);
    add_concurrent!(EvStructInst, ev_struct_inst);
    add_concurrent!(KvCheckpoints, kv_checkpoints);
    add_concurrent!(KvEpochEnds, kv_epoch_ends);
    add_concurrent!(KvEpochStarts, kv_epoch_starts);
    add_concurrent!(KvObjects, kv_objects);
    add_concurrent!(KvTransactions, kv_transactions);
    add_concurrent!(ObjInfo, obj_info);
    add_concurrent!(ObjVersions, obj_versions);
    add_concurrent!(TxAffectedAddresses, tx_affected_addresses);
    add_concurrent!(TxAffectedObjects, tx_affected_objects);
    add_concurrent!(TxBalanceChanges, tx_balance_changes);
    add_concurrent!(TxCalls, tx_calls);
    add_concurrent!(TxDigests, tx_digests);
    add_concurrent!(TxKinds, tx_kinds);

    let h_indexer = indexer.run().await.context("Failed to start indexer")?;

    cancel.cancelled().await;
    let _ = h_indexer.await;
    Ok(())
}
