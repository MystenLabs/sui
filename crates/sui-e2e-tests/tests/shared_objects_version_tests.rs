// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use sui_macros::*;
use sui_test_transaction_builder::publish_package;
use sui_types::base_types::{ObjectID, ObjectRef, SequenceNumber};
use sui_types::effects::TransactionEffectsAPI;
use sui_types::effects::{TransactionEffects, TransactionEvents};
use sui_types::execution_status::{ExecutionFailureStatus, ExecutionStatus};
use sui_types::object::{Owner, OBJECT_START_VERSION};
use sui_types::transaction::{CallArg, ObjectArg};
use sui_types::SUI_FRAMEWORK_ADDRESS;
use test_cluster::{TestCluster, TestClusterBuilder};

#[sim_test]
async fn fresh_shared_object_initial_version_matches_current() {
    let env = TestEnvironment::new().await;
    let ((_, curr, _), owner) = env.create_shared_counter().await;
    assert!(is_shared_at(&owner, curr));
}

#[sim_test]
async fn objects_transitioning_to_shared_remember_their_previous_version() {
    let env = TestEnvironment::new().await;
    let (counter, _) = env.create_counter().await;

    let (counter, _) = env.increment_owned_counter(counter).await;
    assert_ne!(counter.1, OBJECT_START_VERSION);

    let ExecutionFailureStatus::MoveAbort(location, code) =
        env.share_counter(counter).await.unwrap_err()
    else {
        panic!()
    };
    assert_eq!(location.module.address(), &SUI_FRAMEWORK_ADDRESS);
    assert_eq!(location.module.name().as_str(), "transfer");
    assert_eq!(code, 0 /* ESharedNonNewObject */);
}

#[sim_test]
async fn shared_object_owner_doesnt_change_on_write() {
    let env = TestEnvironment::new().await;
    let (counter, _) = env.create_counter().await;

    let (inc_counter, _) = env.increment_owned_counter(counter).await;
    let ExecutionFailureStatus::MoveAbort(location, code) =
        env.share_counter(inc_counter).await.unwrap_err()
    else {
        panic!()
    };
    assert_eq!(location.module.address(), &SUI_FRAMEWORK_ADDRESS);
    assert_eq!(location.module.name().as_str(), "transfer");
    assert_eq!(code, 0 /* ESharedNonNewObject */);
}

#[sim_test]
async fn initial_shared_version_mismatch_start_version() {
    let env = TestEnvironment::new().await;
    let (counter, _) = env.create_counter().await;

    let (counter, _) = env.increment_owned_counter(counter).await;
    let ExecutionFailureStatus::MoveAbort(location, code) =
        env.share_counter(counter).await.unwrap_err()
    else {
        panic!()
    };
    assert_eq!(location.module.address(), &SUI_FRAMEWORK_ADDRESS);
    assert_eq!(location.module.name().as_str(), "transfer");
    assert_eq!(code, 0 /* ESharedNonNewObject */);
}

#[sim_test]
async fn initial_shared_version_mismatch_current_version() {
    let env = TestEnvironment::new().await;
    let (counter, _) = env.create_counter().await;

    let ExecutionFailureStatus::MoveAbort(location, code) =
        env.share_counter(counter).await.unwrap_err()
    else {
        panic!()
    };
    assert_eq!(location.module.address(), &SUI_FRAMEWORK_ADDRESS);
    assert_eq!(location.module.name().as_str(), "transfer");
    assert_eq!(code, 0 /* ESharedNonNewObject */);
}

#[sim_test]
async fn shared_object_not_found() {
    let env = TestEnvironment::new().await;
    let nonexistent_id = ObjectID::random();
    let initial_shared_seq = SequenceNumber::from_u64(42);
    assert!(env
        .increment_shared_counter(nonexistent_id, initial_shared_seq)
        .await
        .is_err());
}

fn is_shared_at(owner: &Owner, version: SequenceNumber) -> bool {
    if let Owner::Shared {
        initial_shared_version,
    } = owner
    {
        &version == initial_shared_version
    } else {
        false
    }
}

struct TestEnvironment {
    test_cluster: TestCluster,
    move_package: ObjectID,
}

impl TestEnvironment {
    async fn new() -> Self {
        let test_cluster = TestClusterBuilder::new().build().await;

        let move_package = publish_move_package(&test_cluster).await.0;

        Self {
            test_cluster,
            move_package,
        }
    }

    async fn move_call(
        &self,
        function: &'static str,
        arguments: Vec<CallArg>,
    ) -> anyhow::Result<(TransactionEffects, TransactionEvents)> {
        let transaction = self
            .test_cluster
            .test_transaction_builder()
            .await
            .move_call(
                self.move_package,
                "shared_objects_version",
                function,
                arguments,
            )
            .build();
        let transaction = self.test_cluster.wallet.sign_transaction(&transaction);
        self.test_cluster
            .execute_transaction_return_raw_effects(transaction)
            .await
    }

    async fn create_counter(&self) -> (ObjectRef, Owner) {
        let (fx, _) = self.move_call("create_counter", vec![]).await.unwrap();
        assert!(fx.status().is_ok());

        fx.created()
            .iter()
            .find(|(_, owner)| matches!(owner, Owner::AddressOwner(_)))
            .cloned()
            .expect("Owned object created")
    }

    async fn create_shared_counter(&self) -> (ObjectRef, Owner) {
        let (fx, _) = self
            .move_call("create_shared_counter", vec![])
            .await
            .unwrap();
        assert!(fx.status().is_ok());

        fx.created()
            .iter()
            .find(|(_, owner)| owner.is_shared())
            .cloned()
            .expect("Shared object created")
    }

    async fn share_counter(
        &self,
        counter: ObjectRef,
    ) -> Result<(ObjectRef, Owner), ExecutionFailureStatus> {
        let (fx, _) = self
            .move_call(
                "share_counter",
                vec![CallArg::Object(ObjectArg::ImmOrOwnedObject(counter))],
            )
            .await
            .unwrap();

        if let ExecutionStatus::Failure { error, .. } = fx.status() {
            return Err(error.clone());
        }

        Ok(fx
            .mutated()
            .iter()
            .find(|(obj, _)| obj.0 == counter.0)
            .cloned()
            .expect("Counter mutated"))
    }

    async fn increment_owned_counter(&self, counter: ObjectRef) -> (ObjectRef, Owner) {
        let (fx, _) = self
            .move_call(
                "increment_counter",
                vec![CallArg::Object(ObjectArg::ImmOrOwnedObject(counter))],
            )
            .await
            .unwrap();

        fx.mutated()
            .iter()
            .find(|(obj, _)| obj.0 == counter.0)
            .cloned()
            .expect("Counter modified")
    }

    async fn increment_shared_counter(
        &self,
        counter: ObjectID,
        initial_shared_version: SequenceNumber,
    ) -> anyhow::Result<(ObjectRef, Owner)> {
        let (fx, _) = self
            .move_call(
                "increment_counter",
                vec![CallArg::Object(ObjectArg::SharedObject {
                    id: counter,
                    initial_shared_version,
                    mutable: true,
                })],
            )
            .await?;

        Ok(fx
            .mutated()
            .iter()
            .find(|(obj, _)| obj.0 == counter)
            .cloned()
            .expect("Counter modified"))
    }
}

async fn publish_move_package(test_cluster: &TestCluster) -> ObjectRef {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/move_test_code");
    publish_package(&test_cluster.wallet, path).await
}
