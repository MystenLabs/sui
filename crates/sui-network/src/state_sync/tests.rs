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
use std::{collections::HashMap, time::Duration};
use sui_swarm_config::test_utils::{empty_contents, CommitteeFixture};
use sui_types::{
    messages_checkpoint::CheckpointDigest,
    storage::{ReadStore, SharedInMemoryStore, WriteStore},
};
use tokio::time::timeout;

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
    ) = Builder::new().store(store).build_internal();
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
            .highest_known_checkpoint()
            .unwrap()
            .data(),
        checkpoint.data(),
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
    let (builder, server) = Builder::new().store(SharedInMemoryStore::default()).build();
    let network_1 = build_network(|router| router.add_rpc_service(server));
    let (mut event_loop_1, _handle_1) = builder.build(network_1.clone());
    let (builder, server) = Builder::new().store(SharedInMemoryStore::default()).build();
    let network_2 = build_network(|router| router.add_rpc_service(server));
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
async fn sync_with_checkpoints_being_inserted() {
    telemetry_subscribers::init_for_testing();
    let committee = CommitteeFixture::generate(rand::rngs::OsRng, 0, 4);
    // build mock data
    let (ordered_checkpoints, _contents, sequence_number_to_digest, checkpoints) =
        committee.make_empty_checkpoints(4, None);

    // Build and connect two nodes
    let (builder, server) = Builder::new().store(SharedInMemoryStore::default()).build();
    let network_1 = build_network(|router| router.add_rpc_service(server));
    let (event_loop_1, handle_1) = builder.build(network_1.clone());
    let (builder, server) = Builder::new().store(SharedInMemoryStore::default()).build();
    let network_2 = build_network(|router| router.add_rpc_service(server));
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

#[tokio::test]
async fn sync_with_checkpoints_watermark() {
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
    let (builder, server) = Builder::new().store(SharedInMemoryStore::default()).build();
    let network_1 = build_network(|router| router.add_rpc_service(server));
    let (event_loop_1, handle_1) = builder.build(network_1.clone());
    let (builder, server) = Builder::new().store(SharedInMemoryStore::default()).build();
    let network_2 = build_network(|router| router.add_rpc_service(server));
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
    for (checkpoint, contents) in checkpoint_iter.zip(contents_iter) {
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
            .zip(contents.clone().into_iter().skip(2))
        {
            assert_eq!(subscriber_1.recv().await.unwrap().data(), checkpoint.data());
            let content_digest = contents.into_checkpoint_contents_digest();
            store_1
                .get_full_checkpoint_contents(&content_digest)
                .unwrap()
                .unwrap();
            assert_eq!(
                store_2
                    .get_full_checkpoint_contents(&content_digest)
                    .unwrap(),
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
    let (builder, server) = Builder::new().store(SharedInMemoryStore::default()).build();
    let network_3 = build_network(|router| router.add_rpc_service(server));
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

    // Peer 3 is able to sync checkpoint 1 with teh help from Peer 2
    timeout(Duration::from_secs(1), async {
        assert_eq!(
            subscriber_3.recv().await.unwrap().data(),
            ordered_checkpoints[1].data()
        );
        let content_digest = contents[1].clone().into_checkpoint_contents_digest();
        store_3
            .get_full_checkpoint_contents(&content_digest)
            .unwrap()
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
    timeout(Duration::from_secs(6), async {
        for (checkpoint, contents) in ordered_checkpoints[2..]
            .iter()
            .zip(contents.clone().into_iter().skip(2))
        {
            assert_eq!(subscriber_2.recv().await.unwrap().data(), checkpoint.data());
            assert_eq!(subscriber_3.recv().await.unwrap().data(), checkpoint.data());
            let content_digest = contents.into_checkpoint_contents_digest();
            store_2
                .get_full_checkpoint_contents(&content_digest)
                .unwrap()
                .unwrap();
            store_3
                .get_full_checkpoint_contents(&content_digest)
                .unwrap()
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
    let (builder, server) = Builder::new().store(SharedInMemoryStore::default()).build();
    let network_4 = build_network(|router| router.add_rpc_service(server));
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
            .zip(contents.clone().into_iter().skip(1))
        {
            assert_eq!(subscriber_4.recv().await.unwrap().data(), checkpoint.data());
            let content_digest = contents.into_checkpoint_contents_digest();
            store_4
                .get_full_checkpoint_contents(&content_digest)
                .unwrap()
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
