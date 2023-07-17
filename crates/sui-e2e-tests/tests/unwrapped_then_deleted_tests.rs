// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(msim)]
mod sim_only_tests {
    use std::path::PathBuf;
    use sui_core::authority::authority_store_tables::LiveObject;
    use sui_json_rpc_types::{SuiTransactionBlockEffects, SuiTransactionBlockEffectsAPI};
    use sui_macros::sim_test;
    use sui_node::SuiNode;
    use sui_protocol_config::{ProtocolConfig, ProtocolVersion, SupportedProtocolVersions};
    use sui_test_transaction_builder::publish_package;
    use sui_types::base_types::ObjectID;
    use test_cluster::{TestCluster, TestClusterBuilder};

    // This test exercise the protocol upgrade where we flip the feature flag
    // simplified_unwrap_then_delete. It demonstrates the behavior difference before and after
    // this upgrade in unwrapped_then_deleted and modified_at_versions fields in effects.
    #[sim_test]
    async fn test_simplified_unwrap_then_delete_protocol_upgrade() {
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

        create_and_wrap_child(&test_cluster, package_id, object_id).await;
        assert_eq!(count_fullnode_wrapped_tombstones(&test_cluster), 0);

        let effects = unwrap_and_delete_child(&test_cluster, package_id, object_id).await;
        assert!(effects.unwrapped_then_deleted().is_empty());
        // Only include gas and object.
        assert_eq!(effects.modified_at_versions().len(), 2);
        assert_eq!(count_fullnode_wrapped_tombstones(&test_cluster), 0);

        let child = create_owned_child(&test_cluster, package_id).await;
        wrap_child(&test_cluster, package_id, object_id, child).await;
        assert_eq!(count_fullnode_wrapped_tombstones(&test_cluster), 1);

        let effects = unwrap_and_delete_child(&test_cluster, package_id, object_id).await;
        // modified_at_versions includes: gas, object, child.
        assert_eq!(effects.modified_at_versions().len(), 3);
        assert_eq!(effects.unwrapped_then_deleted().len(), 1);
        assert_eq!(count_fullnode_wrapped_tombstones(&test_cluster), 0);
        assert!(test_cluster
            .get_object_or_tombstone_from_fullnode_store(child)
            .await
            .2
            .is_deleted());

        drop(_guard);
        let _guard = ProtocolConfig::apply_overrides_for_testing(|_, mut config| {
            config.set_simplified_unwrap_then_delete(true);
            config
        });
        test_cluster.trigger_reconfiguration().await;

        create_and_wrap_child(&test_cluster, package_id, object_id).await;
        assert_eq!(count_fullnode_wrapped_tombstones(&test_cluster), 0);

        let effects = unwrap_and_delete_child(&test_cluster, package_id, object_id).await;
        // This is where it becomes different after the protocol upgrade.
        assert_eq!(effects.unwrapped_then_deleted().len(), 1);
        // Only include gas and object.
        assert_eq!(effects.modified_at_versions().len(), 2);
        assert_eq!(count_fullnode_wrapped_tombstones(&test_cluster), 0);

        let child_id = create_owned_child(&test_cluster, package_id).await;
        wrap_child(&test_cluster, package_id, object_id, child_id).await;
        assert_eq!(count_fullnode_wrapped_tombstones(&test_cluster), 1);

        let effects = unwrap_and_delete_child(&test_cluster, package_id, object_id).await;
        // This is also different after the protocol upgrade.
        // modified_at_versions only include gas and object, does not include child.
        assert_eq!(effects.modified_at_versions().len(), 2);
        assert_eq!(effects.unwrapped_then_deleted().len(), 1);
        assert_eq!(count_fullnode_wrapped_tombstones(&test_cluster), 0);
        assert!(test_cluster
            .get_object_or_tombstone_from_fullnode_store(child_id)
            .await
            .2
            .is_deleted());
    }

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
                let db = node.state().db();
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

    async fn create_and_wrap_child(
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
                        "create_and_wrap_child",
                        vec![object.into(), true.into()],
                    )
                    .build(),
            )
            .await
            .effects
            .unwrap();
        assert!(effects.created().is_empty() && effects.wrapped().is_empty());
        effects
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

    fn count_fullnode_wrapped_tombstones(test_cluster: &TestCluster) -> usize {
        test_cluster
            .fullnode_handle
            .sui_node
            .with(|node| count_wrapped_tombstone(node))
    }

    fn count_wrapped_tombstone(node: &SuiNode) -> usize {
        let db = node.state().db();
        db.iter_live_object_set(true)
            .filter(|o| matches!(o, LiveObject::Wrapped(_)))
            .count()
    }
}
