// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{sync::Arc, time::Duration};

use bytes::Bytes;
use consensus_config::NetworkKeyPair;
use futures::StreamExt as _;
use parking_lot::Mutex;
use rstest::rstest;
use tokio::time::sleep;

use super::{
    anemo_network::AnemoManager, test_network::TestService, tonic_network::TonicManager,
    ExtendedSerializedBlock, NetworkClient, NetworkManager,
};
use crate::{
    block::{TestBlock, VerifiedBlock},
    context::Context,
    Round,
};

trait ManagerBuilder {
    fn build(
        &self,
        context: Arc<Context>,
        network_keypair: NetworkKeyPair,
    ) -> impl NetworkManager<Mutex<TestService>>;
}

struct AnemoManagerBuilder {}

impl ManagerBuilder for AnemoManagerBuilder {
    fn build(
        &self,
        context: Arc<Context>,
        network_keypair: NetworkKeyPair,
    ) -> impl NetworkManager<Mutex<TestService>> {
        AnemoManager::new(context, network_keypair)
    }
}

struct TonicManagerBuilder {}

impl ManagerBuilder for TonicManagerBuilder {
    fn build(
        &self,
        context: Arc<Context>,
        network_keypair: NetworkKeyPair,
    ) -> impl NetworkManager<Mutex<TestService>> {
        TonicManager::new(context, network_keypair)
    }
}

fn block_for_round(round: Round) -> ExtendedSerializedBlock {
    ExtendedSerializedBlock {
        block: Bytes::from(vec![round as u8; 16]),
        excluded_ancestors: vec![],
    }
}

fn service_with_own_blocks() -> Arc<Mutex<TestService>> {
    let service = Arc::new(Mutex::new(TestService::new()));
    {
        let mut service = service.lock();
        let own_blocks = (0..=100u8)
            .map(|i| block_for_round(i as Round))
            .collect::<Vec<_>>();
        service.add_own_blocks(own_blocks);
    }
    service
}

// TODO: figure out the issue with using simulated time with tonic in this test.
// When waiting for the server to become ready, it may need to use std::thread::sleep()
// instead of tokio::time::sleep().
#[rstest]
#[tokio::test]
async fn send_and_receive_blocks_with_auth(
    #[values(AnemoManagerBuilder {}, TonicManagerBuilder {})] manager_builder: impl ManagerBuilder,
) {
    let (context, keys) = Context::new_for_test(4);

    let context_0 = Arc::new(
        context
            .clone()
            .with_authority_index(context.committee.to_authority_index(0).unwrap()),
    );
    let mut manager_0 = manager_builder.build(context_0.clone(), keys[0].0.clone());
    let client_0 = manager_0.client();
    let service_0 = service_with_own_blocks();
    manager_0.install_service(service_0.clone()).await;

    let context_1 = Arc::new(
        context
            .clone()
            .with_authority_index(context.committee.to_authority_index(1).unwrap()),
    );
    let mut manager_1 = manager_builder.build(context_1.clone(), keys[1].0.clone());
    let client_1 = manager_1.client();
    let service_1 = service_with_own_blocks();
    manager_1.install_service(service_1.clone()).await;

    // Wait for anemo to initialize.
    sleep(Duration::from_secs(5)).await;

    // Test that servers can receive client RPCs.
    let test_block_0 = VerifiedBlock::new_for_test(TestBlock::new(9, 0).build());
    client_0
        .send_block(
            context.committee.to_authority_index(1).unwrap(),
            &test_block_0,
            Duration::from_secs(5),
        )
        .await
        .unwrap();
    let test_block_1 = VerifiedBlock::new_for_test(TestBlock::new(9, 1).build());
    client_1
        .send_block(
            context.committee.to_authority_index(0).unwrap(),
            &test_block_1,
            Duration::from_secs(5),
        )
        .await
        .unwrap();

    assert_eq!(service_0.lock().handle_send_block.len(), 1);
    assert_eq!(service_0.lock().handle_send_block[0].0.value(), 1);
    assert_eq!(
        service_0.lock().handle_send_block[0].1,
        ExtendedSerializedBlock {
            block: test_block_1.serialized().clone(),
            excluded_ancestors: vec![],
        },
    );
    assert_eq!(service_1.lock().handle_send_block.len(), 1);
    assert_eq!(service_1.lock().handle_send_block[0].0.value(), 0);
    assert_eq!(
        service_1.lock().handle_send_block[0].1,
        ExtendedSerializedBlock {
            block: test_block_0.serialized().clone(),
            excluded_ancestors: vec![],
        },
    );

    // `Committee` is generated with the same random seed in Context::new_for_test(),
    // so the first 4 authorities are the same.
    let (context_4, keys_4) = Context::new_for_test(5);
    let context_4 = Arc::new(
        context_4
            .clone()
            .with_authority_index(context_4.committee.to_authority_index(4).unwrap()),
    );
    let mut manager_4 = manager_builder.build(context_4.clone(), keys_4[4].0.clone());
    let client_4 = manager_4.client();
    let service_4 = service_with_own_blocks();
    manager_4.install_service(service_4.clone()).await;

    // client_4 should not be able to reach service_0 or service_1, because of the
    // AllowedPeers filter.
    let test_block_2 = VerifiedBlock::new_for_test(TestBlock::new(9, 2).build());
    assert!(client_4
        .send_block(
            context.committee.to_authority_index(0).unwrap(),
            &test_block_2,
            Duration::from_secs(5),
        )
        .await
        .is_err());
    let test_block_3 = VerifiedBlock::new_for_test(TestBlock::new(9, 3).build());
    assert!(client_4
        .send_block(
            context.committee.to_authority_index(1).unwrap(),
            &test_block_3,
            Duration::from_secs(5),
        )
        .await
        .is_err());
}

#[rstest]
#[tokio::test]
async fn subscribe_and_receive_blocks(
    // Only network supporting streaming can be tested.
    #[values(TonicManagerBuilder {})] manager_builder: impl ManagerBuilder,
) {
    let (context, keys) = Context::new_for_test(4);

    let context_0 = Arc::new(
        context
            .clone()
            .with_authority_index(context.committee.to_authority_index(0).unwrap()),
    );
    let mut manager_0 = manager_builder.build(context_0.clone(), keys[0].0.clone());
    let client_0 = manager_0.client();
    let service_0 = service_with_own_blocks();
    manager_0.install_service(service_0.clone()).await;

    let context_1 = Arc::new(
        context
            .clone()
            .with_authority_index(context.committee.to_authority_index(1).unwrap()),
    );
    let mut manager_1 = manager_builder.build(context_1.clone(), keys[1].0.clone());
    let client_1 = manager_1.client();
    let service_1 = service_with_own_blocks();
    manager_1.install_service(service_1.clone()).await;

    let client_0_round = 50;
    let receive_stream_0 = client_0
        .subscribe_blocks(
            context_0.committee.to_authority_index(1).unwrap(),
            client_0_round,
            Duration::from_secs(5),
        )
        .await
        .unwrap();

    let count = receive_stream_0
        .enumerate()
        .then(|(i, item)| async move {
            assert_eq!(item, block_for_round(client_0_round + i as Round + 1));
            1
        })
        .fold(0, |a, b| async move { a + b })
        .await;
    // Round 51 to 100 blocks should have been received.
    assert_eq!(count, 50);

    let client_1_round = 100;
    let mut receive_stream_1 = client_1
        .subscribe_blocks(
            context_1.committee.to_authority_index(0).unwrap(),
            client_1_round,
            Duration::from_secs(5),
        )
        .await
        .unwrap();
    assert!(receive_stream_1.next().await.is_none());
}
