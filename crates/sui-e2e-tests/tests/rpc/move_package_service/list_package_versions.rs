// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::ident_str;
use sui_json_rpc_types::ObjectChange;
use sui_macros::sim_test;
use sui_move_build::BuildConfig;
use sui_rpc_api::proto::rpc::v2alpha::{
    move_package_service_client::MovePackageServiceClient, ListPackageVersionsRequest,
};
use sui_types::{
    move_package::UpgradePolicy,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{ObjectArg, TransactionData, TEST_ONLY_GAS_UNIT_FOR_PUBLISH},
    SUI_FRAMEWORK_PACKAGE_ID,
};
use test_cluster::TestClusterBuilder;

#[sim_test]
async fn test_list_package_versions_system_package() {
    let cluster = TestClusterBuilder::new().build().await;
    let mut service = MovePackageServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let request = ListPackageVersionsRequest {
        package_id: Some("0x2".to_string()),
        page_size: None,
        page_token: None,
    };

    let response = service.list_package_versions(request).await.unwrap();
    let response = response.into_inner();

    assert_eq!(response.versions.len(), 1);

    let version = &response.versions[0];
    assert_eq!(
        version.package_id,
        Some("0x0000000000000000000000000000000000000000000000000000000000000002".to_string())
    );
    assert_eq!(version.version, Some(1));
}

#[sim_test]
async fn test_list_package_versions_with_upgrades() {
    let cluster = TestClusterBuilder::new().build().await;
    let mut service = MovePackageServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let mut test_package_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    test_package_path.push("tests/move_test_code");

    let compiled_package = BuildConfig::new_for_testing()
        .build(&test_package_path)
        .unwrap();
    let modules = compiled_package.get_package_bytes(false);
    let dependencies = compiled_package.get_dependency_storage_package_ids();

    let address = cluster.get_address_0();
    let gas_price = cluster.wallet.get_reference_gas_price().await.unwrap();
    let gas_object = cluster
        .wallet
        .get_one_gas_object_owned_by_address(address)
        .await
        .unwrap()
        .unwrap();

    let mut builder = ProgrammableTransactionBuilder::new();
    let upgrade_cap = builder.publish_upgradeable(modules.clone(), dependencies.clone());
    builder.transfer_arg(cluster.get_address_0(), upgrade_cap);
    let pt = builder.finish();

    let transaction_data = TransactionData::new_programmable(
        address,
        vec![gas_object],
        pt,
        TEST_ONLY_GAS_UNIT_FOR_PUBLISH * gas_price,
        gas_price,
    );

    let response = cluster
        .sign_and_execute_transaction(&transaction_data)
        .await;

    let mut package_ids = vec![];

    let object_changes = response.object_changes.unwrap();

    let initial_package_id = object_changes
        .iter()
        .find_map(|object| match object {
            ObjectChange::Published { package_id, .. } => Some(*package_id),
            _ => None,
        })
        .unwrap();

    package_ids.push(initial_package_id);

    let mut current_upgrade_cap = object_changes
        .iter()
        .find_map(|object| match object {
            ObjectChange::Created {
                object_id,
                object_type,
                digest,
                version,
                ..
            } if object_type.module.as_str() == "package"
                && object_type.name.as_str() == "UpgradeCap" =>
            {
                Some((*object_id, *version, *digest))
            }
            _ => None,
        })
        .unwrap();

    let request = ListPackageVersionsRequest {
        package_id: Some(package_ids[0].to_string()),
        page_size: None,
        page_token: None,
    };

    let grpc_response = service
        .list_package_versions(request.clone())
        .await
        .unwrap();
    let grpc_response = grpc_response.into_inner();

    assert_eq!(grpc_response.versions.len(), 1);
    assert_eq!(grpc_response.versions[0].version, Some(1));
    assert_eq!(
        grpc_response.versions[0].package_id,
        Some(package_ids[0].to_string())
    );

    // Perform first "upgrade" by republishing the same code
    let mut builder = ProgrammableTransactionBuilder::new();

    let cap = builder
        .obj(ObjectArg::ImmOrOwnedObject(current_upgrade_cap))
        .unwrap();

    let policy = builder.pure(UpgradePolicy::COMPATIBLE).unwrap();
    let digest = builder
        .pure(compiled_package.get_package_digest(false).to_vec())
        .unwrap();

    let ticket = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        ident_str!("package").to_owned(),
        ident_str!("authorize_upgrade").to_owned(),
        vec![],
        vec![cap, policy, digest],
    );

    let receipt = builder.upgrade(
        package_ids[0],
        ticket,
        compiled_package.get_dependency_storage_package_ids(),
        compiled_package.get_package_bytes(false),
    );

    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        ident_str!("package").to_owned(),
        ident_str!("commit_upgrade").to_owned(),
        vec![],
        vec![cap, receipt],
    );

    let pt = builder.finish();

    let gas_object = cluster
        .wallet
        .get_one_gas_object_owned_by_address(address)
        .await
        .unwrap()
        .unwrap();

    let transaction_data = TransactionData::new_programmable(
        address,
        vec![gas_object],
        pt,
        TEST_ONLY_GAS_UNIT_FOR_PUBLISH * gas_price,
        gas_price,
    );

    let response = cluster
        .sign_and_execute_transaction(&transaction_data)
        .await;

    let object_changes = response.object_changes.unwrap();

    let new_package_id = object_changes
        .iter()
        .find_map(|object| match object {
            ObjectChange::Published { package_id, .. } => Some(*package_id),
            _ => None,
        })
        .unwrap();

    package_ids.push(new_package_id);

    current_upgrade_cap = object_changes
        .iter()
        .find_map(|object| match object {
            ObjectChange::Mutated {
                object_id,
                object_type,
                digest,
                version,
                ..
            } if object_type.module.as_str() == "package"
                && object_type.name.as_str() == "UpgradeCap" =>
            {
                Some((*object_id, *version, *digest))
            }
            _ => None,
        })
        .unwrap();

    let grpc_response = service
        .list_package_versions(request.clone())
        .await
        .unwrap();
    let grpc_response = grpc_response.into_inner();

    assert_eq!(grpc_response.versions.len(), 2);
    assert_eq!(grpc_response.versions[0].version, Some(1));
    assert_eq!(
        grpc_response.versions[0].package_id,
        Some(package_ids[0].to_string())
    );
    assert_eq!(grpc_response.versions[1].version, Some(2));
    assert_eq!(
        grpc_response.versions[1].package_id,
        Some(package_ids[1].to_string())
    );

    // Perform second upgrade
    let mut builder = ProgrammableTransactionBuilder::new();

    let cap = builder
        .obj(ObjectArg::ImmOrOwnedObject(current_upgrade_cap))
        .unwrap();

    let policy = builder.pure(UpgradePolicy::COMPATIBLE).unwrap();
    let digest = builder
        .pure(compiled_package.get_package_digest(false).to_vec())
        .unwrap();

    let ticket = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        ident_str!("package").to_owned(),
        ident_str!("authorize_upgrade").to_owned(),
        vec![],
        vec![cap, policy, digest],
    );

    let receipt = builder.upgrade(
        package_ids[1],
        ticket,
        compiled_package.get_dependency_storage_package_ids(),
        compiled_package.get_package_bytes(false),
    );

    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        ident_str!("package").to_owned(),
        ident_str!("commit_upgrade").to_owned(),
        vec![],
        vec![cap, receipt],
    );

    let pt = builder.finish();

    let gas_object = cluster
        .wallet
        .get_one_gas_object_owned_by_address(address)
        .await
        .unwrap()
        .unwrap();

    let transaction_data = TransactionData::new_programmable(
        address,
        vec![gas_object],
        pt,
        TEST_ONLY_GAS_UNIT_FOR_PUBLISH * gas_price,
        gas_price,
    );

    let response = cluster
        .sign_and_execute_transaction(&transaction_data)
        .await;

    let object_changes = response.object_changes.unwrap();

    let third_package_id = object_changes
        .iter()
        .find_map(|object| match object {
            ObjectChange::Published { package_id, .. } => Some(*package_id),
            _ => None,
        })
        .unwrap();

    package_ids.push(third_package_id);

    let grpc_response = service.list_package_versions(request).await.unwrap();
    let grpc_response = grpc_response.into_inner();

    assert_eq!(grpc_response.versions.len(), 3);

    assert_eq!(grpc_response.versions[0].version, Some(1));
    assert_eq!(grpc_response.versions[1].version, Some(2));
    assert_eq!(grpc_response.versions[2].version, Some(3));

    assert_eq!(
        grpc_response.versions[0].package_id,
        Some(package_ids[0].to_string())
    );
    assert_eq!(
        grpc_response.versions[1].package_id,
        Some(package_ids[1].to_string())
    );
    assert_eq!(
        grpc_response.versions[2].package_id,
        Some(package_ids[2].to_string())
    );

    // Sanity Check - Verify all package IDs are different
    assert_ne!(package_ids[0], package_ids[1]);
    assert_ne!(package_ids[1], package_ids[2]);
    assert_ne!(package_ids[0], package_ids[2]);

    // Test pagination
    // Test 1: Get first page with page_size = 2
    let request = ListPackageVersionsRequest {
        package_id: Some(package_ids[0].to_string()),
        page_size: Some(2),
        page_token: None,
    };

    let response = service
        .list_package_versions(request)
        .await
        .unwrap()
        .into_inner();

    assert_eq!(response.versions.len(), 2);
    assert_eq!(response.versions[0].version, Some(1));
    assert_eq!(response.versions[1].version, Some(2));
    assert!(response.next_page_token.is_some());

    // Test 2: Get second page using page token
    let request = ListPackageVersionsRequest {
        package_id: Some(package_ids[0].to_string()),
        page_size: Some(2),
        page_token: response.next_page_token,
    };

    let response = service
        .list_package_versions(request)
        .await
        .unwrap()
        .into_inner();

    assert_eq!(response.versions.len(), 1);
    assert_eq!(response.versions[0].version, Some(3));
    assert!(response.next_page_token.is_none());
}

#[sim_test]
async fn test_list_package_versions_not_found() {
    let cluster = TestClusterBuilder::new().build().await;
    let mut service = MovePackageServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let request = ListPackageVersionsRequest {
        package_id: Some(
            "0x0000000000000000000000000000000000000000000000000000000000000999".to_string(),
        ),
        page_size: None,
        page_token: None,
    };

    let error = service.list_package_versions(request).await.unwrap_err();
    assert_eq!(error.code(), tonic::Code::NotFound);
}

#[sim_test]
async fn test_list_package_versions_invalid_package_id() {
    let cluster = TestClusterBuilder::new().build().await;
    let mut service = MovePackageServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let request = ListPackageVersionsRequest {
        package_id: Some("invalid-package-id".to_string()),
        page_size: None,
        page_token: None,
    };

    let error = service.list_package_versions(request).await.unwrap_err();
    assert_eq!(error.code(), tonic::Code::InvalidArgument);
    assert!(
        error.message().contains("invalid package_id"),
        "Error message: {}",
        error.message()
    );
}

#[sim_test]
async fn test_list_package_versions_missing_package_id() {
    let cluster = TestClusterBuilder::new().build().await;
    let mut service = MovePackageServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let request = ListPackageVersionsRequest {
        package_id: None,
        page_size: None,
        page_token: None,
    };

    let error = service.list_package_versions(request).await.unwrap_err();
    assert_eq!(error.code(), tonic::Code::InvalidArgument);
    assert!(
        error.message().contains("missing package_id"),
        "Error message: {}",
        error.message()
    );
}

#[sim_test]
async fn test_list_package_versions_invalid_pagination() {
    let cluster = TestClusterBuilder::new().build().await;
    let mut service = MovePackageServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    // Test 1: Invalid page token encoding
    let request = ListPackageVersionsRequest {
        package_id: Some("0x2".to_string()),
        page_size: Some(10),
        page_token: Some(vec![0xFF, 0xFF, 0xFF].into()), // Invalid BCS encoding
    };

    let error = service.list_package_versions(request).await.unwrap_err();
    assert_eq!(error.code(), tonic::Code::InvalidArgument);
    assert!(
        error.message().contains("invalid page token encoding"),
        "Error message: {}",
        error.message()
    );

    // Test 2: Page token with mismatched package ID
    // Create a valid page token for a different package ID
    use sui_types::base_types::ObjectID;

    #[derive(serde::Serialize)]
    struct PageToken {
        original_package_id: ObjectID,
        version: u64,
    }

    let different_package_id = ObjectID::from_hex_literal(
        "0x0000000000000000000000000000000000000000000000000000000000000999",
    )
    .unwrap();

    let page_token = PageToken {
        original_package_id: different_package_id,
        version: 1,
    };

    let encoded_token = bcs::to_bytes(&page_token).unwrap();

    let request = ListPackageVersionsRequest {
        package_id: Some("0x2".to_string()),
        page_size: Some(10),
        page_token: Some(encoded_token.into()),
    };

    let error = service.list_package_versions(request).await.unwrap_err();
    assert_eq!(error.code(), tonic::Code::InvalidArgument);
    assert!(
        error
            .message()
            .contains("page token package ID does not match request package ID"),
        "Error message: {}",
        error.message()
    );
}
