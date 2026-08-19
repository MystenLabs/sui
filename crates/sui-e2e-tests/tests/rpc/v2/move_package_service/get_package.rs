// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::ident_str;
use sui_macros::sim_test;
use sui_move_build::BuildConfig;
use sui_rpc::proto::sui::rpc::v2::{
    GetPackageRequest, move_package_service_client::MovePackageServiceClient,
};
use sui_types::{
    SUI_FRAMEWORK_PACKAGE_ID,
    move_package::UpgradePolicy,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{ObjectArg, TEST_ONLY_GAS_UNIT_FOR_PUBLISH, TransactionData},
};
use test_cluster::TestClusterBuilder;

use crate::v2::move_package_service::system_package_expectations::validate_system_package;

#[sim_test]
async fn test_get_package_system() {
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;
    let mut service = MovePackageServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let mut request = GetPackageRequest::default();
    request.package_id = Some("0x3".to_string());

    let response = service.get_package(request).await.unwrap();
    let package = response.into_inner().package.unwrap();

    validate_system_package(&package);
}

#[sim_test]
async fn test_get_package_not_found() {
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;
    let mut service = MovePackageServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let mut request = GetPackageRequest::default();
    request.package_id =
        Some("0xDEADBEEFDEADBEEFDEADBEEFDEADBEEFDEADBEEFDEADBEEFDEADBEEFDEADBEEF".to_string());

    let error = service.get_package(request).await.unwrap_err();
    assert_eq!(error.code(), tonic::Code::NotFound);
}

#[sim_test]
async fn test_get_package_invalid_hex() {
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;
    let mut service = MovePackageServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let mut request = GetPackageRequest::default();
    request.package_id = Some("0xINVALID".to_string());

    let error = service.get_package(request).await.unwrap_err();
    assert_eq!(error.code(), tonic::Code::InvalidArgument);
    assert!(error.message().contains("invalid package_id"));
}

#[sim_test]
async fn test_get_package_lineage_lookups() {
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;
    let mut service = MovePackageServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let mut test_package_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    test_package_path.push("tests/move_test_code");

    let compiled_package = BuildConfig::new_for_testing()
        .build_async(&test_package_path)
        .await
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
    builder.transfer_arg(address, upgrade_cap);
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

    let original_id = response.get_new_package_obj().unwrap().0;
    let upgrade_cap = response.get_new_package_upgrade_cap().unwrap();

    // Upgrade by republishing the same code.
    let mut builder = ProgrammableTransactionBuilder::new();
    let cap = builder
        .obj(ObjectArg::ImmOrOwnedObject(upgrade_cap))
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
        original_id,
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
    let (upgraded_id, upgraded_version, _) = response.get_new_package_obj().unwrap();
    let upgraded_version = upgraded_version.value();

    // Fetch first version which should be the original id.
    let mut request = GetPackageRequest::default();
    request.package_id = Some(upgraded_id.to_string());
    request.version = Some(1);
    let package = service
        .get_package(request)
        .await
        .unwrap()
        .into_inner()
        .package
        .unwrap();
    assert_eq!(
        package.storage_id,
        Some(original_id.to_canonical_string(true))
    );
    assert_eq!(package.version, Some(1));

    // Exact version through the original id resolves forward to the upgrade.
    let mut request = GetPackageRequest::default();
    request.package_id = Some(original_id.to_string());
    request.version = Some(upgraded_version);
    let package = service
        .get_package(request)
        .await
        .unwrap()
        .into_inner()
        .package
        .unwrap();
    assert_eq!(
        package.storage_id,
        Some(upgraded_id.to_canonical_string(true))
    );
    assert_eq!(package.version, Some(upgraded_version));

    // A version that was never published is not found.
    let mut request = GetPackageRequest::default();
    request.package_id = Some(original_id.to_string());
    request.version = Some(100);
    let error = service.get_package(request).await.unwrap_err();
    assert_eq!(error.code(), tonic::Code::NotFound);

    // A checkpoint upper-bound above the tip resolves to the latest version.
    let mut request = GetPackageRequest::default();
    request.package_id = Some(original_id.to_string());
    request.at_checkpoint = Some(u64::MAX);
    let package = service
        .get_package(request)
        .await
        .unwrap()
        .into_inner()
        .package
        .unwrap();
    assert_eq!(
        package.storage_id,
        Some(upgraded_id.to_canonical_string(true))
    );
    assert_eq!(package.version, Some(upgraded_version));

    // The checkpoint path also accepts any lineage member's id, and returns the latest version in
    // that lineage as of the checkpoint.
    let mut request = GetPackageRequest::default();
    request.package_id = Some(upgraded_id.to_string());
    request.at_checkpoint = Some(u64::MAX);
    let package = service
        .get_package(request)
        .await
        .unwrap()
        .into_inner()
        .package
        .unwrap();
    assert_eq!(
        package.storage_id,
        Some(upgraded_id.to_canonical_string(true))
    );
    assert_eq!(package.version, Some(upgraded_version));

    // The package did not exist as of the genesis checkpoint.
    let mut request = GetPackageRequest::default();
    request.package_id = Some(original_id.to_string());
    request.at_checkpoint = Some(0);
    let error = service.get_package(request).await.unwrap_err();
    assert_eq!(error.code(), tonic::Code::NotFound);

    // With no version bound, the storage id is looked up exactly.
    let mut request = GetPackageRequest::default();
    request.package_id = Some(original_id.to_string());
    let package = service
        .get_package(request)
        .await
        .unwrap()
        .into_inner()
        .package
        .unwrap();
    assert_eq!(
        package.storage_id,
        Some(original_id.to_canonical_string(true))
    );
    assert_eq!(package.version, Some(1));

    // Setting both bounds is rejected.
    let mut request = GetPackageRequest::default();
    request.package_id = Some(original_id.to_string());
    request.version = Some(1);
    request.at_checkpoint = Some(0);
    let error = service.get_package(request).await.unwrap_err();
    assert_eq!(error.code(), tonic::Code::InvalidArgument);
}

#[sim_test]
async fn test_get_package_missing_id() {
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;
    let mut service = MovePackageServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let request = GetPackageRequest::default();

    let error = service.get_package(request).await.unwrap_err();
    assert_eq!(error.code(), tonic::Code::InvalidArgument);
    assert!(error.message().contains("missing package_id"));
}

#[sim_test]
async fn test_get_package_at_checkpoint_below_available_floor() {
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;
    let mut service = MovePackageServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    // Assert on system package before bumping the floor.
    let mut request = GetPackageRequest::default();
    request.package_id = Some("0x3".to_string());
    request.at_checkpoint = Some(0);
    let package = service
        .get_package(request)
        .await
        .unwrap()
        .into_inner()
        .package
        .unwrap();
    assert_eq!(package.version, Some(1));

    // Raise the serving floor by bumping the checkpoint store's pruned watermark past genesis,
    // which moves the floor `get_lowest_available_checkpoint` reports.
    let pruned_to = loop {
        let highest_executed = cluster.fullnode_handle.sui_node.with(|node| {
            node.state()
                .get_checkpoint_store()
                .get_highest_executed_checkpoint()
                .unwrap()
        });
        if let Some(checkpoint) = highest_executed
            && *checkpoint.sequence_number() >= 1
        {
            cluster.fullnode_handle.sui_node.with(|node| {
                node.state()
                    .get_checkpoint_store()
                    .update_highest_pruned_checkpoint(&checkpoint)
                    .unwrap()
            });
            break *checkpoint.sequence_number();
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    };

    // Request now below the floor, should be rejected.
    let mut request = GetPackageRequest::default();
    request.package_id = Some("0x3".to_string());
    request.at_checkpoint = Some(pruned_to);
    let error = service.get_package(request).await.unwrap_err();
    assert_eq!(error.code(), tonic::Code::NotFound);
    assert!(error.message().contains("has been pruned"));

    // Bounds at or above the floor still resolve.
    let mut request = GetPackageRequest::default();
    request.package_id = Some("0x3".to_string());
    request.at_checkpoint = Some(u64::MAX);
    let package = service
        .get_package(request)
        .await
        .unwrap()
        .into_inner()
        .package
        .unwrap();
    assert_eq!(package.version, Some(1));
}
