// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::ident_str;
use prost::bytes::Bytes;
use sui_macros::sim_test;
use sui_move_build::{BuildConfig, CompiledPackage};
use sui_rpc::proto::sui::rpc::v2::{
    ListPackageVersionsRequest, ListPackageVersionsResponse, Ordering, QueryOptions,
    move_package_service_client::MovePackageServiceClient,
};
use sui_rpc_api::grpc::v2::move_package_service::PackageVersionCursor;
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

use crate::v2::RpcStatusExt;
use crate::v2::assert_above_latest_available;
use crate::v2::assert_below_earliest_available;
use crate::v2::error_info;
use crate::v2::halt_chain;
use crate::v2::prune_objects_below_tip;

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
    at_checkpoint: Option<u64>,
    page_size: Option<u32>,
    page_token: Option<prost::bytes::Bytes>,
) -> Result<ListPackageVersionsResponse, tonic::Status> {
    let mut request = ListPackageVersionsRequest::default();
    request.package_id = Some(package_id.to_string());
    request.at_checkpoint = at_checkpoint;
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

fn query_options(
    limit: Option<u32>,
    ordering: Ordering,
    after: Option<&Bytes>,
    before: Option<&Bytes>,
) -> QueryOptions {
    let mut options = QueryOptions::default();
    options.limit = limit;
    options.ordering = Some(ordering as i32);
    options.after = after.cloned();
    options.before = before.cloned();
    options
}

/// A cursor-mode request listing the versions of `package_id`'s lineage.
fn cursor_request(
    package_id: ObjectID,
    limit: Option<u32>,
    ordering: Ordering,
    after: Option<&Bytes>,
    before: Option<&Bytes>,
) -> ListPackageVersionsRequest {
    let mut request = ListPackageVersionsRequest::default();
    request.package_id = Some(package_id.to_string());
    request.options = Some(query_options(limit, ordering, after, before));
    request
}

/// A request listing the versions of every package.
fn all_packages_request(
    limit: Option<u32>,
    ordering: Ordering,
    after: Option<&Bytes>,
    before: Option<&Bytes>,
) -> ListPackageVersionsRequest {
    let mut request = ListPackageVersionsRequest::default();
    request.options = Some(query_options(limit, ordering, after, before));
    request
}

async fn list_request(
    service: &mut MovePackageServiceClient<Channel>,
    request: ListPackageVersionsRequest,
) -> ListPackageVersionsResponse {
    service
        .list_package_versions(request)
        .await
        .unwrap()
        .into_inner()
}

/// The versions a response lists, in order, checking that each carries a cursor.
fn listed_with_cursors(response: &ListPackageVersionsResponse) -> Vec<u64> {
    response
        .versions
        .iter()
        .map(|v| {
            assert!(v.cursor.is_some(), "version {:?} has no cursor", v.version);
            v.version.unwrap()
        })
        .collect()
}

/// The cursor of the last version a response lists.
fn last_cursor(response: &ListPackageVersionsResponse) -> Bytes {
    response.versions.last().unwrap().cursor.clone().unwrap()
}

/// `(publish checkpoint, original id, version, storage id)` of each version a listing of every
/// package returns, in order, checking that each carries a cursor.
fn listed_publishes(response: &ListPackageVersionsResponse) -> Vec<(u64, ObjectID, u64, ObjectID)> {
    assert!(response.next_page_token.is_none());
    response
        .versions
        .iter()
        .map(|v| {
            assert!(v.cursor.is_some(), "version {v:?} has no cursor");
            (
                v.checkpoint.unwrap(),
                v.original_id.as_deref().unwrap().parse().unwrap(),
                v.version.unwrap(),
                v.package_id.as_deref().unwrap().parse().unwrap(),
            )
        })
        .collect()
}

/// Every version a listing of every package returns, read in pages of `limit` in `ordering`, each
/// page bounded by the cursor of the last version of the page before it.
async fn page_through_all_packages(
    service: &mut MovePackageServiceClient<Channel>,
    limit: u32,
    ordering: Ordering,
    expected_len: usize,
) -> Vec<(u64, ObjectID, u64, ObjectID)> {
    let mut listed = vec![];
    let mut cursor = None;
    // Bounded so a cursor that fails to advance fails the test instead of looping forever.
    for _ in 0..=expected_len {
        let (after, before) = match ordering {
            Ordering::Descending => (None, cursor.as_ref()),
            _ => (cursor.as_ref(), None),
        };
        let page = list_request(
            service,
            all_packages_request(Some(limit), ordering, after, before),
        )
        .await;
        let publishes = listed_publishes(&page);
        listed.extend(publishes.iter().copied());
        if publishes.len() < limit as usize {
            return listed;
        }
        cursor = Some(last_cursor(&page));
    }
    panic!("paging did not reach a short page");
}

#[sim_test]
async fn test_list_package_versions_at_checkpoint() {
    // Background pruning stays off so bounds just below the first publish stay above the floor.
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .disable_fullnode_pruning()
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

    // Every version reports its publish checkpoint, with or without a bound.
    for at_checkpoint in [None, Some(cp3)] {
        let response = list_versions(&mut service, v1, at_checkpoint, None, None)
            .await
            .unwrap();
        assert_eq!(
            listed(&response),
            vec![entry_1.clone(), entry_2.clone(), entry_3.clone()],
            "at_checkpoint {at_checkpoint:?}"
        );
    }

    // A bound cuts the lineage at the last version published at or before it, through any
    // lineage member.
    for (package_id, at_checkpoint, expected) in [
        (v1, cp1, vec![entry_1.clone()]),
        (v3, cp1, vec![entry_1.clone()]),
        (v1, cp2 - 1, vec![entry_1.clone()]),
        (v1, cp2, vec![entry_1.clone(), entry_2.clone()]),
        (v2, cp3 - 1, vec![entry_1.clone(), entry_2.clone()]),
        (v1, cp1 - 1, vec![]),
    ] {
        let response = list_versions(&mut service, package_id, Some(at_checkpoint), None, None)
            .await
            .unwrap();
        assert_eq!(listed(&response), expected, "at_checkpoint {at_checkpoint}");
        assert!(response.next_page_token.is_none());
    }

    // A bound above the indexed ceiling is rejected, never clamped to the latest known version,
    // and a bound exactly at the ceiling is served.
    let error = list_versions(&mut service, v1, Some(u64::MAX), None, None)
        .await
        .unwrap_err();
    assert_above_latest_available(&error, "at_checkpoint", u64::MAX);
    let first_unavailable: u64 =
        error_info(&error.rpc_status()).metadata["firstUnavailableCheckpoint"]
            .parse()
            .unwrap();
    let highest_indexed = first_unavailable - 1;
    let response = list_versions(&mut service, v1, Some(highest_indexed), None, None)
        .await
        .unwrap();
    assert_eq!(
        listed(&response),
        vec![entry_1.clone(), entry_2.clone(), entry_3.clone()]
    );

    // Pages stay within the bound: the token points at the next version under it, and a page
    // whose next version is above the bound carries no token.
    let first = list_versions(&mut service, v1, Some(cp3), Some(2), None)
        .await
        .unwrap();
    assert_eq!(listed(&first), vec![entry_1.clone(), entry_2.clone()]);
    let second = list_versions(&mut service, v1, Some(cp3), Some(2), first.next_page_token)
        .await
        .unwrap();
    assert_eq!(listed(&second), vec![entry_3.clone()]);
    assert!(second.next_page_token.is_none());

    let first = list_versions(&mut service, v1, Some(cp2), Some(1), None)
        .await
        .unwrap();
    assert_eq!(listed(&first), vec![entry_1.clone()]);
    let second = list_versions(&mut service, v1, Some(cp2), Some(1), first.next_page_token)
        .await
        .unwrap();
    assert_eq!(listed(&second), vec![entry_2.clone()]);
    assert!(second.next_page_token.is_none());
}

#[sim_test]
async fn test_list_package_versions_at_checkpoint_above_indexed_is_checked_before_lookup() {
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .build()
        .await;
    let mut service = MovePackageServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();
    cluster.wait_for_rpc_index_ready().await;

    // A package that was never published gets `OutOfRange`, not `NotFound`, for a bound above
    // the indexed ceiling.
    let never_published = ObjectID::from_hex_literal("0x999").unwrap();
    let error = list_versions(&mut service, never_published, Some(u64::MAX), None, None)
        .await
        .unwrap_err();
    assert_above_latest_available(&error, "at_checkpoint", u64::MAX);

    // So does a package published above the ceiling but at or below the bound. Sign a publish and
    // derive its package id, the transaction's first created object, before the fullnode has
    // executed it. The bound is far enough above the ceiling that the publish lands at or below it.
    let first_unavailable: u64 =
        error_info(&error.rpc_status()).metadata["firstUnavailableCheckpoint"]
            .parse()
            .unwrap();
    let at_checkpoint = first_unavailable + 1000;

    let mut test_package_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    test_package_path.push("tests/move_test_code");
    let package = BuildConfig::new_for_testing()
        .build_async(&test_package_path)
        .await
        .unwrap();
    let transaction =
        sign_package_write(&cluster, publish_upgradeable_pt(&cluster, &package)).await;
    let package_id = ObjectID::derive_id(*transaction.digest(), 0);

    let error = list_versions(&mut service, package_id, Some(at_checkpoint), None, None)
        .await
        .unwrap_err();
    assert_above_latest_available(&error, "at_checkpoint", at_checkpoint);

    let (published_id, _, published_checkpoint) =
        execute_signed_package_write(&cluster, transaction).await;
    assert_eq!(published_id, package_id);
    assert!(
        published_checkpoint <= at_checkpoint,
        "{published_checkpoint} {at_checkpoint}"
    );
}

#[sim_test]
async fn test_list_package_versions_cursor_paging() {
    // Background pruning stays off so `at_checkpoint` bounds stay above the floor.
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .disable_fullnode_pruning()
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
    let (_, _, cp3) = upgrade(&cluster, &package, v2, cap).await;
    assert!(0 < cp1 && cp1 < cp2 && cp2 < cp3, "{cp1} {cp2} {cp3}");
    cluster.wait_for_rpc_index_ready().await;

    // Every version's cursor, through a later lineage member.
    let all = list_request(
        &mut service,
        cursor_request(v2, None, Ordering::Ascending, None, None),
    )
    .await;
    assert_eq!(listed_with_cursors(&all), vec![1, 2, 3]);
    assert!(all.next_page_token.is_none());
    let cursor = |version: usize| all.versions[version - 1].cursor.clone().unwrap();
    let (c1, c2, c3) = (cursor(1), cursor(2), cursor(3));

    let response = list_request(
        &mut service,
        cursor_request(v1, None, Ordering::Descending, None, None),
    )
    .await;
    assert_eq!(listed_with_cursors(&response), vec![3, 2, 1]);
    assert!(response.next_page_token.is_none());

    // A backward page from the end, then the page before it, which is short because it reaches
    // the first version.
    let last = list_request(
        &mut service,
        cursor_request(v1, Some(2), Ordering::Descending, None, None),
    )
    .await;
    assert_eq!(listed_with_cursors(&last), vec![3, 2]);
    let earlier = list_request(
        &mut service,
        cursor_request(
            v1,
            Some(2),
            Ordering::Descending,
            None,
            Some(&last_cursor(&last)),
        ),
    )
    .await;
    assert_eq!(listed_with_cursors(&earlier), vec![1]);

    // Forward pages from the backward page's lowest cursor, until a short page reaches the end.
    let next = list_request(
        &mut service,
        cursor_request(
            v1,
            Some(1),
            Ordering::Ascending,
            Some(&last_cursor(&last)),
            None,
        ),
    )
    .await;
    assert_eq!(listed_with_cursors(&next), vec![3]);
    let end = list_request(
        &mut service,
        cursor_request(
            v1,
            Some(1),
            Ordering::Ascending,
            Some(&last_cursor(&next)),
            None,
        ),
    )
    .await;
    assert_eq!(listed_with_cursors(&end), Vec::<u64>::new());
    assert!(end.next_page_token.is_none());

    // Both cursors bound the listing in either ordering.
    for ordering in [Ordering::Ascending, Ordering::Descending] {
        let response = list_request(
            &mut service,
            cursor_request(v1, None, ordering, Some(&c1), Some(&c3)),
        )
        .await;
        assert_eq!(listed_with_cursors(&response), vec![2], "{ordering:?}");
    }

    // Version filters, cursors and `at_checkpoint` intersect. Each case lists the matching
    // versions in ascending order; descending lists them in reverse.
    for (start_version, end_version, after, at_checkpoint, ascending) in [
        (Some(2), None, None, None, vec![2, 3]),
        (None, Some(3), None, None, vec![1, 2]),
        (Some(1), Some(4), Some(&c1), Some(cp2), vec![2]),
        (None, None, None, Some(cp3 - 1), vec![1, 2]),
        (Some(3), None, None, Some(cp2), vec![]),
    ] {
        for (ordering, expected) in [
            (Ordering::Ascending, ascending.clone()),
            (
                Ordering::Descending,
                ascending.iter().rev().copied().collect(),
            ),
        ] {
            let mut request = cursor_request(v1, None, ordering, after, None);
            request.start_version = start_version;
            request.end_version = end_version;
            request.at_checkpoint = at_checkpoint;
            let response = list_request(&mut service, request).await;
            assert_eq!(
                listed_with_cursors(&response),
                expected,
                "versions [{start_version:?}, {end_version:?}) after {after:?} at_checkpoint \
                 {at_checkpoint:?} {ordering:?}"
            );
        }
    }

    // Under `at_checkpoint`, a descending page skips the versions published after it.
    let mut request = cursor_request(v1, Some(1), Ordering::Descending, None, None);
    request.at_checkpoint = Some(cp2);
    let first = list_request(&mut service, request).await;
    assert_eq!(listed_with_cursors(&first), vec![2]);
    let mut request = cursor_request(
        v1,
        Some(1),
        Ordering::Descending,
        None,
        Some(&last_cursor(&first)),
    );
    request.at_checkpoint = Some(cp2);
    let second = list_request(&mut service, request).await;
    assert_eq!(listed_with_cursors(&second), vec![1]);

    // A cursor for a version this node has no row for continues a listing from a newer view of
    // the lineage, such as another server's. A page from it is rejected in either order rather
    // than listing from the newest version here: a descending page would skip the versions in
    // between, and an ascending page would end at the newest version here as if the lineage
    // ended there.
    let unpublished = PackageVersionCursor::Lineage {
        original_package_id: v1,
        version: 4,
    }
    .encode();
    for ordering in [Ordering::Ascending, Ordering::Descending] {
        for (after, before) in [(Some(&unpublished), None), (None, Some(&unpublished))] {
            let error = service
                .list_package_versions(cursor_request(v1, None, ordering, after, before))
                .await
                .unwrap_err();
            let first_unavailable: u64 =
                error_info(&error.rpc_status()).metadata["firstUnavailableCheckpoint"]
                    .parse()
                    .unwrap();
            assert_above_latest_available(&error, "cursor", first_unavailable);
        }
    }

    // Page tokens resume after the last listed version, and their versions carry the same
    // cursors.
    let first = list_versions(&mut service, v1, None, Some(2), None)
        .await
        .unwrap();
    assert_eq!(listed_with_cursors(&first), vec![1, 2]);
    assert_eq!(first.versions[0].cursor, Some(c1));
    assert_eq!(first.versions[1].cursor, Some(c2));
    let second = list_versions(&mut service, v1, None, Some(2), first.next_page_token)
        .await
        .unwrap();
    assert_eq!(listed_with_cursors(&second), vec![3]);
    assert_eq!(second.versions[0].cursor, Some(c3));
    assert!(second.next_page_token.is_none());
}

#[sim_test]
async fn test_list_all_package_versions() {
    // Background pruning stays off so genesis stays above the floor.
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .disable_fullnode_pruning()
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
    let (v2, _, cp2) = upgrade(&cluster, &package, v1, cap).await;
    let (other, _, cp3) = publish_upgradeable(&cluster, &package).await;
    assert!(0 < cp1 && cp1 < cp2 && cp2 < cp3, "{cp1} {cp2} {cp3}");
    cluster.wait_for_rpc_index_ready().await;

    // Every version in publish order: the system packages from genesis, then the publishes above.
    let everything = list_request(&mut service, ListPackageVersionsRequest::default()).await;
    let all = listed_publishes(&everything);
    let (genesis, published) = all.split_at(all.len() - 3);
    assert_eq!(
        published,
        [(cp1, v1, 1, v1), (cp2, v1, 2, v2), (cp3, other, 1, other)]
    );
    assert!(
        genesis.iter().all(
            |&(checkpoint, original_id, version, storage_id)| checkpoint == 0
                && version == 1
                && original_id == storage_id
        ),
        "{genesis:?}"
    );
    assert!(genesis.contains(&(0, SUI_FRAMEWORK_PACKAGE_ID, 1, SUI_FRAMEWORK_PACKAGE_ID)));
    assert!(all.windows(2).all(|pair| pair[0] < pair[1]), "{all:?}");
    // More versions share the genesis checkpoint than fit in the pages below.
    assert!(genesis.len() > 2, "{genesis:?}");

    // Pages of two cover the listing exactly in either direction, splitting checkpoints.
    assert_eq!(
        page_through_all_packages(&mut service, 2, Ordering::Ascending, all.len()).await,
        all
    );
    let mut reversed = all.clone();
    reversed.reverse();
    assert_eq!(
        page_through_all_packages(&mut service, 2, Ordering::Descending, all.len()).await,
        reversed
    );

    // A cursor from a descending page bounds an ascending one: the four newest versions end with
    // the last genesis package, and everything after it is the publishes above.
    let newest = list_request(
        &mut service,
        all_packages_request(Some(4), Ordering::Descending, None, None),
    )
    .await;
    let after_genesis = list_request(
        &mut service,
        all_packages_request(None, Ordering::Ascending, Some(&last_cursor(&newest)), None),
    )
    .await;
    assert_eq!(listed_publishes(&after_genesis), published);

    // `start_checkpoint`, `at_checkpoint` and cursors intersect. Each case lists the matching
    // versions in ascending order; descending lists them in reverse.
    let c1 = everything.versions[genesis.len()].cursor.clone().unwrap();
    for (start_checkpoint, at_checkpoint, after, ascending) in [
        (Some(cp1), None, None, published.to_vec()),
        (Some(cp1), Some(cp2), None, published[..2].to_vec()),
        (None, Some(cp2), None, all[..all.len() - 1].to_vec()),
        (Some(1), Some(cp3), Some(&c1), published[1..].to_vec()),
        (Some(cp3), Some(cp2), None, vec![]),
    ] {
        for (ordering, expected) in [
            (Ordering::Ascending, ascending.clone()),
            (
                Ordering::Descending,
                ascending.iter().rev().copied().collect(),
            ),
        ] {
            let mut request = all_packages_request(None, ordering, after, None);
            request.start_checkpoint = start_checkpoint;
            request.at_checkpoint = at_checkpoint;
            let response = list_request(&mut service, request).await;
            assert_eq!(
                listed_publishes(&response),
                expected,
                "[{start_checkpoint:?}, {at_checkpoint:?}] after {after:?} {ordering:?}"
            );
        }
    }

    // A bound above the indexed ceiling is rejected.
    let mut request = ListPackageVersionsRequest::default();
    request.at_checkpoint = Some(u64::MAX);
    let error = service.list_package_versions(request).await.unwrap_err();
    assert_above_latest_available(&error, "at_checkpoint", u64::MAX);

    // A cursor above the indexed tip continues a listing from a newer view of the ledger, such as
    // another server's. A page from it is rejected in either order rather than listing from the
    // tip: a descending page would skip the versions in between, and an ascending page would end
    // at the tip as if the listing ended there.
    let above_tip = PackageVersionCursor::AllPackages {
        checkpoint: u64::MAX,
        original_package_id: v1,
        version: 1,
    }
    .encode();
    for ordering in [Ordering::Ascending, Ordering::Descending] {
        for (after, before) in [(Some(&above_tip), None), (None, Some(&above_tip))] {
            let error = service
                .list_package_versions(all_packages_request(None, ordering, after, before))
                .await
                .unwrap_err();
            assert_above_latest_available(&error, "cursor", u64::MAX);
        }
    }

    // One package's listing names its lineage on every version.
    let lineage = list_request(
        &mut service,
        cursor_request(v2, None, Ordering::Ascending, None, None),
    )
    .await;
    assert_eq!(listed_with_cursors(&lineage), vec![1, 2]);
    for version in &lineage.versions {
        assert_eq!(version.original_id, Some(v1.to_string()));
    }
}

#[sim_test]
async fn test_list_package_versions_at_checkpoint_below_object_pruning_floor() {
    // Background pruning stays off so only the explicit prune below moves the floor.
    let cluster = TestClusterBuilder::new()
        .with_num_validators(1)
        .disable_fullnode_pruning()
        .build()
        .await;
    let mut service = MovePackageServiceClient::connect(cluster.rpc_url().to_owned())
        .await
        .unwrap();
    let framework = SUI_FRAMEWORK_PACKAGE_ID;
    let genesis_framework = (1, framework.to_string(), Some(0));

    cluster.wait_for_rpc_index_ready().await;
    let response = list_versions(&mut service, framework, Some(0), None, None)
        .await
        .unwrap();
    assert_eq!(listed(&response), vec![genesis_framework.clone()]);
    // A position in the listing of every package, at genesis, taken before the floor rises.
    let genesis_cursor = last_cursor(
        &list_request(
            &mut service,
            all_packages_request(Some(1), Ordering::Ascending, None, None),
        )
        .await,
    );

    // Prune object versions, which raises the floor above genesis to the last pruned checkpoint:
    // the state at its end survives the prune.
    halt_chain(&cluster).await;
    let floor = prune_objects_below_tip(&cluster).await;

    // With background pruning off, the prune is the only floor, and a bound at genesis is below
    // it.
    let error = list_versions(&mut service, framework, Some(0), None, None)
        .await
        .unwrap_err();
    assert_below_earliest_available(&error, floor);

    // Every bound below the floor fails rather than returning a partial list.
    let error = list_versions(&mut service, framework, Some(floor - 1), None, None)
        .await
        .unwrap_err();
    assert_below_earliest_available(&error, floor);

    // The floor itself, and an unbounded listing, are still served.
    for at_checkpoint in [Some(floor), None] {
        let response = list_versions(&mut service, framework, at_checkpoint, None, None)
            .await
            .unwrap();
        assert_eq!(
            listed(&response).first(),
            Some(&genesis_framework),
            "at_checkpoint {at_checkpoint:?}"
        );
    }

    // The listing of every package must hold every publish in its range, which it can promise
    // only from the lowest available checkpoint, one above the last pruned one: a store restored
    // from a snapshot records no publish below there. It fails for either bound below that floor,
    // and so does a cursor below it when `start_checkpoint` is unset, in either direction:
    // continuing from it would skip versions without saying so.
    let publish_floor = floor + 1;
    let mut below_floor = vec![];
    for (start_checkpoint, at_checkpoint) in [(Some(floor), None), (None, Some(floor))] {
        let mut request = ListPackageVersionsRequest::default();
        request.start_checkpoint = start_checkpoint;
        request.at_checkpoint = at_checkpoint;
        below_floor.push(request);
    }
    for (after, before) in [(Some(&genesis_cursor), None), (None, Some(&genesis_cursor))] {
        below_floor.push(all_packages_request(
            None,
            Ordering::Ascending,
            after,
            before,
        ));
    }
    for request in below_floor {
        let error = service.list_package_versions(request).await.unwrap_err();
        assert_below_earliest_available(&error, publish_floor);
    }

    // Otherwise the listing starts at that floor, past genesis, and with `start_checkpoint` at the
    // floor, the genesis cursor only bounds it. Every package was published at genesis, so the
    // listing is empty.
    let mut after_genesis =
        all_packages_request(None, Ordering::Ascending, Some(&genesis_cursor), None);
    after_genesis.start_checkpoint = Some(publish_floor);
    let mut from_floor = ListPackageVersionsRequest::default();
    from_floor.start_checkpoint = Some(publish_floor);
    for request in [
        after_genesis,
        from_floor,
        ListPackageVersionsRequest::default(),
    ] {
        let response = list_request(&mut service, request.clone()).await;
        assert_eq!(listed_publishes(&response), vec![], "{request:?}");
    }
}
