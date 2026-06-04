// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeSet, sync::Arc, time::Duration};

use consensus_config::{NetworkPublicKey, ObserverParameters, PeerRecord};
use sui_macros::sim_test;
use sui_types::crypto::KeypairTraits;
use sui_types::node_role::FullNodeSyncMode;
use test_cluster::TestClusterBuilder;
use tracing::info;

/// Helper to build observer peers from a test cluster's validator nodes.
fn build_observer_peers(test_cluster: &test_cluster::TestCluster) -> Vec<PeerRecord> {
    test_cluster
        .swarm
        .validator_nodes()
        .filter_map(|v| {
            let config = v.config();
            let consensus_config = config.consensus_config.as_ref()?;
            let observer_port = consensus_config
                .parameters
                .as_ref()
                .and_then(|p| p.observer.server_port)?;

            let network_public_key =
                NetworkPublicKey::new(config.network_key_pair().public().clone());

            let host = config
                .network_address
                .to_socket_addr()
                .unwrap()
                .ip()
                .to_string();

            let address: sui_types::multiaddr::Multiaddr =
                format!("/ip4/{}/udp/{}/http", host, observer_port)
                    .parse()
                    .unwrap();

            Some(PeerRecord {
                public_key: network_public_key,
                address,
            })
        })
        .take(1)
        .collect()
}

/// Verifies that an observer full node takes the verify-locally-built-checkpoint
/// path in the checkpoint executor. The observer processes consensus commits
/// (executes transactions + builds checkpoints locally), so the checkpoint executor
/// should find locally computed checkpoints and verify them rather than re-executing
/// transactions from synced checkpoints.
#[sim_test]
async fn test_observer_uses_verify_checkpoint_path() {
    telemetry_subscribers::init_for_testing();

    let mut test_cluster = TestClusterBuilder::new()
        .with_num_validators(4)
        .with_validator_observer_config(Arc::new(|_idx| Some(ObserverParameters::default())))
        .build()
        .await;

    let observer_peers = build_observer_peers(&test_cluster);
    assert!(
        !observer_peers.is_empty(),
        "need at least one observer peer"
    );

    let observer_config = test_cluster
        .fullnode_config_builder()
        .with_observer_config(ObserverParameters {
            peers: observer_peers,
            ..Default::default()
        })
        .build(&mut rand::rngs::OsRng, test_cluster.swarm.config());

    let observer_handle = test_cluster
        .start_fullnode_from_config(observer_config)
        .await;
    let observer_state = observer_handle.sui_node.state();

    // Confirm the observer has the correct role.
    let node_role = observer_state.epoch_store_for_testing().node_role();
    assert!(node_role.is_fullnode());
    assert!(node_role.runs_consensus());
    assert_eq!(
        node_role,
        sui_types::node_role::NodeRole::FullNode(FullNodeSyncMode::ConsensusObserver)
    );

    // Submit transactions, capturing their digests.
    let sender = test_cluster.get_address_0();
    let mut tx_digests = Vec::new();
    for _ in 0..5 {
        let (digest, _effects) = test_cluster
            .sign_and_execute_transaction_directly(
                &test_cluster
                    .test_transaction_builder_with_sender(sender)
                    .await
                    .transfer_sui(None, sender)
                    .build(),
            )
            .await
            .unwrap();
        tx_digests.push(digest);
    }

    let checkpoint_store = observer_state.get_checkpoint_store();

    let checkpoint_seqs = tokio::time::timeout(Duration::from_secs(30), async {
        let checkpoint_seqs = observer_state
            .epoch_store_for_testing()
            .transactions_executed_in_checkpoint_notify(tx_digests.clone())
            .await
            .expect("observer should finalize submitted transactions");
        let max_checkpoint_seq = checkpoint_seqs
            .iter()
            .copied()
            .max()
            .expect("submitted transactions should have checkpoints");
        checkpoint_store
            .notify_read_executed_checkpoint(max_checkpoint_seq)
            .await;
        checkpoint_seqs
    })
    .await
    .expect("timed out waiting for observer to execute submitted transactions in checkpoints");

    let checkpoint_seq_set = checkpoint_seqs.iter().copied().collect::<BTreeSet<_>>();
    let max_checkpoint_seq = *checkpoint_seq_set
        .iter()
        .next_back()
        .expect("submitted transactions should have checkpoints");

    info!(
        "Observer executed submitted transactions in checkpoints {:?}",
        checkpoint_seq_set
    );
    assert!(
        max_checkpoint_seq > 0,
        "Observer should execute submitted transactions after the genesis checkpoint"
    );

    // Verify that the observer built the transaction checkpoints locally.
    // Locally computed checkpoints are produced by the checkpoint builder when the
    // node processes consensus commits. Their presence proves the observer entered
    // verify_locally_built_checkpoint (which looks them up) rather than always
    // falling back to the synced execution path.
    let missing_local_checkpoints = checkpoint_seq_set
        .iter()
        .copied()
        .filter(|seq| {
            checkpoint_store
                .get_locally_computed_checkpoint(*seq)
                .expect("db error")
                .is_none()
        })
        .collect::<Vec<_>>();

    assert!(
        missing_local_checkpoints.is_empty(),
        "Observer should have locally built every checkpoint containing submitted transactions, missing {:?}",
        missing_local_checkpoints,
    );

    // Verify that the RPC index is being populated on the observer.
    // The verify path calls process_checkpoint_data which feeds index_checkpoint,
    // and the pipeline then commits the index updates. A non-zero highest indexed
    // checkpoint proves the indexing pipeline is working end-to-end.
    let rpc_index = observer_state
        .rpc_index
        .as_ref()
        .expect("observer should have an rpc_index");
    let highest_indexed = rpc_index
        .get_highest_indexed_checkpoint_seq_number()
        .expect("db error")
        .unwrap_or(0);

    info!("Observer highest indexed checkpoint: {}", highest_indexed);
    assert!(
        highest_indexed >= max_checkpoint_seq,
        "Observer RPC index should have indexed through checkpoint {}",
        max_checkpoint_seq
    );

    // Verify the legacy IndexStore post-processing pipeline works.
    // commit_post_processing_index_batches collects per-transaction index data
    // (built during execution) and commits it at checkpoint boundaries. If this
    // works, submitted transactions will have a sequence number in the index.
    let index_store = observer_state
        .indexes
        .as_ref()
        .expect("observer should have an IndexStore");
    let missing_indexed_txs = tx_digests
        .iter()
        .copied()
        .filter(|digest| {
            index_store
                .get_transaction_seq(digest)
                .expect("db error")
                .is_none()
        })
        .collect::<Vec<_>>();

    info!(
        "Observer IndexStore indexed {} submitted transactions",
        tx_digests.len() - missing_indexed_txs.len(),
    );
    assert!(
        missing_indexed_txs.is_empty(),
        "Observer IndexStore should have indexed every submitted transaction, missing {:?}",
        missing_indexed_txs,
    );
}
