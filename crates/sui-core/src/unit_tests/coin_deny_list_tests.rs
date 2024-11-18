// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_tests::send_and_confirm_transaction_;
use crate::authority::move_integration_tests::build_and_try_publish_test_package;
use crate::authority::test_authority_builder::TestAuthorityBuilder;
use crate::authority::AuthorityState;
use move_core_types::ident_str;
use move_core_types::language_storage::{StructTag, TypeTag};
use std::sync::Arc;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::{dbg_addr, ObjectID, ObjectRef, SuiAddress};
use sui_types::crypto::{get_account_key_pair, AccountKeyPair};
use sui_types::deny_list_v1::{CoinDenyCap, RegulatedCoinMetadata};
use sui_types::deny_list_v2::{
    check_address_denied_by_config, check_global_pause, get_per_type_coin_deny_list_v2, DenyCapV2,
};
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};
use sui_types::object::Object;
use sui_types::transaction::{CallArg, ObjectArg, TEST_ONLY_GAS_UNIT_FOR_PUBLISH};
use sui_types::{SUI_DENY_LIST_OBJECT_ID, SUI_FRAMEWORK_PACKAGE_ID};

// Test that a regulated coin can be created and all the necessary objects are created with the right types.
// Make sure that these types can be converted to Rust types.
#[tokio::test]
async fn test_regulated_coin_v1_creation() {
    let env = new_authority_and_publish("coin_deny_list_v1").await;

    let mut deny_cap_object = None;
    let mut metadata_object = None;
    let mut regulated_metadata_object = None;
    for (oref, _owner) in env.publish_effects.created() {
        let object = env.authority.get_object(&oref.0).await.unwrap();
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

// Test that a v2 regulated coin can be created and all the necessary objects are created with the right types.
// Also test that we could create the deny list config for the coin and all types can be loaded in Rust.
#[tokio::test]
async fn test_regulated_coin_v2_types() {
    let env = new_authority_and_publish("coin_deny_list_v2").await;

    // Step 1: Publish the regulated coin and check basic types.
    let mut deny_cap_object = None;
    let mut metadata_object = None;
    let mut regulated_metadata_object = None;
    let mut package_id = None;
    for (oref, _owner) in env.publish_effects.created() {
        let object = env.authority.get_object(&oref.0).await.unwrap();
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

    // Step 2: Deny an address and check the denylist types.
    let deny_list_object_init_version = env.get_latest_object_ref(&SUI_DENY_LIST_OBJECT_ID).await.1;
    let regulated_coin_type = TypeTag::Struct(Box::new(StructTag {
        address: package_id.into(),
        module: ident_str!("regulated_coin").to_owned(),
        name: ident_str!("REGULATED_COIN").to_owned(),
        type_params: vec![],
    }));
    let deny_address = dbg_addr(2);
    let tx = TestTransactionBuilder::new(
        env.sender,
        env.get_latest_object_ref(&env.gas_object_id).await,
        env.authority.reference_gas_price_for_testing().unwrap(),
    )
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
            CallArg::Pure(bcs::to_bytes(&deny_address).unwrap()),
        ],
    )
    .with_type_args(vec![regulated_coin_type.clone()])
    .build_and_sign(&env.keypair);
    let (_, effects) = send_and_confirm_transaction_(&env.authority, None, tx, true)
        .await
        .unwrap();
    if effects.status().is_err() {
        panic!("Failed to add address to deny list: {:?}", effects.status());
    }
    let coin_deny_config = get_per_type_coin_deny_list_v2(
        &regulated_coin_type.to_canonical_string(false),
        &env.authority.get_object_store(),
    )
    .unwrap();
    // Updates from the current epoch will not be read.
    assert!(!check_address_denied_by_config(
        &coin_deny_config,
        deny_address,
        &env.authority.get_object_store(),
        Some(0),
    ));
    // If no epoch is specified, we always read the latest value, and it should be denied.
    assert!(check_address_denied_by_config(
        &coin_deny_config,
        deny_address,
        &env.authority.get_object_store(),
        None,
    ));
    // If no epoch is specified, we always read the latest value, and it should be denied.
    assert!(check_address_denied_by_config(
        &coin_deny_config,
        deny_address,
        &env.authority.get_object_store(),
        None,
    ));

    // If we change the current epoch to be 1, the change from epoch 0
    // would be considered as from previous epoch, and hence will be
    // used.
    assert!(check_address_denied_by_config(
        &coin_deny_config,
        deny_address,
        &env.authority.get_object_store(),
        Some(1),
    ));
    // Check a different address, and it should not be denied.
    assert!(!check_address_denied_by_config(
        &coin_deny_config,
        dbg_addr(3),
        &env.authority.get_object_store(),
        Some(1),
    ));

    // Step 3: Enable global pause and check the global pause types.
    let tx = TestTransactionBuilder::new(
        env.sender,
        env.get_latest_object_ref(&env.gas_object_id).await,
        env.authority.reference_gas_price_for_testing().unwrap(),
    )
    .move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        "coin",
        "deny_list_v2_enable_global_pause",
        vec![
            CallArg::Object(ObjectArg::SharedObject {
                id: SUI_DENY_LIST_OBJECT_ID,
                initial_shared_version: deny_list_object_init_version,
                mutable: true,
            }),
            CallArg::Object(ObjectArg::ImmOrOwnedObject(
                env.get_latest_object_ref(&deny_cap_object.id()).await,
            )),
        ],
    )
    .with_type_args(vec![regulated_coin_type.clone()])
    .build_and_sign(&env.keypair);
    let (_, effects) = send_and_confirm_transaction_(&env.authority, None, tx, true)
        .await
        .unwrap();
    if effects.status().is_err() {
        panic!("Failed to enable global pause: {:?}", effects.status());
    }
    println!("Effects: {:?}", effects);
    assert!(check_global_pause(
        &coin_deny_config,
        &env.authority.get_object_store(),
        None,
    ));
    assert!(!check_global_pause(
        &coin_deny_config,
        &env.authority.get_object_store(),
        Some(0),
    ));
    assert!(check_global_pause(
        &coin_deny_config,
        &env.authority.get_object_store(),
        Some(1),
    ));
}

struct TestEnv {
    authority: Arc<AuthorityState>,
    sender: SuiAddress,
    keypair: AccountKeyPair,
    gas_object_id: ObjectID,
    publish_effects: TransactionEffects,
}

impl TestEnv {
    async fn get_latest_object_ref(&self, id: &ObjectID) -> ObjectRef {
        self.authority
            .get_object(id)
            .await
            .unwrap()
            .compute_object_reference()
    }
}

async fn new_authority_and_publish(path: &str) -> TestEnv {
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
        path,
        TEST_ONLY_GAS_UNIT_FOR_PUBLISH * rgp,
        rgp,
        false,
    )
    .await;
    TestEnv {
        authority,
        sender,
        keypair,
        gas_object_id,
        publish_effects: effects.into_data(),
    }
}
