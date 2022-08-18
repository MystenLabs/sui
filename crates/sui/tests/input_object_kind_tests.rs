// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use sui_config::{NetworkConfig, ValidatorInfo};
use sui_node::SuiNode;
use sui_types::base_types::{ObjectID, ObjectRef, SequenceNumber};
use sui_types::messages::{CallArg, ObjectArg};
use sui_types::object::{Object, Owner};
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

async fn create_parent_and_child(
    env: &mut TestEnv,
    create: &'static str,
) -> (ObjectID, Option<ObjectID>) {
    let transaction = move_transaction(
        env.gas_objects.pop().unwrap(),
        "quasi_shared_objects",
        create,
        env.package_ref,
        /* arguments */ vec![],
    );
    let effects = submit_single_owner_transaction(transaction, env.configs.validator_set()).await;
    assert!(effects.status.is_ok());
    let ((parent_id, _, _), _) = *effects
        .created
        .iter()
        .find(|(_, owner)| !matches!(owner, Owner::ObjectOwner(_)))
        .unwrap();
    let child_id = effects
        .created
        .iter()
        .find(|(_, owner)| matches!(owner, Owner::ObjectOwner(_)))
        .map(|((child_id, _, _), _)| child_id)
        .copied();
    (parent_id, child_id)
}

#[tokio::test]
async fn shared_valid() {
    let env = &mut setup_network_and_publish_test_package().await;
    let (parent_id, child_id) =
        create_parent_and_child(env, "create_shared_parent_and_child").await;
    let child_id = child_id.unwrap();
    let tx = move_transaction(
        env.gas_objects.pop().unwrap(),
        "quasi_shared_objects",
        "increment_counter",
        env.package_ref,
        vec![
            CallArg::Object(ObjectArg::SharedObject(parent_id)),
            CallArg::Object(ObjectArg::QuasiSharedObject(child_id)),
        ],
    );
    let effects = submit_shared_object_transaction(tx, env.configs.validator_set())
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
        SequenceNumber::from(2)
    );
}

#[tokio::test]
async fn owned_valid() {
    let env = &mut setup_network_and_publish_test_package().await;
    let (parent_id, child_id) = create_parent_and_child(env, "create_owned_parent_and_child").await;
    let child_id = child_id.unwrap();
    let parent_ref = get_latest_object(&env.configs.validator_set()[0], parent_id)
        .await
        .unwrap()
        .compute_object_reference();
    let child_ref = get_latest_object(&env.configs.validator_set()[0], child_id)
        .await
        .unwrap()
        .compute_object_reference();
    let tx = move_transaction(
        env.gas_objects.pop().unwrap(),
        "quasi_shared_objects",
        "increment_counter",
        env.package_ref,
        vec![
            CallArg::Object(ObjectArg::ImmOrOwnedObject(parent_ref)),
            CallArg::Object(ObjectArg::ImmOrOwnedObject(child_ref)),
        ],
    );
    let effects = submit_shared_object_transaction(tx, env.configs.validator_set())
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
        SequenceNumber::from(2)
    );
}

#[tokio::test]
async fn imm_valid() {
    let env = &mut setup_network_and_publish_test_package().await;
    let (parent_id, child_id) = create_parent_and_child(env, "create_immutable_parent").await;
    assert!(child_id.is_none());
    let parent_ref = get_latest_object(&env.configs.validator_set()[0], parent_id)
        .await
        .unwrap()
        .compute_object_reference();
    let tx = move_transaction(
        env.gas_objects.pop().unwrap(),
        "quasi_shared_objects",
        "use_parent",
        env.package_ref,
        vec![CallArg::Object(ObjectArg::ImmOrOwnedObject(parent_ref))],
    );
    let effects = submit_shared_object_transaction(tx, env.configs.validator_set())
        .await
        .unwrap();
    assert!(effects.status.is_ok());
}

async fn run_and_assert_error_contains(
    env: &mut TestEnv,
    function: &'static str,
    parent: ObjectArg,
    child: Option<ObjectArg>,
    prefix: &str,
) {
    let mut args = vec![CallArg::Object(parent)];
    if let Some(child) = child {
        args.push(CallArg::Object(child))
    }

    let tx = move_transaction(
        env.gas_objects.pop().unwrap(),
        "quasi_shared_objects",
        function,
        env.package_ref,
        args,
    );
    let msg = match submit_shared_object_transaction(tx, env.configs.validator_set()).await {
        Ok(_) => "Ok(_)".to_owned(),
        Err(e) => e.to_string(),
    };
    if !msg.contains(&format!(
        "Error checking transaction input objects: [{prefix}"
    )) {
        panic!(
            "Expected error that starts with '{}' but got '{}'",
            prefix, msg
        )
    }
}

async fn run_increment_and_assert_error_contains(
    env: &mut TestEnv,
    parent: ObjectArg,
    child: ObjectArg,
    prefix: &str,
) {
    run_and_assert_error_contains(env, "increment_counter", parent, Some(child), prefix).await
}

#[tokio::test]
async fn object_shared_mismatch() {
    // This test tries to misuse objects, by passing CallArg that's not compatible with
    // the actual object ownership type.

    let env = &mut setup_network_and_publish_test_package().await;
    let (parent_id, child_id) =
        create_parent_and_child(env, "create_shared_parent_and_child").await;
    let child_id = child_id.unwrap();

    //
    // Misuse child
    //

    // Use a quasi-shared object as shared object
    run_increment_and_assert_error_contains(
        env,
        ObjectArg::SharedObject(parent_id),
        ObjectArg::SharedObject(child_id),
        "NotSharedObjectError",
    )
    .await;

    // Use a quasi-shared object as imm/owned object
    let child_ref = get_latest_object(&env.configs.validator_set()[0], child_id)
        .await
        .unwrap()
        .compute_object_reference();
    run_increment_and_assert_error_contains(
        env,
        ObjectArg::SharedObject(parent_id),
        ObjectArg::ImmOrOwnedObject(child_ref),
        "NotImmutableOrOwnedObject {",
    )
    .await;

    //
    // Misuse Parent
    //

    // Use a shared object as quasi-shared object
    run_increment_and_assert_error_contains(
        env,
        ObjectArg::QuasiSharedObject(parent_id),
        ObjectArg::QuasiSharedObject(child_id),
        "NotQuasiSharedObject {",
    )
    .await;

    // Use a shared object as imm/owned object
    let parent_ref = get_latest_object(&env.configs.validator_set()[0], parent_id)
        .await
        .unwrap()
        .compute_object_reference();
    run_increment_and_assert_error_contains(
        env,
        ObjectArg::ImmOrOwnedObject(parent_ref),
        ObjectArg::QuasiSharedObject(child_id),
        "NotImmutableOrOwnedObject {",
    )
    .await;
}

#[tokio::test]
async fn object_owned_mismatch() {
    // This test tries to misuse objects, by passing CallArg that's not compatible with
    // the actual object ownership type.

    let env = &mut setup_network_and_publish_test_package().await;
    let (parent_id, child_id) = create_parent_and_child(env, "create_owned_parent_and_child").await;
    let child_id = child_id.unwrap();

    //
    // Misuse child
    //

    // Use an owned object as shared object
    let parent_ref = get_latest_object(&env.configs.validator_set()[0], parent_id)
        .await
        .unwrap()
        .compute_object_reference();
    run_increment_and_assert_error_contains(
        env,
        ObjectArg::ImmOrOwnedObject(parent_ref),
        ObjectArg::SharedObject(child_id),
        "NotSharedObjectError",
    )
    .await;

    // Use an owned object as quasi-shared object
    let parent_ref = get_latest_object(&env.configs.validator_set()[0], parent_id)
        .await
        .unwrap()
        .compute_object_reference();
    run_increment_and_assert_error_contains(
        env,
        ObjectArg::ImmOrOwnedObject(parent_ref),
        ObjectArg::QuasiSharedObject(child_id),
        "NotQuasiSharedObject {",
    )
    .await;

    //
    // Misuse Parent
    //

    // Use an account owned object as quasi-shared object
    let child_ref = get_latest_object(&env.configs.validator_set()[0], child_id)
        .await
        .unwrap()
        .compute_object_reference();
    run_increment_and_assert_error_contains(
        env,
        ObjectArg::QuasiSharedObject(parent_id),
        ObjectArg::ImmOrOwnedObject(child_ref),
        "NotQuasiSharedObject {",
    )
    .await;

    // Use an account owned object as shared object
    let child_ref = get_latest_object(&env.configs.validator_set()[0], child_id)
        .await
        .unwrap()
        .compute_object_reference();
    run_increment_and_assert_error_contains(
        env,
        ObjectArg::SharedObject(parent_id),
        ObjectArg::ImmOrOwnedObject(child_ref),
        "NotSharedObject",
    )
    .await;
}

#[tokio::test]
async fn object_imm_mismatch() {
    // This test tries to misuse objects, by passing CallArg that's not compatible with
    // the actual object ownership type.

    let env = &mut setup_network_and_publish_test_package().await;
    let (parent_id, child_id) = create_parent_and_child(env, "create_immutable_parent").await;
    assert!(child_id.is_none());

    // Use an imm object as quasi-shared object
    run_and_assert_error_contains(
        env,
        "use_parent",
        ObjectArg::QuasiSharedObject(parent_id),
        None,
        "NotQuasiSharedObject {",
    )
    .await;

    // Use an imm object as shared object
    run_and_assert_error_contains(
        env,
        "use_parent",
        ObjectArg::SharedObject(parent_id),
        None,
        "NotSharedObject",
    )
    .await;
}
