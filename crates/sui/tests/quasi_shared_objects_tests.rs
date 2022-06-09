// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use sui_config::{NetworkConfig, ValidatorInfo};
use sui_node::SuiNode;
use sui_types::base_types::{ObjectID, ObjectRef, SequenceNumber};
use sui_types::messages::CallArg;
use sui_types::object::Object;
use test_utils::authority::{get_latest_object, spawn_test_authorities, test_authority_configs};
use test_utils::messages::move_transaction;
use test_utils::objects::test_gas_objects;
use test_utils::transaction::{
    publish_package, submit_shared_object_transaction, submit_single_owner_transaction,
};

async fn publish_move_test_package(gas_object: Object, configs: &[ValidatorInfo]) -> ObjectRef {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/move_test_code");
    publish_package(gas_object, path, configs).await
}

struct TestEnv {
    pub gas_objects: Vec<Object>,
    pub configs: NetworkConfig,
    /// It is important to not drop the handles (or the authorities will stop).
    #[allow(dead_code)]
    pub handles: Vec<SuiNode>,
    pub package_ref: ObjectRef,
}

async fn setup_network_and_publish_test_package() -> TestEnv {
    let mut gas_objects = test_gas_objects();
    let configs = test_authority_configs();
    let handles = spawn_test_authorities(gas_objects.clone(), &configs).await;
    tokio::task::yield_now().await;
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    let package_ref =
        publish_move_test_package(gas_objects.pop().unwrap(), configs.validator_set()).await;
    tokio::task::yield_now().await;

    TestEnv {
        gas_objects,
        configs,
        handles,
        package_ref,
    }
}

async fn create_shared_parent_and_child(env: &mut TestEnv) -> (ObjectID, ObjectID) {
    let transaction = move_transaction(
        env.gas_objects.pop().unwrap(),
        "quasi_shared_objects",
        "create_shared_parent_and_child",
        env.package_ref,
        /* arguments */ Vec::default(),
    );
    let effects = submit_single_owner_transaction(transaction, env.configs.validator_set()).await;
    assert!(effects.status.is_ok());
    let ((parent_id, _, _), _) = *effects
        .created
        .iter()
        .find(|(_, owner)| owner.is_shared())
        .unwrap();
    let ((child_id, _, _), _) = *effects
        .created
        .iter()
        .find(|(_, owner)| !owner.is_shared())
        .unwrap();
    (parent_id, child_id)
}

#[tokio::test]
async fn normal_quasi_shared_object_flow() {
    // This test exercises a simple flow of quasi shared objects:
    // Create a shared object and a child of it, submit two transactions that both try to mutate the
    // child object (and specify the child object as QuasiSharedObject in the tx input).

    let mut env = setup_network_and_publish_test_package().await;

    let (parent_id, child_id) = create_shared_parent_and_child(&mut env).await;

    let tx1 = move_transaction(
        env.gas_objects.pop().unwrap(),
        "quasi_shared_objects",
        "increment_child_counter",
        env.package_ref,
        vec![
            CallArg::SharedObject(parent_id),
            CallArg::QuasiSharedObject(child_id),
        ],
    );
    let effects = submit_shared_object_transaction(tx1, env.configs.validator_set())
        .await
        .unwrap();
    assert!(effects.status.is_ok());

    let tx2 = move_transaction(
        env.gas_objects.pop().unwrap(),
        "quasi_shared_objects",
        "increment_child_counter",
        env.package_ref,
        vec![
            CallArg::SharedObject(parent_id),
            CallArg::QuasiSharedObject(child_id),
        ],
    );
    let effects = submit_shared_object_transaction(tx2, env.configs.validator_set())
        .await
        .unwrap();
    assert!(effects.status.is_ok());
    assert_eq!(
        effects
            .mutated
            .iter()
            .find(|((id, _, _), _)| id == &child_id)
            .unwrap()
            .0
             .1,
        SequenceNumber::from(3)
    );
}

#[tokio::test]
async fn quasi_shared_object_mismatch() {
    // This test tries to misuse quasi shared object, by passing CallArg that's not compatible with
    // the actual object ownership type.

    let mut env = setup_network_and_publish_test_package().await;

    let (parent_id, child_id) = create_shared_parent_and_child(&mut env).await;
    let tx = move_transaction(
        env.gas_objects.pop().unwrap(),
        "quasi_shared_objects",
        "increment_child_counter",
        env.package_ref,
        vec![
            CallArg::SharedObject(parent_id),
            // Use a quasi-shared object as shared object, this will trigger an error.
            CallArg::SharedObject(child_id),
        ],
    );
    assert!(
        submit_shared_object_transaction(tx, env.configs.validator_set())
            .await
            .is_err()
    );

    let child_object_ref = get_latest_object(&env.configs.validator_set()[0], child_id)
        .await
        .unwrap()
        .compute_object_reference();
    let tx = move_transaction(
        env.gas_objects.pop().unwrap(),
        "quasi_shared_objects",
        "increment_child_counter",
        env.package_ref,
        vec![
            CallArg::SharedObject(parent_id),
            // Use a quasi-shared object as owned object, this will trigger an error.
            CallArg::ImmOrOwnedObject(child_object_ref),
        ],
    );
    assert!(
        submit_shared_object_transaction(tx, env.configs.validator_set())
            .await
            .is_err()
    );

    let tx = move_transaction(
        env.gas_objects.pop().unwrap(),
        "quasi_shared_objects",
        "increment_child_counter",
        env.package_ref,
        vec![
            // Use a shared object as quasi-shared object, this will trigger an error.
            CallArg::QuasiSharedObject(parent_id),
            CallArg::QuasiSharedObject(child_id),
        ],
    );
    assert!(
        submit_shared_object_transaction(tx, env.configs.validator_set())
            .await
            .is_err()
    );
}
