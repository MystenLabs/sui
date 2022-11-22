// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use sui_config::{NetworkConfig, ValidatorInfo};
use sui_macros::*;
use sui_node::SuiNodeHandle;
use sui_types::base_types::{ObjectRef, SequenceNumber};
use sui_types::error::{SuiError, SuiResult};
use sui_types::messages::{
    CallArg, ExecutionFailureStatus, ExecutionStatus, ObjectArg, TransactionEffects,
};
use sui_types::object::{Object, Owner, OBJECT_START_VERSION};
use sui_types::SUI_FRAMEWORK_ADDRESS;
use test_utils::authority::{spawn_test_authorities, test_authority_configs};
use test_utils::messages::move_transaction;
use test_utils::objects::test_gas_objects;
use test_utils::transaction::{
    publish_package, submit_shared_object_transaction, submit_single_owner_transaction,
};

#[sim_test]
async fn fresh_shared_objects_get_start_version() {
    let mut env = TestEnvironment::new().await;
    let (_, owner) = env.create_shared_counter().await;
    assert!(is_shared_at(&owner, OBJECT_START_VERSION));
}

#[sim_test]
async fn objects_transitioning_to_shared_remember_their_previous_version() {
    let mut env = TestEnvironment::new().await;
    let (counter, _) = env.create_counter().await;

    let (counter, _) = env.increment_owned_counter(counter).await;
    assert_ne!(counter.1, OBJECT_START_VERSION);

    let ExecutionFailureStatus::MoveAbort(location, code) =
        env.share_counter(counter).await.unwrap_err() else { panic!() };
    assert_eq!(location.module.address(), &SUI_FRAMEWORK_ADDRESS);
    assert_eq!(location.module.name().as_str(), "transfer");
    assert_eq!(code, 0 /* ESharedNonNewObject */);
    // assert_ne!(counter.1, OBJECT_START_VERSION);
    // assert!(is_shared_at(&owner, counter.1));
}

#[sim_test]
async fn shared_object_owner_doesnt_change_on_write() {
    let mut env = TestEnvironment::new().await;
    let (counter, _) = env.create_counter().await;

    let (inc_counter, _) = env.increment_owned_counter(counter).await;
    let ExecutionFailureStatus::MoveAbort(location, code) =
        env.share_counter(inc_counter).await.unwrap_err() else { panic!() };
    assert_eq!(location.module.address(), &SUI_FRAMEWORK_ADDRESS);
    assert_eq!(location.module.name().as_str(), "transfer");
    assert_eq!(code, 0 /* ESharedNonNewObject */);
    // let (_, new_owner) = env
    //     .increment_shared_counter(old_counter, old_counter.1)
    //     .await
    //     .expect("Successful shared increment");

    // assert_eq!(new_owner, old_owner);
}

#[sim_test]
async fn initial_shared_version_mismatch_start_version() {
    let mut env = TestEnvironment::new().await;
    let (counter, _) = env.create_counter().await;

    let (counter, _) = env.increment_owned_counter(counter).await;
    let ExecutionFailureStatus::MoveAbort(location, code) =
        env.share_counter(counter).await.unwrap_err() else { panic!() };
    assert_eq!(location.module.address(), &SUI_FRAMEWORK_ADDRESS);
    assert_eq!(location.module.name().as_str(), "transfer");
    assert_eq!(code, 0 /* ESharedNonNewObject */);

    // let fx = env
    //     .increment_shared_counter(counter, OBJECT_START_VERSION)
    //     .await;

    // let err = fx.expect_err("Transaction fails");
    // assert!(
    //     is_txn_input_error(&err, "SharedObjectStartingVersionMismatch"),
    //     "{}",
    //     err
    // );
}

#[sim_test]
async fn initial_shared_version_mismatch_current_version() {
    let mut env = TestEnvironment::new().await;
    let (counter, _) = env.create_counter().await;

    let ExecutionFailureStatus::MoveAbort(location, code) =
        env.share_counter(counter).await.unwrap_err() else { panic!() };
    assert_eq!(location.module.address(), &SUI_FRAMEWORK_ADDRESS);
    assert_eq!(location.module.name().as_str(), "transfer");
    assert_eq!(code, 0 /* ESharedNonNewObject */);
    // let (counter, _) = env
    //     .increment_shared_counter(counter, counter.1)
    //     .await
    //     .unwrap();

    // let fx = env.increment_shared_counter(counter, counter.1).await;
    // let err = fx.expect_err("Transaction fails");
    // assert!(
    //     is_txn_input_error(&err, "SharedObjectStartingVersionMismatch"),
    //     "{}",
    //     err
    // );
}

#[sim_test]
async fn initial_shared_version_mismatch_arbitrary() {
    let mut env = TestEnvironment::new().await;
    let (counter, _) = env.create_shared_counter().await;

    let fx = env
        .increment_shared_counter(counter, SequenceNumber::from_u64(42))
        .await;
    let err = fx.expect_err("Transaction fails");
    assert!(
        is_txn_input_error(&err, "SharedObjectPriorVersionsPendingExecution"),
        "{}",
        err
    );
}

fn is_txn_input_error(err: &SuiError, err_case: &str) -> bool {
    err.to_string().contains(&format!(
        "Error checking transaction input objects: [{err_case}"
    ))
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
    gas_objects: Vec<Object>,
    configs: NetworkConfig,
    #[allow(dead_code)]
    node_handles: Vec<SuiNodeHandle>,
    move_package: ObjectRef,
}

impl TestEnvironment {
    async fn new() -> Self {
        let mut gas_objects = test_gas_objects();
        let configs = test_authority_configs();
        let node_handles = spawn_test_authorities(gas_objects.clone(), &configs).await;

        let move_package =
            publish_move_package(gas_objects.pop().unwrap(), configs.validator_set()).await;

        Self {
            gas_objects,
            configs,
            node_handles,
            move_package,
        }
    }

    async fn owned_move_call(
        &mut self,
        function: &'static str,
        arguments: Vec<CallArg>,
    ) -> TransactionEffects {
        submit_single_owner_transaction(
            move_transaction(
                self.gas_objects.pop().unwrap(),
                "shared_objects_version",
                function,
                self.move_package,
                arguments,
            ),
            self.configs.validator_set(),
        )
        .await
    }

    async fn shared_move_call(
        &mut self,
        function: &'static str,
        arguments: Vec<CallArg>,
    ) -> SuiResult<TransactionEffects> {
        submit_shared_object_transaction(
            move_transaction(
                self.gas_objects.pop().unwrap(),
                "shared_objects_version",
                function,
                self.move_package,
                arguments,
            ),
            self.configs.validator_set(),
        )
        .await
    }

    async fn create_counter(&mut self) -> (ObjectRef, Owner) {
        let fx = self.owned_move_call("create_counter", vec![]).await;
        assert!(fx.status.is_ok());

        *fx.created
            .iter()
            .find(|(_, owner)| matches!(owner, Owner::AddressOwner(_)))
            .expect("Owned object created")
    }

    async fn create_shared_counter(&mut self) -> (ObjectRef, Owner) {
        let fx = self.owned_move_call("create_shared_counter", vec![]).await;
        assert!(fx.status.is_ok());

        *fx.created
            .iter()
            .find(|(_, owner)| owner.is_shared())
            .expect("Shared object created")
    }

    async fn share_counter(
        &mut self,
        counter: ObjectRef,
    ) -> Result<(ObjectRef, Owner), ExecutionFailureStatus> {
        let fx = self
            .owned_move_call(
                "share_counter",
                vec![CallArg::Object(ObjectArg::ImmOrOwnedObject(counter))],
            )
            .await;

        if let ExecutionStatus::Failure { error } = fx.status {
            return Err(error);
        }

        Ok(*fx
            .mutated
            .iter()
            .find(|(obj, _)| obj.0 == counter.0)
            .expect("Counter mutated"))
    }

    async fn increment_owned_counter(&mut self, counter: ObjectRef) -> (ObjectRef, Owner) {
        let fx = self
            .owned_move_call(
                "increment_counter",
                vec![CallArg::Object(ObjectArg::ImmOrOwnedObject(counter))],
            )
            .await;

        assert!(fx.status.is_ok());

        *fx.mutated
            .iter()
            .find(|(obj, _)| obj.0 == counter.0)
            .expect("Counter modified")
    }

    async fn increment_shared_counter(
        &mut self,
        counter: ObjectRef,
        initial_shared_version: SequenceNumber,
    ) -> SuiResult<(ObjectRef, Owner)> {
        let fx = self
            .shared_move_call(
                "increment_counter",
                vec![CallArg::Object(ObjectArg::SharedObject {
                    id: counter.0,
                    initial_shared_version,
                })],
            )
            .await?;

        assert!(fx.status.is_ok());

        Ok(*fx
            .mutated
            .iter()
            .find(|(obj, _)| obj.0 == counter.0)
            .expect("Counter modified"))
    }
}

async fn publish_move_package(gas: Object, validators: &[ValidatorInfo]) -> ObjectRef {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/move_test_code");
    publish_package(gas, path, validators).await
}
