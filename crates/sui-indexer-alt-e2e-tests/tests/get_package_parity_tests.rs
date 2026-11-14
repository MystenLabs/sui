// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `MovePackageService` on a fullnode and on kv-rpc indexing the same chain.

use std::fmt::Debug;
use std::path::PathBuf;
use std::time::Duration;

use move_core_types::ident_str;
use prometheus::Registry;
use prost::Message;
use sui_indexer_alt_e2e_tests::OffchainCluster;
use sui_indexer_alt_e2e_tests::OffchainClusterConfig;
use sui_indexer_alt_framework::ingestion::ClientArgs;
use sui_indexer_alt_framework::ingestion::ingestion_client::IngestionClientArgs;
use sui_move_build::BuildConfig;
use sui_move_build::CompiledPackage;
use sui_rpc::field::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::google::rpc::ErrorInfo;
use sui_rpc::proto::sui::rpc::v2::GetCheckpointRequest;
use sui_rpc::proto::sui::rpc::v2::GetDatatypeRequest;
use sui_rpc::proto::sui::rpc::v2::GetDatatypeResponse;
use sui_rpc::proto::sui::rpc::v2::GetFunctionRequest;
use sui_rpc::proto::sui::rpc::v2::GetFunctionResponse;
use sui_rpc::proto::sui::rpc::v2::GetPackageRequest;
use sui_rpc::proto::sui::rpc::v2::GetPackageResponse;
use sui_rpc::proto::sui::rpc::v2::GetTransactionRequest;
use sui_rpc::proto::sui::rpc::v2::ListPackageVersionsRequest;
use sui_rpc::proto::sui::rpc::v2::ListPackageVersionsResponse;
use sui_rpc::proto::sui::rpc::v2::ListSystemPackagesRequest;
use sui_rpc::proto::sui::rpc::v2::ListSystemPackagesResponse;
use sui_rpc::proto::sui::rpc::v2::Ordering;
use sui_rpc::proto::sui::rpc::v2::PackageVersion;
use sui_rpc::proto::sui::rpc::v2::get_package_request::Selector;
use sui_rpc::proto::sui::rpc::v2::ledger_service_client::LedgerServiceClient;
use sui_rpc::proto::sui::rpc::v2::move_package_service_client::MovePackageServiceClient;
use sui_types::MOVE_STDLIB_PACKAGE_ID;
use sui_types::SUI_FRAMEWORK_PACKAGE_ID;
use sui_types::SUI_SYSTEM_PACKAGE_ID;
use sui_types::base_types::ObjectID;
use sui_types::base_types::ObjectRef;
use sui_types::base_types::SuiAddress;
use sui_types::digests::TransactionDigest;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::move_package::UpgradePolicy;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::ObjectArg;
use sui_types::transaction::TEST_ONLY_GAS_UNIT_FOR_PUBLISH;
use sui_types::transaction::TransactionData;
use tempfile::TempDir;
use test_cluster::TestCluster;
use test_cluster::TestClusterBuilder;
use tonic::transport::Channel;

/// The package the user lineage publishes and upgrades, republishing the same code each time.
fn package_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("packages/event/emit_test_event")
}

async fn latest_checkpoint(client: &mut LedgerServiceClient<Channel>) -> u64 {
    client
        .get_checkpoint(
            GetCheckpointRequest::latest()
                .with_read_mask(FieldMask::from_paths(["sequence_number"])),
        )
        .await
        .unwrap()
        .into_inner()
        .checkpoint
        .unwrap()
        .sequence_number
        .unwrap()
}

/// The checkpoint the fullnode includes transaction `digest` in, once it has one.
async fn checkpoint_of(
    client: &mut LedgerServiceClient<Channel>,
    digest: &TransactionDigest,
) -> u64 {
    tokio::time::timeout(Duration::from_secs(60), async {
        loop {
            let mut request = GetTransactionRequest::default();
            request.digest = Some(digest.to_string());
            request.read_mask = Some(FieldMask::from_paths(["checkpoint"]));
            if let Ok(response) = client.get_transaction(request).await
                && let Some(checkpoint) = response
                    .into_inner()
                    .transaction
                    .and_then(|transaction| transaction.checkpoint)
            {
                return checkpoint;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("transaction {digest} is checkpointed"))
}

/// Execute the publish or upgrade `builder` holds, and return the package it writes, its upgrade
/// cap, and the checkpoint it is in.
async fn write_package(
    cluster: &TestCluster,
    ledger: &mut LedgerServiceClient<Channel>,
    builder: ProgrammableTransactionBuilder,
) -> (ObjectID, ObjectRef, u64) {
    let sender = cluster.get_address_0();
    let gas_price = cluster.wallet.get_reference_gas_price().await.unwrap();
    let gas = cluster
        .wallet
        .get_one_gas_object_owned_by_address(sender)
        .await
        .unwrap()
        .unwrap();
    let data = TransactionData::new_programmable(
        sender,
        vec![gas],
        builder.finish(),
        TEST_ONLY_GAS_UNIT_FOR_PUBLISH * gas_price,
        gas_price,
    );
    let executed = cluster.sign_and_execute_transaction(&data).await;
    let package_id = executed.get_new_package_obj().expect("package written").0;
    let upgrade_cap = executed
        .get_new_package_upgrade_cap()
        .expect("upgrade cap written");
    let checkpoint = checkpoint_of(ledger, executed.effects.transaction_digest()).await;
    (package_id, upgrade_cap, checkpoint)
}

/// A PTB publishing `package` and handing its upgrade cap to `sender`.
fn publish_ptb(package: &CompiledPackage, sender: SuiAddress) -> ProgrammableTransactionBuilder {
    let mut builder = ProgrammableTransactionBuilder::new();
    let cap = builder.publish_upgradeable(
        package.get_package_bytes(false),
        package.get_dependency_storage_package_ids(),
    );
    builder.transfer_arg(sender, cap);
    builder
}

/// A PTB upgrading `package_id` by republishing `package`.
fn upgrade_ptb(
    package: &CompiledPackage,
    package_id: ObjectID,
    upgrade_cap: ObjectRef,
) -> ProgrammableTransactionBuilder {
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
    builder
}

/// What a refusal says that both servers must agree on: its code and its `ErrorInfo` reason.
/// Messages and `ErrorInfo` metadata are left out: the code and reason are the contract (decision
/// 9), the wording is each server's own, and metadata such as `firstUnavailableCheckpoint` names
/// the answering server's own tip.
fn refusal(status: &tonic::Status) -> (tonic::Code, Option<String>) {
    let reason = sui_rpc::proto::google::rpc::Status::decode(status.details())
        .ok()
        .and_then(|details| {
            details
                .details
                .iter()
                .find_map(|detail| detail.to_msg::<ErrorInfo>().ok())
        })
        .map(|info| info.reason);
    (status.code(), reason)
}

/// Check that the fullnode and kv-rpc gave the same answer to `request`, and return it.
fn same_answer<T: PartialEq + Debug>(
    request: &dyn Debug,
    fullnode: Result<tonic::Response<T>, tonic::Status>,
    kv_rpc: Result<tonic::Response<T>, tonic::Status>,
) -> Result<T, tonic::Status> {
    match (
        fullnode.map(tonic::Response::into_inner),
        kv_rpc.map(tonic::Response::into_inner),
    ) {
        (Ok(fullnode), Ok(kv_rpc)) => {
            assert_eq!(
                fullnode, kv_rpc,
                "{request:?}: fullnode (left) and kv-rpc (right) answer differently"
            );
            Ok(fullnode)
        }
        (Err(fullnode), Err(kv_rpc)) => {
            assert_eq!(
                refusal(&fullnode),
                refusal(&kv_rpc),
                "{request:?}: fullnode refused with {fullnode:?}, kv-rpc with {kv_rpc:?}"
            );
            Err(fullnode)
        }
        (fullnode, kv_rpc) => {
            panic!("{request:?}: fullnode answered {fullnode:?}, kv-rpc answered {kv_rpc:?}")
        }
    }
}

/// A `MovePackageService` client for each server. Every request goes to both, and every method
/// checks that they answer the same before returning the answer.
struct Servers {
    fullnode: MovePackageServiceClient<Channel>,
    kv_rpc: MovePackageServiceClient<Channel>,
}

impl Servers {
    async fn get_package(
        &mut self,
        package_id: ObjectID,
        selector: Option<Selector>,
    ) -> Result<GetPackageResponse, tonic::Status> {
        let mut request = GetPackageRequest::default();
        request.package_id = Some(package_id.to_string());
        request.selector = selector;
        same_answer(
            &request,
            self.fullnode.get_package(request.clone()).await,
            self.kv_rpc.get_package(request.clone()).await,
        )
    }

    async fn get_datatype(
        &mut self,
        package_id: ObjectID,
        module_name: &str,
        name: &str,
    ) -> Result<GetDatatypeResponse, tonic::Status> {
        let mut request = GetDatatypeRequest::default();
        request.package_id = Some(package_id.to_string());
        request.module_name = Some(module_name.to_owned());
        request.name = Some(name.to_owned());
        same_answer(
            &request,
            self.fullnode.get_datatype(request.clone()).await,
            self.kv_rpc.get_datatype(request.clone()).await,
        )
    }

    async fn get_function(
        &mut self,
        package_id: ObjectID,
        module_name: &str,
        name: &str,
    ) -> Result<GetFunctionResponse, tonic::Status> {
        let mut request = GetFunctionRequest::default();
        request.package_id = Some(package_id.to_string());
        request.module_name = Some(module_name.to_owned());
        request.name = Some(name.to_owned());
        same_answer(
            &request,
            self.fullnode.get_function(request.clone()).await,
            self.kv_rpc.get_function(request.clone()).await,
        )
    }

    async fn list_package_versions(
        &mut self,
        request: &ListPackageVersionsRequest,
    ) -> ListPackageVersionsResponse {
        same_answer(
            request,
            self.fullnode.list_package_versions(request.clone()).await,
            self.kv_rpc.list_package_versions(request.clone()).await,
        )
        .unwrap_or_else(|status| panic!("{request:?}: {status:?}"))
    }

    async fn list_system_packages(&mut self, at_checkpoint: u64) -> ListSystemPackagesResponse {
        let mut request = ListSystemPackagesRequest::default();
        request.at_checkpoint = Some(at_checkpoint);
        same_answer(
            &request,
            self.fullnode.list_system_packages(request).await,
            self.kv_rpc.list_system_packages(request).await,
        )
        .unwrap_or_else(|status| panic!("{request:?}: {status:?}"))
    }

    /// Page through `request` in page-token mode, `page_size` versions at a time, and return
    /// every version listed. Every page is asked of both servers, so each page token one server
    /// minted is also redeemed by the other.
    async fn walk_token_pages(
        &mut self,
        mut request: ListPackageVersionsRequest,
        page_size: u32,
    ) -> Vec<PackageVersion> {
        request.page_size = Some(page_size);
        let mut listed = vec![];
        for _ in 0..100 {
            let response = self.list_package_versions(&request).await;
            listed.extend(response.versions);
            match response.next_page_token {
                Some(token) => request.page_token = Some(token),
                None => return listed,
            }
        }
        panic!("{request:?} did not end within 100 pages");
    }

    /// Page through `request` in cursor mode, `limit` versions at a time, continuing each page
    /// from the last cursor of the page before: through `after` when ascending, `before` when
    /// descending. Returns every version listed. Every page is asked of both servers, so each
    /// cursor one server minted is also accepted by the other. Every page but the last holds
    /// `limit` versions (decision 11), so a short page ends the listing.
    async fn walk_cursor_pages(
        &mut self,
        mut request: ListPackageVersionsRequest,
        limit: u32,
    ) -> Vec<PackageVersion> {
        let options = request.options.get_or_insert_default();
        options.limit = Some(limit);
        let descending = options.ordering == Some(Ordering::Descending as i32);
        let mut listed = vec![];
        for _ in 0..100 {
            let response = self.list_package_versions(&request).await;
            let full = response.versions.len() == limit as usize;
            listed.extend(response.versions);
            if !full {
                return listed;
            }
            let cursor = listed
                .last()
                .and_then(|version| version.cursor.clone())
                .expect("listed version carries a cursor");
            let options = request.options.as_mut().unwrap();
            if descending {
                options.before = Some(cursor);
            } else {
                options.after = Some(cursor);
            }
        }
        panic!("{request:?} did not end within 100 pages");
    }
}

/// A `ListPackageVersions` request in cursor mode, listing `package_id`'s lineage or, without
/// it, every package.
fn cursor_request(
    package_id: Option<ObjectID>,
    ordering: Ordering,
    at_checkpoint: Option<u64>,
    start_checkpoint: Option<u64>,
) -> ListPackageVersionsRequest {
    let mut request = ListPackageVersionsRequest::default();
    request.package_id = package_id.map(|id| id.to_string());
    request.at_checkpoint = at_checkpoint;
    request.start_checkpoint = start_checkpoint;
    request.options.get_or_insert_default().ordering = Some(ordering as i32);
    request
}

/// A fullnode and kv-rpc serving the same chain answer every `MovePackageService` read the same:
/// a user package published and upgraded twice, and the system packages, by storage ID, by
/// version and at checkpoints around each publish; its lineage and every package's versions,
/// paged with page tokens and cursors in both orders, bounded by checkpoints; and the system
/// packages at several checkpoints.
///
/// The fullnode runs with pruning disabled, so both servers serve history from genesis and no
/// read meets a pruning floor. kv-rpc follows the fullnode's checkpoints and is caught up to the
/// fullnode's tip before the comparisons; no package is published after the upgrades, so reads
/// without a checkpoint bound answer the same on both whatever each server's own tip is.
#[tokio::test]
async fn test_move_package_service_matches_between_fullnode_and_kv_rpc() {
    let ingestion_dir = TempDir::new().unwrap();
    let onchain = TestClusterBuilder::new()
        .with_num_validators(1)
        .with_data_ingestion_dir(ingestion_dir.path().to_path_buf())
        .with_epoch_duration_ms(300_000)
        .disable_fullnode_pruning()
        .build()
        .await;
    onchain.wait_for_rpc_index_ready().await;
    let mut ledger = LedgerServiceClient::connect(onchain.rpc_url().to_owned())
        .await
        .unwrap();

    let offchain = OffchainCluster::new(
        ClientArgs {
            ingestion: IngestionClientArgs {
                local_ingestion_path: Some(ingestion_dir.path().to_path_buf()),
                ..Default::default()
            },
            ..Default::default()
        },
        OffchainClusterConfig::default(),
        &Registry::new(),
    )
    .await
    .unwrap();

    // A user package, upgraded twice.
    let package = BuildConfig::new_for_testing()
        .build(&package_path())
        .expect("build package");
    let sender = onchain.get_address_0();
    let (v1, cap, cp1) = write_package(&onchain, &mut ledger, publish_ptb(&package, sender)).await;
    let (v2, cap, cp2) = write_package(&onchain, &mut ledger, upgrade_ptb(&package, v1, cap)).await;
    let (v3, _, cp3) = write_package(&onchain, &mut ledger, upgrade_ptb(&package, v2, cap)).await;
    assert!(cp1 < cp2 && cp2 < cp3, "{cp1} {cp2} {cp3}");
    let lineage = [v1, v2, v3];
    let publishes = [(v1, cp1), (v2, cp2), (v3, cp3)];

    onchain.wait_for_rpc_index_ready().await;
    let tip = latest_checkpoint(&mut ledger).await;
    offchain
        .wait_for_kv_rpc(tip, Duration::from_secs(60))
        .await
        .unwrap_or_else(|_| panic!("kv-rpc serves checkpoint {tip}"));
    // The fullnode's indexes reach every checkpoint it has executed, so it serves `tip` as an
    // `at_checkpoint` too.
    onchain.wait_for_rpc_index_ready().await;

    let mut servers = Servers {
        fullnode: MovePackageServiceClient::connect(onchain.rpc_url().to_owned())
            .await
            .unwrap(),
        kv_rpc: MovePackageServiceClient::connect(offchain.kv_rpc_url().to_string())
            .await
            .unwrap(),
    };

    let system = [
        MOVE_STDLIB_PACKAGE_ID,
        SUI_FRAMEWORK_PACKAGE_ID,
        SUI_SYSTEM_PACKAGE_ID,
    ];
    let never_published = ObjectID::from_single_byte(0x99);
    let mut checkpoints = vec![0, cp1 - 1, cp1, cp2 - 1, cp2, cp3 - 1, cp3, tip];
    checkpoints.sort();
    checkpoints.dedup();

    // GetPackage by storage ID.
    for (version, id) in [(1, v1), (2, v2), (3, v3)] {
        let package = servers
            .get_package(id, None)
            .await
            .unwrap()
            .package
            .unwrap();
        assert_eq!(package.version, Some(version), "{id}");
    }
    for id in system {
        servers.get_package(id, None).await.unwrap();
    }
    let missing = servers
        .get_package(never_published, None)
        .await
        .unwrap_err();
    assert_eq!(missing.code(), tonic::Code::NotFound);

    // GetPackage by version, through every lineage member, past the lineage's last version, and
    // at a system package version that never existed.
    for id in lineage {
        for version in 1..=4 {
            let answer = servers
                .get_package(id, Some(Selector::Version(version)))
                .await;
            match answer {
                Ok(response) => {
                    assert!(version <= 3, "{id} version {version}");
                    assert_eq!(response.package.unwrap().version, Some(version));
                }
                Err(status) => {
                    assert_eq!(version, 4, "{id} version {version}: {status:?}");
                    assert_eq!(status.code(), tonic::Code::NotFound);
                }
            }
        }
    }
    for id in system {
        for version in [1, 2] {
            let _ = servers
                .get_package(id, Some(Selector::Version(version)))
                .await;
        }
    }

    // GetPackage at checkpoints before, at and between the publishes, and at the tip.
    for &checkpoint in &checkpoints {
        let expected = publishes
            .iter()
            .filter(|(_, published)| *published <= checkpoint)
            .count() as u64;
        for id in lineage {
            match servers
                .get_package(id, Some(Selector::AtCheckpoint(checkpoint)))
                .await
            {
                Ok(response) => {
                    assert_eq!(
                        response.package.unwrap().version,
                        Some(expected),
                        "{id} at {checkpoint}"
                    );
                }
                Err(status) => {
                    assert_eq!(expected, 0, "{id} at {checkpoint}: {status:?}");
                    assert_eq!(status.code(), tonic::Code::NotFound);
                }
            }
        }
        for id in system {
            let package = servers
                .get_package(id, Some(Selector::AtCheckpoint(checkpoint)))
                .await
                .unwrap()
                .package
                .unwrap();
            assert_eq!(package.version, Some(1), "{id} at {checkpoint}");
        }
    }

    // GetDatatype and GetFunction have no checkpoint selector: both resolve a storage ID at the
    // server's own height, which is above every publish on both. Present and missing modules,
    // datatypes and functions, on every lineage member and on the system packages.
    for id in lineage {
        for (module, name) in [
            ("emit_test_event", "TestEvent"),
            ("emit_test_event", "TestObject"),
            ("emit_test_event", "Missing"),
            ("missing", "TestEvent"),
        ] {
            let _ = servers.get_datatype(id, module, name).await;
        }
        for (module, name) in [
            ("emit_test_event", "emit_with_value"),
            ("emit_test_event", "emit_and_create"),
            ("emit_test_event", "missing"),
            ("missing", "emit_with_value"),
        ] {
            let _ = servers.get_function(id, module, name).await;
        }
        servers
            .get_datatype(id, "emit_test_event", "TestEvent")
            .await
            .unwrap();
        servers
            .get_function(id, "emit_test_event", "emit_with_value")
            .await
            .unwrap();
    }
    for (id, module, datatype, function) in [
        (MOVE_STDLIB_PACKAGE_ID, "option", "Option", "is_some"),
        (SUI_FRAMEWORK_PACKAGE_ID, "coin", "Coin", "value"),
        (SUI_SYSTEM_PACKAGE_ID, "staking_pool", "StakedSui", "split"),
    ] {
        servers.get_datatype(id, module, datatype).await.unwrap();
        servers.get_function(id, module, function).await.unwrap();
    }
    let missing = servers
        .get_datatype(never_published, "emit_test_event", "TestEvent")
        .await
        .unwrap_err();
    assert_eq!(missing.code(), tonic::Code::NotFound);
    let missing = servers
        .get_function(never_published, "emit_test_event", "emit_with_value")
        .await
        .unwrap_err();
    assert_eq!(missing.code(), tonic::Code::NotFound);

    // ListPackageVersions for one lineage, in page-token mode: unbounded and at every
    // checkpoint, through every member, whole and one or two versions per page.
    let mut at_checkpoints = vec![None];
    at_checkpoints.extend(checkpoints.iter().copied().map(Some));
    for id in lineage {
        for &at_checkpoint in &at_checkpoints {
            let mut request = ListPackageVersionsRequest::default();
            request.package_id = Some(id.to_string());
            request.at_checkpoint = at_checkpoint;
            let whole = servers.list_package_versions(&request).await;
            let expected = publishes
                .iter()
                .filter(|(_, published)| at_checkpoint.is_none_or(|c| *published <= c))
                .count();
            assert_eq!(whole.versions.len(), expected, "{request:?}");
            assert!(whole.next_page_token.is_none());
            for page_size in [1, 2] {
                assert_eq!(
                    servers.walk_token_pages(request.clone(), page_size).await,
                    whole.versions,
                    "{request:?} by {page_size}"
                );
            }
        }
    }

    // The same lineage in cursor mode, ascending and descending, one or two versions per page,
    // and with a version bound.
    for id in [v1, v3] {
        for ordering in [Ordering::Ascending, Ordering::Descending] {
            for &at_checkpoint in &at_checkpoints {
                let request = cursor_request(Some(id), ordering, at_checkpoint, None);
                let whole = servers.list_package_versions(&request).await;
                for limit in [1, 2] {
                    assert_eq!(
                        servers.walk_cursor_pages(request.clone(), limit).await,
                        whole.versions,
                        "{request:?} by {limit}"
                    );
                }
                let mut from_version_2 = request.clone();
                from_version_2.start_version = Some(2);
                servers.walk_cursor_pages(from_version_2, 1).await;
            }
        }
    }

    // A system package's lineage.
    for ordering in [Ordering::Ascending, Ordering::Descending] {
        let request = cursor_request(Some(SUI_FRAMEWORK_PACKAGE_ID), ordering, Some(cp3), None);
        let listed = servers.walk_cursor_pages(request, 1).await;
        assert_eq!(listed.len(), 1);
    }

    // ListPackageVersions for every package: unbounded, bounded by `start_checkpoint` and
    // `at_checkpoint` around the publishes, ascending and descending, whole and two per page.
    let bounds = [
        (None, None),
        (None, Some(0)),
        (None, Some(cp1)),
        (None, Some(cp2)),
        (Some(cp1), None),
        (Some(cp1), Some(cp2)),
        (Some(cp2), Some(cp2)),
        (Some(cp1 + 1), Some(cp3)),
        (Some(cp3), Some(tip)),
    ];
    for ordering in [Ordering::Ascending, Ordering::Descending] {
        for (start_checkpoint, at_checkpoint) in bounds {
            let request = cursor_request(None, ordering, at_checkpoint, start_checkpoint);
            let whole = servers.list_package_versions(&request).await;
            assert_eq!(
                servers.walk_cursor_pages(request.clone(), 2).await,
                whole.versions,
                "{request:?} by 2"
            );
        }
    }
    let everything = servers
        .list_package_versions(&cursor_request(None, Ordering::Ascending, None, None))
        .await;
    let user_publishes: Vec<_> = everything
        .versions
        .iter()
        .filter(|version| version.checkpoint.is_some_and(|checkpoint| checkpoint > 0))
        .map(|version| (version.package_id.clone().unwrap(), version.checkpoint))
        .collect();
    assert_eq!(
        user_publishes,
        publishes
            .iter()
            .map(|(id, checkpoint)| (id.to_string(), Some(*checkpoint)))
            .collect::<Vec<_>>(),
    );

    // ListSystemPackages at genesis, around the publishes, and at the tip.
    for &checkpoint in &checkpoints {
        let response = servers.list_system_packages(checkpoint).await;
        for id in system {
            assert!(
                response
                    .system_packages
                    .iter()
                    .any(|package| package.package_id == Some(id.to_string())),
                "{id} at {checkpoint}: {response:?}"
            );
        }
    }
}
