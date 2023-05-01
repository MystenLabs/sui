// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use std::time::Duration;
use sui_config::NetworkConfig;
use sui_macros::*;
use sui_node::SuiNodeHandle;
use sui_types::base_types::{ObjectID, ObjectRef, SequenceNumber};
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI, TransactionEvents};
use sui_types::error::SuiResult;
use sui_types::execution_status::{ExecutionFailureStatus, ExecutionStatus};
use sui_types::messages::{CallArg, ObjectArg, TEST_ONLY_GAS_UNIT_FOR_GENERIC};
use sui_types::multiaddr::Multiaddr;
use sui_types::object::{generate_test_gas_objects, Object, Owner, OBJECT_START_VERSION};
use sui_types::SUI_FRAMEWORK_ADDRESS;
use test_utils::authority::{spawn_test_authorities, test_authority_configs_with_objects};
use test_utils::messages::move_transaction;
use test_utils::transaction::{
    publish_package, submit_shared_object_transaction, submit_single_owner_transaction,
};
use tokio::time::timeout;

#[sim_test]
async fn fresh_shared_object_initial_version_matches_current() {
    let mut env = TestEnvironment::new().await;
    let ((_, curr, _), owner) = env.create_shared_counter().await;
    assert!(is_shared_at(&owner, curr));
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
    //     .increment_shared_counter(counter.0, counter.1)
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
}

#[sim_test]
async fn shared_object_not_found() {
    let mut env = TestEnvironment::new().await;
    let nonexistent_id = ObjectID::random();
    let initial_shared_seq = SequenceNumber::from_u64(42);
    if timeout(
        Duration::from_secs(10),
        env.increment_shared_counter(nonexistent_id, initial_shared_seq),
    )
    .await
    .is_ok()
    {
        panic!("Executing transaction with nonexistent input should not return!");
    };
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
    move_package: ObjectID,
}

impl TestEnvironment {
    async fn new() -> Self {
        let gas_objects = generate_test_gas_objects();
        let (configs, mut gas_objects) = test_authority_configs_with_objects(gas_objects);
        let rgp = configs.genesis.reference_gas_price();
        let node_handles = spawn_test_authorities(&configs).await;

        let move_package =
            publish_move_package(gas_objects.pop().unwrap(), &configs.net_addresses(), rgp)
                .await
                .0;

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
    ) -> (TransactionEffects, TransactionEvents, Vec<Object>) {
        let rgp = self.configs.genesis.reference_gas_price();
        submit_single_owner_transaction(
            move_transaction(
                self.gas_objects.pop().unwrap(),
                "shared_objects_version",
                function,
                self.move_package,
                arguments,
                rgp * TEST_ONLY_GAS_UNIT_FOR_GENERIC,
                rgp,
            ),
            &self.configs.net_addresses(),
        )
        .await
    }

    async fn shared_move_call(
        &mut self,
        function: &'static str,
        arguments: Vec<CallArg>,
    ) -> SuiResult<(TransactionEffects, TransactionEvents, Vec<Object>)> {
        let rgp = self.configs.genesis.reference_gas_price();
        submit_shared_object_transaction(
            move_transaction(
                self.gas_objects.pop().unwrap(),
                "shared_objects_version",
                function,
                self.move_package,
                arguments,
                rgp * TEST_ONLY_GAS_UNIT_FOR_GENERIC,
                rgp,
            ),
            &self.configs.net_addresses(),
        )
        .await
    }

    async fn create_counter(&mut self) -> (ObjectRef, Owner) {
        let (fx, _, _) = self.owned_move_call("create_counter", vec![]).await;
        assert!(fx.status().is_ok());

        *fx.created()
            .iter()
            .find(|(_, owner)| matches!(owner, Owner::AddressOwner(_)))
            .expect("Owned object created")
    }

    async fn create_shared_counter(&mut self) -> (ObjectRef, Owner) {
        let (fx, _, _) = self.owned_move_call("create_shared_counter", vec![]).await;
        assert!(fx.status().is_ok());

        *fx.created()
            .iter()
            .find(|(_, owner)| owner.is_shared())
            .expect("Shared object created")
    }

    async fn share_counter(
        &mut self,
        counter: ObjectRef,
    ) -> Result<(ObjectRef, Owner), ExecutionFailureStatus> {
        let (fx, _, _) = self
            .owned_move_call(
                "share_counter",
                vec![CallArg::Object(ObjectArg::ImmOrOwnedObject(counter))],
            )
            .await;

        if let ExecutionStatus::Failure { error, .. } = fx.status() {
            return Err(error.clone());
        }

        Ok(*fx
            .mutated()
            .iter()
            .find(|(obj, _)| obj.0 == counter.0)
            .expect("Counter mutated"))
    }

    async fn increment_owned_counter(&mut self, counter: ObjectRef) -> (ObjectRef, Owner) {
        let (fx, _, _) = self
            .owned_move_call(
                "increment_counter",
                vec![CallArg::Object(ObjectArg::ImmOrOwnedObject(counter))],
            )
            .await;

        assert!(fx.status().is_ok());

        *fx.mutated()
            .iter()
            .find(|(obj, _)| obj.0 == counter.0)
            .expect("Counter modified")
    }

    async fn increment_shared_counter(
        &mut self,
        counter: ObjectID,
        initial_shared_version: SequenceNumber,
    ) -> SuiResult<(ObjectRef, Owner)> {
        let (fx, _, _) = self
            .shared_move_call(
                "increment_counter",
                vec![CallArg::Object(ObjectArg::SharedObject {
                    id: counter,
                    initial_shared_version,
                    mutable: true,
                })],
            )
            .await?;

        assert!(fx.status().is_ok());

        Ok(*fx
            .mutated()
            .iter()
            .find(|(obj, _)| obj.0 == counter)
            .expect("Counter modified"))
    }
}

async fn publish_move_package(
    gas: Object,
    net_addresses: &[Multiaddr],
    gas_price: u64,
) -> ObjectRef {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/move_test_code");
    publish_package(gas, path, net_addresses, gas_price).await
}
