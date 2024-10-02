// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use jsonrpsee::core::RpcResult;
use simulacrum::Simulacrum;
use std::sync::Arc;
use sui_indexer::apis::read_api::ReadApi;
use sui_indexer::indexer_reader::IndexerReader;
use sui_indexer::test_utils::{set_up, wait_for_checkpoint};
use sui_json_rpc_api::ReadApiServer;
use tempfile::tempdir;

#[tokio::test]
async fn test_checkpoint_apis() -> RpcResult<()> {
    let tempdir = tempdir().unwrap();
    let mut sim = Simulacrum::new();
    let data_ingestion_path = tempdir.path().to_path_buf();
    sim.set_data_ingestion_path(data_ingestion_path.clone());
    sim.create_checkpoint();
    sim.create_checkpoint();

    let (_, pg_store, _, _database) = set_up(Arc::new(sim), data_ingestion_path).await;
    wait_for_checkpoint(&pg_store, 2).await.unwrap();

    // Test get_latest_checkpoint_sequence_number
    let read_api = ReadApi::new(IndexerReader::new(pg_store.pool()));
    let latest_checkpoint = read_api.get_latest_checkpoint_sequence_number().await?;
    assert_eq!(latest_checkpoint.into_inner(), 2);

    // Test get_checkpoint
    let checkpoint_id = sui_json_rpc_types::CheckpointId::SequenceNumber(1);
    let checkpoint = read_api.get_checkpoint(checkpoint_id).await?;
    assert_eq!(checkpoint.sequence_number, 1);

    // Test get_checkpoints
    let checkpoints = read_api.get_checkpoints(None, Some(10), false).await?;
    assert_eq!(checkpoints.data.len(), 3); // 0, 1, 2
    assert!(!checkpoints.has_next_page);
    assert_eq!(checkpoints.next_cursor, Some(2.into()));

    let checkpoints = read_api
        .get_checkpoints(Some(2.into()), Some(2), true)
        .await?;
    assert_eq!(checkpoints.data.len(), 2);
    assert!(!checkpoints.has_next_page);
    assert_eq!(checkpoints.next_cursor, Some(0.into()));
    assert_eq!(checkpoints.data[0].sequence_number, 1);
    assert_eq!(checkpoints.data[1].sequence_number, 0);
    Ok(())
}
