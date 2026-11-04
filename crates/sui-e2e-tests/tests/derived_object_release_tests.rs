// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(msim)]
mod sim_only_tests {
    use std::path::PathBuf;
    use std::time::Duration;
    use sui_macros::sim_test;
    use sui_node::SuiNode;
    use sui_test_transaction_builder::publish_package;
    use sui_types::base_types::ObjectID;
    use sui_types::digests::TransactionDigest;
    use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};
    use sui_types::messages_checkpoint::CheckpointSequenceNumber;
    use sui_types::object::Owner;
    use sui_types::transaction::{CallArg, ObjectArg, SharedObjectMutability};
    use test_cluster::{TestCluster, TestClusterBuilder};
    use tokio::time::timeout;

    #[derive(Clone, Copy, Debug)]
    enum Holding {
        Owned,
        Party,
        Wrapped,
    }

    /// Releases and re-claims a derived object as an owned, party, and wrapped object across several
    /// epochs. Each epoch change sees a re-created object live and runs the state consistency check.
    /// The fullnode then prunes all history of the re-created ID except its live version.
    #[sim_test]
    async fn derived_object_release_across_epochs() {
        let mut test_cluster = TestClusterBuilder::new().build().await;
        let package_id = publish_package(
            &test_cluster.wallet,
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/derived_object_release"),
        )
        .await
        .0;
        let parent_id = call(&test_cluster, package_id, "create_parent", vec![])
            .await
            .created()[0]
            .0
            .0;

        let holdings = [Holding::Owned, Holding::Party, Holding::Wrapped];
        let mut count = 0u64;
        for live_holding in holdings {
            for holding in holdings {
                let held = claim(&test_cluster, package_id, parent_id, holding, &mut count).await;
                release(&test_cluster, package_id, parent_id, holding, held).await;
            }
            let held = claim(
                &test_cluster,
                package_id,
                parent_id,
                live_holding,
                &mut count,
            )
            .await;
            test_cluster.trigger_reconfiguration().await;
            release(&test_cluster, package_id, parent_id, live_holding, held).await;
        }

        let parent = parent_arg(&test_cluster, parent_id).await;
        let effects = call(
            &test_cluster,
            package_id,
            "claim_owned",
            vec![parent, CallArg::Pure(bcs::to_bytes(&count).unwrap())],
        )
        .await;
        let ObjectArg::ImmOrOwnedObject(live) = address_owned(&effects) else {
            unreachable!()
        };
        let digest = *effects.transaction_digest();

        test_cluster
            .fullnode_handle
            .sui_node
            .with_async(|node| async move {
                let checkpoint = timeout(
                    Duration::from_secs(60),
                    wait_until_txn_in_checkpoint(node, &digest),
                )
                .await
                .unwrap();
                timeout(
                    Duration::from_secs(60),
                    wait_until_checkpoint_pruned(node, checkpoint),
                )
                .await
                .unwrap();
                let state = node.state();
                state
                    .database_for_testing()
                    .prune_objects_and_compact_for_testing(state.get_checkpoint_store())
                    .await;
                let cache = state.get_object_cache_reader();
                assert_eq!(
                    cache.get_latest_object_ref_or_tombstone(live.0).unwrap(),
                    live
                );
                assert!(cache.get_object_by_key(&live.0, live.1).is_some());
                assert_eq!(
                    state.database_for_testing().count_object_versions(live.0),
                    1
                );
            })
            .await;

        // A new fullnode executes the whole history from checkpoints.
        let fullnode = test_cluster.spawn_new_fullnode().await.sui_node;
        fullnode
            .state()
            .get_transaction_cache_reader()
            .notify_read_executed_effects("", &[digest])
            .await;
    }

    /// Claims the derived object, held as `holding`, and returns the argument that releases it.
    async fn claim(
        test_cluster: &TestCluster,
        package_id: ObjectID,
        parent_id: ObjectID,
        holding: Holding,
        count: &mut u64,
    ) -> ObjectArg {
        let function = match holding {
            Holding::Owned => "claim_owned",
            Holding::Party => "claim_party",
            Holding::Wrapped => "claim_wrapped",
        };
        let parent = parent_arg(test_cluster, parent_id).await;
        let effects = call(
            test_cluster,
            package_id,
            function,
            vec![parent, CallArg::Pure(bcs::to_bytes(count).unwrap())],
        )
        .await;
        *count += 1;
        match holding {
            Holding::Owned | Holding::Wrapped => address_owned(&effects),
            Holding::Party => party_owned(&effects),
        }
    }

    async fn release(
        test_cluster: &TestCluster,
        package_id: ObjectID,
        parent_id: ObjectID,
        holding: Holding,
        held: ObjectArg,
    ) {
        let function = match holding {
            Holding::Owned | Holding::Party => "release",
            Holding::Wrapped => "release_wrapped",
        };
        let parent = parent_arg(test_cluster, parent_id).await;
        call(
            test_cluster,
            package_id,
            function,
            vec![parent, CallArg::Object(held)],
        )
        .await;
    }

    async fn call(
        test_cluster: &TestCluster,
        package_id: ObjectID,
        function: &'static str,
        args: Vec<CallArg>,
    ) -> TransactionEffects {
        let tx = test_cluster
            .test_transaction_builder()
            .await
            .move_call(package_id, "m", function, args)
            .build();
        let effects = test_cluster.sign_and_execute_transaction(&tx).await.effects;
        assert!(
            effects.status().is_ok(),
            "{function} failed: {:?}",
            effects.status()
        );
        effects
    }

    async fn parent_arg(test_cluster: &TestCluster, parent_id: ObjectID) -> CallArg {
        CallArg::Object(ObjectArg::ImmOrOwnedObject(
            test_cluster.wallet.get_object_ref(parent_id).await.unwrap(),
        ))
    }

    fn address_owned(effects: &TransactionEffects) -> ObjectArg {
        let (obj_ref, _) = effects
            .created()
            .into_iter()
            .find(|(_, owner)| matches!(owner, Owner::AddressOwner(_)))
            .unwrap();
        ObjectArg::ImmOrOwnedObject(obj_ref)
    }

    fn party_owned(effects: &TransactionEffects) -> ObjectArg {
        let ((id, _, _), owner) = effects
            .created()
            .into_iter()
            .find(|(_, owner)| matches!(owner, Owner::ConsensusAddressOwner { .. }))
            .unwrap();
        ObjectArg::SharedObject {
            id,
            initial_shared_version: owner.start_version().unwrap(),
            mutability: SharedObjectMutability::Mutable,
        }
    }

    async fn wait_until_txn_in_checkpoint(
        node: &SuiNode,
        digest: &TransactionDigest,
    ) -> CheckpointSequenceNumber {
        loop {
            if let Some(seq) = node
                .state()
                .epoch_store_for_testing()
                .get_transaction_checkpoint(digest)
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
