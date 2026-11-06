// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::ident_str;
use sui_macros::sim_test;
use sui_move_build::BuildConfig;
use sui_rpc::proto::sui::rpc::v2::{
    GetPackageRequest, get_package_request::Selector,
    move_package_service_client::MovePackageServiceClient,
};
use sui_types::{
    SUI_FRAMEWORK_PACKAGE_ID,
    effects::TransactionEffectsAPI,
    move_package::UpgradePolicy,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{ObjectArg, TEST_ONLY_GAS_UNIT_FOR_PUBLISH, TransactionData},
};
use test_cluster::TestClusterBuilder;

use crate::v2::RpcStatusExt;
use crate::v2::assert_above_latest_available;
use crate::v2::assert_below_earliest_available;
use crate::v2::error_info;
use crate::v2::halt_chain;
use crate::v2::move_package_service::system_package_expectations::validate_system_package;
use crate::v2::prune_objects_below_tip;

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
        // Zero-epoch retention, the default, runs the tidehunter objects compactor, and a node
        // that runs it refuses `at_checkpoint` reads. It also lets the objects pruner raise the
        // floor above the checkpoints read below.
        .disable_fullnode_pruning()
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

    // The checkpoint that contains the upgrade transaction.
    let upgrade_checkpoint = cluster.fullnode_handle.sui_node.with(|node| {
        *node
            .state()
            .get_transaction_checkpoint_for_tests(
                response.effects.transaction_digest(),
                &node.state().epoch_store_for_testing(),
            )
            .unwrap()
            .unwrap()
            .sequence_number()
    });
    assert!(upgrade_checkpoint > 0);

    // Lineage and version lookups read the rpc-store's index pipelines, which lag
    // transaction execution; wait for them to catch up before any selector lookup.
    cluster.wait_for_rpc_index_ready().await;

    // Fetch first version which should be the original id.
    let mut request = GetPackageRequest::default();
    request.package_id = Some(upgraded_id.to_string());
    request.selector = Some(Selector::Version(1));
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
    request.selector = Some(Selector::Version(upgraded_version));
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
    request.selector = Some(Selector::Version(100));
    let error = service.get_package(request).await.unwrap_err();
    assert_eq!(error.code(), tonic::Code::NotFound);

    // The checkpoint that contains the upgrade transaction resolves to the upgraded package.
    let mut request = GetPackageRequest::default();
    request.package_id = Some(original_id.to_string());
    request.selector = Some(Selector::AtCheckpoint(upgrade_checkpoint));
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

    // The checkpoint path also accepts any lineage member's id.
    let mut request = GetPackageRequest::default();
    request.package_id = Some(upgraded_id.to_string());
    request.selector = Some(Selector::AtCheckpoint(upgrade_checkpoint));
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

    // The checkpoint just before the upgrade resolves to the original package: each
    // `sign_and_execute_transaction` waits for its checkpoint to execute before the next
    // transaction is built, so the publish settled in an earlier checkpoint than the upgrade.
    let mut request = GetPackageRequest::default();
    request.package_id = Some(original_id.to_string());
    request.selector = Some(Selector::AtCheckpoint(upgrade_checkpoint - 1));
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

    // The package did not exist as of the genesis checkpoint.
    let mut request = GetPackageRequest::default();
    request.package_id = Some(original_id.to_string());
    request.selector = Some(Selector::AtCheckpoint(0));
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

/// Pruning object versions through a checkpoint leaves packages readable at that checkpoint and
/// rejects every checkpoint below it as pruned.
#[sim_test]
async fn test_get_package_at_checkpoint_below_available_floor() {
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        // Only the explicit prune below moves the floor.
        .disable_fullnode_pruning()
        .build()
        .await;
    let mut service = MovePackageServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    halt_chain(&cluster).await;
    let pruned_to = prune_objects_below_tip(&cluster).await;

    let request = |package_id: &str, at_checkpoint| {
        let mut request = GetPackageRequest::default();
        request.package_id = Some(package_id.to_string());
        request.selector = Some(Selector::AtCheckpoint(at_checkpoint));
        request
    };
    // A pruned checkpoint is out of range whether or not the package exists.
    for package_id in ["0x3", "0xdeadbeef"] {
        for checkpoint in [0, pruned_to - 1] {
            let error = service
                .get_package(request(package_id, checkpoint))
                .await
                .unwrap_err();
            assert_below_earliest_available(&error, pruned_to);
        }
    }
    let package = service
        .get_package(request("0x3", pruned_to))
        .await
        .unwrap()
        .into_inner()
        .package
        .unwrap();
    assert_eq!(package.version, Some(1));
}

/// `at_checkpoint` is served through the highest checkpoint the rpc indexes have committed, and
/// is out of range above it whether or not the package exists.
#[sim_test]
async fn test_get_package_at_checkpoint_above_indexed_tip() {
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .disable_fullnode_pruning()
        .build()
        .await;
    let mut service = MovePackageServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let tip = halt_chain(&cluster).await;

    let request = |package_id: &str, at_checkpoint| {
        let mut request = GetPackageRequest::default();
        request.package_id = Some(package_id.to_string());
        request.selector = Some(Selector::AtCheckpoint(at_checkpoint));
        request
    };
    let package = service
        .get_package(request("0x3", tip))
        .await
        .unwrap()
        .into_inner()
        .package
        .unwrap();
    assert_eq!(package.version, Some(1));
    for package_id in ["0x3", "0xdeadbeef"] {
        let error = service
            .get_package(request(package_id, tip + 1))
            .await
            .unwrap_err();
        assert_above_latest_available(&error, "at_checkpoint", tip + 1);
        assert_eq!(
            error_info(&error.rpc_status()).metadata["firstUnavailableCheckpoint"],
            (tip + 1).to_string()
        );
    }
}
