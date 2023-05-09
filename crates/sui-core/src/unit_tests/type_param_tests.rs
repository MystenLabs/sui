// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use crate::authority::{
    authority_tests::{call_move, init_state_with_ids, TestCallArg},
    move_integration_tests::build_and_publish_test_package,
};

use move_core_types::language_storage::TypeTag;

use sui_types::effects::TransactionEffectsAPI;
use sui_types::{
    base_types::ObjectID,
    crypto::{get_key_pair, AccountKeyPair},
};

#[tokio::test]
#[cfg_attr(msim, ignore)]
async fn test_same_module_type_param() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas = ObjectID::random();
    let authority = init_state_with_ids(vec![(sender, gas)]).await;

    let package = build_and_publish_test_package(
        &authority,
        &sender,
        &sender_key,
        &gas,
        "type_params",
        /* with_unpublished_deps */ true,
    )
    .await;

    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
        "m1",
        "create_and_transfer",
        vec![],
        vec![
            TestCallArg::Pure(bcs::to_bytes(&(16_u64)).unwrap()),
            TestCallArg::Pure(bcs::to_bytes(&sender).unwrap()),
        ],
    )
    .await
    .unwrap();

    let created_object_id = effects.created()[0].0 .0;
    let type_param = TypeTag::from_str(format!("{}::m1::Object", package.0).as_str()).unwrap();

    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
        "m1",
        "transfer_object",
        vec![type_param],
        vec![
            TestCallArg::Object(created_object_id),
            TestCallArg::Pure(bcs::to_bytes(&sender).unwrap()),
        ],
    )
    .await
    .unwrap();

    assert!(effects.status().is_ok());
}

#[tokio::test]
#[cfg_attr(msim, ignore)]
async fn test_different_module_type_param() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas = ObjectID::random();
    let authority = init_state_with_ids(vec![(sender, gas)]).await;

    let package = build_and_publish_test_package(
        &authority,
        &sender,
        &sender_key,
        &gas,
        "type_params",
        /* with_unpublished_deps */ true,
    )
    .await;

    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
        "m2",
        "create_and_transfer",
        vec![],
        vec![
            TestCallArg::Pure(bcs::to_bytes(&(16_u64)).unwrap()),
            TestCallArg::Pure(bcs::to_bytes(&sender).unwrap()),
        ],
    )
    .await
    .unwrap();

    let created_object_id = effects.created()[0].0 .0;
    let type_param =
        TypeTag::from_str(format!("{}::m2::AnotherObject", package.0).as_str()).unwrap();

    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
        // a different module than the one where the type was defined
        "m1",
        "transfer_object",
        vec![type_param],
        vec![
            TestCallArg::Object(created_object_id),
            TestCallArg::Pure(bcs::to_bytes(&sender).unwrap()),
        ],
    )
    .await
    .unwrap();

    assert!(effects.status().is_ok());
}

#[tokio::test]
#[cfg_attr(msim, ignore)]
async fn test_nested_type_param() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas = ObjectID::random();
    let authority = init_state_with_ids(vec![(sender, gas)]).await;

    let package = build_and_publish_test_package(
        &authority,
        &sender,
        &sender_key,
        &gas,
        "type_params",
        /* with_unpublished_deps */ true,
    )
    .await;

    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
        "m1",
        "create_and_transfer_gen",
        vec![],
        vec![
            TestCallArg::Pure(bcs::to_bytes(&(16_u64)).unwrap()),
            TestCallArg::Pure(bcs::to_bytes(&sender).unwrap()),
        ],
    )
    .await
    .unwrap();

    let created_object_id = effects.created()[0].0 .0;
    let type_param = TypeTag::from_str(
        format!(
            "{}::m1::GenObject<{}::m2::AnotherObject>",
            package.0, package.0
        )
        .as_str(),
    )
    .unwrap();

    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
        "m1",
        "transfer_object",
        // outer type comes from the same module but nested one from a different module
        vec![type_param],
        vec![
            TestCallArg::Object(created_object_id),
            TestCallArg::Pure(bcs::to_bytes(&sender).unwrap()),
        ],
    )
    .await
    .unwrap();

    assert!(effects.status().is_ok());
}

#[tokio::test]
#[cfg_attr(msim, ignore)]
async fn test_nested_type_param_different_module() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas = ObjectID::random();
    let authority = init_state_with_ids(vec![(sender, gas)]).await;

    let package = build_and_publish_test_package(
        &authority,
        &sender,
        &sender_key,
        &gas,
        "type_params",
        /* with_unpublished_deps */ true,
    )
    .await;

    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
        "m1",
        "create_and_transfer_gen",
        vec![],
        vec![
            TestCallArg::Pure(bcs::to_bytes(&(16_u64)).unwrap()),
            TestCallArg::Pure(bcs::to_bytes(&sender).unwrap()),
        ],
    )
    .await
    .unwrap();

    let created_object_id = effects.created()[0].0 .0;
    let type_param = TypeTag::from_str(
        format!(
            "{}::m1::GenObject<{}::m2::AnotherObject>",
            package.0, package.0
        )
        .as_str(),
    )
    .unwrap();

    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
        // a different module than those where types where defined
        "m3",
        "transfer_object",
        vec![type_param],
        vec![
            TestCallArg::Object(created_object_id),
            TestCallArg::Pure(bcs::to_bytes(&sender).unwrap()),
        ],
    )
    .await
    .unwrap();

    assert!(effects.status().is_ok());
}

#[tokio::test]
#[cfg_attr(msim, ignore)]
async fn test_different_package_type_param() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas = ObjectID::random();
    let authority = init_state_with_ids(vec![(sender, gas)]).await;

    let package = build_and_publish_test_package(
        &authority,
        &sender,
        &sender_key,
        &gas,
        "type_params",
        /* with_unpublished_deps */ true,
    )
    .await;

    let package_extra = build_and_publish_test_package(
        &authority,
        &sender,
        &sender_key,
        &gas,
        "type_params_extra",
        /* with_unpublished_deps */ true,
    )
    .await;

    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
        "m2",
        "create_and_transfer",
        vec![],
        vec![
            TestCallArg::Pure(bcs::to_bytes(&(16_u64)).unwrap()),
            TestCallArg::Pure(bcs::to_bytes(&sender).unwrap()),
        ],
    )
    .await
    .unwrap();

    let created_object_id = effects.created()[0].0 .0;
    let type_param =
        TypeTag::from_str(format!("{}::m2::AnotherObject", package.0).as_str()).unwrap();

    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        // a different package than the one where the type was defined
        &package_extra.0,
        "m1",
        "transfer_object",
        vec![type_param],
        vec![
            TestCallArg::Object(created_object_id),
            TestCallArg::Pure(bcs::to_bytes(&sender).unwrap()),
        ],
    )
    .await
    .unwrap();

    assert!(effects.status().is_ok());
}

#[tokio::test]
#[cfg_attr(msim, ignore)]
async fn test_nested_type_param_different_package() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas = ObjectID::random();
    let authority = init_state_with_ids(vec![(sender, gas)]).await;

    let package = build_and_publish_test_package(
        &authority,
        &sender,
        &sender_key,
        &gas,
        "type_params",
        /* with_unpublished_deps */ true,
    )
    .await;

    let package_extra = build_and_publish_test_package(
        &authority,
        &sender,
        &sender_key,
        &gas,
        "type_params_extra",
        /* with_unpublished_deps */ true,
    )
    .await;

    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
        "m1",
        "create_and_transfer_gen",
        vec![],
        vec![
            TestCallArg::Pure(bcs::to_bytes(&(16_u64)).unwrap()),
            TestCallArg::Pure(bcs::to_bytes(&sender).unwrap()),
        ],
    )
    .await
    .unwrap();

    let created_object_id = effects.created()[0].0 .0;
    let type_param = TypeTag::from_str(
        format!(
            "{}::m1::GenObject<{}::m2::AnotherObject>",
            package.0, package.0
        )
        .as_str(),
    )
    .unwrap();

    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        // a different package than those where types where defined
        &package_extra.0,
        "m1",
        "transfer_object",
        vec![type_param],
        vec![
            TestCallArg::Object(created_object_id),
            TestCallArg::Pure(bcs::to_bytes(&sender).unwrap()),
        ],
    )
    .await
    .unwrap();

    assert!(effects.status().is_ok());
}
