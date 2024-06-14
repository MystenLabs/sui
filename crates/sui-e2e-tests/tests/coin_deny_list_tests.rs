// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::ident_str;
use move_core_types::language_storage::{StructTag, TypeTag};
use std::path::PathBuf;
use sui_json_rpc_types::SuiTransactionBlockEffectsAPI;
use sui_macros::sim_test;
use sui_types::base_types::{dbg_addr, SuiAddress};
use sui_types::deny_list_v1::CoinDenyCap;
use sui_types::deny_list_v1::RegulatedCoinMetadata;
use sui_types::deny_list_v2::DenyCapV2;
use sui_types::transaction::{CallArg, ObjectArg};
use sui_types::{SUI_DENY_LIST_OBJECT_ID, SUI_FRAMEWORK_PACKAGE_ID};
use test_cluster::TestClusterBuilder;

#[sim_test]
async fn test_regulated_coin_v1_creation() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/move_test_code");
    let tx_data = test_cluster
        .test_transaction_builder()
        .await
        .publish(path)
        .build();
    let effects = test_cluster
        .sign_and_execute_transaction(&tx_data)
        .await
        .effects
        .unwrap();
    let mut deny_cap_object = None;
    let mut metadata_object = None;
    let mut regulated_metadata_object = None;
    for created in effects.created() {
        let object = test_cluster
            .get_object_from_fullnode_store(&created.object_id())
            .await
            .unwrap();
        if object.is_package() {
            continue;
        }
        let t = object.type_().unwrap();
        if t.is_coin_deny_cap() {
            assert!(deny_cap_object.is_none());
            deny_cap_object = Some(object);
        } else if t.is_regulated_coin_metadata() {
            assert!(regulated_metadata_object.is_none());
            regulated_metadata_object = Some(object);
        } else if t.is_coin_metadata() {
            assert!(metadata_object.is_none());
            metadata_object = Some(object);
        }
    }
    // Check that publishing the package created
    // the metadata, deny cap, and regulated metadata.
    // Check that all their fields are consistent.
    let metadata_object = metadata_object.unwrap();
    let deny_cap_object = deny_cap_object.unwrap();
    let deny_cap: CoinDenyCap = deny_cap_object.to_rust().unwrap();
    assert_eq!(deny_cap.id.id.bytes, deny_cap_object.id());

    let regulated_metadata_object = regulated_metadata_object.unwrap();
    let regulated_metadata: RegulatedCoinMetadata = regulated_metadata_object.to_rust().unwrap();
    assert_eq!(
        regulated_metadata.id.id.bytes,
        regulated_metadata_object.id()
    );
    assert_eq!(
        regulated_metadata.deny_cap_object.bytes,
        deny_cap_object.id()
    );
    assert_eq!(
        regulated_metadata.coin_metadata_object.bytes,
        metadata_object.id()
    );
}

#[sim_test]
async fn test_regulated_coin_v2_types() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let deny_list_object_init_version = test_cluster
        .get_latest_object_ref(&SUI_DENY_LIST_OBJECT_ID)
        .await
        .1;
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/regulated_coin_v2");
    let tx_data = test_cluster
        .test_transaction_builder()
        .await
        .publish(path)
        .build();
    let effects = test_cluster
        .sign_and_execute_transaction(&tx_data)
        .await
        .effects
        .unwrap();
    let mut deny_cap_object = None;
    let mut metadata_object = None;
    let mut regulated_metadata_object = None;
    let mut package_id = None;
    for created in effects.created() {
        let object = test_cluster
            .get_object_from_fullnode_store(&created.object_id())
            .await
            .unwrap();
        if object.is_package() {
            package_id = Some(object.id());
            continue;
        }
        let t = object.type_().unwrap();
        if t.is_coin_deny_cap_v2() {
            assert!(deny_cap_object.is_none());
            deny_cap_object = Some(object);
        } else if t.is_regulated_coin_metadata() {
            assert!(regulated_metadata_object.is_none());
            regulated_metadata_object = Some(object);
        } else if t.is_coin_metadata() {
            assert!(metadata_object.is_none());
            metadata_object = Some(object);
        }
    }
    let package_id = package_id.unwrap();
    // Check that publishing the package created
    // the metadata, deny cap, and regulated metadata.
    // Check that all their fields are consistent.
    let metadata_object = metadata_object.unwrap();
    let deny_cap_object = deny_cap_object.unwrap();
    let deny_cap: DenyCapV2 = deny_cap_object.to_rust().unwrap();
    assert_eq!(deny_cap.id.id.bytes, deny_cap_object.id());
    assert!(deny_cap.allow_global_pause);

    let regulated_metadata_object = regulated_metadata_object.unwrap();
    let regulated_metadata: RegulatedCoinMetadata = regulated_metadata_object.to_rust().unwrap();
    assert_eq!(
        regulated_metadata.id.id.bytes,
        regulated_metadata_object.id()
    );
    assert_eq!(
        regulated_metadata.deny_cap_object.bytes,
        deny_cap_object.id()
    );
    assert_eq!(
        regulated_metadata.coin_metadata_object.bytes,
        metadata_object.id()
    );

    let tx_data = test_cluster
        .test_transaction_builder()
        .await
        .move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            "coin",
            "deny_list_v2_add",
            vec![
                CallArg::Object(ObjectArg::SharedObject {
                    id: SUI_DENY_LIST_OBJECT_ID,
                    initial_shared_version: deny_list_object_init_version,
                    mutable: true,
                }),
                CallArg::Object(ObjectArg::ImmOrOwnedObject(
                    deny_cap_object.compute_object_reference(),
                )),
                CallArg::Pure(bcs::to_bytes(&dbg_addr(2)).unwrap()),
            ],
        )
        .with_type_args(vec![TypeTag::Struct(Box::new(StructTag {
            address: package_id.into(),
            module: ident_str!("regulated_coin").to_owned(),
            name: ident_str!("REGULATED_COIN").to_owned(),
            type_params: vec![],
        }))])
        .build();
    test_cluster
        .sign_and_execute_transaction(&tx_data)
        .await
        .effects
        .unwrap();
}
