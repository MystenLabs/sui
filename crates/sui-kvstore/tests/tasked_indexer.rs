// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! A tasked indexer runs selected concurrent pipelines under their own watermark rows, e.g. to
//! backfill a pipeline while the main indexer keeps it at the tip. Requires `gcloud`, `cbt`, and
//! the BigTable emulator on PATH.

use std::time::Duration;

use sui_indexer_alt_framework::IndexerArgs;
use sui_indexer_alt_framework::TaskArgs;
use sui_indexer_alt_framework::ingestion::ClientArgs;
use sui_indexer_alt_framework::ingestion::IngestionConfig;
use sui_indexer_alt_framework::ingestion::ingestion_client::IngestionClientArgs;
use sui_kvstore::BigTableClient;
use sui_kvstore::BigTableIndexer;
use sui_kvstore::IndexerConfig;
use sui_kvstore::OBJECT_TOMBSTONES_PIPELINE;
use sui_kvstore::PipelineLayer;
use sui_kvstore::default_committer_config;
use sui_kvstore::testing::BigTableEmulator;
use sui_kvstore::testing::INSTANCE_ID;
use sui_kvstore::testing::create_tables;
use sui_kvstore::testing::require_bigtable_emulator;
use sui_protocol_config::Chain;
use sui_synthetic_ingestion::synthetic_ingestion;

/// The always-registered sequential bitmap pipelines do not stop a tasked indexer that
/// `--pipeline` limits to concurrent pipelines. It records progress under
/// `{pipeline}@{task}` and leaves the pipeline's main watermark row alone.
#[tokio::test]
async fn tasked_indexer_runs_selected_concurrent_pipeline() {
    require_bigtable_emulator();
    let emulator = tokio::task::spawn_blocking(BigTableEmulator::start)
        .await
        .unwrap()
        .unwrap();
    create_tables(emulator.host(), INSTANCE_ID).await.unwrap();
    let mut client =
        BigTableClient::new_local(emulator.host().to_string(), INSTANCE_ID.to_string())
            .await
            .unwrap();

    const LAST_CHECKPOINT: u64 = 4;
    let ingestion_dir = tempfile::tempdir().unwrap();
    synthetic_ingestion::generate_ingestion(synthetic_ingestion::Config {
        ingestion_dir: ingestion_dir.path().to_owned(),
        starting_checkpoint: 0,
        num_checkpoints: LAST_CHECKPOINT + 1,
        checkpoint_size: 1,
    })
    .await;

    let indexer_args = IndexerArgs {
        last_checkpoint: Some(LAST_CHECKPOINT),
        pipeline: vec![OBJECT_TOMBSTONES_PIPELINE.to_string()],
        task: TaskArgs::tasked("backfill".to_string(), 3_600_000),
        ..Default::default()
    };
    let client_args = ClientArgs {
        ingestion: IngestionClientArgs {
            local_ingestion_path: Some(ingestion_dir.path().to_owned()),
            ..Default::default()
        },
        ..Default::default()
    };
    let indexer = BigTableIndexer::new(
        client.clone(),
        indexer_args,
        client_args,
        IngestionConfig::default().into(),
        default_committer_config(),
        IndexerConfig::default(),
        PipelineLayer::default(),
        &[OBJECT_TOMBSTONES_PIPELINE],
        Chain::Unknown,
        &prometheus::Registry::new(),
    )
    .await
    .unwrap();

    assert_eq!(indexer.pipeline_names(), [OBJECT_TOMBSTONES_PIPELINE]);

    let task_watermark = format!("{OBJECT_TOMBSTONES_PIPELINE}@backfill");
    assert_eq!(
        indexer.pipeline_tasks(),
        std::slice::from_ref(&task_watermark)
    );

    let service = indexer.run().await.unwrap();
    tokio::time::timeout(Duration::from_secs(60), service.main())
        .await
        .expect("bounded tasked indexer did not finish")
        .unwrap();

    let watermarks = client
        .get_pipeline_watermarks(&[OBJECT_TOMBSTONES_PIPELINE, &task_watermark])
        .await
        .unwrap();
    assert_eq!(
        watermarks[&task_watermark].checkpoint_hi_inclusive,
        Some(LAST_CHECKPOINT)
    );
    assert!(
        !watermarks.contains_key(OBJECT_TOMBSTONES_PIPELINE),
        "the main watermark row was written"
    );
}
