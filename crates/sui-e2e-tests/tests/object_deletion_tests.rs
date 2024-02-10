// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(msim)]
mod sim_only_tests {
    use std::path::PathBuf;
    use std::time::Duration;
    use sui_core::authority::authority_store_tables::LiveObject;
    use sui_core::state_accumulator::AccumulatorStore;
    use sui_json_rpc_types::{SuiTransactionBlockEffects, SuiTransactionBlockEffectsAPI};
    use sui_macros::sim_test;
    use sui_node::SuiNode;
    use sui_protocol_config::{ProtocolConfig, ProtocolVersion, SupportedProtocolVersions};
    use sui_test_transaction_builder::publish_package;
    use sui_types::messages_checkpoint::CheckpointSequenceNumber;
    use sui_types::{base_types::ObjectID, digests::TransactionDigest};
    use test_cluster::{TestCluster, TestClusterBuilder};
    use tokio::time::timeout;

    /// This test checks that after we enable simplified_unwrap_then_delete, we no longer depend
    /// on wrapped tombstones when generating effects and using effects.
    #[sim_test]
    async fn test_no_more_dependency_on_wrapped_tombstone() {
        let mut _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
            config.set_simplified_unwrap_then_delete(false);
            config
        });

        let test_cluster = TestClusterBuilder::new()
            .with_supported_protocol_versions(SupportedProtocolVersions::new_for_testing(
                ProtocolVersion::MAX.as_u64(),
                ProtocolVersion::MAX_ALLOWED.as_u64(),
            ))
            .build()
            .await;

        let (package_id, object_id) = publish_package_and_create_parent_object(&test_cluster).await;

        let child_id = create_owned_child(&test_cluster, package_id).await;
        wrap_child(&test_cluster, package_id, object_id, child_id).await;
        assert_eq!(count_fullnode_wrapped_tombstones(&test_cluster), 1);

        // At this point, we should have a wrapped tombstone in the db of every node.

        drop(_guard);
        let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
            config.set_simplified_unwrap_then_delete(true);
            config
        });
        // At this epoch change, we should be re-accumulating without wrapped tombstone and now
        // flips the feature flag simplified_unwrap_then_delete to true.
        test_cluster.trigger_reconfiguration().await;

        // Remove the wrapped tombstone on some nodes but not all.
        for (idx, validator) in test_cluster.swarm.validator_nodes().enumerate() {
            validator.get_node_handle().unwrap().with(|node| {
                let db = node.state().database_for_testing().clone();
                assert_eq!(count_wrapped_tombstone(&node), 1);
                if idx % 2 == 0 {
                    db.remove_all_versions_of_object(child_id);
                    assert_eq!(count_wrapped_tombstone(&node), 0);
                }
            })
        }

        let effects = unwrap_and_delete_child(&test_cluster, package_id, object_id).await;
        assert_eq!(effects.modified_at_versions().len(), 2);
        assert_eq!(effects.unwrapped_then_deleted().len(), 1);

        test_cluster.trigger_reconfiguration().await;
    }

    // Tests that object pruning can prune objects correctly.
    // Specifically, we first wrap a child object into a root object (tests wrap tombstone),
    // then unwrap and delete the child object (tests unwrap and delete),
    // and last delete the root object (tests object deletion).
    #[sim_test]
    async fn object_pruning_test() {
        let test_cluster = TestClusterBuilder::new().build().await;
        let fullnode = &test_cluster.fullnode_handle.sui_node;

        // Create a root object and a child object. Wrap the child object inside the root object.
        let (package_id, object_id) = publish_package_and_create_parent_object(&test_cluster).await;
        let child_id = create_owned_child(&test_cluster, package_id).await;
        let wrap_child_txn_digest = wrap_child(&test_cluster, package_id, object_id, child_id)
            .await
            .transaction_digest()
            .clone();

        fullnode
            .with_async(|node| async {
                // Wait until the wraping transaction is included in checkpoint.
                let checkpoint = timeout(
                    Duration::from_secs(60),
                    wait_until_txn_in_checkpoint(node, &wrap_child_txn_digest),
                )
                .await
                .unwrap();

                // Wait until the above checkpoint is pruned.
                let _ = timeout(
                    Duration::from_secs(60),
                    wait_until_checkpoint_pruned(node, checkpoint),
                )
                .await
                .unwrap();

                // Manually initiating a pruning and compaction job to make sure that deleted objects are gong from object store.
                node.state().prune_objects_and_compact_for_testing().await;

                // Check that no object with `child_id` exists in object store.
                assert_eq!(
                    node.state()
                        .database_for_testing()
                        .count_object_versions(child_id),
                    0
                );
                assert!(
                    node.state()
                        .database_for_testing()
                        .count_object_versions(object_id)
                        > 0
                );
            })
            .await;

        // Next, we unwrap and delete the child object, as well as delete the root object.
        let unwrap_delete_txn_digest =
            unwrap_and_delete_child(&test_cluster, package_id, object_id)
                .await
                .transaction_digest()
                .clone();
        let delete_root_obj_txn_digest = delete_object(&test_cluster, package_id, object_id)
            .await
            .transaction_digest()
            .clone();

        fullnode
            .with_async(|node| async {
                // Wait for both transactions to be included in checkpoint.
                let checkpoint1 = timeout(
                    Duration::from_secs(60),
                    wait_until_txn_in_checkpoint(node, &unwrap_delete_txn_digest),
                )
                .await
                .unwrap();
                let checkpoint2 = timeout(
                    Duration::from_secs(60),
                    wait_until_txn_in_checkpoint(node, &delete_root_obj_txn_digest),
                )
                .await
                .unwrap();

                let _ = timeout(
                    Duration::from_secs(60),
                    wait_until_checkpoint_pruned(node, std::cmp::max(checkpoint1, checkpoint2)),
                )
                .await
                .unwrap();

                // Manually initiating a pruning and compaction job to make sure that deleted objects are gong from object store.
                node.state().prune_objects_and_compact_for_testing().await;

                // Check that both root and child objects are gone from object store.
                assert_eq!(
                    node.state()
                        .database_for_testing()
                        .count_object_versions(child_id),
                    0
                );
                assert_eq!(
                    node.state()
                        .database_for_testing()
                        .count_object_versions(object_id),
                    0
                );
            })
            .await;
    }

    async fn publish_package_and_create_parent_object(
        test_cluster: &TestCluster,
    ) -> (ObjectID, ObjectID) {
        let package_id = publish_package(
            &test_cluster.wallet,
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../sui-surfer/tests/move_building_blocks"),
        )
        .await
        .0;

        let object_id = test_cluster
            .sign_and_execute_transaction(
                &test_cluster
                    .test_transaction_builder()
                    .await
                    .move_call(package_id, "objects", "create_owned_object", vec![])
                    .build(),
            )
            .await
            .effects
            .unwrap()
            .created()[0]
            .reference
            .object_id;

        (package_id, object_id)
    }

    async fn create_owned_child(test_cluster: &TestCluster, package_id: ObjectID) -> ObjectID {
        test_cluster
            .sign_and_execute_transaction(
                &test_cluster
                    .test_transaction_builder()
                    .await
                    .move_call(package_id, "objects", "create_owned_child", vec![])
                    .build(),
            )
            .await
            .effects
            .unwrap()
            .created()[0]
            .reference
            .to_object_ref()
            .0
    }

    async fn wrap_child(
        test_cluster: &TestCluster,
        package_id: ObjectID,
        object_id: ObjectID,
        child_id: ObjectID,
    ) -> SuiTransactionBlockEffects {
        let object = test_cluster.wallet.get_object_ref(object_id).await.unwrap();
        let child = test_cluster.wallet.get_object_ref(child_id).await.unwrap();
        let effects = test_cluster
            .sign_and_execute_transaction(
                &test_cluster
                    .test_transaction_builder()
                    .await
                    .move_call(
                        package_id,
                        "objects",
                        "wrap_child",
                        vec![object.into(), child.into(), true.into()],
                    )
                    .build(),
            )
            .await
            .effects
            .unwrap();
        assert_eq!(effects.wrapped().len(), 1);
        assert!(test_cluster
            .get_object_or_tombstone_from_fullnode_store(child_id)
            .await
            .2
            .is_wrapped());
        effects
    }

    async fn unwrap_and_delete_child(
        test_cluster: &TestCluster,
        package_id: ObjectID,
        object_id: ObjectID,
    ) -> SuiTransactionBlockEffects {
        let object = test_cluster.wallet.get_object_ref(object_id).await.unwrap();
        let effects = test_cluster
            .sign_and_execute_transaction(
                &test_cluster
                    .test_transaction_builder()
                    .await
                    .move_call(
                        package_id,
                        "objects",
                        "unwrap_and_delete_child",
                        vec![object.into()],
                    )
                    .build(),
            )
            .await
            .effects
            .unwrap();
        assert!(effects.deleted().is_empty());
        effects
    }

    async fn delete_object(
        test_cluster: &TestCluster,
        package_id: ObjectID,
        object_id: ObjectID,
    ) -> SuiTransactionBlockEffects {
        let object = test_cluster.wallet.get_object_ref(object_id).await.unwrap();
        let effects = test_cluster
            .sign_and_execute_transaction(
                &test_cluster
                    .test_transaction_builder()
                    .await
                    .move_call(package_id, "objects", "delete", vec![object.into()])
                    .build(),
            )
            .await
            .effects
            .unwrap();
        assert_eq!(effects.deleted().len(), 1);
        effects
    }

    fn count_fullnode_wrapped_tombstones(test_cluster: &TestCluster) -> usize {
        test_cluster
            .fullnode_handle
            .sui_node
            .with(|node| count_wrapped_tombstone(node))
    }

    fn count_wrapped_tombstone(node: &SuiNode) -> usize {
        let store = node.state().get_execution_cache();
        store
            .iter_live_object_set(true)
            .filter(|o| matches!(o, LiveObject::Wrapped(_)))
            .count()
    }

    async fn wait_until_txn_in_checkpoint(
        node: &SuiNode,
        digest: &TransactionDigest,
    ) -> CheckpointSequenceNumber {
        loop {
            if let Some(seq) = node
                .state()
                .epoch_store_for_testing()
                .get_transaction_checkpoint(&digest)
                .unwrap()
            {
                return seq;
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }

    async fn wait_until_checkpoint_pruned(node: &SuiNode, checkpoint: CheckpointSequenceNumber) {
        loop {
            if node
                .state()
                .get_highest_pruned_checkpoint_for_testing()
                .unwrap()
                >= checkpoint
            {
                return;
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
}
