// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Mirrors
//! `sui-e2e-tests/tests/rpc/v2/move_package_service/list_package_versions.rs`.
//! `test_list_package_versions_with_upgrades` publishes and
//! upgrades the on-disk `move_test_code` package; we reuse the
//! e2e crate's copy of the sources rather than duplicating them.

use std::path::PathBuf;

use move_core_types::ident_str;
use sui_move_build::BuildConfig;
use sui_rpc::proto::sui::rpc::v2::ListPackageVersionsRequest;
use sui_rpc::proto::sui::rpc::v2::move_package_service_client::MovePackageServiceClient;
use sui_types::SUI_FRAMEWORK_PACKAGE_ID;
use sui_types::base_types::ObjectID;
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::move_package::UpgradePolicy;
use sui_types::object::Owner;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::ObjectArg;
use sui_types::transaction::TransactionData;
use sui_types::utils::to_sender_signed_transaction;
use tonic::transport::Channel;

use crate::cluster::LocalCluster;

async fn service(cluster: &LocalCluster) -> MovePackageServiceClient<Channel> {
    MovePackageServiceClient::connect(cluster.grpc_url().to_string())
        .await
        .unwrap()
}

fn move_test_code_path() -> PathBuf {
    // The Move source tree lives in `sui-e2e-tests`; reusing it
    // here avoids duplicating Move sources that are already
    // version-controlled there.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("sui-e2e-tests")
        .join("tests")
        .join("move_test_code")
}

#[tokio::test]
async fn list_package_versions_system_package() {
    let cluster = LocalCluster::new().await.unwrap();
    let mut svc = service(&cluster).await;

    let mut request = ListPackageVersionsRequest::default();
    request.package_id = Some("0x2".to_string());

    let response = svc
        .list_package_versions(request)
        .await
        .unwrap()
        .into_inner();

    assert_eq!(response.versions.len(), 1);
    let version = &response.versions[0];
    assert_eq!(
        version.package_id,
        Some("0x0000000000000000000000000000000000000000000000000000000000000002".to_string()),
    );
    assert_eq!(version.version, Some(1));
}

#[tokio::test]
async fn list_package_versions_not_found() {
    let cluster = LocalCluster::new().await.unwrap();
    let mut svc = service(&cluster).await;

    let mut request = ListPackageVersionsRequest::default();
    request.package_id =
        Some("0x0000000000000000000000000000000000000000000000000000000000000999".to_string());

    let err = svc.list_package_versions(request).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::NotFound);
}

#[tokio::test]
async fn list_package_versions_invalid_package_id() {
    let cluster = LocalCluster::new().await.unwrap();
    let mut svc = service(&cluster).await;

    let mut request = ListPackageVersionsRequest::default();
    request.package_id = Some("invalid-package-id".to_string());

    let err = svc.list_package_versions(request).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert!(
        err.message().contains("invalid package_id"),
        "unexpected message: {}",
        err.message(),
    );
}

#[tokio::test]
async fn list_package_versions_missing_package_id() {
    let cluster = LocalCluster::new().await.unwrap();
    let mut svc = service(&cluster).await;

    let err = svc
        .list_package_versions(ListPackageVersionsRequest::default())
        .await
        .unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert!(
        err.message().contains("missing package_id"),
        "unexpected message: {}",
        err.message(),
    );
}

#[tokio::test]
async fn list_package_versions_invalid_pagination() {
    let cluster = LocalCluster::new().await.unwrap();
    let mut svc = service(&cluster).await;

    // Invalid BCS in the page token.
    let mut request = ListPackageVersionsRequest::default();
    request.package_id = Some("0x2".to_string());
    request.page_size = Some(10);
    request.page_token = Some(vec![0xFF, 0xFF, 0xFF].into());

    let err = svc.list_package_versions(request).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert!(
        err.message().contains("invalid page token encoding"),
        "unexpected message: {}",
        err.message(),
    );

    // Valid-shape page token, but for a different package.
    #[derive(serde::Serialize)]
    struct PageToken {
        original_package_id: ObjectID,
        version: u64,
    }
    let different_package_id = ObjectID::from_hex_literal(
        "0x0000000000000000000000000000000000000000000000000000000000000999",
    )
    .unwrap();
    let encoded = bcs::to_bytes(&PageToken {
        original_package_id: different_package_id,
        version: 1,
    })
    .unwrap();

    let mut request = ListPackageVersionsRequest::default();
    request.package_id = Some("0x2".to_string());
    request.page_size = Some(10);
    request.page_token = Some(encoded.into());

    let err = svc.list_package_versions(request).await.unwrap_err();
    assert_eq!(err.code(), tonic::Code::InvalidArgument);
    assert!(
        err.message()
            .contains("page token package ID does not match request package ID"),
        "unexpected message: {}",
        err.message(),
    );
}

#[tokio::test]
async fn list_package_versions_with_upgrades() {
    let cluster = LocalCluster::new().await.unwrap();
    let mut svc = service(&cluster).await;

    // One funded account drives the whole publish-then-upgrade
    // sequence. The amount needs to cover three publishes / two
    // upgrades at typical Simulacrum gas rates.
    let (address, keypair, gas) = cluster.funded_account(1_000_000_000_000).await.unwrap();
    let rgp = cluster.reference_gas_price().await;

    let compiled = BuildConfig::new_for_testing()
        .build_async(&move_test_code_path())
        .await
        .unwrap();
    let modules = compiled.get_package_bytes(false);
    let dependencies = compiled.get_dependency_storage_package_ids();
    let digest = compiled.get_package_digest(false).to_vec();

    // Initial publish — keep the UpgradeCap so we can run upgrades
    // against it. Mirrors the e2e helper's `publish_upgradeable`
    // pattern.
    let mut builder = ProgrammableTransactionBuilder::new();
    let cap_arg = builder.publish_upgradeable(modules.clone(), dependencies.clone());
    builder.transfer_arg(address, cap_arg);
    let pt = builder.finish();
    let tx_data = TransactionData::new_programmable(
        address,
        vec![gas],
        pt,
        // 200M MIST: comfortably covers publish + upgrade gas.
        200_000_000,
        rgp,
    );
    let signed = to_sender_signed_transaction(tx_data, &keypair);
    let (effects, err) = cluster.execute_transaction(signed).await.unwrap();
    assert!(err.is_none(), "publish must succeed: {err:?}");

    let mut package_ids = vec![package_id_from_effects(&effects, &cluster).await];
    let mut upgrade_cap = upgrade_cap_from_effects(&cluster, &effects).await;
    let mut gas_ref = effects.gas_object().expect("publish must have gas").0;
    cluster.create_checkpoint().await.unwrap();

    // First page-1 snapshot.
    let mut request = ListPackageVersionsRequest::default();
    request.package_id = Some(package_ids[0].to_string());
    let response = svc
        .list_package_versions(request.clone())
        .await
        .unwrap()
        .into_inner();
    assert_eq!(response.versions.len(), 1);
    assert_eq!(response.versions[0].version, Some(1));
    assert_eq!(
        response.versions[0].package_id,
        Some(package_ids[0].to_string()),
    );

    // Two consecutive upgrades, each publishing the same bytes
    // again — the on-chain version still bumps even when the
    // module bodies are identical.
    for _ in 0..2 {
        let prev_package_id = *package_ids.last().unwrap();
        let (new_effects, new_upgrade_cap, new_gas) = upgrade_package(
            &cluster,
            address,
            &keypair,
            gas_ref,
            upgrade_cap,
            prev_package_id,
            modules.clone(),
            dependencies.clone(),
            digest.clone(),
            rgp,
        )
        .await;
        package_ids.push(package_id_from_effects(&new_effects, &cluster).await);
        upgrade_cap = new_upgrade_cap;
        gas_ref = new_gas;
        cluster.create_checkpoint().await.unwrap();
    }

    let response = svc
        .list_package_versions(request.clone())
        .await
        .unwrap()
        .into_inner();
    assert_eq!(response.versions.len(), 3);
    for (i, version) in response.versions.iter().enumerate() {
        assert_eq!(version.version, Some((i + 1) as u64));
        assert_eq!(version.package_id, Some(package_ids[i].to_string()));
    }
    assert_ne!(package_ids[0], package_ids[1]);
    assert_ne!(package_ids[1], package_ids[2]);
    assert_ne!(package_ids[0], package_ids[2]);

    // First page with page_size = 2.
    let mut paged = ListPackageVersionsRequest::default();
    paged.package_id = Some(package_ids[0].to_string());
    paged.page_size = Some(2);
    let first = svc.list_package_versions(paged).await.unwrap().into_inner();
    assert_eq!(first.versions.len(), 2);
    assert_eq!(first.versions[0].version, Some(1));
    assert_eq!(first.versions[1].version, Some(2));
    let next_token = first
        .next_page_token
        .expect("first page should carry a continuation token");

    // Second page consumes the cursor.
    let mut next = ListPackageVersionsRequest::default();
    next.package_id = Some(package_ids[0].to_string());
    next.page_size = Some(2);
    next.page_token = Some(next_token);
    let second = svc.list_package_versions(next).await.unwrap().into_inner();
    assert_eq!(second.versions.len(), 1);
    assert_eq!(second.versions[0].version, Some(3));
    assert!(second.next_page_token.is_none());
}

async fn package_id_from_effects(effects: &TransactionEffects, cluster: &LocalCluster) -> ObjectID {
    // The publish creates the package as `Immutable`, but
    // `move_test_code`'s init functions also freeze a
    // CoinMetadata that's also Immutable. Distinguish by
    // `Object::is_package` — true only for Move package objects.
    for (oref, owner) in effects.created() {
        if !matches!(owner, Owner::Immutable) {
            continue;
        }
        let Some(obj) = cluster.get_object(oref.0).await else {
            continue;
        };
        if obj.is_package() {
            return oref.0;
        }
    }
    panic!("publish effects should contain an immutable package object");
}

async fn upgrade_cap_from_effects(
    cluster: &LocalCluster,
    effects: &TransactionEffects,
) -> sui_types::base_types::ObjectRef {
    // `move_test_code`'s `init` functions also create owned
    // objects (e.g. a regulated coin's TreasuryCap), so we can't
    // just pick the first AddressOwner. Look it up by type
    // instead.
    for (oref, owner) in effects.created() {
        if !matches!(owner, Owner::AddressOwner(_)) {
            continue;
        }
        let Some(obj) = cluster.get_object(oref.0).await else {
            continue;
        };
        if obj.type_().map(|t| t.is_upgrade_cap()).unwrap_or(false) {
            return oref;
        }
    }
    panic!("publish effects should expose an owned UpgradeCap");
}

#[allow(clippy::too_many_arguments)]
async fn upgrade_package(
    cluster: &LocalCluster,
    address: sui_types::base_types::SuiAddress,
    keypair: &sui_types::crypto::AccountKeyPair,
    gas: sui_types::base_types::ObjectRef,
    upgrade_cap: sui_types::base_types::ObjectRef,
    package_id: ObjectID,
    modules: Vec<Vec<u8>>,
    dependencies: Vec<ObjectID>,
    digest: Vec<u8>,
    rgp: u64,
) -> (
    TransactionEffects,
    sui_types::base_types::ObjectRef,
    sui_types::base_types::ObjectRef,
) {
    let mut builder = ProgrammableTransactionBuilder::new();
    let cap = builder
        .obj(ObjectArg::ImmOrOwnedObject(upgrade_cap))
        .unwrap();
    let policy = builder.pure(UpgradePolicy::COMPATIBLE).unwrap();
    let digest_arg = builder.pure(digest).unwrap();
    let ticket = builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        ident_str!("package").to_owned(),
        ident_str!("authorize_upgrade").to_owned(),
        vec![],
        vec![cap, policy, digest_arg],
    );
    let receipt = builder.upgrade(package_id, ticket, dependencies, modules);
    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        ident_str!("package").to_owned(),
        ident_str!("commit_upgrade").to_owned(),
        vec![],
        vec![cap, receipt],
    );
    let pt = builder.finish();
    let tx_data = TransactionData::new_programmable(address, vec![gas], pt, 200_000_000, rgp);
    let signed = to_sender_signed_transaction(tx_data, keypair);
    let (effects, err) = cluster.execute_transaction(signed).await.unwrap();
    assert!(err.is_none(), "upgrade must succeed: {err:?}");

    // The UpgradeCap moves on every upgrade (its version bumps).
    let new_upgrade_cap = effects
        .mutated()
        .into_iter()
        .find(|((id, _, _), _)| id == &upgrade_cap.0)
        .map(|(oref, _)| oref)
        .expect("upgrade effects should mutate the UpgradeCap");
    let new_gas = effects.gas_object().expect("upgrade must have gas").0;
    (effects, new_upgrade_cap, new_gas)
}
