// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::{
    authority_tests::{call_move, init_state_with_ids, submit_and_execute},
    move_integration_tests::{build_and_publish_test_package, build_test_package},
};

use move_binary_format::CompiledModule;
use sui_types::{
    base_types::ObjectID,
    error::{SuiErrorKind, UserInputError},
    object::{Data, ObjectRead, Owner},
    transaction::{TEST_ONLY_GAS_UNIT_FOR_PUBLISH, TransactionData},
    utils::to_sender_signed_transaction,
};

use sui_types::crypto::{AccountKeyPair, get_key_pair};

use crate::authority::move_integration_tests::{
    build_multi_publish_txns, build_package, run_multi_txns,
};
use std::collections::HashSet;
use sui_framework::BuiltInFramework;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::execution_status::{ExecutionFailureStatus, ExecutionStatus};
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;

#[tokio::test]
#[cfg_attr(msim, ignore)]
async fn test_publishing_with_unpublished_deps() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas = ObjectID::random();
    let authority = init_state_with_ids(vec![(sender, gas)]).await;

    let package = build_and_publish_test_package(
        &authority,
        &sender,
        &sender_key,
        &gas,
        "depends_on_basics",
        /* with_unpublished_deps */ true,
    )
    .await;

    let ObjectRead::Exists(read_ref, package_obj, _) =
        authority.get_object_read(&package.0).unwrap()
    else {
        panic!("Can't read package")
    };

    assert_eq!(package, read_ref);
    let Data::Package(move_package) = package_obj.into_inner().data else {
        panic!("Not a package")
    };

    // Check that the published package includes its depended upon module.
    assert_eq!(
        move_package
            .serialized_module_map()
            .keys()
            .map(String::as_str)
            .collect::<HashSet<_>>(),
        HashSet::from(["depends_on_basics", "object_basics"]),
    );

    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
        "depends_on_basics",
        "delegate",
        vec![],
        vec![],
    )
    .await
    .unwrap();

    assert!(effects.status().is_ok());
    assert_eq!(effects.created().len(), 1);
    let ((_, v, _), owner) = effects.created()[0].clone();

    // Check that calling the function does what we expect
    assert!(matches!(
        owner,
        Owner::Shared { initial_shared_version: initial } if initial == v
    ));
}

#[tokio::test]
#[cfg_attr(msim, ignore)]
async fn test_publish_empty_package() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas = ObjectID::random();
    let authority = init_state_with_ids(vec![(sender, gas)]).await;
    let rgp = authority.reference_gas_price_for_testing().unwrap();
    let gas_object = authority.get_object(&gas).await;
    let gas_object_ref = gas_object.unwrap().compute_object_reference();

    // empty package
    let data = TransactionData::new_module(
        sender,
        gas_object_ref,
        vec![],
        vec![],
        rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH,
        rgp,
    );
    let transaction = to_sender_signed_transaction(data, &sender_key);
    let err = submit_and_execute(&authority, transaction)
        .await
        .unwrap_err();
    assert_eq!(
        err,
        SuiErrorKind::UserInputError {
            error: UserInputError::EmptyCommandInput
        }
    );

    // empty module
    let data = TransactionData::new_module(
        sender,
        gas_object_ref,
        vec![vec![]],
        vec![],
        rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH,
        rgp,
    );
    let transaction = to_sender_signed_transaction(data, &sender_key);
    let result = submit_and_execute(&authority, transaction).await.unwrap().1;
    assert_eq!(
        result.status(),
        &ExecutionStatus::Failure {
            error: ExecutionFailureStatus::VMVerificationOrDeserializationError,
            command: Some(0)
        }
    )
}

#[tokio::test]
#[cfg_attr(msim, ignore)]
async fn test_publish_duplicate_modules() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas = ObjectID::random();
    let authority = init_state_with_ids(vec![(sender, gas)]).await;
    let gas_object = authority.get_object(&gas).await;
    let gas_object_ref = gas_object.unwrap().compute_object_reference();
    let rgp = authority.reference_gas_price_for_testing().unwrap();

    // empty package
    let mut modules = build_test_package("object_owner", /* with_unpublished_deps */ false);
    assert_eq!(modules.len(), 1);
    modules.push(modules[0].clone());
    let data = TransactionData::new_module(
        sender,
        gas_object_ref,
        modules,
        BuiltInFramework::all_package_ids(),
        rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH,
        rgp,
    );
    let transaction = to_sender_signed_transaction(data, &sender_key);
    let result = submit_and_execute(&authority, transaction).await.unwrap().1;
    assert_eq!(
        result.status(),
        &ExecutionStatus::Failure {
            error: ExecutionFailureStatus::VMVerificationOrDeserializationError,
            command: Some(0)
        }
    )
}

#[tokio::test]
#[cfg_attr(msim, ignore)]
async fn test_publish_extraneous_bytes_modules() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas = ObjectID::random();
    let authority = init_state_with_ids(vec![(sender, gas)]).await;
    let gas_object = authority.get_object(&gas).await;
    let gas_object_ref = gas_object.unwrap().compute_object_reference();
    let rgp = authority.reference_gas_price_for_testing().unwrap();

    // test valid module bytes
    let correct_modules =
        build_test_package("object_owner", /* with_unpublished_deps */ false);
    assert_eq!(correct_modules.len(), 1);
    let data = TransactionData::new_module(
        sender,
        gas_object_ref,
        correct_modules.clone(),
        BuiltInFramework::all_package_ids(),
        rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH,
        rgp,
    );
    let transaction = to_sender_signed_transaction(data, &sender_key);
    let result = submit_and_execute(&authority, transaction).await.unwrap().1;
    assert_eq!(result.status(), &ExecutionStatus::Success);

    // make the bytes invalid
    let gas_object = authority.get_object(&gas).await;
    let gas_object_ref = gas_object.unwrap().compute_object_reference();
    let mut modules = correct_modules.clone();
    modules[0].push(0);
    assert_eq!(modules.len(), 1);
    let data = TransactionData::new_module(
        sender,
        gas_object_ref,
        modules,
        BuiltInFramework::all_package_ids(),
        rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH,
        rgp,
    );
    let transaction = to_sender_signed_transaction(data, &sender_key);
    let result = submit_and_execute(&authority, transaction).await.unwrap().1;
    assert_eq!(
        result.status(),
        &ExecutionStatus::Failure {
            error: ExecutionFailureStatus::VMVerificationOrDeserializationError,
            command: Some(0)
        }
    );

    // make the bytes invalid, in a different way
    let gas_object = authority.get_object(&gas).await;
    let gas_object_ref = gas_object.unwrap().compute_object_reference();
    let mut modules = correct_modules.clone();
    let first_module = modules[0].clone();
    modules[0].extend(first_module);
    assert_eq!(modules.len(), 1);
    let data = TransactionData::new_module(
        sender,
        gas_object_ref,
        modules,
        BuiltInFramework::all_package_ids(),
        rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH,
        rgp,
    );
    let transaction = to_sender_signed_transaction(data, &sender_key);
    let result = submit_and_execute(&authority, transaction).await.unwrap().1;
    assert_eq!(
        result.status(),
        &ExecutionStatus::Failure {
            error: ExecutionFailureStatus::VMVerificationOrDeserializationError,
            command: Some(0)
        }
    );

    // make the bytes invalid by adding metadata
    let gas_object = authority.get_object(&gas).await;
    let gas_object_ref = gas_object.unwrap().compute_object_reference();
    let mut modules = correct_modules.clone();
    let new_bytes = {
        let mut m = CompiledModule::deserialize_with_defaults(&modules[0]).unwrap();
        m.metadata.push(move_core_types::metadata::Metadata {
            key: vec![0],
            value: vec![1],
        });
        let mut buf = vec![];
        m.serialize_with_version(m.version, &mut buf).unwrap();
        buf
    };
    modules[0] = new_bytes;
    assert_eq!(modules.len(), 1);
    let data = TransactionData::new_module(
        sender,
        gas_object_ref,
        modules,
        BuiltInFramework::all_package_ids(),
        rgp * TEST_ONLY_GAS_UNIT_FOR_PUBLISH,
        rgp,
    );
    let transaction = to_sender_signed_transaction(data, &sender_key);
    let result = submit_and_execute(&authority, transaction).await.unwrap().1;
    assert_eq!(
        result.status(),
        &ExecutionStatus::Failure {
            error: ExecutionFailureStatus::VMVerificationOrDeserializationError,
            command: Some(0)
        }
    )
}

#[tokio::test]
#[cfg_attr(msim, ignore)]
async fn test_publish_max_packages() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas_object_id = ObjectID::random();
    let authority = init_state_with_ids(vec![(sender, gas_object_id)]).await;

    let (_, modules, dependencies) = build_package("object_basics", false);

    // push max number of packages allowed to publish
    let max_pub_cmd = authority
        .epoch_store_for_testing()
        .protocol_config()
        .max_publish_or_upgrade_per_ptb_as_option()
        .unwrap_or(0);
    assert!(max_pub_cmd > 0);
    let packages = vec![(modules, dependencies); max_pub_cmd as usize];

    let mut builder = ProgrammableTransactionBuilder::new();
    build_multi_publish_txns(&mut builder, sender, packages);
    let result = run_multi_txns(&authority, sender, &sender_key, &gas_object_id, builder)
        .await
        .unwrap()
        .1;
    let effects = result.into_data();
    assert_eq!(effects.status(), &ExecutionStatus::Success);
}

#[tokio::test]
#[cfg_attr(msim, ignore)]
async fn test_publish_more_than_max_packages_error() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas_object_id = ObjectID::random();
    let authority = init_state_with_ids(vec![(sender, gas_object_id)]).await;

    let (_, modules, dependencies) = build_package("object_basics", false);

    // push max number of packages allowed to publish
    let max_pub_cmd = authority
        .epoch_store_for_testing()
        .protocol_config()
        .max_publish_or_upgrade_per_ptb_as_option()
        .unwrap_or(0);
    assert!(max_pub_cmd > 0);
    let packages = vec![(modules, dependencies); (max_pub_cmd + 1) as usize];

    let mut builder = ProgrammableTransactionBuilder::new();
    build_multi_publish_txns(&mut builder, sender, packages);
    let err = run_multi_txns(&authority, sender, &sender_key, &gas_object_id, builder)
        .await
        .unwrap_err();
    assert_eq!(
        err,
        SuiErrorKind::UserInputError {
            error: UserInputError::MaxPublishCountExceeded {
                max_publish_commands: max_pub_cmd,
                publish_count: max_pub_cmd + 1,
            }
        }
    );
}
