// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use std::sync::Arc;

use crate::authority::authority_test_utils::{
    publish_package_on_single_authority, upgrade_package_on_single_authority,
};
use crate::authority::test_authority_builder::TestAuthorityBuilder;
use crate::authority::AuthorityState;
use fastcrypto::ed25519::Ed25519KeyPair;
use fastcrypto::traits::KeyPair;
use move_core_types::ident_str;
use sui_config::genesis_config::{AccountConfig, DEFAULT_GAS_AMOUNT};
use sui_config::transaction_deny_config::{TransactionDenyConfig, TransactionDenyConfigBuilder};
use sui_config::NetworkConfig;
use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress};
use sui_types::error::{SuiError, SuiResult, UserInputError};
use sui_types::messages::{
    CallArg, HandleTransactionResponse, TransactionData, TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
};
use sui_types::utils::{
    to_sender_signed_transaction, to_sender_signed_transaction_with_multi_signers,
};
use test_utils::messages::make_staking_transaction;
use test_utils::transaction::make_publish_package;

const ACCOUNT_NUM: usize = 5;
const GAS_OBJECT_COUNT: usize = 15;

async fn setup_test(deny_config: TransactionDenyConfig) -> (NetworkConfig, Arc<AuthorityState>) {
    let network_config = sui_config::builder::ConfigBuilder::new_with_temp_dir()
        .with_accounts(vec![
            AccountConfig {
                address: None,
                gas_amounts: vec![DEFAULT_GAS_AMOUNT; GAS_OBJECT_COUNT],
            };
            ACCOUNT_NUM
        ])
        .build();
    let state = TestAuthorityBuilder::new()
        .with_transaction_deny_config(deny_config)
        .with_network_config(&network_config)
        .build()
        .await;
    (network_config, state)
}

async fn reload_state_with_new_deny_config(
    network_config: &NetworkConfig,
    state: Arc<AuthorityState>,
    config: TransactionDenyConfig,
) -> Arc<AuthorityState> {
    TestAuthorityBuilder::new()
        .with_transaction_deny_config(config)
        .with_network_config(network_config)
        .with_store(state.db())
        .build()
        .await
}

type Account = (SuiAddress, Ed25519KeyPair, Vec<ObjectRef>);

fn get_accounts_and_coins(
    network_config: &NetworkConfig,
    state: &Arc<AuthorityState>,
) -> Vec<Account> {
    let accounts: Vec<_> = network_config
        .account_keys
        .iter()
        .map(|account| {
            let address: SuiAddress = account.public().into();
            let objects: Vec<_> = state
                .get_owner_objects(address, None, GAS_OBJECT_COUNT, None)
                .unwrap()
                .into_iter()
                .map(|o| o.into())
                .collect();
            assert_eq!(objects.len(), GAS_OBJECT_COUNT);
            (address, account.copy(), objects)
        })
        .collect();
    assert_eq!(accounts.len(), ACCOUNT_NUM);
    accounts
}

async fn transfer_with_account(
    sender_account: &Account,
    sponsor_account: &Account,
    state: &Arc<AuthorityState>,
) -> SuiResult<HandleTransactionResponse> {
    let rgp = state.reference_gas_price_for_testing().unwrap();
    let data = TransactionData::new_transfer_sui_allow_sponsor(
        sender_account.0,
        sender_account.0,
        None,
        sponsor_account.2[0],
        TEST_ONLY_GAS_UNIT_FOR_TRANSFER * rgp,
        rgp,
        sponsor_account.0,
    );
    let tx = if sender_account.0 == sponsor_account.0 {
        to_sender_signed_transaction(data, &sender_account.1)
    } else {
        to_sender_signed_transaction_with_multi_signers(
            data,
            vec![&sender_account.1, &sponsor_account.1],
        )
    };
    state
        .handle_transaction(&state.epoch_store_for_testing(), tx)
        .await
}

async fn handle_move_call_transaction(
    state: &Arc<AuthorityState>,
    package: ObjectID,
    module_name: &'static str,
    function_name: &'static str,
    args: Vec<CallArg>,
    account: &Account,
    gas_payment_index: usize,
) -> SuiResult<HandleTransactionResponse> {
    let rgp = state.reference_gas_price_for_testing().unwrap();
    let data = TransactionData::new_move_call(
        account.0,
        package,
        ident_str!(module_name).to_owned(),
        ident_str!(function_name).to_owned(),
        vec![],
        account.2[gas_payment_index],
        args,
        TEST_ONLY_GAS_UNIT_FOR_TRANSFER * rgp,
        rgp,
    )
    .unwrap();
    let tx = to_sender_signed_transaction(data, &account.1);
    state
        .handle_transaction(&state.epoch_store_for_testing(), tx)
        .await
}

fn assert_denied<T: std::fmt::Debug>(result: &SuiResult<T>) {
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
    assert_denied(&transfer_with_account(&accounts[0], &accounts[0], &state).await);
}

#[tokio::test]
async fn test_object_denied() {
    // We need to create the authority state once to get one of the gas coin object IDs.
    let (network_config, state) = setup_test(TransactionDenyConfigBuilder::new().build()).await;
    let accounts = get_accounts_and_coins(&network_config, &state);
    // Re-create the state such that we could specify a gas coin object to be denied.
    let obj_ref = accounts[0].2[0];
    let state = reload_state_with_new_deny_config(
        &network_config,
        state,
        TransactionDenyConfigBuilder::new()
            .add_denied_object(obj_ref.0)
            .build(),
    )
    .await;
    assert_denied(&transfer_with_account(&accounts[0], &accounts[0], &state).await);
}

#[tokio::test]
async fn test_signer_denied() {
    // We need to create the authority state once to get one of the account addresses.
    let (network_config, state) = setup_test(TransactionDenyConfigBuilder::new().build()).await;
    let accounts = get_accounts_and_coins(&network_config, &state);

    // Re-create the state such that we could specify an address to be denied.
    let state = reload_state_with_new_deny_config(
        &network_config,
        state,
        TransactionDenyConfigBuilder::new()
            .add_denied_address(accounts[0].0)
            .add_denied_address(accounts[1].0)
            .build(),
    )
    .await;
    // Test that sender (accounts[0]) would be denied.
    assert_denied(&transfer_with_account(&accounts[0], &accounts[0], &state).await);
    // Test that sponsor (accounts[1]) would be denied.
    assert_denied(&transfer_with_account(&accounts[2], &accounts[1], &state).await);
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
    let (network_config, state) = setup_test(TransactionDenyConfigBuilder::new().build()).await;
    let accounts = get_accounts_and_coins(&network_config, &state);
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // Publish 3 packages, where b depends on c, and a depends on b.
    // Also upgrade c to c', and upgrade b to b' (which will start using c' instead of c as dependency).
    let (package_c, cap_c) = publish_package_on_single_authority(
        path.join("src/unit_tests/data/package_deny/c"),
        accounts[0].0,
        &accounts[0].1,
        accounts[0].2[0],
        [("c", ObjectID::ZERO)],
        vec![],
        &state,
    )
    .await
    .unwrap();
    let (package_b, cap_b) = publish_package_on_single_authority(
        path.join("src/unit_tests/data/package_deny/b"),
        accounts[0].0,
        &accounts[0].1,
        accounts[0].2[1],
        [("b", ObjectID::ZERO), ("c", package_c)],
        vec![package_c],
        &state,
    )
    .await
    .unwrap();
    let (package_a, cap_a) = publish_package_on_single_authority(
        path.join("src/unit_tests/data/package_deny/a"),
        accounts[0].0,
        &accounts[0].1,
        accounts[0].2[2],
        [("b", package_b), ("c", package_c)],
        vec![package_b, package_c],
        &state,
    )
    .await
    .unwrap();
    let package_c_prime = upgrade_package_on_single_authority(
        path.join("src/unit_tests/data/package_deny/c"),
        accounts[0].0,
        &accounts[0].1,
        accounts[0].2[3],
        package_c,
        cap_c,
        [("c", ObjectID::ZERO)],
        vec![],
        &state,
    )
    .await
    .unwrap();
    let package_b_prime = upgrade_package_on_single_authority(
        path.join("src/unit_tests/data/package_deny/b"),
        accounts[0].0,
        &accounts[0].1,
        accounts[0].2[4],
        package_b,
        cap_b,
        [("b", ObjectID::ZERO), ("c", package_c)],
        [("C", package_c_prime)],
        &state,
    )
    .await
    .unwrap();

    // Re-create the state such that we could deny package c.
    let state = reload_state_with_new_deny_config(
        &network_config,
        state,
        TransactionDenyConfigBuilder::new()
            .add_denied_package(package_c)
            .build(),
    )
    .await;

    // Calling modules in package c directly should fail.
    let result =
        handle_move_call_transaction(&state, package_c, "c", "c", vec![], &accounts[0], 5).await;
    assert_denied(&result);

    // Calling modules in package b should fail too as it directly depends on c.
    let result =
        handle_move_call_transaction(&state, package_c, "b", "b", vec![], &accounts[0], 6).await;
    assert_denied(&result);

    // Calling modules in package a should fail too as it indirectly depends on c.
    let result =
        handle_move_call_transaction(&state, package_c, "a", "a", vec![], &accounts[0], 7).await;
    assert_denied(&result);

    // Calling modules in c' should succeed as it is not denied.
    let result =
        handle_move_call_transaction(&state, package_c_prime, "c", "c", vec![], &accounts[0], 8)
            .await;
    assert!(result.is_ok());

    // Calling modules in b' should succeed as it no longer depends on c.
    let result =
        handle_move_call_transaction(&state, package_b_prime, "b", "b", vec![], &accounts[0], 9)
            .await;
    assert!(result.is_ok());

    // Publish a should fail because it has a dependency on c, which is denied.
    let result = publish_package_on_single_authority(
        path.join("src/unit_tests/data/package_deny/a"),
        accounts[0].0,
        &accounts[0].1,
        accounts[0].2[10],
        [("b", package_b), ("c", package_c)],
        vec![package_b, package_c],
        &state,
    )
    .await;
    assert_denied(&result);

    // Upgrade a using old c as dependency should fail.
    let result = upgrade_package_on_single_authority(
        path.join("src/unit_tests/data/package_deny/a"),
        accounts[0].0,
        &accounts[0].1,
        accounts[0].2[11],
        package_a,
        cap_a,
        [("b", package_b), ("c", package_c)],
        [("B", package_b), ("C", package_c)],
        &state,
    )
    .await;
    assert_denied(&result);

    // Upgrade a using c' as dependency will succeed since it no longer depends on c.
    let result = upgrade_package_on_single_authority(
        path.join("src/unit_tests/data/package_deny/a"),
        accounts[0].0,
        &accounts[0].1,
        accounts[0].2[12],
        package_a,
        cap_a,
        [("b", package_b), ("c", package_c)],
        [("B", package_b), ("C", package_c_prime)],
        &state,
    )
    .await;
    assert!(result.is_ok());

    // TODO: We will need to upgrade c to c', and publish a new package b' that depends on c'.
    // Then we could test that calling c' and b' would both succeed.
}
