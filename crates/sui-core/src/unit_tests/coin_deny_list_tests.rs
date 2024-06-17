// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::move_integration_tests::build_and_try_publish_test_package;
use crate::authority::test_authority_builder::TestAuthorityBuilder;
use sui_types::crypto::get_account_key_pair;
use sui_types::deny_list_v1::{CoinDenyCap, RegulatedCoinMetadata};
use sui_types::effects::TransactionEffectsAPI;
use sui_types::object::Object;
use sui_types::transaction::TEST_ONLY_GAS_UNIT_FOR_PUBLISH;

// Test that a regulated coin can be created and all the necessary objects are created with the right types.
// Make sure that these types can be converted to Rust types.
#[tokio::test]
async fn test_regulated_coin_v1_creation() {
    let (sender, keypair) = get_account_key_pair();
    let gas_object = Object::with_owner_for_testing(sender);
    let gas_object_id = gas_object.id();
    let authority = TestAuthorityBuilder::new()
        .with_starting_objects(&[gas_object])
        .build()
        .await;
    let rgp = authority.reference_gas_price_for_testing().unwrap();
    let (_, effects) = build_and_try_publish_test_package(
        &authority,
        &sender,
        &keypair,
        &gas_object_id,
        "coin_deny_list_v1",
        TEST_ONLY_GAS_UNIT_FOR_PUBLISH * rgp,
        rgp,
        false,
    )
    .await;

    let mut deny_cap_object = None;
    let mut metadata_object = None;
    let mut regulated_metadata_object = None;
    for (oref, _owner) in effects.created() {
        let object = authority.get_object(&oref.0).await.unwrap().unwrap();
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
