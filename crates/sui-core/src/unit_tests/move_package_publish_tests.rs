// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::{
    authority_tests::{call_move, init_state_with_ids, send_and_confirm_transaction},
    move_integration_tests::{build_and_publish_test_package, build_test_package},
};

use move_binary_format::CompiledModule;
use sui_types::{
    base_types::ObjectID,
    error::UserInputError,
    messages::{TransactionData, TEST_ONLY_GAS_UNIT_FOR_PUBLISH},
    object::{Data, ObjectRead, Owner},
    utils::to_sender_signed_transaction,
};

use move_package::source_package::manifest_parser;
use sui_move_build::{check_unpublished_dependencies, gather_published_ids, BuildConfig};
use sui_types::{
    crypto::{get_key_pair, AccountKeyPair},
    error::SuiError,
};

use expect_test::expect;
use std::env;
use std::fs::File;
use std::io::Read;
use std::{collections::HashSet, path::PathBuf};
use sui_framework::BuiltInFramework;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::execution_status::{ExecutionFailureStatus, ExecutionStatus};

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

    let ObjectRead::Exists(read_ref, package_obj, _) = authority
        .get_object_read(&package.0)
        .unwrap()
    else {
        panic!("Can't read package")
    };

    assert_eq!(package, read_ref);
    let Data::Package(move_package) = package_obj.data else {
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
    let ((_, v, _), owner) = effects.created()[0];

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
    let gas_object = authority.get_object(&gas).await.unwrap();
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
    let err = send_and_confirm_transaction(&authority, transaction)
        .await
        .unwrap_err();
    assert_eq!(
        err,
        SuiError::UserInputError {
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
    let result = send_and_confirm_transaction(&authority, transaction)
        .await
        .unwrap()
        .1;
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
    let gas_object = authority.get_object(&gas).await.unwrap();
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
    let result = send_and_confirm_transaction(&authority, transaction)
        .await
        .unwrap()
        .1;
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
async fn test_generate_lock_file() {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["src", "unit_tests", "data", "generate_move_lock_file"]);

    let tmp = tempfile::tempdir().expect("Could not create temp dir for Move.lock");
    let lock_file_path = tmp.path().join("Move.lock");

    let mut build_config = BuildConfig::new_for_testing();
    build_config.config.lock_file = Some(lock_file_path.clone());
    build_config
        .build(path)
        .expect("Move package did not build");

    let mut lock_file_contents = String::new();
    File::open(lock_file_path)
        .expect("Cannot open lock file")
        .read_to_string(&mut lock_file_contents)
        .expect("Error reading Move.lock file");

    let expected = expect![[r##"
        # @generated by Move, please check-in and do not edit manually.

        [move]
        version = 0

        dependencies = [
          { name = "Examples" },
          { name = "Sui" },
        ]

        [[move.package]]
        name = "Examples"
        source = { local = "../object_basics" }

        dependencies = [
          { name = "Sui" },
        ]

        [[move.package]]
        name = "MoveStdlib"
        source = { local = "../../../../../sui-framework/packages/move-stdlib" }

        [[move.package]]
        name = "Sui"
        source = { local = "../../../../../sui-framework/packages/sui-framework" }

        dependencies = [
          { name = "MoveStdlib" },
        ]
    "##]];
    expected.assert_eq(lock_file_contents.as_str());
}

#[tokio::test]
#[cfg_attr(msim, ignore)]
async fn test_custom_property_parse_published_at() {
    let build_config = BuildConfig::new_for_testing();
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["src", "unit_tests", "data", "custom_properties_in_manifest"]);

    build_config
        .build(path.clone())
        .expect("Move package did not build");
    let manifest = manifest_parser::parse_move_manifest_from_file(path.as_path())
        .expect("Could not parse Move.toml");
    let properties = manifest
        .package
        .custom_properties
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect::<Vec<_>>();

    let expected = expect![[r#"
        [
            (
                "published-at",
                "0x777",
            ),
        ]
    "#]];
    expected.assert_debug_eq(&properties)
}

#[tokio::test]
#[cfg_attr(msim, ignore)]
async fn test_custom_property_check_unpublished_dependencies() {
    let build_config = BuildConfig::new_for_testing();
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend([
        "src",
        "unit_tests",
        "data",
        "custom_properties_in_manifest_ensure_published_at",
    ]);

    let resolution_graph = build_config
        .config
        .resolution_graph_for_package(&path, &mut std::io::sink())
        .expect("Could not build resolution graph.");

    let SuiError::ModulePublishFailure { error } =
        check_unpublished_dependencies(&gather_published_ids(&resolution_graph).1.unpublished)
            .err()
            .unwrap()
     else {
        panic!("Expected ModulePublishFailure")
    };

    let expected = expect![[r#"
        Package dependency "CustomPropertiesInManifestDependencyMissingPublishedAt" does not specify a published address (the Move.toml manifest for "CustomPropertiesInManifestDependencyMissingPublishedAt" does not contain a published-at field).
        If this is intentional, you may use the --with-unpublished-dependencies flag to continue publishing these dependencies as part of your package (they won't be linked against existing packages on-chain)."#]];
    expected.assert_eq(&error)
}

#[tokio::test]
#[cfg_attr(msim, ignore)]
async fn test_publish_extraneous_bytes_modules() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas = ObjectID::random();
    let authority = init_state_with_ids(vec![(sender, gas)]).await;
    let gas_object = authority.get_object(&gas).await.unwrap();
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
    let result = send_and_confirm_transaction(&authority, transaction)
        .await
        .unwrap()
        .1;
    assert_eq!(result.status(), &ExecutionStatus::Success);

    // make the bytes invalid
    let gas_object = authority.get_object(&gas).await.unwrap();
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
    let result = send_and_confirm_transaction(&authority, transaction)
        .await
        .unwrap()
        .1;
    assert_eq!(
        result.status(),
        &ExecutionStatus::Failure {
            error: ExecutionFailureStatus::VMVerificationOrDeserializationError,
            command: Some(0)
        }
    );

    // make the bytes invalid, in a different way
    let gas_object = authority.get_object(&gas).await.unwrap();
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
    let result = send_and_confirm_transaction(&authority, transaction)
        .await
        .unwrap()
        .1;
    assert_eq!(
        result.status(),
        &ExecutionStatus::Failure {
            error: ExecutionFailureStatus::VMVerificationOrDeserializationError,
            command: Some(0)
        }
    );

    // make the bytes invalid by adding metadata
    let gas_object = authority.get_object(&gas).await.unwrap();
    let gas_object_ref = gas_object.unwrap().compute_object_reference();
    let mut modules = correct_modules.clone();
    let new_bytes = {
        let mut m = CompiledModule::deserialize_with_defaults(&modules[0]).unwrap();
        m.metadata.push(move_core_types::metadata::Metadata {
            key: vec![0],
            value: vec![1],
        });
        let mut buf = vec![];
        m.serialize(&mut buf).unwrap();
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
    let result = send_and_confirm_transaction(&authority, transaction)
        .await
        .unwrap()
        .1;
    assert_eq!(
        result.status(),
        &ExecutionStatus::Failure {
            error: ExecutionFailureStatus::VMVerificationOrDeserializationError,
            command: Some(0)
        }
    )
}
