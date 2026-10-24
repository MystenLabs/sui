// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    state_sync::{
        Builder, GetCheckpointSummaryRequest, PeerStateSyncInfo, StateSync, StateSyncMessage,
        UnstartedStateSync,
    },
    utils::build_network,
};
use anemo::{PeerId, Request};
use anyhow::anyhow;
use mysten_common::ZipDebugEqIteratorExt;
use prost::Message;
use std::num::NonZeroUsize;
use std::{
    collections::HashMap,
    time::{Duration, Instant as StdInstant},
};
use sui_config::node::ArchiveReaderConfig;
use sui_config::object_storage_config::ObjectStoreConfig;
use sui_config::p2p::StateSyncConfig;
use sui_rpc::field::{FieldMask, FieldMaskUtil};
use sui_rpc::merge::Merge;
use sui_rpc::proto::sui::rpc;
use sui_swarm_config::test_utils::{CommitteeFixture, empty_contents};
use sui_types::full_checkpoint_content::CheckpointData;
use sui_types::{
    messages_checkpoint::CheckpointDigest,
    storage::{ReadStore, SharedInMemoryStore, WriteStore},
};
use tempfile::tempdir;
use tokio::time::{Instant, timeout};

#[tokio::test]
async fn server_push_checkpoint() {
    let committee = CommitteeFixture::generate(rand::rngs::OsRng, 0, 4);
    let (ordered_checkpoints, _, _sequence_number_to_digest, _checkpoints) =
        committee.make_empty_checkpoints(2, None);
    let store = SharedInMemoryStore::default();
    store.inner_mut().insert_genesis_state(
        ordered_checkpoints.first().cloned().unwrap(),
        empty_contents(),
        committee.committee().to_owned(),
    );

    let (
        UnstartedStateSync {
            handle: _handle,
            mut mailbox,
            peer_heights,
            ..
        },
        server,
    ) = Builder::new()
        .store(store)
        .config(StateSyncConfig::randomized_for_testing())
        .build_internal();
    let peer_id = PeerId([9; 32]); // fake PeerId

    peer_heights.write().unwrap().peers.insert(
        peer_id,
        PeerStateSyncInfo {
            genesis_checkpoint_digest: *ordered_checkpoints[0].digest(),
            on_same_chain_as_us: true,
            height: 0,
            lowest: 0,
        },
    );

    let checkpoint = ordered_checkpoints[1].inner().to_owned();
    let request = Request::new(checkpoint.clone()).with_extension(peer_id);
    server.push_checkpoint_summary(request).await.unwrap();

    assert_eq!(
        peer_heights.read().unwrap().peers.get(&peer_id),
        Some(&PeerStateSyncInfo {
            genesis_checkpoint_digest: *ordered_checkpoints[0].digest(),
            on_same_chain_as_us: true,
            height: 1,
            lowest: 0,
        })
    );
    assert_eq!(
        peer_heights
            .read()
            .unwrap()
            .unprocessed_checkpoints
            .get(checkpoint.digest())
            .unwrap()
            .data(),
        checkpoint.data(),
    );
    assert_eq!(
        peer_heights
            .read()
            .unwrap()
            .highest_known_checkpoint_sequence_number(),
        Some(*checkpoint.sequence_number()),
    );
    assert!(matches!(
        mailbox.try_recv().unwrap(),
        StateSyncMessage::StartSyncJob
    ));
}

#[tokio::test]
async fn server_get_checkpoint() {
    let committee = CommitteeFixture::generate(rand::rngs::OsRng, 0, 4);
    let (ordered_checkpoints, _, _sequence_number_to_digest, _checkpoints) =
        committee.make_empty_checkpoints(3, None);

    let (builder, server) = Builder::new()
        .store(SharedInMemoryStore::default())
        .config(StateSyncConfig::randomized_for_testing())
        .build_internal();

    builder.store.inner_mut().insert_genesis_state(
        ordered_checkpoints.first().cloned().unwrap(),
        empty_contents(),
        committee.committee().to_owned(),
    );

    // Requests for the Latest checkpoint should return the genesis checkpoint
    let response = server
        .get_checkpoint_summary(Request::new(GetCheckpointSummaryRequest::Latest))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(
        response.unwrap().data(),
        ordered_checkpoints.first().unwrap().data(),
    );

    // Requests for checkpoints that aren't in the server's store
    let requests = [
        GetCheckpointSummaryRequest::BySequenceNumber(9),
        GetCheckpointSummaryRequest::ByDigest(CheckpointDigest::new([10; 32])),
    ];
    for request in requests {
        let response = server
            .get_checkpoint_summary(Request::new(request))
            .await
            .unwrap()
            .into_inner();
        assert!(response.is_none());
    }

    // Populate the node's store with some checkpoints
    for checkpoint in ordered_checkpoints.clone() {
        builder.store.inner_mut().insert_checkpoint(&checkpoint)
    }
    let latest = ordered_checkpoints.last().unwrap().clone();
    builder
        .store
        .inner_mut()
        .update_highest_synced_checkpoint(&latest);

    let request = Request::new(GetCheckpointSummaryRequest::Latest);
    let response = server
        .get_checkpoint_summary(request)
        .await
        .unwrap()
        .into_inner()
        .unwrap();
    assert_eq!(response.data(), latest.data());

    for checkpoint in ordered_checkpoints {
        let request = Request::new(GetCheckpointSummaryRequest::ByDigest(*checkpoint.digest()));
        let response = server
            .get_checkpoint_summary(request)
            .await
            .unwrap()
            .into_inner()
            .unwrap();
        assert_eq!(response.data(), checkpoint.data());

        let request = Request::new(GetCheckpointSummaryRequest::BySequenceNumber(
            *checkpoint.sequence_number(),
        ));
        let response = server
            .get_checkpoint_summary(request)
            .await
            .unwrap()
            .into_inner()
            .unwrap();
        assert_eq!(response.data(), checkpoint.data());
    }
}

#[tokio::test]
async fn isolated_sync_job() {
    let committee = CommitteeFixture::generate(rand::rngs::OsRng, 0, 4);
    // build mock data
    let (ordered_checkpoints, _, sequence_number_to_digest, checkpoints) =
        committee.make_empty_checkpoints(100, None);

    // Build and connect two nodes
    let (builder, state_sync_router) = Builder::new()
        .store(SharedInMemoryStore::default())
        .config(StateSyncConfig::randomized_for_testing())
        .build();
    let network_1 = build_network(|router| router.merge(state_sync_router));
    let (mut event_loop_1, _handle_1) = builder.build(network_1.clone());
    let (builder, state_sync_router) = Builder::new()
        .store(SharedInMemoryStore::default())
        .config(StateSyncConfig::randomized_for_testing())
        .build();
    let network_2 = build_network(|router| router.merge(state_sync_router));
    let (event_loop_2, _handle_2) = builder.build(network_2.clone());
    network_1.connect(network_2.local_addr()).await.unwrap();

    // Init the root committee in both nodes
    event_loop_1.store.inner_mut().insert_genesis_state(
        ordered_checkpoints.first().cloned().unwrap(),
        empty_contents(),
        committee.committee().to_owned(),
    );
    event_loop_2.store.inner_mut().insert_genesis_state(
        ordered_checkpoints.first().cloned().unwrap(),
        empty_contents(),
        committee.committee().to_owned(),
    );

    // Node 2 will have all the data
    {
        let mut store = event_loop_2.store.inner_mut();
        for checkpoint in ordered_checkpoints.clone() {
            store.insert_checkpoint(&checkpoint);
        }
    }

    // Node 1 will know that Node 2 has the data
    event_loop_1.peer_heights.write().unwrap().peers.insert(
        network_2.peer_id(),
        PeerStateSyncInfo {
            genesis_checkpoint_digest: *ordered_checkpoints[0].digest(),
            on_same_chain_as_us: true,
            height: *ordered_checkpoints.last().unwrap().sequence_number(),
            lowest: 0,
        },
    );
    event_loop_1
        .peer_heights
        .write()
        .unwrap()
        .insert_checkpoint(ordered_checkpoints.last().cloned().unwrap().into_inner());

    // Sync the data
    event_loop_1.maybe_start_checkpoint_summary_sync_task();
    event_loop_1.tasks.join_next().await.unwrap().unwrap();
    assert_eq!(
        ordered_checkpoints.last().map(|x| x.data()),
        Some(
            event_loop_1
                .store
                .get_highest_verified_checkpoint()
                .unwrap()
                .data()
        )
    );

    {
        let store = event_loop_1.store.inner();
        let expected = checkpoints
            .iter()
            .map(|(key, value)| (key, value.data()))
            .collect::<HashMap<_, _>>();
        let actual = store
            .checkpoints()
            .iter()
            .map(|(key, value)| (key, value.data()))
            .collect::<HashMap<_, _>>();
        assert_eq!(actual, expected);
        assert_eq!(
            store.checkpoint_sequence_number_to_digest(),
            &sequence_number_to_digest
        );
    }
}

#[tokio::test]
async fn test_state_sync_using_archive() -> anyhow::Result<()> {
    let committee = CommitteeFixture::generate(rand::rngs::OsRng, 0, 4);
    // build mock data
    let (ordered_checkpoints, ordered_contents, sequence_number_to_digest, checkpoints) =
        committee.make_empty_checkpoints(100, None);
    let temp_dir = tempdir()?.keep();
    // We will delete all checkpoints older than this checkpoint on Node 2
    let oldest_checkpoint_to_keep: u64 = 10;

    // Populate the local directory with checkpoint files
    // It will be used as a checkpoint bucket
    for (idx, summary) in ordered_checkpoints.iter().enumerate() {
        let chk = CheckpointData {
            checkpoint_summary: summary.clone().into(),
            checkpoint_contents: ordered_contents[idx].clone().into_checkpoint_contents(),
            transactions: vec![],
        };
        let checkpoint: sui_types::full_checkpoint_content::Checkpoint = chk.into();
        let mask = FieldMask::from_paths([
            rpc::v2::Checkpoint::path_builder().sequence_number(),
            rpc::v2::Checkpoint::path_builder().summary().bcs().value(),
            rpc::v2::Checkpoint::path_builder().signature().finish(),
            rpc::v2::Checkpoint::path_builder().contents().bcs().value(),
        ]);
        let proto_checkpoint = rpc::v2::Checkpoint::merge_from(&checkpoint, &mask.into());
        let proto_bytes = proto_checkpoint.encode_to_vec();
        let compressed = zstd::encode_all(&proto_bytes[..], 3)?;
        let file_path = temp_dir.join(format!("{}.binpb.zst", summary.sequence_number));
        std::fs::write(file_path, compressed)?;
    }
    let archive_reader_config = ArchiveReaderConfig {
        remote_store_config: ObjectStoreConfig::default(),
        download_concurrency: NonZeroUsize::new(1).unwrap(),
        ingestion_url: Some(format!("file://{}", temp_dir.display())),
        remote_store_options: vec![],
        remote_store_headers: vec![],
    };
    // Build and connect two nodes where Node 1 will be given access to an archive store
    // Node 2 will prune older checkpoints, so Node 1 is forced to backfill from the archive
    let (builder, state_sync_router) = Builder::new()
        .store(SharedInMemoryStore::default())
        .config(StateSyncConfig::randomized_for_testing())
        .archive_config(Some(archive_reader_config))
        .build();
    let network_1 = build_network(|router| router.merge(state_sync_router));
    let (event_loop_1, _handle_1) = builder.build(network_1.clone());
    let (builder, state_sync_router) = Builder::new()
        .store(SharedInMemoryStore::default())
        .config(StateSyncConfig::randomized_for_testing())
        .build();
    let network_2 = build_network(|router| router.merge(state_sync_router));
    let (event_loop_2, _handle_2) = builder.build(network_2.clone());
    network_1.connect(network_2.local_addr()).await.unwrap();

    // Init the root committee in both nodes
    event_loop_1.store.inner_mut().insert_genesis_state(
        ordered_checkpoints.first().cloned().unwrap(),
        empty_contents(),
        committee.committee().to_owned(),
    );
    event_loop_2.store.inner_mut().insert_genesis_state(
        ordered_checkpoints.first().cloned().unwrap(),
        empty_contents(),
        committee.committee().to_owned(),
    );

    // Node 2 will have all the data at first
    {
        let mut store = event_loop_2.store.inner_mut();
        for checkpoint in ordered_checkpoints.clone() {
            store.insert_checkpoint(&checkpoint);
            store.insert_checkpoint_contents(&checkpoint, empty_contents());
            store.update_highest_synced_checkpoint(&checkpoint);
        }
    }
    // Prune first 10 checkpoint contents from Node 2
    {
        let mut store = event_loop_2.store.inner_mut();
        for checkpoint in &ordered_checkpoints[0..(oldest_checkpoint_to_keep as usize)] {
            store.delete_checkpoint_content_test_only(checkpoint.sequence_number)?;
        }
        // Now Node 2 has deleted checkpoint contents from range [0, 10) on local store
        assert_eq!(
            store.get_lowest_available_checkpoint(),
            oldest_checkpoint_to_keep
        );
        assert_eq!(
            store
                .get_highest_synced_checkpoint()
                .unwrap()
                .sequence_number,
            ordered_checkpoints.last().unwrap().sequence_number
        );
        assert_eq!(
            store
                .get_highest_verified_checkpoint()
                .unwrap()
                .sequence_number,
            ordered_checkpoints.last().unwrap().sequence_number
        );
    }

    // Node 1 will know that Node 2 has the data starting checkpoint 10
    event_loop_1.peer_heights.write().unwrap().peers.insert(
        network_2.peer_id(),
        PeerStateSyncInfo {
            genesis_checkpoint_digest: *ordered_checkpoints[0].digest(),
            on_same_chain_as_us: true,
            height: *ordered_checkpoints.last().unwrap().sequence_number(),
            lowest: oldest_checkpoint_to_keep,
        },
    );

    // Get handle to node 1 store
    let store_1 = event_loop_1.store.clone();

    // Sync the data
    // Start both event loops
    tokio::spawn(event_loop_1.start());
    tokio::spawn(event_loop_2.start());

    let total_time = Instant::now();
    loop {
        {
            let store = store_1.inner();
            if let Some(highest_synced_checkpoint) = store.get_highest_synced_checkpoint()
                && highest_synced_checkpoint.sequence_number
                    == ordered_checkpoints.last().unwrap().sequence_number
            {
                // Node 1 is fully synced to the latest checkpoint on Node 2
                let expected = checkpoints
                    .iter()
                    .map(|(key, value)| (key, value.data()))
                    .collect::<HashMap<_, _>>();
                let actual = store
                    .checkpoints()
                    .iter()
                    .map(|(key, value)| (key, value.data()))
                    .collect::<HashMap<_, _>>();
                assert_eq!(actual, expected);
                assert_eq!(
                    store.checkpoint_sequence_number_to_digest(),
                    &sequence_number_to_digest
                );
                break;
            }
        }
        if total_time.elapsed() > Duration::from_secs(120) {
            return Err(anyhow!("Test timed out"));
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    Ok(())
}

#[tokio::test]
async fn sync_with_checkpoints_being_inserted() {
    telemetry_subscribers::init_for_testing();
    let committee = CommitteeFixture::generate(rand::rngs::OsRng, 0, 4);
    // build mock data
    let (ordered_checkpoints, _contents, sequence_number_to_digest, checkpoints) =
        committee.make_empty_checkpoints(4, None);

    // Build and connect two nodes
    let (builder, state_sync_router) = Builder::new()
        .store(SharedInMemoryStore::default())
        .config(StateSyncConfig::randomized_for_testing())
        .build();
    let network_1 = build_network(|router| router.merge(state_sync_router));
    let (event_loop_1, handle_1) = builder.build(network_1.clone());
    let (builder, state_sync_router) = Builder::new()
        .store(SharedInMemoryStore::default())
        .config(StateSyncConfig::randomized_for_testing())
        .build();
    let network_2 = build_network(|router| router.merge(state_sync_router));
    let (event_loop_2, handle_2) = builder.build(network_2.clone());
    network_1.connect(network_2.local_addr()).await.unwrap();

    // Init the root committee in both nodes
    event_loop_1.store.inner_mut().insert_genesis_state(
        ordered_checkpoints.first().cloned().unwrap(),
        empty_contents(),
        committee.committee().to_owned(),
    );
    event_loop_2.store.inner_mut().insert_genesis_state(
        ordered_checkpoints.first().cloned().unwrap(),
        empty_contents(),
        committee.committee().to_owned(),
    );

    // get handles to each node's stores
    let store_1 = event_loop_1.store.clone();
    let store_2 = event_loop_2.store.clone();
    // make sure that node_1 knows about node_2
    event_loop_1.peer_heights.write().unwrap().peers.insert(
        network_2.peer_id(),
        PeerStateSyncInfo {
            genesis_checkpoint_digest: *ordered_checkpoints[0].digest(),
            on_same_chain_as_us: true,
            height: 0,
            lowest: 0,
        },
    );
    // Start both event loops
    tokio::spawn(event_loop_1.start());
    tokio::spawn(event_loop_2.start());

    let mut subscriber_1 = handle_1.subscribe_to_synced_checkpoints();
    let mut subscriber_2 = handle_2.subscribe_to_synced_checkpoints();

    // Inject one checkpoint and verify that it was shared with the other node
    let mut checkpoint_iter = ordered_checkpoints.clone().into_iter().skip(1);
    let checkpoint = checkpoint_iter.next().unwrap();
    store_1
        .insert_checkpoint_contents(&checkpoint, empty_contents())
        .unwrap();
    store_1.insert_certified_checkpoint(&checkpoint);
    handle_1.send_checkpoint(checkpoint).await;

    timeout(Duration::from_secs(1), async {
        assert_eq!(
            subscriber_1.recv().await.unwrap().data(),
            ordered_checkpoints[1].data(),
        );
        assert_eq!(
            subscriber_2.recv().await.unwrap().data(),
            ordered_checkpoints[1].data()
        );
    })
    .await
    .unwrap();

    // Inject all the checkpoints
    for checkpoint in checkpoint_iter {
        store_1.insert_certified_checkpoint(&checkpoint);
        handle_1.send_checkpoint(checkpoint).await;
    }

    timeout(Duration::from_secs(1), async {
        for checkpoint in &ordered_checkpoints[2..] {
            assert_eq!(subscriber_1.recv().await.unwrap().data(), checkpoint.data());
            assert_eq!(subscriber_2.recv().await.unwrap().data(), checkpoint.data());
        }
    })
    .await
    .unwrap();

    let store_1 = store_1.inner();
    let store_2 = store_2.inner();
    assert_eq!(
        ordered_checkpoints.last().map(|x| x.digest()),
        store_1
            .get_highest_verified_checkpoint()
            .as_ref()
            .map(|x| x.digest())
    );
    assert_eq!(
        ordered_checkpoints.last().map(|x| x.digest()),
        store_2
            .get_highest_verified_checkpoint()
            .as_ref()
            .map(|x| x.digest())
    );

    let expected = checkpoints
        .iter()
        .map(|(key, value)| (key, value.data()))
        .collect::<HashMap<_, _>>();
    let actual_1 = store_1
        .checkpoints()
        .iter()
        .map(|(key, value)| (key, value.data()))
        .collect::<HashMap<_, _>>();
    assert_eq!(actual_1, expected);
    assert_eq!(
        store_1.checkpoint_sequence_number_to_digest(),
        &sequence_number_to_digest
    );

    let actual_2 = store_2
        .checkpoints()
        .iter()
        .map(|(key, value)| (key, value.data()))
        .collect::<HashMap<_, _>>();
    assert_eq!(actual_2, expected);
    assert_eq!(
        store_2.checkpoint_sequence_number_to_digest(),
        &sequence_number_to_digest
    );
}

fn watermark_test_config(hedge_delay_ms: Option<u64>) -> StateSyncConfig {
    StateSyncConfig {
        checkpoint_content_hedge_delay_ms: hedge_delay_ms.and_then(std::num::NonZeroU64::new),
        ..StateSyncConfig::randomized_for_testing()
    }
}

#[tokio::test]
async fn sync_with_checkpoints_watermark() {
    run_sync_with_checkpoints_watermark(None).await;
}

#[tokio::test]
async fn sync_with_checkpoints_watermark_hedged() {
    run_sync_with_checkpoints_watermark(Some(100)).await;
}

async fn run_sync_with_checkpoints_watermark(hedge_delay_ms: Option<u64>) {
    telemetry_subscribers::init_for_testing();
    let committee = CommitteeFixture::generate(rand::rngs::OsRng, 0, 4);
    // build mock data
    let (ordered_checkpoints, contents, _sequence_number_to_digest, _checkpoints) =
        committee.make_random_checkpoints(4, None);
    let last_checkpoint_seq = *ordered_checkpoints
        .last()
        .cloned()
        .unwrap()
        .sequence_number();
    // Build and connect two nodes
    let (builder, state_sync_router) = Builder::new()
        .store(SharedInMemoryStore::default())
        .config(watermark_test_config(hedge_delay_ms))
        .build();
    let network_1 = build_network(|router| router.merge(state_sync_router));
    let (event_loop_1, handle_1) = builder.build(network_1.clone());
    let (builder, state_sync_router) = Builder::new()
        .store(SharedInMemoryStore::default())
        .config(watermark_test_config(hedge_delay_ms))
        .build();
    let network_2 = build_network(|router| router.merge(state_sync_router));
    let (event_loop_2, handle_2) = builder.build(network_2.clone());

    // Init the root committee in both nodes
    let genesis_checkpoint_content = contents.first().cloned().unwrap();
    event_loop_1.store.inner_mut().insert_genesis_state(
        ordered_checkpoints.first().cloned().unwrap(),
        genesis_checkpoint_content.clone(),
        committee.committee().to_owned(),
    );
    event_loop_2.store.inner_mut().insert_genesis_state(
        ordered_checkpoints.first().cloned().unwrap(),
        genesis_checkpoint_content.clone(),
        committee.committee().to_owned(),
    );

    // get handles to each node's stores
    let store_1 = event_loop_1.store.clone();
    let store_2 = event_loop_2.store.clone();
    let peer_id_1 = network_1.peer_id();

    let peer_heights_1 = event_loop_1.peer_heights.clone();
    let peer_heights_2 = event_loop_2.peer_heights.clone();
    peer_heights_1
        .write()
        .unwrap()
        .set_wait_interval_when_no_peer_to_sync_content(Duration::from_secs(1));
    peer_heights_2
        .write()
        .unwrap()
        .set_wait_interval_when_no_peer_to_sync_content(Duration::from_secs(1));

    // Start both event loops
    tokio::spawn(event_loop_1.start());
    tokio::spawn(event_loop_2.start());

    let mut subscriber_1 = handle_1.subscribe_to_synced_checkpoints();
    let mut subscriber_2 = handle_2.subscribe_to_synced_checkpoints();

    network_1.connect(network_2.local_addr()).await.unwrap();

    // Inject one checkpoint and verify that it was shared with the other node
    let mut checkpoint_iter = ordered_checkpoints.clone().into_iter().skip(1);
    let mut contents_iter = contents.clone().into_iter().skip(1);
    let checkpoint_1 = checkpoint_iter.next().unwrap();
    let contents_1 = contents_iter.next().unwrap();
    let checkpoint_seq = checkpoint_1.sequence_number();
    store_1
        .insert_checkpoint_contents(&checkpoint_1, contents_1.clone())
        .unwrap();
    store_1.insert_certified_checkpoint(&checkpoint_1);
    handle_1.send_checkpoint(checkpoint_1.clone()).await;

    timeout(Duration::from_secs(3), async {
        assert_eq!(
            subscriber_1.recv().await.unwrap().data(),
            ordered_checkpoints[1].data(),
        );
        assert_eq!(
            subscriber_2.recv().await.unwrap().data(),
            ordered_checkpoints[1].data()
        );
    })
    .await
    .unwrap();

    assert_eq!(
        store_1
            .get_highest_synced_checkpoint()
            .unwrap()
            .sequence_number(),
        checkpoint_seq
    );
    assert_eq!(
        store_2
            .get_highest_synced_checkpoint()
            .unwrap()
            .sequence_number(),
        checkpoint_seq
    );
    assert_eq!(
        store_1
            .get_highest_verified_checkpoint()
            .unwrap()
            .sequence_number(),
        &1
    );
    assert_eq!(
        store_2
            .get_highest_verified_checkpoint()
            .unwrap()
            .sequence_number(),
        &1
    );

    // So far so good.
    // Now we increase Peer 1's low watermark to a high number.
    let a_very_high_checkpoint_seq = 1000;
    store_1
        .inner_mut()
        .set_lowest_available_checkpoint(a_very_high_checkpoint_seq);

    assert!(peer_heights_2.write().unwrap().update_peer_info(
        peer_id_1,
        checkpoint_1.clone().into(),
        Some(a_very_high_checkpoint_seq),
    ));

    // Inject all the checkpoints to Peer 1
    for (checkpoint, contents) in checkpoint_iter.zip_debug_eq(contents_iter) {
        store_1
            .insert_checkpoint_contents(&checkpoint, contents)
            .unwrap();
        store_1.insert_certified_checkpoint(&checkpoint);
        handle_1.send_checkpoint(checkpoint).await;
    }

    // Peer 1 has all the checkpoint contents, but not Peer 2
    timeout(Duration::from_secs(1), async {
        for (checkpoint, contents) in ordered_checkpoints[2..]
            .iter()
            .zip_debug_eq(contents.clone().into_iter().skip(2))
        {
            assert_eq!(subscriber_1.recv().await.unwrap().data(), checkpoint.data());
            let content_digest = contents.into_checkpoint_contents_digest();
            store_1
                .get_full_checkpoint_contents(None, &content_digest)
                .unwrap();
            assert_eq!(
                store_2.get_full_checkpoint_contents(None, &content_digest),
                None
            );
        }
    })
    .await
    .unwrap();
    subscriber_2.try_recv().unwrap_err();

    assert_eq!(
        store_1
            .get_highest_synced_checkpoint()
            .unwrap()
            .sequence_number(),
        ordered_checkpoints.last().unwrap().sequence_number()
    );
    assert_eq!(
        store_2
            .get_highest_synced_checkpoint()
            .unwrap()
            .sequence_number(),
        ordered_checkpoints[1].sequence_number()
    );

    assert_eq!(
        store_1
            .get_highest_verified_checkpoint()
            .unwrap()
            .sequence_number(),
        &last_checkpoint_seq
    );

    // Add Peer 3
    let (builder, state_sync_router) = Builder::new()
        .store(SharedInMemoryStore::default())
        .config(watermark_test_config(hedge_delay_ms))
        .build();
    let network_3 = build_network(|router| router.merge(state_sync_router));
    let (event_loop_3, handle_3) = builder.build(network_3.clone());

    let mut subscriber_3 = handle_3.subscribe_to_synced_checkpoints();
    network_3.connect(network_1.local_addr()).await.unwrap();
    network_3.connect(network_2.local_addr()).await.unwrap();
    let store_3 = event_loop_3.store.clone();
    let peer_heights_3 = event_loop_3.peer_heights.clone();
    peer_heights_3
        .write()
        .unwrap()
        .set_wait_interval_when_no_peer_to_sync_content(Duration::from_secs(1));
    event_loop_3.store.inner_mut().insert_genesis_state(
        ordered_checkpoints.first().cloned().unwrap(),
        genesis_checkpoint_content.clone(),
        committee.committee().to_owned(),
    );
    tokio::spawn(event_loop_3.start());

    // Peer 3 is able to sync checkpoint 1 with the help from Peer 2
    timeout(Duration::from_secs(1), async {
        assert_eq!(
            subscriber_3.recv().await.unwrap().data(),
            ordered_checkpoints[1].data()
        );
        let content_digest = contents[1].clone().into_checkpoint_contents_digest();
        store_3
            .get_full_checkpoint_contents(None, &content_digest)
            .unwrap();
    })
    .await
    .unwrap();
    subscriber_3.try_recv().unwrap_err();
    subscriber_2.try_recv().unwrap_err();

    assert_eq!(
        store_2
            .get_highest_synced_checkpoint()
            .unwrap()
            .sequence_number(),
        ordered_checkpoints[1].sequence_number(),
    );
    assert_eq!(
        store_3
            .get_highest_synced_checkpoint()
            .unwrap()
            .sequence_number(),
        ordered_checkpoints[1].sequence_number(),
    );

    // Now set Peer 1's low watermark back to 0
    store_1.inner_mut().set_lowest_available_checkpoint(0);

    // Peer 2 and Peer 3 will know about this change by `get_checkpoint_availability`
    // Soon we expect them to have all checkpoints's content.
    timeout(Duration::from_secs(10), async {
        for (checkpoint, contents) in ordered_checkpoints[2..]
            .iter()
            .zip_debug_eq(contents.clone().into_iter().skip(2))
        {
            assert_eq!(subscriber_2.recv().await.unwrap().data(), checkpoint.data());
            assert_eq!(subscriber_3.recv().await.unwrap().data(), checkpoint.data());
            let content_digest = contents.into_checkpoint_contents_digest();
            store_2
                .get_full_checkpoint_contents(None, &content_digest)
                .unwrap();
            store_3
                .get_full_checkpoint_contents(None, &content_digest)
                .unwrap();
        }
    })
    .await
    .unwrap();
    assert_eq!(
        store_2
            .get_highest_synced_checkpoint()
            .unwrap()
            .sequence_number(),
        &last_checkpoint_seq
    );
    assert_eq!(
        store_3
            .get_highest_synced_checkpoint()
            .unwrap()
            .sequence_number(),
        &last_checkpoint_seq
    );
    assert_eq!(
        store_2
            .get_highest_verified_checkpoint()
            .unwrap()
            .sequence_number(),
        &last_checkpoint_seq
    );
    assert_eq!(
        store_3
            .get_highest_verified_checkpoint()
            .unwrap()
            .sequence_number(),
        &last_checkpoint_seq
    );

    // Now set Peer 1 and 2's low watermark to a very high number
    store_1
        .inner_mut()
        .set_lowest_available_checkpoint(a_very_high_checkpoint_seq);

    store_2
        .inner_mut()
        .set_lowest_available_checkpoint(a_very_high_checkpoint_seq);

    // Start Peer 4
    let (builder, state_sync_router) = Builder::new()
        .store(SharedInMemoryStore::default())
        .config(watermark_test_config(hedge_delay_ms))
        .build();
    let network_4 = build_network(|router| router.merge(state_sync_router));
    let (event_loop_4, handle_4) = builder.build(network_4.clone());

    let mut subscriber_4 = handle_4.subscribe_to_synced_checkpoints();
    let store_4 = event_loop_4.store.clone();
    let peer_heights_4 = event_loop_4.peer_heights.clone();
    peer_heights_4
        .write()
        .unwrap()
        .set_wait_interval_when_no_peer_to_sync_content(Duration::from_secs(1));
    event_loop_4.store.inner_mut().insert_genesis_state(
        ordered_checkpoints.first().cloned().unwrap(),
        genesis_checkpoint_content,
        committee.committee().to_owned(),
    );
    tokio::spawn(event_loop_4.start());
    // Need to connect 4 to 1, 2, 3 manually, as it does not have discovery enabled
    network_4.connect(network_1.local_addr()).await.unwrap();
    network_4.connect(network_2.local_addr()).await.unwrap();
    network_4.connect(network_3.local_addr()).await.unwrap();

    // Peer 4 syncs everything with Peer 3
    timeout(Duration::from_secs(3), async {
        for (checkpoint, contents) in ordered_checkpoints[1..]
            .iter()
            .zip_debug_eq(contents.clone().into_iter().skip(1))
        {
            assert_eq!(subscriber_4.recv().await.unwrap().data(), checkpoint.data());
            let content_digest = contents.into_checkpoint_contents_digest();
            store_4
                .get_full_checkpoint_contents(None, &content_digest)
                .unwrap();
        }
    })
    .await
    .unwrap();
    assert_eq!(
        store_4
            .get_highest_synced_checkpoint()
            .unwrap()
            .sequence_number(),
        &last_checkpoint_seq
    );
}

/// Tests that the max_checkpoint_lookahead config correctly limits how far ahead
/// pushed checkpoints can be stored, and that state sync still works correctly
/// to eventually sync all checkpoints.
#[tokio::test]
async fn sync_with_max_lookahead_rejection() {
    telemetry_subscribers::init_for_testing();
    let committee = CommitteeFixture::generate(rand::rngs::OsRng, 0, 4);

    let num_checkpoints: u64 = 20;
    let (ordered_checkpoints, _contents, _sequence_number_to_digest, _checkpoints) =
        committee.make_empty_checkpoints(num_checkpoints as usize, None);
    let small_lookahead: u64 = 5;
    let config_with_small_lookahead = StateSyncConfig {
        max_checkpoint_lookahead: Some(small_lookahead),
        ..StateSyncConfig::randomized_for_testing()
    };

    // Build Node 1 (the receiving node) with small lookahead.
    let store_1 = SharedInMemoryStore::default();
    let (
        UnstartedStateSync {
            handle: _handle_1,
            mailbox: _mailbox_1,
            peer_heights: peer_heights_1,
            ..
        },
        server_1,
    ) = Builder::new()
        .store(store_1.clone())
        .config(config_with_small_lookahead.clone())
        .build_internal();

    // Build Node 2 (the source node with all checkpoints)
    let (builder, state_sync_router) = Builder::new()
        .store(SharedInMemoryStore::default())
        .config(StateSyncConfig::randomized_for_testing())
        .build();
    let network_2 = build_network(|router| router.merge(state_sync_router));
    let (event_loop_2, _handle_2) = builder.build(network_2.clone());

    // Init genesis state in both nodes
    store_1.inner_mut().insert_genesis_state(
        ordered_checkpoints.first().cloned().unwrap(),
        empty_contents(),
        committee.committee().to_owned(),
    );
    event_loop_2.store.inner_mut().insert_genesis_state(
        ordered_checkpoints.first().cloned().unwrap(),
        empty_contents(),
        committee.committee().to_owned(),
    );

    // Populate Node 2's store with all checkpoints and contents
    {
        let mut store = event_loop_2.store.inner_mut();
        for checkpoint in ordered_checkpoints.iter().skip(1) {
            store.insert_certified_checkpoint(checkpoint);
            store.insert_checkpoint_contents(checkpoint, empty_contents());
        }
        store.update_highest_synced_checkpoint(ordered_checkpoints.last().unwrap());
    }

    // Set up peer info so the server recognizes the fake peer for initial test
    let fake_peer_id = PeerId([9; 32]);
    peer_heights_1.write().unwrap().insert_peer_info(
        fake_peer_id,
        PeerStateSyncInfo {
            genesis_checkpoint_digest: *ordered_checkpoints[0].digest(),
            on_same_chain_as_us: true,
            height: 0,
            lowest: 0,
        },
    );

    // Phase 1: Verify lookahead rejection by manually pushing checkpoints
    // Push all checkpoints via the server handler - server should reject those beyond lookahead
    for checkpoint in ordered_checkpoints.iter().skip(1) {
        let request = Request::new(checkpoint.clone().into_inner()).with_extension(fake_peer_id);
        server_1.push_checkpoint_summary(request).await.unwrap();
    }

    // Verify the lookahead logic:
    // - Checkpoints 1-5 should be stored (within lookahead from genesis at 0)
    // - Checkpoints 6-19 should NOT be stored (beyond lookahead)
    {
        let heights = peer_heights_1.read().unwrap();

        for seq in 1..=small_lookahead {
            let checkpoint = &ordered_checkpoints[seq as usize];
            assert!(
                heights
                    .unprocessed_checkpoints
                    .contains_key(checkpoint.digest()),
                "Checkpoint {seq} should be stored (within lookahead of {small_lookahead})",
            );
        }

        for seq in (small_lookahead + 1)..num_checkpoints {
            let checkpoint = &ordered_checkpoints[seq as usize];
            assert!(
                !heights
                    .unprocessed_checkpoints
                    .contains_key(checkpoint.digest()),
                "Checkpoint {seq} should NOT be stored (beyond lookahead of {small_lookahead})",
            );
        }

        // Peer height should be updated even for rejected checkpoints
        let peer_info = heights.peers.get(&fake_peer_id).unwrap();
        assert_eq!(
            peer_info.height,
            *ordered_checkpoints.last().unwrap().sequence_number(),
            "Peer height should be updated even for rejected checkpoints"
        );
    }

    // Phase 2: Now build a proper Node 1 with networking and start the sync loop
    // to verify that sync works correctly despite the lookahead limit
    let (builder, state_sync_router) = Builder::new()
        .store(store_1.clone())
        .config(config_with_small_lookahead)
        .build();
    let network_1 = build_network(|router| router.merge(state_sync_router));
    let (event_loop_1, _handle_1) = builder.build(network_1.clone());

    let peer_heights_1 = event_loop_1.peer_heights.clone();
    peer_heights_1
        .write()
        .unwrap()
        .set_wait_interval_when_no_peer_to_sync_content(Duration::from_secs(1));

    let peer_id_2 = network_2.peer_id();

    // Start both event loops
    tokio::spawn(event_loop_1.start());
    tokio::spawn(event_loop_2.start());

    // Connect the networks
    network_1.connect(network_2.local_addr()).await.unwrap();

    // Wait for peer discovery
    timeout(Duration::from_secs(5), async {
        loop {
            if peer_heights_1
                .read()
                .unwrap()
                .peers
                .contains_key(&peer_id_2)
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("Peer discovery timed out");

    // Wait for sync to complete - Node 1 should eventually sync all checkpoints
    // despite the lookahead limit, because the sync loop handles this correctly
    let last_checkpoint_seq = *ordered_checkpoints.last().unwrap().sequence_number();
    timeout(Duration::from_secs(10), async {
        loop {
            let highest_synced = store_1
                .get_highest_synced_checkpoint()
                .map(|c| *c.sequence_number())
                .unwrap_or(0);
            if highest_synced >= last_checkpoint_seq {
                break;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("Sync timed out - Node 1 should have synced all checkpoints");

    // Verify final state: all checkpoints should be synced
    assert_eq!(
        store_1
            .get_highest_synced_checkpoint()
            .unwrap()
            .sequence_number(),
        &last_checkpoint_seq,
        "Node 1 should have synced all checkpoints"
    );
    assert_eq!(
        store_1
            .get_highest_verified_checkpoint()
            .unwrap()
            .sequence_number(),
        &last_checkpoint_seq,
        "Node 1 should have verified all checkpoints"
    );

    // Verify all checkpoint contents are available
    for checkpoint in ordered_checkpoints.iter().skip(1) {
        let seq = *checkpoint.sequence_number();
        let contents_digest = &checkpoint.content_digest;
        assert!(
            store_1
                .get_full_checkpoint_contents(Some(seq), contents_digest)
                .is_some(),
            "Checkpoint {} contents should be available",
            seq
        );
    }
}

#[test]
fn test_peer_score_throughput_calculation() {
    use super::PeerScore;

    let window = Duration::from_secs(60);
    let failure_rate = 0.3;
    let mut score = PeerScore::new(window, failure_rate);

    // No samples - should return None
    assert!(score.effective_throughput().is_none());
    assert!(!score.is_failing());

    // Single sample: 100 units in 1 second = 100 throughput
    score.record_success(100, Duration::from_secs(1));
    let throughput = score.effective_throughput().unwrap();
    assert!((throughput - 100.0).abs() < 0.01);

    // Add another sample: 200 units in 2 seconds = 100 throughput
    // Combined: 300 units in 3 seconds = 100 throughput
    score.record_success(200, Duration::from_secs(2));
    let throughput = score.effective_throughput().unwrap();
    assert!((throughput - 100.0).abs() < 0.01);

    // Add a faster sample: 500 units in 1 second
    // Combined: 800 units in 4 seconds = 200 throughput
    score.record_success(500, Duration::from_secs(1));
    let throughput = score.effective_throughput().unwrap();
    assert!((throughput - 200.0).abs() < 0.01);
}

#[test]
fn test_peer_score_failure_tracking() {
    use super::PeerScore;

    let window = Duration::from_secs(60);
    let failure_rate = 0.3;
    let mut score = PeerScore::new(window, failure_rate);

    // Initially not failing (no samples)
    assert!(!score.is_failing());

    // Record 7 successes
    for _ in 0..7 {
        score.record_success(100, Duration::from_secs(1));
    }

    // Record 2 failures: 2/9 = 22% < 30%, and below min samples (10)
    score.record_failure();
    score.record_failure();
    assert!(!score.is_failing());

    // Record 1 more success to reach 10 samples: 2/10 = 20% < 30%, not failing
    score.record_success(100, Duration::from_secs(1));
    assert!(!score.is_failing());

    // Record 1 more failure: 3/11 = 27% < 30%, not failing
    score.record_failure();
    assert!(!score.is_failing());

    // Record 1 more failure: 4/12 = 33% >= 30%, is_failing
    score.record_failure();
    assert!(score.is_failing());
}

#[test]
fn test_peer_heights_score_recording() {
    use super::{PeerHeights, PeerStateSyncInfo};
    use anemo::PeerId;
    use sui_types::digests::CheckpointDigest;

    let peer_id = PeerId([1; 32]);

    let mut peer_heights = PeerHeights {
        peers: HashMap::from([(
            peer_id,
            PeerStateSyncInfo {
                genesis_checkpoint_digest: CheckpointDigest::default(),
                on_same_chain_as_us: true,
                height: 0,
                lowest: 0,
            },
        )]),
        unprocessed_checkpoints: HashMap::new(),
        sequence_number_to_digest: HashMap::new(),
        scores: HashMap::new(),
        wait_interval_when_no_peer_to_sync_content: Duration::from_secs(1),
        peer_scoring_window: Duration::from_secs(60),
        peer_failure_rate: 0.3,
        checkpoint_content_timeout_min: Duration::from_secs(10),
        checkpoint_content_timeout_max: Duration::from_secs(30),
        exploration_probability: 0.1,
    };

    // Initially no throughput data and not failing
    assert!(peer_heights.get_throughput(&peer_id).is_none());
    assert!(!peer_heights.is_failing(&peer_id));

    // Record some successes
    peer_heights.record_success(peer_id, 100, Duration::from_secs(1));
    let throughput = peer_heights.get_throughput(&peer_id).unwrap();
    assert!((throughput - 100.0).abs() < 0.01);

    // Record more successes
    peer_heights.record_success(peer_id, 200, Duration::from_secs(1));
    let throughput = peer_heights.get_throughput(&peer_id).unwrap();
    // 300 bytes / 2 seconds = 150 bytes/sec
    assert!((throughput - 150.0).abs() < 0.01);

    // Record more successes to reach 8 total
    for _ in 0..6 {
        peer_heights.record_success(peer_id, 100, Duration::from_secs(1));
    }

    // Record 2 failures: 2/10 = 20% < 30%, not failing
    peer_heights.record_failure(peer_id);
    peer_heights.record_failure(peer_id);
    assert!(!peer_heights.is_failing(&peer_id));

    // Record 2 more failures: 4/12 = 33% >= 30%, is_failing
    peer_heights.record_failure(peer_id);
    peer_heights.record_failure(peer_id);
    assert!(peer_heights.is_failing(&peer_id));
}

#[tokio::test]
async fn test_peer_balancer_sorts_by_throughput() {
    use super::{PeerBalancer, PeerCheckpointRequestType, PeerHeights, PeerStateSyncInfo};
    use std::sync::{Arc, RwLock};

    let committee = CommitteeFixture::generate(rand::rngs::OsRng, 0, 4);
    let (ordered_checkpoints, _, _, _) = committee.make_empty_checkpoints(2, None);

    let network_1 = build_network(|r| r);
    let network_2 = build_network(|r| r);
    let network_3 = build_network(|r| r);

    network_1.connect(network_2.local_addr()).await.unwrap();
    network_1.connect(network_3.local_addr()).await.unwrap();

    let mut peer_heights = PeerHeights {
        peers: HashMap::new(),
        unprocessed_checkpoints: HashMap::new(),
        sequence_number_to_digest: HashMap::new(),
        scores: HashMap::new(),
        wait_interval_when_no_peer_to_sync_content: Duration::from_secs(1),
        peer_scoring_window: Duration::from_secs(60),
        peer_failure_rate: 0.3,
        checkpoint_content_timeout_min: Duration::from_secs(10),
        checkpoint_content_timeout_max: Duration::from_secs(30),
        exploration_probability: 0.1,
    };

    let peer_2_id = network_2.peer_id();
    let peer_3_id = network_3.peer_id();

    peer_heights.peers.insert(
        peer_2_id,
        PeerStateSyncInfo {
            genesis_checkpoint_digest: *ordered_checkpoints[0].digest(),
            on_same_chain_as_us: true,
            height: 10,
            lowest: 0,
        },
    );
    peer_heights.peers.insert(
        peer_3_id,
        PeerStateSyncInfo {
            genesis_checkpoint_digest: *ordered_checkpoints[0].digest(),
            on_same_chain_as_us: true,
            height: 10,
            lowest: 0,
        },
    );

    // peer_2: slow (10 bytes/sec)
    peer_heights.record_success(peer_2_id, 100, Duration::from_secs(10));
    // peer_3: fast (1000 bytes/sec)
    peer_heights.record_success(peer_3_id, 1000, Duration::from_secs(1));

    let peer_heights = Arc::new(RwLock::new(peer_heights));

    let balancer = PeerBalancer::new(&network_1, peer_heights, PeerCheckpointRequestType::Summary);

    let peers: Vec<_> = balancer.collect();

    // Both peers should be present, fast peer first
    assert_eq!(peers.len(), 2);
    assert_eq!(peers[0].inner().peer_id(), peer_3_id);
    assert_eq!(peers[1].inner().peer_id(), peer_2_id);
}

#[test]
fn test_peer_score_failing_since_tracking() {
    use super::PeerScore;

    let window = Duration::from_secs(60);
    let failure_rate = 0.3;
    let mut score = PeerScore::new(window, failure_rate);

    // Initially, failing_since should be None
    assert!(score.failing_since.is_none());

    // Not enough samples to be failing, update_failing_state should keep None
    score.update_failing_state();
    assert!(score.failing_since.is_none());

    // Make the peer failing: 7 successes + 4 failures = 11 samples, 4/11 = 36% > 30%
    for _ in 0..7 {
        score.record_success(100, Duration::from_secs(1));
    }
    for _ in 0..4 {
        score.record_failure();
    }
    assert!(score.is_failing());

    // update_failing_state should set failing_since
    score.update_failing_state();
    assert!(score.failing_since.is_some());
    let first_failing_since = score.failing_since.unwrap();

    // Calling again should not change the timestamp
    std::thread::sleep(Duration::from_millis(10));
    score.update_failing_state();
    assert_eq!(score.failing_since.unwrap(), first_failing_since);

    // A single success does NOT clear failing_since while the windowed failure rate is still
    // above the threshold (8 successes + 4 failures = 33% >= 30%, still failing).
    score.record_success(100, Duration::from_secs(1));
    assert!(score.is_failing());
    score.update_failing_state();
    assert_eq!(score.failing_since.unwrap(), first_failing_since);

    // failing_since is cleared only once the windowed failure rate drops back below the
    // threshold (14 successes + 4 failures = ~22% < 30%), which update_failing_state detects.
    for _ in 0..6 {
        score.record_success(100, Duration::from_secs(1));
    }
    assert!(!score.is_failing());
    score.update_failing_state();
    assert!(score.failing_since.is_none());
}

#[test]
fn test_peer_score_consistently_failing() {
    use super::PeerScore;

    let window = Duration::from_secs(60);
    let failure_rate = 0.3;
    let mut score = PeerScore::new(window, failure_rate);

    // Not failing yet
    assert!(!score.consistently_failing(Duration::from_millis(50)));

    // Make the peer failing
    for _ in 0..7 {
        score.record_success(100, Duration::from_secs(1));
    }
    for _ in 0..4 {
        score.record_failure();
    }
    score.update_failing_state();
    assert!(score.failing_since.is_some());

    // Just became failing, should not be consistently failing with any positive threshold
    assert!(!score.consistently_failing(Duration::from_secs(1)));

    // But should be consistently failing with zero threshold
    assert!(score.consistently_failing(Duration::ZERO));

    // Wait a bit and check with a small threshold
    std::thread::sleep(Duration::from_millis(60));
    assert!(score.consistently_failing(Duration::from_millis(50)));
}

#[test]
fn test_find_peer_to_report_for_failure() {
    use super::{PeerHeights, PeerScore, PeerStateSyncInfo};
    use anemo::PeerId;

    let mut peer_heights = PeerHeights {
        peers: HashMap::new(),
        unprocessed_checkpoints: HashMap::new(),
        sequence_number_to_digest: HashMap::new(),
        scores: HashMap::new(),
        wait_interval_when_no_peer_to_sync_content: Duration::from_secs(1),
        peer_scoring_window: Duration::from_secs(60),
        peer_failure_rate: 0.3,
        checkpoint_content_timeout_min: Duration::from_secs(10),
        checkpoint_content_timeout_max: Duration::from_secs(30),
        exploration_probability: 0.1,
    };

    let peer_a = PeerId([1; 32]);
    let peer_b = PeerId([2; 32]);
    let genesis_digest = CheckpointDigest::default();

    peer_heights.peers.insert(
        peer_a,
        PeerStateSyncInfo {
            genesis_checkpoint_digest: genesis_digest,
            on_same_chain_as_us: true,
            height: 10,
            lowest: 0,
        },
    );
    peer_heights.peers.insert(
        peer_b,
        PeerStateSyncInfo {
            genesis_checkpoint_digest: genesis_digest,
            on_same_chain_as_us: true,
            height: 10,
            lowest: 0,
        },
    );

    // No scores yet, should return None
    assert!(
        peer_heights
            .find_peer_to_report_for_failure(Duration::from_secs(1))
            .is_none()
    );

    // Create a score for peer_a that has been failing for longer
    let mut score_a = PeerScore::new(Duration::from_secs(60), 0.3);
    for _ in 0..7 {
        score_a.record_success(100, Duration::from_secs(1));
    }
    for _ in 0..4 {
        score_a.record_failure();
    }
    // Manually set failing_since to a time in the past
    score_a.failing_since = Some(StdInstant::now() - Duration::from_secs(120));
    peer_heights.scores.insert(peer_a, score_a);

    // Create a score for peer_b that has been failing for less time
    let mut score_b = PeerScore::new(Duration::from_secs(60), 0.3);
    for _ in 0..7 {
        score_b.record_success(100, Duration::from_secs(1));
    }
    for _ in 0..4 {
        score_b.record_failure();
    }
    score_b.failing_since = Some(StdInstant::now() - Duration::from_secs(30));
    peer_heights.scores.insert(peer_b, score_b);

    // With a 10-second threshold, both are eligible but peer_a has been failing longer
    let result = peer_heights.find_peer_to_report_for_failure(Duration::from_secs(10));
    assert_eq!(result, Some(peer_a));

    // With a 60-second threshold, only peer_a qualifies
    let result = peer_heights.find_peer_to_report_for_failure(Duration::from_secs(60));
    assert_eq!(result, Some(peer_a));

    // With a 200-second threshold, neither qualifies
    let result = peer_heights.find_peer_to_report_for_failure(Duration::from_secs(200));
    assert!(result.is_none());
}

#[test]
fn test_find_peer_to_report_respects_same_chain() {
    use super::{PeerHeights, PeerScore, PeerStateSyncInfo};
    use anemo::PeerId;

    let mut peer_heights = PeerHeights {
        peers: HashMap::new(),
        unprocessed_checkpoints: HashMap::new(),
        sequence_number_to_digest: HashMap::new(),
        scores: HashMap::new(),
        wait_interval_when_no_peer_to_sync_content: Duration::from_secs(1),
        peer_scoring_window: Duration::from_secs(60),
        peer_failure_rate: 0.3,
        checkpoint_content_timeout_min: Duration::from_secs(10),
        checkpoint_content_timeout_max: Duration::from_secs(30),
        exploration_probability: 0.1,
    };

    let peer_a = PeerId([1; 32]);
    let genesis_digest = CheckpointDigest::default();

    peer_heights.peers.insert(
        peer_a,
        PeerStateSyncInfo {
            genesis_checkpoint_digest: genesis_digest,
            on_same_chain_as_us: true,
            height: 10,
            lowest: 0,
        },
    );

    // Peer not on same chain should not be selected
    let peer_b = PeerId([2; 32]);
    peer_heights.peers.insert(
        peer_b,
        PeerStateSyncInfo {
            genesis_checkpoint_digest: genesis_digest,
            on_same_chain_as_us: false,
            height: 10,
            lowest: 0,
        },
    );

    let mut score_a = PeerScore::new(Duration::from_secs(60), 0.3);
    for _ in 0..7 {
        score_a.record_success(100, Duration::from_secs(1));
    }
    for _ in 0..4 {
        score_a.record_failure();
    }
    score_a.failing_since = Some(StdInstant::now() - Duration::from_secs(600));
    peer_heights.scores.insert(peer_a, score_a);

    // peer_b has a failing score too, but is NOT on same chain
    let mut score_b = PeerScore::new(Duration::from_secs(60), 0.3);
    for _ in 0..7 {
        score_b.record_success(100, Duration::from_secs(1));
    }
    for _ in 0..4 {
        score_b.record_failure();
    }
    score_b.failing_since = Some(StdInstant::now() - Duration::from_secs(600));
    peer_heights.scores.insert(peer_b, score_b);

    // find_peer_to_report should only return peer_a (on same chain)
    let result = peer_heights.find_peer_to_report_for_failure(Duration::from_secs(10));
    assert_eq!(result, Some(peer_a));
}

#[test]
fn test_lost_peer_clears_scores() {
    use super::{PeerHeights, PeerStateSyncInfo};
    use anemo::PeerId;

    let mut peer_heights = PeerHeights {
        peers: HashMap::new(),
        unprocessed_checkpoints: HashMap::new(),
        sequence_number_to_digest: HashMap::new(),
        scores: HashMap::new(),
        wait_interval_when_no_peer_to_sync_content: Duration::from_secs(1),
        peer_scoring_window: Duration::from_secs(60),
        peer_failure_rate: 0.3,
        checkpoint_content_timeout_min: Duration::from_secs(10),
        checkpoint_content_timeout_max: Duration::from_secs(30),
        exploration_probability: 0.1,
    };

    let peer_id = PeerId([1; 32]);
    let genesis_digest = CheckpointDigest::default();

    peer_heights.peers.insert(
        peer_id,
        PeerStateSyncInfo {
            genesis_checkpoint_digest: genesis_digest,
            on_same_chain_as_us: true,
            height: 10,
            lowest: 0,
        },
    );

    peer_heights.record_success(peer_id, 100, Duration::from_secs(1));
    assert!(peer_heights.scores.contains_key(&peer_id));

    // Simulate LostPeer: remove both peers and scores
    peer_heights.peers.remove(&peer_id);
    peer_heights.scores.remove(&peer_id);

    assert!(!peer_heights.peers.contains_key(&peer_id));
    assert!(!peer_heights.scores.contains_key(&peer_id));
    assert!(peer_heights.get_throughput(&peer_id).is_none());
    assert!(!peer_heights.is_failing(&peer_id));
}

#[test]
fn test_adaptive_timeout_calculation() {
    use super::compute_adaptive_timeout;

    let min_timeout = Duration::from_secs(10);
    let max_timeout = Duration::from_secs(30);

    // Helper to check timeout is within expected range (base ± 10% jitter)
    let assert_in_range = |timeout: Duration, expected_base: f64| {
        let timeout_secs = timeout.as_secs_f64();
        let jitter_range = expected_base * 0.1;
        let min_expected = (expected_base - jitter_range).max(min_timeout.as_secs_f64());
        let max_expected = expected_base + jitter_range;
        assert!(
            timeout_secs >= min_expected && timeout_secs <= max_expected,
            "timeout {} not in range [{}, {}]",
            timeout_secs,
            min_expected,
            max_expected
        );
    };

    // Empty checkpoint - base timeout only (10s ± 10%)
    let timeout = compute_adaptive_timeout(0, min_timeout, max_timeout);
    assert_in_range(timeout, 10.0);

    // Medium checkpoint with 1000 txns: base 12s ± 10%
    let timeout = compute_adaptive_timeout(1000, min_timeout, max_timeout);
    assert_in_range(timeout, 12.0);

    // Half-full checkpoint with 5000 txns: base 20s ± 10%
    let timeout = compute_adaptive_timeout(5000, min_timeout, max_timeout);
    assert_in_range(timeout, 20.0);

    // Max checkpoint with 10000 txns: base 30s ± 10%
    let timeout = compute_adaptive_timeout(10000, min_timeout, max_timeout);
    assert_in_range(timeout, 30.0);
}

/// Serves state sync like `Server`, but waits `delay` before answering each checkpoint
/// contents request.
struct DelayedContentsServer<S> {
    inner: super::server::Server<S>,
    delay: Duration,
}

#[anemo::async_trait]
impl<S> StateSync for DelayedContentsServer<S>
where
    S: WriteStore + Send + Sync + 'static,
{
    async fn push_checkpoint_summary(
        &self,
        request: Request<sui_types::messages_checkpoint::CertifiedCheckpointSummary>,
    ) -> Result<anemo::Response<()>, anemo::rpc::Status> {
        self.inner.push_checkpoint_summary(request).await
    }

    async fn get_checkpoint_summary(
        &self,
        request: Request<GetCheckpointSummaryRequest>,
    ) -> Result<
        anemo::Response<Option<sui_types::messages_checkpoint::CertifiedCheckpointSummary>>,
        anemo::rpc::Status,
    > {
        self.inner.get_checkpoint_summary(request).await
    }

    async fn get_checkpoint_availability(
        &self,
        request: Request<()>,
    ) -> Result<anemo::Response<super::GetCheckpointAvailabilityResponse>, anemo::rpc::Status> {
        self.inner.get_checkpoint_availability(request).await
    }

    async fn get_checkpoint_contents(
        &self,
        request: Request<sui_types::digests::CheckpointContentsDigest>,
    ) -> Result<
        anemo::Response<Option<sui_types::messages_checkpoint::FullCheckpointContents>>,
        anemo::rpc::Status,
    > {
        tokio::time::sleep(self.delay).await;
        self.inner.get_checkpoint_contents(request).await
    }

    async fn get_checkpoint_contents_v2(
        &self,
        request: Request<sui_types::digests::CheckpointContentsDigest>,
    ) -> Result<
        anemo::Response<Option<sui_types::messages_checkpoint::VersionedFullCheckpointContents>>,
        anemo::rpc::Status,
    > {
        tokio::time::sleep(self.delay).await;
        self.inner.get_checkpoint_contents_v2(request).await
    }
}

/// Builds a peer whose store holds the first `checkpoints_with_contents` of `checkpoints`
/// (genesis included) with their contents. It answers each contents request after
/// `contents_delay`.
fn build_contents_peer(
    committee: &CommitteeFixture,
    checkpoints: &[sui_types::messages_checkpoint::VerifiedCheckpoint],
    contents: &[sui_types::messages_checkpoint::VerifiedCheckpointContents],
    checkpoints_with_contents: usize,
    contents_delay: Duration,
) -> anemo::Network {
    let store = SharedInMemoryStore::default();
    store.inner_mut().insert_genesis_state(
        checkpoints[0].clone(),
        contents[0].clone(),
        committee.committee().to_owned(),
    );
    for (checkpoint, checkpoint_contents) in checkpoints
        .iter()
        .zip_debug_eq(contents)
        .take(checkpoints_with_contents)
        .skip(1)
    {
        store.insert_checkpoint(checkpoint).unwrap();
        store
            .insert_checkpoint_contents(checkpoint, checkpoint_contents.clone())
            .unwrap();
        store.update_highest_synced_checkpoint(checkpoint).unwrap();
    }
    let (_state_sync, server) = Builder::new().store(store).build_internal();
    let router =
        anemo::Router::new().add_rpc_service(super::StateSyncServer::new(DelayedContentsServer {
            inner: server,
            delay: contents_delay,
        }));
    build_network(|r| r.merge(router))
}

/// A node that downloads checkpoint contents from test peers, with the peer heights it uses to
/// pick them.
struct HedgeTestClient {
    network: anemo::Network,
    store: SharedInMemoryStore,
    peer_heights: std::sync::Arc<std::sync::RwLock<super::PeerHeights>>,
    metrics: super::metrics::Metrics,
    genesis_digest: CheckpointDigest,
}

impl HedgeTestClient {
    /// Connects to each peer and records it as having announced the given height.
    async fn new(genesis_digest: CheckpointDigest, peers: &[(&anemo::Network, u64)]) -> Self {
        let store = SharedInMemoryStore::default();
        let (state_sync, _) = Builder::new().store(store.clone()).build_internal();
        let client = Self {
            network: build_network(|r| r),
            store,
            peer_heights: state_sync.peer_heights.clone(),
            metrics: super::metrics::Metrics::enabled(&prometheus::Registry::new()),
            genesis_digest,
        };
        for (peer, height) in peers {
            client.network.connect(peer.local_addr()).await.unwrap();
            client.announce(peer.peer_id(), *height);
        }
        client
    }

    fn announce(&self, peer_id: PeerId, height: u64) {
        self.peer_heights.write().unwrap().peers.insert(
            peer_id,
            PeerStateSyncInfo {
                genesis_checkpoint_digest: self.genesis_digest,
                on_same_chain_as_us: true,
                height,
                lowest: 0,
            },
        );
    }

    /// Records `peer_id` as having announced `height` after `delay`.
    fn announce_after(
        &self,
        peer_id: PeerId,
        height: u64,
        delay: Duration,
    ) -> tokio::task::JoinHandle<()> {
        let peer_heights = self.peer_heights.clone();
        let genesis_checkpoint_digest = self.genesis_digest;
        tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            peer_heights.write().unwrap().peers.insert(
                peer_id,
                PeerStateSyncInfo {
                    genesis_checkpoint_digest,
                    on_same_chain_as_us: true,
                    height,
                    lowest: 0,
                },
            );
        })
    }

    /// Makes `peer_id` slow from this node: it answered within 20 ms a few times, but most
    /// requests to it were still running after 500 ms when a hedge won. Its slow-response
    /// threshold is 40 ms, and its expected response time is 20 ms.
    fn record_slow_peer_responses(&self, peer_id: PeerId) {
        use super::ContentResponseTime::{Answered, LongerThan};

        let mut peer_heights = self.peer_heights.write().unwrap();
        for _ in 0..3 {
            peer_heights.record_content_response_time(
                peer_id,
                Answered(Duration::from_millis(20)),
                false,
            );
        }
        for _ in 0..7 {
            peer_heights.record_content_response_time(
                peer_id,
                LongerThan(Duration::from_millis(500)),
                false,
            );
        }
        assert!(peer_heights.slow_peer_hedge_delay(&peer_id).is_some());
    }

    /// Downloads `checkpoint`'s contents in hedged mode with a fixed hedge delay, giving up after
    /// `limit`.
    async fn fetch_hedged(
        &self,
        checkpoint: &sui_types::messages_checkpoint::VerifiedCheckpoint,
        hedge_delay: Duration,
        highest_synced: u64,
        limit: Duration,
    ) -> Result<
        Option<sui_types::messages_checkpoint::VersionedFullCheckpointContents>,
        tokio::time::error::Elapsed,
    > {
        let hedge = super::ContentHedge {
            delay: hedge_delay,
            adaptive_delay: false,
            closer_peer: false,
            highest_synced: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(highest_synced)),
        };
        self.fetch_with_hedge(checkpoint, &hedge, limit).await
    }

    /// Downloads `checkpoint`'s contents with `hedge`, giving up after `limit`.
    async fn fetch_with_hedge(
        &self,
        checkpoint: &sui_types::messages_checkpoint::VerifiedCheckpoint,
        hedge: &super::ContentHedge,
        limit: Duration,
    ) -> Result<
        Option<sui_types::messages_checkpoint::VersionedFullCheckpointContents>,
        tokio::time::error::Elapsed,
    > {
        let fetch = super::ContentFetch {
            network: &self.network,
            peer_heights: &self.peer_heights,
            checkpoint,
            use_get_checkpoint_contents_v2: true,
            timeout: Duration::from_secs(20),
            metrics: &self.metrics,
        };
        timeout(
            limit,
            super::get_full_checkpoint_contents(fetch, &self.store, Some(hedge)),
        )
        .await
    }
}

/// When the download of checkpoint 1 starts, only a peer that answers after 30 s has announced
/// it. Another peer announces it 50 ms later. The hedge must go to that peer.
#[tokio::test]
async fn hedged_contents_fetch_asks_peer_that_announced_after_start() {
    use super::metrics::{ContentRequestOutcome, HedgeEvent};

    let committee = CommitteeFixture::generate(rand::rngs::OsRng, 0, 4);
    let (checkpoints, contents, _, _) = committee.make_random_checkpoints(2, None);
    let slow_peer = build_contents_peer(
        &committee,
        &checkpoints,
        &contents,
        2,
        Duration::from_secs(30),
    );
    let fast_peer = build_contents_peer(&committee, &checkpoints, &contents, 2, Duration::ZERO);
    let client = HedgeTestClient::new(
        *checkpoints[0].digest(),
        &[(&slow_peer, 1), (&fast_peer, 0)],
    )
    .await;
    let announce = client.announce_after(fast_peer.peer_id(), 1, Duration::from_millis(50));

    let fetched = client
        .fetch_hedged(
            &checkpoints[1],
            Duration::from_millis(100),
            0,
            Duration::from_secs(5),
        )
        .await
        .expect("the hedge to the second peer should finish long before the slow peer answers");
    announce.await.unwrap();

    assert!(fetched.is_some());
    assert!(
        client
            .store
            .get_full_checkpoint_contents(Some(1), &checkpoints[1].content_digest)
            .is_some()
    );
    assert_eq!(
        client.metrics.checkpoint_content_hedges(HedgeEvent::Sent),
        1
    );
    assert_eq!(client.metrics.checkpoint_content_hedges(HedgeEvent::Won), 1);
    // Test peers are not in the client's known-peer set, so they share the `other` label.
    assert_eq!(
        client
            .metrics
            .checkpoint_content_peer_requests("other", ContentRequestOutcome::Success),
        1
    );
    assert_eq!(
        client
            .metrics
            .checkpoint_content_peer_requests("other", ContentRequestOutcome::Cancelled),
        1
    );
}

/// When the download of checkpoint 1 starts, the only peer that has announced it doesn't have
/// its contents. Another peer announces it 300 ms later. The download must move on to that peer
/// instead of giving up and waiting for the retry interval.
#[tokio::test]
async fn hedged_contents_fetch_falls_back_to_peer_that_announced_after_failure() {
    use super::metrics::HedgeEvent;

    let committee = CommitteeFixture::generate(rand::rngs::OsRng, 0, 4);
    let (checkpoints, contents, _, _) = committee.make_random_checkpoints(2, None);
    let empty_peer = build_contents_peer(&committee, &checkpoints, &contents, 1, Duration::ZERO);
    let good_peer = build_contents_peer(&committee, &checkpoints, &contents, 2, Duration::ZERO);
    let client = HedgeTestClient::new(
        *checkpoints[0].digest(),
        &[(&empty_peer, 1), (&good_peer, 0)],
    )
    .await;
    let announce = client.announce_after(good_peer.peer_id(), 1, Duration::from_millis(300));

    let fetched = client
        .fetch_hedged(
            &checkpoints[1],
            Duration::from_millis(100),
            0,
            Duration::from_secs(5),
        )
        .await
        .expect("the download should move on to the second peer once it announces the checkpoint");
    announce.await.unwrap();

    assert!(fetched.is_some());
    // The second peer is normally reached by a fallback after the first request failed. If the
    // first request is still running when the second peer announces, a hedge reaches it instead.
    assert_eq!(
        client.metrics.checkpoint_content_fallback_requests()
            + client.metrics.checkpoint_content_hedges(HedgeEvent::Sent),
        1
    );
}

/// A response that arrives before the hedge delay ends the download without a hedge.
#[tokio::test]
async fn hedged_contents_fetch_does_not_hedge_a_response_within_the_delay() {
    use super::metrics::HedgeEvent;

    let committee = CommitteeFixture::generate(rand::rngs::OsRng, 0, 4);
    let (checkpoints, contents, _, _) = committee.make_random_checkpoints(2, None);
    let peer_a = build_contents_peer(&committee, &checkpoints, &contents, 2, Duration::ZERO);
    let peer_b = build_contents_peer(&committee, &checkpoints, &contents, 2, Duration::ZERO);
    let client =
        HedgeTestClient::new(*checkpoints[0].digest(), &[(&peer_a, 1), (&peer_b, 1)]).await;

    let fetched = client
        .fetch_hedged(
            &checkpoints[1],
            Duration::from_secs(2),
            0,
            Duration::from_secs(1),
        )
        .await
        .expect("a peer that answers right away should finish the download");

    assert!(fetched.is_some());
    assert_eq!(
        client.metrics.checkpoint_content_hedges(HedgeEvent::Sent),
        0
    );
}

/// With three slow peers, a download has at most two requests in flight: the first request
/// and one hedge.
#[tokio::test]
async fn hedged_contents_fetch_keeps_at_most_two_requests_in_flight() {
    use super::metrics::HedgeEvent;

    let committee = CommitteeFixture::generate(rand::rngs::OsRng, 0, 4);
    let (checkpoints, contents, _, _) = committee.make_random_checkpoints(2, None);
    let slow = || {
        build_contents_peer(
            &committee,
            &checkpoints,
            &contents,
            2,
            Duration::from_secs(30),
        )
    };
    let (peer_a, peer_b, peer_c) = (slow(), slow(), slow());
    let client = HedgeTestClient::new(
        *checkpoints[0].digest(),
        &[(&peer_a, 1), (&peer_b, 1), (&peer_c, 1)],
    )
    .await;

    // Ten hedge delays pass while every request is still running.
    let result = client
        .fetch_hedged(
            &checkpoints[1],
            Duration::from_millis(50),
            0,
            Duration::from_millis(500),
        )
        .await;

    assert!(result.is_err(), "no peer answers within the limit");
    assert_eq!(
        client.metrics.checkpoint_content_hedges(HedgeEvent::Sent),
        1
    );
}

/// Checkpoints more than `CONTENT_HEDGE_WINDOW` past the highest synced checkpoint are not
/// hedged, even when another peer has them.
#[tokio::test]
async fn hedged_contents_fetch_does_not_hedge_far_from_release_frontier() {
    use super::metrics::HedgeEvent;

    let committee = CommitteeFixture::generate(rand::rngs::OsRng, 0, 4);
    let (checkpoints, contents, _, _) = committee.make_random_checkpoints(11, None);
    let slow_peer = build_contents_peer(
        &committee,
        &checkpoints,
        &contents,
        11,
        Duration::from_secs(30),
    );
    let fast_peer = build_contents_peer(&committee, &checkpoints, &contents, 11, Duration::ZERO);
    let client = HedgeTestClient::new(
        *checkpoints[0].digest(),
        &[(&slow_peer, 10), (&fast_peer, 0)],
    )
    .await;
    let announce = client.announce_after(fast_peer.peer_id(), 10, Duration::from_millis(20));

    // Checkpoint 10 is 9 past the highest synced checkpoint 1.
    let result = client
        .fetch_hedged(
            &checkpoints[10],
            Duration::from_millis(50),
            1,
            Duration::from_millis(500),
        )
        .await;
    announce.await.unwrap();

    assert!(
        result.is_err(),
        "only the slow peer should be asked for a checkpoint outside the hedge window"
    );
    assert_eq!(
        client.metrics.checkpoint_content_hedges(HedgeEvent::Sent),
        0
    );
}

/// With adaptive delays, a request to a peer that has been slow from this node is hedged after
/// the peer's slow-response threshold instead of the configured delay. No other peer has the
/// checkpoint at that point, so the hedge goes out as soon as one announces it.
#[tokio::test]
async fn adaptive_hedge_delay_hedges_slow_peer_early() {
    use super::ContentResponseTime::{Answered, LongerThan};
    use super::metrics::{HedgeEvent, OtherPeerWaitOutcome};

    let committee = CommitteeFixture::generate(rand::rngs::OsRng, 0, 4);
    let (checkpoints, contents, _, _) = committee.make_random_checkpoints(2, None);
    let slow_peer = build_contents_peer(
        &committee,
        &checkpoints,
        &contents,
        2,
        Duration::from_secs(30),
    );
    let fast_peer = build_contents_peer(&committee, &checkpoints, &contents, 2, Duration::ZERO);
    let client = HedgeTestClient::new(
        *checkpoints[0].digest(),
        &[(&slow_peer, 1), (&fast_peer, 0)],
    )
    .await;
    // The slow peer answered within 20 ms a few times, but most requests to it were still
    // running after 500 ms when a hedge won. Its slow-response threshold is 40 ms.
    {
        let mut peer_heights = client.peer_heights.write().unwrap();
        for _ in 0..3 {
            peer_heights.record_content_response_time(
                slow_peer.peer_id(),
                Answered(Duration::from_millis(20)),
                false,
            );
        }
        for _ in 0..7 {
            peer_heights.record_content_response_time(
                slow_peer.peer_id(),
                LongerThan(Duration::from_millis(500)),
                false,
            );
        }
    }
    let announce = client.announce_after(fast_peer.peer_id(), 1, Duration::from_millis(100));
    let hedge = super::ContentHedge {
        delay: Duration::from_secs(10),
        adaptive_delay: true,
        closer_peer: false,
        highest_synced: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
    };

    let fetched = client
        .fetch_with_hedge(&checkpoints[1], &hedge, Duration::from_secs(5))
        .await
        .expect("the slow peer's request should be hedged long before the 10 s hedge delay");
    announce.await.unwrap();

    assert!(fetched.is_some());
    assert_eq!(
        client
            .metrics
            .checkpoint_content_slow_peer_hedge_delays("other"),
        1
    );
    // The hedge was due at 40 ms, before the other peer announced the checkpoint at 100 ms.
    assert_eq!(
        client
            .metrics
            .checkpoint_content_hedges(HedgeEvent::NoCandidate),
        1
    );
    assert_eq!(
        client.metrics.checkpoint_content_hedges(HedgeEvent::Sent),
        1
    );
    assert_eq!(client.metrics.checkpoint_content_hedges(HedgeEvent::Won), 1);
    assert_eq!(
        client
            .metrics
            .checkpoint_content_other_peer_waits(OtherPeerWaitOutcome::Announced),
        1
    );

    // The slow peer's request was cancelled when the hedge won, after it had run past the 40 ms
    // threshold, so it adds a slow sample that keeps the peer flagged. The winning hedge adds no
    // sample for the fast peer.
    let peer_heights = client.peer_heights.read().unwrap();
    let slow_samples = &peer_heights.scores[&slow_peer.peer_id()].content_response_times;
    assert_eq!(slow_samples.len(), 11);
    assert!(matches!(
        slow_samples.back(),
        Some((_, LongerThan(ran))) if *ran >= Duration::from_millis(40)
    ));
    assert!(
        peer_heights
            .scores
            .get(&fast_peer.peer_id())
            .is_none_or(|score| score.content_response_times.is_empty())
    );
    assert!(
        peer_heights
            .slow_peer_hedge_delay(&slow_peer.peer_id())
            .is_some()
    );
}

/// With hedging to a closer peer, a request to a peer that is far away from this node is hedged
/// as soon as a nearby peer announces the checkpoint, long before the hedge delay. Local round
/// trips are all about zero, so the slow peer's recent answers make it look far away.
#[tokio::test]
async fn closer_peer_hedge_goes_out_when_a_faster_peer_announces() {
    use super::ContentResponseTime::Answered;
    use super::metrics::HedgeEvent;

    let committee = CommitteeFixture::generate(rand::rngs::OsRng, 0, 4);
    let (checkpoints, contents, _, _) = committee.make_random_checkpoints(2, None);
    let slow_peer = build_contents_peer(
        &committee,
        &checkpoints,
        &contents,
        2,
        Duration::from_secs(30),
    );
    let fast_peer = build_contents_peer(&committee, &checkpoints, &contents, 2, Duration::ZERO);
    let client = HedgeTestClient::new(
        *checkpoints[0].digest(),
        &[(&slow_peer, 1), (&fast_peer, 0)],
    )
    .await;
    // The slow peer's expected response time is about 500 ms.
    {
        let mut peer_heights = client.peer_heights.write().unwrap();
        for _ in 0..10 {
            peer_heights.record_content_response_time(
                slow_peer.peer_id(),
                Answered(Duration::from_millis(500)),
                false,
            );
        }
    }
    let announce = client.announce_after(fast_peer.peer_id(), 1, Duration::from_millis(50));
    let hedge = super::ContentHedge {
        delay: Duration::from_secs(10),
        adaptive_delay: false,
        closer_peer: true,
        highest_synced: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
    };

    let fetched = client
        .fetch_with_hedge(&checkpoints[1], &hedge, Duration::from_secs(5))
        .await
        .expect("the request to the slow peer should be hedged long before the 10 s hedge delay");
    announce.await.unwrap();

    assert!(fetched.is_some());
    assert_eq!(
        client.metrics.checkpoint_content_hedges(HedgeEvent::Sent),
        1
    );
    assert_eq!(
        client
            .metrics
            .checkpoint_content_hedges(HedgeEvent::SentToCloserPeer),
        1
    );
    assert_eq!(client.metrics.checkpoint_content_hedges(HedgeEvent::Won), 1);

    // The slow peer had the checkpoint when the first request went out, so its announce delay
    // is about zero. The fast peer was first seen at about 50 ms, after its announcement.
    // Test peers are not in the client's known-peer set, so they share the `other` label.
    let metrics = &client.metrics;
    assert_eq!(metrics.checkpoint_content_peer_announce_delays("other"), 2);
    assert_eq!(
        metrics.checkpoint_content_peer_announce_delays_within("other", 0.005),
        1
    );
    assert_eq!(
        metrics.checkpoint_content_peer_announce_delays_within("other", 0.025),
        1
    );
    assert_eq!(
        metrics.checkpoint_content_peer_announce_delays_within("other", 0.2),
        2
    );
}

/// With hedging to a closer peer, the first request goes to a peer that isn't slow from this
/// node, even though a slow peer has a lower expected response time.
#[tokio::test]
async fn closer_peer_first_request_avoids_slow_peer() {
    use super::ContentResponseTime::Answered;
    use super::metrics::HedgeEvent;

    let committee = CommitteeFixture::generate(rand::rngs::OsRng, 0, 4);
    let (checkpoints, contents, _, _) = committee.make_random_checkpoints(2, None);
    let slow_peer = build_contents_peer(
        &committee,
        &checkpoints,
        &contents,
        2,
        Duration::from_secs(30),
    );
    let healthy_peer = build_contents_peer(&committee, &checkpoints, &contents, 2, Duration::ZERO);
    let client = HedgeTestClient::new(
        *checkpoints[0].digest(),
        &[(&slow_peer, 1), (&healthy_peer, 1)],
    )
    .await;
    // The slow peer's expected response time is 20 ms, the healthy peer's 100 ms.
    client.record_slow_peer_responses(slow_peer.peer_id());
    {
        let mut peer_heights = client.peer_heights.write().unwrap();
        for _ in 0..10 {
            peer_heights.record_content_response_time(
                healthy_peer.peer_id(),
                Answered(Duration::from_millis(100)),
                false,
            );
        }
    }
    let hedge = super::ContentHedge {
        delay: Duration::from_secs(10),
        adaptive_delay: false,
        closer_peer: true,
        highest_synced: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
    };

    let fetched = client
        .fetch_with_hedge(&checkpoints[1], &hedge, Duration::from_secs(5))
        .await
        .expect("the first request should go to the healthy peer, which answers right away");

    assert!(fetched.is_some());
    assert_eq!(
        client.metrics.checkpoint_content_hedges(HedgeEvent::Sent),
        0
    );
}

/// With hedging to a closer peer, a request to a slow peer is hedged as soon as a peer that
/// isn't slow announces the checkpoint, even though that peer is not expected to finish
/// `CLOSER_PEER_MIN_GAIN` sooner, and long before the hedge delay.
#[tokio::test]
async fn closer_peer_hedges_around_slow_peer_in_flight() {
    use super::metrics::HedgeEvent;

    let committee = CommitteeFixture::generate(rand::rngs::OsRng, 0, 4);
    let (checkpoints, contents, _, _) = committee.make_random_checkpoints(2, None);
    let slow_peer = build_contents_peer(
        &committee,
        &checkpoints,
        &contents,
        2,
        Duration::from_secs(30),
    );
    let healthy_peer = build_contents_peer(&committee, &checkpoints, &contents, 2, Duration::ZERO);
    let client = HedgeTestClient::new(
        *checkpoints[0].digest(),
        &[(&slow_peer, 1), (&healthy_peer, 0)],
    )
    .await;
    // The slow peer's expected response time is 20 ms, so by the time the healthy peer
    // announces, the request in flight is past its expected finish and the closer-peer rule
    // alone would not hedge it.
    client.record_slow_peer_responses(slow_peer.peer_id());
    let announce = client.announce_after(healthy_peer.peer_id(), 1, Duration::from_millis(50));
    let hedge = super::ContentHedge {
        delay: Duration::from_secs(10),
        adaptive_delay: false,
        closer_peer: true,
        highest_synced: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
    };

    let fetched = client
        .fetch_with_hedge(&checkpoints[1], &hedge, Duration::from_secs(5))
        .await
        .expect("the request to the slow peer should be hedged long before the 10 s hedge delay");
    announce.await.unwrap();

    assert!(fetched.is_some());
    let metrics = &client.metrics;
    assert_eq!(metrics.checkpoint_content_hedges(HedgeEvent::Sent), 1);
    assert_eq!(
        metrics.checkpoint_content_hedges(HedgeEvent::SentAroundSlowPeer),
        1
    );
    assert_eq!(
        metrics.checkpoint_content_hedges(HedgeEvent::SentToCloserPeer),
        0
    );
    assert_eq!(metrics.checkpoint_content_hedges(HedgeEvent::Won), 1);
    assert_eq!(metrics.checkpoint_content_slow_peer_probes("other"), 0);
}

/// A slow-peer probe is not hedged right away just because its peer is slow: the fourth
/// request to a slow peer runs until the hedge delay even after a peer that isn't slow
/// announces the checkpoint.
#[tokio::test]
async fn closer_peer_does_not_hedge_around_slow_peer_probe() {
    use super::metrics::HedgeEvent;

    let committee = CommitteeFixture::generate(rand::rngs::OsRng, 0, 4);
    let (checkpoints, contents, _, _) = committee.make_random_checkpoints(2, None);
    let slow_peer = build_contents_peer(
        &committee,
        &checkpoints,
        &contents,
        2,
        Duration::from_secs(30),
    );
    let healthy_peer = build_contents_peer(&committee, &checkpoints, &contents, 2, Duration::ZERO);
    let client = HedgeTestClient::new(
        *checkpoints[0].digest(),
        &[(&slow_peer, 1), (&healthy_peer, 0)],
    )
    .await;
    client.record_slow_peer_responses(slow_peer.peer_id());
    // Three requests to the slow peer were sent already, so the download's first request is
    // the fourth.
    for _ in 0..3 {
        assert!(
            !client
                .peer_heights
                .write()
                .unwrap()
                .slow_peer_probe_for_new_request(&slow_peer.peer_id())
        );
    }
    let announce = client.announce_after(healthy_peer.peer_id(), 1, Duration::from_millis(50));
    let hedge = super::ContentHedge {
        delay: Duration::from_secs(10),
        adaptive_delay: false,
        closer_peer: true,
        highest_synced: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
    };

    let result = client
        .fetch_with_hedge(&checkpoints[1], &hedge, Duration::from_millis(500))
        .await;
    announce.await.unwrap();

    assert!(
        result.is_err(),
        "the probe to the slow peer should not be hedged before the 10 s hedge delay"
    );
    let metrics = &client.metrics;
    assert_eq!(metrics.checkpoint_content_slow_peer_probes("other"), 1);
    assert_eq!(metrics.checkpoint_content_hedges(HedgeEvent::Sent), 0);
}

#[test]
fn test_closer_peer_to_hedge() {
    use super::{CLOSER_PEER_MIN_GAIN, closer_peer_to_hedge};
    use std::time::Instant;

    let ms = Duration::from_millis;
    let start = Instant::now();
    // The request in flight was just sent to a peer expected to answer in 500 ms.
    let in_flight_expected = ms(500);
    let threshold = in_flight_expected - CLOSER_PEER_MIN_GAIN;

    // A peer expected to finish exactly the minimum gain sooner gets the hedge; one expected to
    // finish 1 ms later does not.
    assert_eq!(
        closer_peer_to_hedge(start, start, in_flight_expected, [("a", threshold)]),
        Some("a")
    );
    assert_eq!(
        closer_peer_to_hedge(start, start, in_flight_expected, [("a", threshold + ms(1))]),
        None
    );

    // The peer expected to finish first is picked.
    assert_eq!(
        closer_peer_to_hedge(
            start,
            start,
            in_flight_expected,
            [("a", ms(300)), ("b", ms(100)), ("c", ms(200))]
        ),
        Some("b")
    );

    // The time the request in flight has already run counts: 300 ms in, it is expected to
    // finish in 200 ms.
    let now = start + ms(300);
    let remaining_threshold = ms(200) - CLOSER_PEER_MIN_GAIN;
    assert_eq!(
        closer_peer_to_hedge(now, start, in_flight_expected, [("a", remaining_threshold)]),
        Some("a")
    );
    assert_eq!(
        closer_peer_to_hedge(
            now,
            start,
            in_flight_expected,
            [("a", remaining_threshold + ms(1))]
        ),
        None
    );

    // A request that has run past its expected finish is not hedged early, even to a peer
    // expected to answer right away.
    assert_eq!(
        closer_peer_to_hedge(
            start + ms(600),
            start,
            in_flight_expected,
            [("a", Duration::ZERO)]
        ),
        None
    );

    // Without candidates, there is nothing to hedge to.
    assert_eq!(
        closer_peer_to_hedge(
            start,
            start,
            in_flight_expected,
            Vec::<(&str, Duration)>::new()
        ),
        None
    );
}

#[test]
fn test_content_hedge_window() {
    use super::{CONTENT_HEDGE_WINDOW, ContentHedge};
    use std::sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    };

    let hedge = ContentHedge {
        delay: Duration::from_millis(100),
        adaptive_delay: false,
        closer_peer: false,
        highest_synced: Arc::new(AtomicU64::new(100)),
    };
    assert!(hedge.covers(101));
    assert!(hedge.covers(100 + CONTENT_HEDGE_WINDOW));
    assert!(!hedge.covers(100 + CONTENT_HEDGE_WINDOW + 1));

    // The window end saturates instead of overflowing.
    hedge.highest_synced.store(u64::MAX - 3, Ordering::Relaxed);
    assert!(hedge.covers(u64::MAX));
}

#[test]
fn test_peer_score_slow_peer_hedge_delay() {
    use super::ContentResponseTime::{Answered, LongerThan};
    use super::PeerScore;

    let ms = Duration::from_millis;
    let mut score = PeerScore::new(Duration::from_secs(60), 0.3);
    assert_eq!(score.slow_peer_hedge_delay(), None);

    // The fastest response is 100 ms, so responses over 125 ms are slow. 3 of 9 are slow, but 9
    // samples are too few to tell.
    for _ in 0..6 {
        score.record_content_response_time(Answered(ms(100)), false);
    }
    for _ in 0..3 {
        score.record_content_response_time(LongerThan(ms(300)), false);
    }
    assert_eq!(score.slow_peer_hedge_delay(), None);

    // A response of exactly the threshold is not slow: 3 of 10 slow is over a quarter.
    score.record_content_response_time(Answered(ms(125)), false);
    assert_eq!(score.slow_peer_hedge_delay(), Some(ms(125)));

    // Two more fast responses: 3 of 12 slow is exactly a quarter, still slow.
    for _ in 0..2 {
        score.record_content_response_time(Answered(ms(100)), false);
    }
    assert_eq!(score.slow_peer_hedge_delay(), Some(ms(125)));

    // 3 of 13 is under a quarter.
    score.record_content_response_time(Answered(ms(110)), false);
    assert_eq!(score.slow_peer_hedge_delay(), None);

    // Requests cancelled before the threshold might have been answered in time, so they don't
    // count either way.
    for _ in 0..5 {
        score.record_content_response_time(LongerThan(ms(110)), false);
    }
    assert_eq!(score.slow_peer_hedge_delay(), None);

    // Only the 20 most recent samples count: 17 slow and 3 fast ones make the peer slow, and 20
    // fast ones after that make it not slow again.
    for _ in 0..17 {
        score.record_content_response_time(LongerThan(ms(300)), false);
    }
    for _ in 0..3 {
        score.record_content_response_time(Answered(ms(100)), false);
    }
    assert_eq!(score.slow_peer_hedge_delay(), Some(ms(125)));
    for _ in 0..20 {
        score.record_content_response_time(Answered(ms(100)), false);
    }
    assert_eq!(score.slow_peer_hedge_delay(), None);

    // For a nearby peer, the threshold is at least 20 ms over the fastest response.
    let mut nearby = PeerScore::new(Duration::from_secs(60), 0.3);
    for _ in 0..7 {
        nearby.record_content_response_time(Answered(ms(40)), false);
    }
    for _ in 0..3 {
        nearby.record_content_response_time(LongerThan(ms(100)), false);
    }
    assert_eq!(nearby.slow_peer_hedge_delay(), Some(ms(60)));
}

/// When asked to, a request cancelled before the peer's slow-response threshold, which can't
/// count either way, is not stored, so it doesn't push out samples that count. Without a
/// threshold, or when not asked to, it is stored.
#[test]
fn test_peer_score_drops_samples_cancelled_before_threshold() {
    use super::ContentResponseTime::{Answered, LongerThan};
    use super::PeerScore;

    let ms = Duration::from_millis;

    // No answers yet, so no threshold to compare against: the sample is kept.
    let mut score = PeerScore::new(Duration::from_secs(60), 0.3);
    score.record_content_response_time(LongerThan(ms(50)), true);
    assert_eq!(score.content_response_times.len(), 1);

    // 7 slow and 3 fast samples: slow, with a 125 ms threshold.
    let mut score = PeerScore::new(Duration::from_secs(60), 0.3);
    for _ in 0..7 {
        score.record_content_response_time(LongerThan(ms(300)), true);
    }
    for _ in 0..3 {
        score.record_content_response_time(Answered(ms(100)), true);
    }
    assert_eq!(score.slow_peer_hedge_delay(), Some(ms(125)));

    // Twenty requests cancelled before the threshold take no slots, so the peer stays slow.
    for _ in 0..20 {
        score.record_content_response_time(LongerThan(ms(110)), true);
    }
    assert_eq!(score.content_response_times.len(), 10);
    assert_eq!(score.slow_peer_hedge_delay(), Some(ms(125)));

    // A request cancelled at the threshold is slow and is kept.
    score.record_content_response_time(LongerThan(ms(125)), true);
    assert_eq!(score.content_response_times.len(), 11);

    // Without dropping, a request cancelled before the threshold is stored.
    score.record_content_response_time(LongerThan(ms(110)), false);
    assert_eq!(score.content_response_times.len(), 12);
}

/// While a peer is slow, every fourth request that is not a hedge is a slow-peer probe.
/// Requests sent while it isn't slow are not counted.
#[test]
fn test_peer_score_slow_peer_probe_interval() {
    use super::ContentResponseTime::{Answered, LongerThan};
    use super::{PeerScore, SLOW_PEER_PROBE_INTERVAL};

    let ms = Duration::from_millis;
    let mut score = PeerScore::new(Duration::from_secs(60), 0.3);
    assert_eq!(SLOW_PEER_PROBE_INTERVAL, 4);

    // Not slow: no probes, and nothing counted.
    for _ in 0..10 {
        assert!(!score.slow_peer_probe_for_new_request());
    }
    assert_eq!(score.requests_since_probe, 0);

    for _ in 0..7 {
        score.record_content_response_time(LongerThan(ms(300)), true);
    }
    for _ in 0..3 {
        score.record_content_response_time(Answered(ms(100)), true);
    }
    assert!(score.slow_peer_hedge_delay().is_some());
    let probes: Vec<bool> = (0..8)
        .map(|_| score.slow_peer_probe_for_new_request())
        .collect();
    assert_eq!(
        probes,
        [false, false, false, true, false, false, false, true]
    );
}

#[test]
fn test_peer_score_cancelled_samples_never_raise_throughput() {
    use super::PeerScore;

    let mut score = PeerScore::new(Duration::from_secs(60), 0.3);

    // Without an estimate, a cancelled request says nothing usable and is dropped.
    score.record_cancelled(100_000, Duration::from_millis(400));
    assert!(score.effective_throughput().is_none());

    for _ in 0..7 {
        score.record_success(100_000, Duration::from_secs(1));
    }
    let estimate = score.effective_throughput().unwrap();
    assert!((estimate - 100_000.0).abs() < 0.01);

    // A request cancelled sooner than this peer usually answers would imply a higher
    // throughput. It is only an upper bound, so it must not raise the estimate.
    score.record_cancelled(100_000, Duration::from_millis(400));
    assert!((score.effective_throughput().unwrap() - estimate).abs() < 0.01);

    // A request that ran longer than this peer usually takes lowers the estimate.
    score.record_cancelled(100_000, Duration::from_secs(4));
    assert!(score.effective_throughput().unwrap() < estimate);

    // Cancelled requests count as neither successes nor failures: 3 failures out of 10
    // samples is still a 30% failure rate.
    for _ in 0..3 {
        score.record_failure();
    }
    score.record_cancelled(100_000, Duration::from_secs(10));
    assert!(score.is_failing());
}
