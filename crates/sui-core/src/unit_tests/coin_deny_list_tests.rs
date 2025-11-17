// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::AuthorityState;
use crate::authority::authority_tests::send_and_confirm_transaction_;
use crate::authority::move_integration_tests::build_and_try_publish_test_package;
use crate::authority::test_authority_builder::TestAuthorityBuilder;
use move_core_types::ident_str;
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::{StructTag, TypeTag};
use std::sync::Arc;
use sui_protocol_config::{Chain, PerObjectCongestionControlMode, ProtocolConfig, ProtocolVersion};
use sui_test_transaction_builder::{FundSource, TestTransactionBuilder};
use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress, dbg_addr};
use sui_types::crypto::{AccountKeyPair, get_account_key_pair};
use sui_types::deny_list_v1::{CoinDenyCap, RegulatedCoinMetadata};
use sui_types::deny_list_v2::{
    DenyCapV2, check_address_denied_by_config, check_global_pause, get_per_type_coin_deny_list_v2,
};
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};
use sui_types::error::{SuiErrorKind, UserInputError};
use sui_types::object::Object;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{
    CallArg, FundsWithdrawalArg, ObjectArg, SharedObjectMutability, TEST_ONLY_GAS_UNIT_FOR_PUBLISH,
    Transaction, TransactionData,
};
use sui_types::type_input::TypeInput;
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

    // Step 1: Check basic regulated coin v2 types.
    let metadata = env.extract_v2_metadata().await;

    // Step 2: Deny an address and check the denylist types.
    let deny_list_object_init_version = env.get_latest_object_ref(&SUI_DENY_LIST_OBJECT_ID).await.1;
    let regulated_coin_type = metadata.regulated_coin_type();
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
                mutability: SharedObjectMutability::Mutable,
            }),
            CallArg::Object(ObjectArg::ImmOrOwnedObject(
                env.get_latest_object_ref(&metadata.deny_cap_id).await,
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
                mutability: SharedObjectMutability::Mutable,
            }),
            CallArg::Object(ObjectArg::ImmOrOwnedObject(
                env.get_latest_object_ref(&metadata.deny_cap_id).await,
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

// Test that a denied address cannot sign a transaction that withdraws regulated coin funds
// using the funds withdrawal argument in programmable transactions.
#[tokio::test]
async fn test_regulated_coin_v2_funds_withdraw_deny() {
    telemetry_subscribers::init_for_testing();
    let env = new_authority_and_publish("coin_deny_list_v2").await;

    let metadata = env.extract_v2_metadata().await;
    let regulated_coin_type = metadata.regulated_coin_type();
    let deny_list_object_init_version = env.get_latest_object_ref(&SUI_DENY_LIST_OBJECT_ID).await.1;
    let mut env_gas_ref = env.get_latest_object_ref(&env.gas_object_id).await;
    let deny_cap_ref = env.get_latest_object_ref(&metadata.deny_cap_id).await;

    // Create a new account that will be denied for the regulated coin.
    let (denied_address, denied_keypair) = get_account_key_pair();

    env.authority
        .settle_transactions_for_testing(0, std::slice::from_ref(&env.publish_effects))
        .await;

    {
        // Fund the denied address
        let tx = TestTransactionBuilder::new(
            env.sender,
            env_gas_ref,
            env.authority.reference_gas_price_for_testing().unwrap(),
        )
        .transfer_funds_to_address_balance(
            FundSource::address_fund(),
            vec![(100_000_000, denied_address)],
            regulated_coin_type.clone(),
        )
        .build_and_sign(&env.keypair);
        let effects = send_and_confirm_transaction_(&env.authority, None, tx, true)
            .await
            .unwrap()
            .1;
        assert!(effects.status().is_ok(), "Funding should succeed");
        env_gas_ref = effects.gas_object().0;

        env.authority
            .settle_transactions_for_testing(1, std::slice::from_ref(&effects))
            .await;
    }

    // Add the denied address to the regulated coin deny list.
    let add_tx = TestTransactionBuilder::new(
        env.sender,
        env_gas_ref,
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
                mutability: SharedObjectMutability::Mutable,
            }),
            CallArg::Object(ObjectArg::ImmOrOwnedObject(deny_cap_ref)),
            CallArg::Pure(bcs::to_bytes(&denied_address).unwrap()),
        ],
    )
    .with_type_args(vec![regulated_coin_type.clone()])
    .build_and_sign(&env.keypair);
    send_and_confirm_transaction_(&env.authority, None, add_tx, true)
        .await
        .unwrap();

    // Build the programmable transaction with a funds withdrawal argument for the denied coin.
    let mut builder = ProgrammableTransactionBuilder::new();
    let withdraw_arg =
        FundsWithdrawalArg::balance_from_sender(1, TypeInput::from(regulated_coin_type.clone()));
    builder.funds_withdrawal(withdraw_arg).unwrap();
    let amount = builder.pure(1u64).unwrap();
    builder.programmable_move_call(
        SUI_FRAMEWORK_PACKAGE_ID,
        Identifier::new("balance").unwrap(),
        Identifier::new("withdraw_from_account").unwrap(),
        vec![regulated_coin_type.clone()],
        vec![amount],
    );
    let pt = builder.finish();

    let rgp = env.authority.reference_gas_price_for_testing().unwrap();
    let tx_data = TransactionData::new_programmable_allow_sponsor(
        denied_address,
        vec![env_gas_ref],
        pt,
        1_000_000,
        rgp,
        env.sender,
    );
    let tx = Transaction::from_data_and_signer(tx_data, vec![&denied_keypair, &env.keypair]);
    let epoch_store = env.authority.load_epoch_store_one_call_per_task();
    let verified = epoch_store.verify_transaction(tx).unwrap();

    let err = env
        .authority
        .handle_sign_transaction(&epoch_store, verified)
        .await
        .expect_err("signing should fail for denied address");

    match err.into_inner() {
        SuiErrorKind::UserInputError {
            error: UserInputError::AddressDeniedForCoin { address, coin_type },
        } => {
            assert_eq!(address, denied_address);
            assert_eq!(coin_type, regulated_coin_type.to_canonical_string(false));
        }
        other => panic!("unexpected error returned: {other:?}"),
    }
}

struct TestEnv {
    authority: Arc<AuthorityState>,
    sender: SuiAddress,
    keypair: AccountKeyPair,
    gas_object_id: ObjectID,
    publish_effects: TransactionEffects,
}

struct V2Metadata {
    package_id: ObjectID,
    deny_cap_id: ObjectID,
}

impl TestEnv {
    async fn get_latest_object_ref(&self, id: &ObjectID) -> ObjectRef {
        self.authority
            .get_object(id)
            .await
            .unwrap()
            .compute_object_reference()
    }

    async fn extract_v2_metadata(&self) -> V2Metadata {
        let mut deny_cap_object = None;
        let mut metadata_object = None;
        let mut regulated_metadata_object = None;
        let mut package_id = None;
        for (oref, _owner) in self.publish_effects.created() {
            let object = self.authority.get_object(&oref.0).await.unwrap();
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
        let regulated_metadata: RegulatedCoinMetadata =
            regulated_metadata_object.to_rust().unwrap();
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
        V2Metadata {
            package_id,
            deny_cap_id: deny_cap_object.id(),
        }
    }
}

impl V2Metadata {
    fn regulated_coin_type(&self) -> TypeTag {
        TypeTag::Struct(Box::new(StructTag {
            address: self.package_id.into(),
            module: ident_str!("regulated_coin").to_owned(),
            name: ident_str!("REGULATED_COIN").to_owned(),
            type_params: vec![],
        }))
    }
}

async fn new_authority_and_publish(path: &str) -> TestEnv {
    let (sender, keypair) = get_account_key_pair();
    let gas_object = Object::with_owner_for_testing(sender);
    let gas_object_id = gas_object.id();

    let mut protocol_config =
        ProtocolConfig::get_for_version(ProtocolVersion::max(), Chain::Unknown);
    protocol_config.enable_accumulators_for_testing();
    protocol_config
        .set_per_object_congestion_control_mode_for_testing(PerObjectCongestionControlMode::None);

    let authority = TestAuthorityBuilder::new()
        .with_starting_objects(&[gas_object])
        .with_protocol_config(protocol_config)
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
