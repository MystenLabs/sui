// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use std::sync::Arc;

use crate::authority::test_authority_builder::TestAuthorityBuilder;
use crate::authority::AuthorityState;
use fastcrypto::ed25519::Ed25519KeyPair;
use fastcrypto::traits::KeyPair;
use move_core_types::ident_str;
use sui_config::genesis_config::GenesisConfig;
use sui_config::transaction_deny_config::{TransactionDenyConfig, TransactionDenyConfigBuilder};
use sui_config::NetworkConfig;
use sui_types::base_types::{ObjectRef, SuiAddress};
use sui_types::error::{SuiError, SuiResult, UserInputError};
use sui_types::gas_coin::GAS;
use sui_types::messages::{
    CallArg, HandleTransactionResponse, ObjectArg, TransactionData, TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
};
use sui_types::utils::to_sender_signed_transaction;
use sui_types::SUI_FRAMEWORK_OBJECT_ID;
use test_utils::messages::make_staking_transaction;
use test_utils::transaction::make_publish_package;

async fn setup_test(deny_config: TransactionDenyConfig) -> (NetworkConfig, Arc<AuthorityState>) {
    let network_config = sui_config::builder::ConfigBuilder::new_with_temp_dir()
        .initial_accounts_config(GenesisConfig::for_local_testing())
        .build();
    let genesis = &network_config.genesis;
    let keypair = network_config.validator_configs[0].protocol_key_pair();
    let state = TestAuthorityBuilder::new()
        .with_transaction_deny_config(deny_config)
        .build(genesis.committee().unwrap(), keypair, genesis)
        .await;
    (network_config, state)
}

fn get_accounts_and_coins(
    network_config: &NetworkConfig,
    state: &Arc<AuthorityState>,
) -> Vec<(SuiAddress, Ed25519KeyPair, Vec<ObjectRef>)> {
    network_config
        .account_keys
        .iter()
        .map(|account| {
            let address: SuiAddress = account.public().into();
            let objects = state
                .get_owner_objects(address, None, 5, None)
                .unwrap()
                .into_iter()
                .map(|o| o.into())
                .collect();
            (address, account.copy(), objects)
        })
        .collect()
}

async fn transfer_with_account(
    account: &(SuiAddress, Ed25519KeyPair, Vec<ObjectRef>),
    state: &Arc<AuthorityState>,
) -> SuiResult<HandleTransactionResponse> {
    let rgp = state.reference_gas_price_for_testing().unwrap();
    let data = TransactionData::new_transfer_sui(
        account.0,
        account.0,
        None,
        account.2[0],
        TEST_ONLY_GAS_UNIT_FOR_TRANSFER * rgp,
        rgp,
    );
    let tx = to_sender_signed_transaction(data, &account.1);
    state
        .handle_transaction(&state.epoch_store_for_testing(), tx)
        .await
}

fn assert_denied(result: &SuiResult<HandleTransactionResponse>) {
    assert!(matches!(
        result.as_ref().unwrap_err(),
        SuiError::UserInputError {
            error: UserInputError::TransactionDenied { .. }
        }
    ));
}

#[tokio::test]
async fn test_user_transaction_disabled() {
    let (network_config, state) = setup_test(
        TransactionDenyConfigBuilder::new()
            .disable_user_transaction()
            .build(),
    )
    .await;
    let accounts = get_accounts_and_coins(&network_config, &state);
    assert_denied(&transfer_with_account(&accounts[0], &state).await);
}

#[tokio::test]
async fn test_object_denied() {
    // We need to create the authority state once to get one of the gas coin object IDs.
    let (network_config, state) = setup_test(TransactionDenyConfigBuilder::new().build()).await;
    let accounts = get_accounts_and_coins(&network_config, &state);
    // Re-create the state such that we could specify a gas coin object to be denied.
    let obj_ref = accounts[0].2[0];
    let state = TestAuthorityBuilder::new()
        .with_transaction_deny_config(
            TransactionDenyConfigBuilder::new()
                .add_denied_object(obj_ref.0)
                .build(),
        )
        .build_with_store(
            network_config.genesis.committee().unwrap(),
            network_config.validator_configs[0].protocol_key_pair(),
            state.db(),
            &[],
        )
        .await;
    assert_denied(&transfer_with_account(&accounts[0], &state).await);
}

#[tokio::test]
async fn test_address_denied() {
    // We need to create the authority state once to get one of the account addresses.
    let (network_config, state) = setup_test(TransactionDenyConfigBuilder::new().build()).await;
    let accounts = get_accounts_and_coins(&network_config, &state);
    // Re-create the state such that we could specify an address to be denied.
    let state = TestAuthorityBuilder::new()
        .with_transaction_deny_config(
            TransactionDenyConfigBuilder::new()
                .add_denied_address(accounts[0].0)
                .build(),
        )
        .build_with_store(
            network_config.genesis.committee().unwrap(),
            network_config.validator_configs[0].protocol_key_pair(),
            state.db(),
            &[],
        )
        .await;
    assert_denied(&transfer_with_account(&accounts[0], &state).await);
}

#[tokio::test]
async fn test_shared_object_transaction_disabled() {
    let (network_config, state) = setup_test(
        TransactionDenyConfigBuilder::new()
            .disable_shared_object_transaction()
            .build(),
    )
    .await;
    let accounts = get_accounts_and_coins(&network_config, &state);
    let gas_price = state.reference_gas_price_for_testing().unwrap();
    let tx = make_staking_transaction(
        accounts[0].2[0],
        accounts[0].2[1],
        SuiAddress::default(),
        accounts[0].0,
        &accounts[0].1,
        gas_price,
    );
    let result = state
        .handle_transaction(&state.epoch_store_for_testing(), tx)
        .await;
    assert_denied(&result);
}

#[tokio::test]
async fn test_package_publish_disabled() {
    let (network_config, state) = setup_test(
        TransactionDenyConfigBuilder::new()
            .disable_package_publish()
            .build(),
    )
    .await;
    let accounts = get_accounts_and_coins(&network_config, &state);
    let rgp = state.reference_gas_price_for_testing().unwrap();
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("src/unit_tests/data/object_basics");
    let tx = make_publish_package(accounts[0].2[0], path, rgp);
    let result = state
        .handle_transaction(&state.epoch_store_for_testing(), tx)
        .await;
    assert_denied(&result);
}

#[tokio::test]
async fn test_package_denied() {
    let (network_config, state) = setup_test(
        TransactionDenyConfigBuilder::new()
            .add_denied_package(SUI_FRAMEWORK_OBJECT_ID)
            .build(),
    )
    .await;
    let accounts = get_accounts_and_coins(&network_config, &state);
    let rgp = state.reference_gas_price_for_testing().unwrap();
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("src/unit_tests/data/object_basics");
    let tx = make_publish_package(accounts[0].2[0], path, rgp);
    let result = state
        .handle_transaction(&state.epoch_store_for_testing(), tx)
        .await;
    assert_denied(&result);

    let tx = make_staking_transaction(
        accounts[1].2[0],
        accounts[1].2[1],
        SuiAddress::default(),
        accounts[1].0,
        &accounts[1].1,
        rgp,
    );
    let result = state
        .handle_transaction(&state.epoch_store_for_testing(), tx)
        .await;
    assert_denied(&result);

    let data = TransactionData::new_move_call(
        accounts[2].0,
        SUI_FRAMEWORK_OBJECT_ID,
        ident_str!("coin").to_owned(),
        ident_str!("split").to_owned(),
        vec![GAS::type_tag()],
        accounts[2].2[0],
        vec![
            CallArg::Object(ObjectArg::ImmOrOwnedObject(accounts[2].2[1])),
            CallArg::Pure(bcs::to_bytes(&1u64).unwrap()),
        ],
        TEST_ONLY_GAS_UNIT_FOR_TRANSFER * rgp,
        rgp,
    )
    .unwrap();
    let tx = to_sender_signed_transaction(data, &accounts[0].1);
    let result = state
        .handle_transaction(&state.epoch_store_for_testing(), tx)
        .await;
    assert_denied(&result);
}
