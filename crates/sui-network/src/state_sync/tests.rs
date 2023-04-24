// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    state_sync::{
        test_utils::{empty_contents, CommitteeFixture},
        Builder, GetCheckpointSummaryRequest, PeerStateSyncInfo, StateSync, StateSyncMessage,
        UnstartedStateSync,
    },
    utils::build_network,
};
use anemo::{PeerId, Request};
use std::{collections::HashMap, time::Duration};
use sui_types::{
    messages_checkpoint::CheckpointDigest,
    storage::{ReadStore, SharedInMemoryStore, WriteStore},
};
use tokio::time::timeout;

#[tokio::test]
async fn server_push_checkpoint() {
    let committee = CommitteeFixture::generate(rand::rngs::OsRng, 0, 4);
    let (ordered_checkpoints, _sequence_number_to_digest, _checkpoints) =
        committee.make_checkpoints(2, None);
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
    let (ordered_checkpoints, _sequence_number_to_digest, _checkpoints) =
        committee.make_checkpoints(3, None);

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
        builder.store.inner_mut().insert_checkpoint(checkpoint)
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
    let (ordered_checkpoints, sequence_number_to_digest, checkpoints) =
        committee.make_checkpoints(100, None);

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
            store.insert_checkpoint(checkpoint);
        }
    }

    // Node 1 will know that Node 2 has the data
    event_loop_1.peer_heights.write().unwrap().peers.insert(
        network_2.peer_id(),
        PeerStateSyncInfo {
            genesis_checkpoint_digest: *ordered_checkpoints[0].digest(),
            on_same_chain_as_us: true,
            height: *ordered_checkpoints.last().unwrap().sequence_number(),
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
    let (ordered_checkpoints, sequence_number_to_digest, checkpoints) =
        committee.make_checkpoints(4, None);

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
