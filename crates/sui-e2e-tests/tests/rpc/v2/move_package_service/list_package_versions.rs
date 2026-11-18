// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::ident_str;
use sui_macros::sim_test;
use sui_move_build::{BuildConfig, CompiledPackage};
use sui_rpc::proto::sui::rpc::v2::{
    ListPackageVersionsRequest, ListPackageVersionsResponse,
    move_package_service_client::MovePackageServiceClient,
};
use sui_types::{
    SUI_FRAMEWORK_PACKAGE_ID,
    base_types::{ObjectID, ObjectRef},
    effects::TransactionEffectsAPI,
    move_package::UpgradePolicy,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{
        ObjectArg, ProgrammableTransaction, TEST_ONLY_GAS_UNIT_FOR_PUBLISH, Transaction,
        TransactionData,
    },
};
use test_cluster::{TestCluster, TestClusterBuilder};
use tonic::transport::Channel;

#[sim_test]
async fn test_list_package_versions_system_package() {
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;
    let mut service = MovePackageServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let mut request = ListPackageVersionsRequest::default();
    request.package_id = Some("0x2".to_string());

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

    let initial_package_id = response.get_new_package_obj().unwrap().0;
    package_ids.push(initial_package_id);

    let mut current_upgrade_cap = response.get_new_package_upgrade_cap().unwrap();

    // A listing stops at the highest checkpoint every rpc index has committed, and the indexes
    // catch up with execution asynchronously, so wait for them before each listing.
    cluster.wait_for_rpc_index_ready().await;
    let mut request = ListPackageVersionsRequest::default();
    request.package_id = Some(package_ids[0].to_string());

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

    let new_package_id = response.get_new_package_obj().unwrap().0;
    package_ids.push(new_package_id);

    current_upgrade_cap = response.get_new_package_upgrade_cap().unwrap();

    cluster.wait_for_rpc_index_ready().await;
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

    let third_package_id = response.get_new_package_obj().unwrap().0;

    package_ids.push(third_package_id);

    cluster.wait_for_rpc_index_ready().await;
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
    let mut request = ListPackageVersionsRequest::default();
    request.package_id = Some(package_ids[0].to_string());
    request.page_size = Some(2);

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
    let mut request = ListPackageVersionsRequest::default();
    request.package_id = Some(package_ids[0].to_string());
    request.page_size = Some(2);
    request.page_token = response.next_page_token;

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
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;
    let mut service = MovePackageServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let mut request = ListPackageVersionsRequest::default();
    request.package_id =
        Some("0x0000000000000000000000000000000000000000000000000000000000000999".to_string());

    let error = service.list_package_versions(request).await.unwrap_err();
    assert_eq!(error.code(), tonic::Code::NotFound);
}

#[sim_test]
async fn test_list_package_versions_invalid_package_id() {
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;
    let mut service = MovePackageServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let mut request = ListPackageVersionsRequest::default();
    request.package_id = Some("invalid-package-id".to_string());

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
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;
    let mut service = MovePackageServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let request = ListPackageVersionsRequest::default();

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
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;
    let mut service = MovePackageServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    // Test 1: Invalid page token encoding
    let mut request = ListPackageVersionsRequest::default();
    request.package_id = Some("0x2".to_string());
    request.page_size = Some(10);
    request.page_token = Some(vec![0xFF, 0xFF, 0xFF].into()); // Invalid BCS encoding

    let error = service.list_package_versions(request).await.unwrap_err();
    assert_eq!(error.code(), tonic::Code::InvalidArgument);
    assert!(
        error.message().contains("invalid page token encoding"),
        "Error message: {}",
        error.message()
    );

    // Test 2: Page token with mismatched package ID
    // Create a valid page token for a different package ID
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

    let mut request = ListPackageVersionsRequest::default();
    request.package_id = Some("0x2".to_string());
    request.page_size = Some(10);
    request.page_token = Some(encoded_token.into());

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

/// Execute a package publish or upgrade from the cluster's first address. Returns the new
/// package's id, the lineage's upgrade cap, and the checkpoint the transaction landed in.
async fn execute_package_write(
    cluster: &TestCluster,
    pt: ProgrammableTransaction,
) -> (ObjectID, ObjectRef, u64) {
    let transaction = sign_package_write(cluster, pt).await;
    execute_signed_package_write(cluster, transaction).await
}

/// Sign a package publish or upgrade from the cluster's first address.
async fn sign_package_write(cluster: &TestCluster, pt: ProgrammableTransaction) -> Transaction {
    let address = cluster.get_address_0();
    let gas_price = cluster.wallet.get_reference_gas_price().await.unwrap();
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
    cluster.sign_transaction(&transaction_data).await
}

/// Execute a transaction signed by [`sign_package_write`]; returns what
/// [`execute_package_write`] returns.
async fn execute_signed_package_write(
    cluster: &TestCluster,
    transaction: Transaction,
) -> (ObjectID, ObjectRef, u64) {
    let response = cluster.execute_transaction(transaction).await;
    let package_id = response.get_new_package_obj().unwrap().0;
    let upgrade_cap = response.get_new_package_upgrade_cap().unwrap();
    let checkpoint = cluster.fullnode_handle.sui_node.with(|node| {
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
    (package_id, upgrade_cap, checkpoint)
}

fn publish_upgradeable_pt(
    cluster: &TestCluster,
    package: &CompiledPackage,
) -> ProgrammableTransaction {
    let mut builder = ProgrammableTransactionBuilder::new();
    let upgrade_cap = builder.publish_upgradeable(
        package.get_package_bytes(false),
        package.get_dependency_storage_package_ids(),
    );
    builder.transfer_arg(cluster.get_address_0(), upgrade_cap);
    builder.finish()
}

async fn publish_upgradeable(
    cluster: &TestCluster,
    package: &CompiledPackage,
) -> (ObjectID, ObjectRef, u64) {
    execute_package_write(cluster, publish_upgradeable_pt(cluster, package)).await
}

/// Upgrade `package_id` by republishing `package`'s code.
async fn upgrade(
    cluster: &TestCluster,
    package: &CompiledPackage,
    package_id: ObjectID,
    upgrade_cap: ObjectRef,
) -> (ObjectID, ObjectRef, u64) {
    let mut builder = ProgrammableTransactionBuilder::new();
    let cap = builder
        .obj(ObjectArg::ImmOrOwnedObject(upgrade_cap))
        .unwrap();
    let policy = builder.pure(UpgradePolicy::COMPATIBLE).unwrap();
    let digest = builder
        .pure(package.get_package_digest(false).to_vec())
        .unwrap();
    let ticket = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        ident_str!("package").to_owned(),
        ident_str!("authorize_upgrade").to_owned(),
        vec![],
        vec![cap, policy, digest],
    );
    let receipt = builder.upgrade(
        package_id,
        ticket,
        package.get_dependency_storage_package_ids(),
        package.get_package_bytes(false),
    );
    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        ident_str!("package").to_owned(),
        ident_str!("commit_upgrade").to_owned(),
        vec![],
        vec![cap, receipt],
    );
    execute_package_write(cluster, builder.finish()).await
}

async fn list_versions(
    service: &mut MovePackageServiceClient<Channel>,
    package_id: ObjectID,
    page_size: Option<u32>,
    page_token: Option<prost::bytes::Bytes>,
) -> Result<ListPackageVersionsResponse, tonic::Status> {
    let mut request = ListPackageVersionsRequest::default();
    request.package_id = Some(package_id.to_string());
    request.page_size = page_size;
    request.page_token = page_token;
    service
        .list_package_versions(request)
        .await
        .map(tonic::Response::into_inner)
}

/// `(version, storage id, publish checkpoint)` of each listed version.
fn listed(response: &ListPackageVersionsResponse) -> Vec<(u64, String, Option<u64>)> {
    response
        .versions
        .iter()
        .map(|v| {
            (
                v.version.unwrap(),
                v.package_id.clone().unwrap(),
                v.checkpoint,
            )
        })
        .collect()
}

#[sim_test]
async fn test_list_package_versions_reports_publish_checkpoints() {
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;
    let mut service = MovePackageServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();

    let mut test_package_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    test_package_path.push("tests/move_test_code");
    let package = BuildConfig::new_for_testing()
        .build_async(&test_package_path)
        .await
        .unwrap();

    let (v1, cap, cp1) = publish_upgradeable(&cluster, &package).await;
    let (v2, cap, cp2) = upgrade(&cluster, &package, v1, cap).await;
    let (v3, _, cp3) = upgrade(&cluster, &package, v2, cap).await;
    // Each write waits for the fullnode to execute it from a checkpoint before the next is sent.
    assert!(0 < cp1 && cp1 < cp2 && cp2 < cp3, "{cp1} {cp2} {cp3}");
    cluster.wait_for_rpc_index_ready().await;

    let entry_1 = (1, v1.to_string(), Some(cp1));
    let entry_2 = (2, v2.to_string(), Some(cp2));
    let entry_3 = (3, v3.to_string(), Some(cp3));

    // Every version reports its publish checkpoint, through any lineage member.
    for package_id in [v1, v3] {
        let response = list_versions(&mut service, package_id, None, None)
            .await
            .unwrap();
        assert_eq!(
            listed(&response),
            vec![entry_1.clone(), entry_2.clone(), entry_3.clone()],
            "{package_id}"
        );
    }

    // So does every page.
    let first = list_versions(&mut service, v1, Some(2), None)
        .await
        .unwrap();
    assert_eq!(listed(&first), vec![entry_1.clone(), entry_2.clone()]);
    let second = list_versions(&mut service, v1, Some(2), first.next_page_token)
        .await
        .unwrap();
    assert_eq!(listed(&second), vec![entry_3.clone()]);
    assert!(second.next_page_token.is_none());
}
