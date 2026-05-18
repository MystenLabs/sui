// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Result, anyhow};
use prost_types::FieldMask;
use rosetta_client::start_rosetta_test_server;
use serde_json::json;
use shared_crypto::intent::Intent;
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::str::FromStr;
use sui_keys::keystore::AccountKeystore;
use sui_rosetta::CoinMetadataCache;
use sui_rosetta::operations::Operations;
use sui_rosetta::types::{
    AccountBalanceRequest, AccountBalanceResponse, AccountIdentifier, Currency, NetworkIdentifier,
    SubAccount, SubAccountType, SuiEnv,
};
use sui_rosetta::types::{Currencies, OperationType, TransactionIdentifierResponse};
use sui_rpc::client::Client as GrpcClient;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::{GetCheckpointRequest, GetEpochRequest, GetTransactionRequest};

mod test_utils;
use sui_swarm_config::genesis_config::{DEFAULT_GAS_AMOUNT, DEFAULT_NUMBER_OF_OBJECT_PER_ACCOUNT};
use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress};
use sui_types::governance::ADD_STAKE_MUL_COIN_FUN_NAME;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::rpc_proto_conversions::ObjectReferenceExt;
use sui_types::sui_system_state::SUI_SYSTEM_MODULE_NAME;
use sui_types::transaction::{
    Argument, CallArg, Command, InputObjectKind, ObjectArg, TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
    Transaction, TransactionData,
};
use sui_types::utils::to_sender_signed_transaction;
use sui_types::{Identifier, SUI_SYSTEM_PACKAGE_ID};
use test_cluster::TestClusterBuilder;
use test_utils::{
    execute_transaction, get_all_coins, get_object_ref, get_random_sui, wait_for_transaction,
};

use crate::rosetta_client::RosettaEndpoint;

#[allow(dead_code)]
mod rosetta_client;

#[tokio::test]
async fn test_get_staked_sui() -> Result<()> {
    let test_cluster = TestClusterBuilder::new().build().await;
    let address = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;

    let mut client = GrpcClient::new(test_cluster.rpc_url())?;
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let network_identifier = NetworkIdentifier {
        blockchain: "sui".to_string(),
        network: SuiEnv::LocalNet,
    };
    // Verify initial balance and stake
    let request = AccountBalanceRequest {
        network_identifier: network_identifier.clone(),
        account_identifier: AccountIdentifier {
            address,
            sub_account: None,
        },
        block_identifier: Default::default(),
        currencies: Currencies(vec![Currency::default()]),
    };

    let response: AccountBalanceResponse = rosetta_client
        .call(RosettaEndpoint::Balance, &request)
        .await
        .unwrap();
    assert_eq!(1, response.balances.len());
    assert_eq!(
        (DEFAULT_GAS_AMOUNT * DEFAULT_NUMBER_OF_OBJECT_PER_ACCOUNT as u64) as i128,
        response.balances[0].value
    );

    let request = AccountBalanceRequest {
        network_identifier: network_identifier.clone(),
        account_identifier: AccountIdentifier {
            address,
            sub_account: Some(SubAccount {
                account_type: SubAccountType::PendingStake,
            }),
        },
        block_identifier: Default::default(),
        currencies: Currencies(vec![Currency::default()]),
    };
    let response: AccountBalanceResponse = rosetta_client
        .call(RosettaEndpoint::Balance, &request)
        .await
        .map_err(|e| anyhow!("Rosetta client error: {e:?}"))?;
    assert_eq!(response.balances[0].value, 0);

    // Stake some sui
    let request = GetEpochRequest::latest().with_read_mask(FieldMask::from_paths(["system_state"]));

    let response = client
        .ledger_client()
        .get_epoch(request)
        .await?
        .into_inner();

    let system_state = response
        .epoch
        .and_then(|epoch| epoch.system_state)
        .ok_or_else(|| anyhow!("Failed to get system state"))?;

    let validator = system_state
        .validators
        .ok_or_else(|| anyhow!("No validators in system state"))?
        .active_validators[0]
        .address()
        .parse::<SuiAddress>()?;
    let coins = get_all_coins(&mut client.clone(), address).await?;
    // Get gas price
    let gas_price = client.get_reference_gas_price().await?;

    // Use first coin for staking, second coin for gas
    let staking_coin_ref = get_object_ref(&mut client.clone(), coins[0].id()).await?;

    // Use second coin as gas
    let gas_object = get_object_ref(&mut client.clone(), coins[1].id())
        .await?
        .as_object_ref();

    // Build PTB for staking
    let mut ptb = ProgrammableTransactionBuilder::new();
    let arguments = vec![
        ptb.input(CallArg::SUI_SYSTEM_MUT)?,
        ptb.make_obj_vec(vec![ObjectArg::ImmOrOwnedObject(
            staking_coin_ref.as_object_ref(),
        )])?,
        ptb.pure(Some(1_000_000_000u64))?,
        ptb.pure(validator)?,
    ];
    ptb.command(Command::move_call(
        SUI_SYSTEM_PACKAGE_ID,
        SUI_SYSTEM_MODULE_NAME.to_owned(),
        ADD_STAKE_MUL_COIN_FUN_NAME.to_owned(),
        vec![],
        arguments,
    ));

    let delegation_tx = TransactionData::new_programmable(
        address,
        vec![gas_object],
        ptb.finish(),
        1_000_000_000,
        gas_price,
    );
    let tx = to_sender_signed_transaction(delegation_tx, keystore.export(&address)?);
    execute_transaction(&mut client.clone(), &tx).await?;

    let response = rosetta_client
        .get_balance(
            network_identifier.clone(),
            address,
            Some(SubAccountType::PendingStake),
        )
        .await;
    assert_eq!(1, response.balances.len());
    assert_eq!(1_000_000_000, response.balances[0].value);

    Ok(())
}

#[tokio::test]
async fn test_stake() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;

    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let request = GetEpochRequest::latest().with_read_mask(FieldMask::from_paths(["system_state"]));

    let response = client
        .ledger_client()
        .get_epoch(request)
        .await
        .unwrap()
        .into_inner();

    let system_state = response.epoch.and_then(|epoch| epoch.system_state).unwrap();

    let validator = system_state.validators.unwrap().active_validators[0]
        .address()
        .parse::<SuiAddress>()
        .unwrap();

    let ops = serde_json::from_value(json!(
        [{
            "operation_identifier":{"index":0},
            "type":"Stake",
            "account": { "address" : sender.to_string() },
            "amount" : { "value": "-1000000000" },
            "metadata": { "Stake" : {"validator": validator.to_string()} }
        }]
    ))
    .unwrap();

    let response: TransactionIdentifierResponse = rosetta_client
        .rosetta_flow(&ops, keystore, None)
        .await
        .submit
        .unwrap()
        .unwrap();

    wait_for_transaction(
        &mut client,
        &response.transaction_identifier.hash.to_string(),
    )
    .await
    .unwrap();

    // Fetch using gRPC
    let grpc_request = GetTransactionRequest::default()
        .with_digest(response.transaction_identifier.hash.to_string())
        .with_read_mask(FieldMask::from_paths([
            "digest",
            "transaction",
            "effects",
            "balance_changes",
            "events",
        ]));

    let grpc_response = client
        .clone()
        .ledger_client()
        .get_transaction(grpc_request)
        .await
        .unwrap()
        .into_inner();

    let tx = grpc_response
        .transaction
        .expect("Response transaction should not be empty");

    assert!(
        tx.effects().status().success(),
        "Transaction failed: {:?}",
        tx.effects().status().error()
    );

    let coin_cache = CoinMetadataCache::new(client.clone(), NonZeroUsize::new(2).unwrap());
    let ops2 = fetch_transaction_and_get_operations(
        &test_cluster,
        tx.digest().parse().unwrap(),
        &coin_cache,
    )
    .await
    .unwrap();
    assert!(
        ops2.contains(&ops),
        "Operation mismatch. expecting:{}, got:{}",
        serde_json::to_string(&ops).unwrap(),
        serde_json::to_string(&ops2).unwrap()
    );
}

#[tokio::test]
async fn test_stake_all() {
    use sui_swarm_config::genesis_config::AccountConfig;

    // Create test cluster with 150K SUI instead of the default 150M
    // 150K SUI = 150_000_000_000_000 MIST (150K * 1e9)
    const AMOUNT_150K_SUI: u64 = 150_000_000_000_000;

    // Create 5 test accounts, but only the first one gets custom amount
    let accounts = (0..5)
        .map(|_| AccountConfig {
            address: None,
            gas_amounts: vec![AMOUNT_150K_SUI], // Single gas object with 150K SUI
        })
        .collect();

    let test_cluster = TestClusterBuilder::new()
        .with_accounts(accounts)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;

    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let request = GetEpochRequest::latest().with_read_mask(FieldMask::from_paths(["system_state"]));

    let response = client
        .ledger_client()
        .get_epoch(request)
        .await
        .unwrap()
        .into_inner();

    let system_state = response.epoch.and_then(|epoch| epoch.system_state).unwrap();

    let validator = system_state.validators.unwrap().active_validators[0]
        .address()
        .parse::<SuiAddress>()
        .unwrap();

    let ops = serde_json::from_value(json!(
        [{
            "operation_identifier":{"index":0},
            "type":"Stake",
            "account": { "address" : sender.to_string() },
            "metadata": { "Stake" : {"validator": validator.to_string()} }
        }]
    ))
    .unwrap();

    let response = rosetta_client
        .rosetta_flow(&ops, keystore, None)
        .await
        .submit
        .unwrap()
        .unwrap();

    wait_for_transaction(
        &mut client,
        &response.transaction_identifier.hash.to_string(),
    )
    .await
    .unwrap();

    // Fetch using gRPC
    let grpc_request = GetTransactionRequest::default()
        .with_digest(response.transaction_identifier.hash.to_string())
        .with_read_mask(FieldMask::from_paths([
            "digest",
            "transaction",
            "effects",
            "balance_changes",
            "events",
        ]));

    let grpc_response = client
        .clone()
        .ledger_client()
        .get_transaction(grpc_request)
        .await
        .unwrap()
        .into_inner();

    let tx = grpc_response
        .transaction
        .expect("Response transaction should not be empty");

    assert!(
        tx.effects().status().success(),
        "Transaction failed: {:?}",
        tx.effects().status().error()
    );

    let coin_cache = CoinMetadataCache::new(client.clone(), NonZeroUsize::new(2).unwrap());
    let ops2 = fetch_transaction_and_get_operations(
        &test_cluster,
        tx.digest().parse().unwrap(),
        &coin_cache,
    )
    .await
    .unwrap();
    assert!(
        ops2.contains(&ops),
        "Operation mismatch. expecting:{}, got:{}",
        serde_json::to_string(&ops).unwrap(),
        serde_json::to_string(&ops2).unwrap()
    );
}

#[tokio::test]
async fn test_withdraw_stake() {
    telemetry_subscribers::init_for_testing();

    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(60000)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;

    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    // First add some stakes
    let request = GetEpochRequest::latest().with_read_mask(FieldMask::from_paths(["system_state"]));

    let response = client
        .ledger_client()
        .get_epoch(request)
        .await
        .unwrap()
        .into_inner();

    let system_state = response.epoch.and_then(|epoch| epoch.system_state).unwrap();

    let validator = system_state.validators.unwrap().active_validators[0]
        .address()
        .parse::<SuiAddress>()
        .unwrap();

    let ops = serde_json::from_value(json!(
        [{
            "operation_identifier":{"index":0},
            "type":"Stake",
            "account": { "address" : sender.to_string() },
            "amount" : { "value": "-1000000000" },
            "metadata": { "Stake" : {"validator": validator.to_string()} }
        }]
    ))
    .unwrap();

    let response = rosetta_client
        .rosetta_flow(&ops, keystore, None)
        .await
        .submit
        .unwrap()
        .unwrap();

    wait_for_transaction(
        &mut client,
        &response.transaction_identifier.hash.to_string(),
    )
    .await
    .unwrap();

    // Fetch using gRPC
    let grpc_request = GetTransactionRequest::default()
        .with_digest(response.transaction_identifier.hash.to_string())
        .with_read_mask(FieldMask::from_paths([
            "digest",
            "transaction",
            "effects",
            "balance_changes",
            "events",
        ]));

    let grpc_response = client
        .clone()
        .ledger_client()
        .get_transaction(grpc_request)
        .await
        .unwrap()
        .into_inner();

    let tx = grpc_response
        .transaction
        .expect("Response transaction should not be empty");

    assert!(
        tx.effects().status().success(),
        "Transaction failed: {:?}",
        tx.effects().status().error()
    );
    // verify balance
    let network_identifier = NetworkIdentifier {
        blockchain: "sui".to_string(),
        network: SuiEnv::LocalNet,
    };
    let response = rosetta_client
        .get_balance(
            network_identifier.clone(),
            sender,
            Some(SubAccountType::PendingStake),
        )
        .await;

    assert_eq!(1, response.balances.len());
    assert_eq!(1000000000, response.balances[0].value);

    // Trigger epoch change.
    test_cluster.trigger_reconfiguration().await;

    // withdraw all stake
    let ops = serde_json::from_value(json!(
        [{
            "operation_identifier":{"index":0},
            "type":"WithdrawStake",
            "account": { "address" : sender.to_string() }
        }]
    ))
    .unwrap();

    let response = rosetta_client
        .rosetta_flow(&ops, keystore, None)
        .await
        .submit
        .unwrap()
        .unwrap();

    wait_for_transaction(
        &mut client,
        &response.transaction_identifier.hash.to_string(),
    )
    .await
    .unwrap();

    // Fetch using gRPC
    let grpc_request = GetTransactionRequest::default()
        .with_digest(response.transaction_identifier.hash.to_string())
        .with_read_mask(FieldMask::from_paths([
            "digest",
            "transaction",
            "effects",
            "balance_changes",
            "events",
        ]));

    let grpc_response = client
        .clone()
        .ledger_client()
        .get_transaction(grpc_request)
        .await
        .unwrap()
        .into_inner();

    let tx = grpc_response
        .transaction
        .expect("Response transaction should not be empty");

    assert!(
        tx.effects().status().success(),
        "Transaction failed: {:?}",
        tx.effects().status().error()
    );
    let coin_cache = CoinMetadataCache::new(client.clone(), NonZeroUsize::new(2).unwrap());
    let ops2 = fetch_transaction_and_get_operations(
        &test_cluster,
        tx.digest().parse().unwrap(),
        &coin_cache,
    )
    .await
    .unwrap();
    assert!(
        ops2.contains(&ops),
        "Operation mismatch. expecting:{}, got:{}",
        serde_json::to_string(&ops).unwrap(),
        serde_json::to_string(&ops2).unwrap()
    );

    // stake should be 0
    let response = rosetta_client
        .get_balance(
            network_identifier.clone(),
            sender,
            Some(SubAccountType::PendingStake),
        )
        .await;

    assert_eq!(1, response.balances.len());
    assert_eq!(0, response.balances[0].value);
}

#[tokio::test]
async fn test_pay_sui() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let recipient = test_cluster.get_address_1();
    let keystore = &test_cluster.wallet.config.keystore;

    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let ops = serde_json::from_value(json!(
        [{
            "operation_identifier":{"index":0},
            "type":"PaySui",
            "account": { "address" : recipient.to_string() },
            "amount" : { "value": "1000000000" }
        },{
            "operation_identifier":{"index":1},
            "type":"PaySui",
            "account": { "address" : sender.to_string() },
            "amount" : { "value": "-1000000000" }
        }]
    ))
    .unwrap();

    let response = rosetta_client
        .rosetta_flow(&ops, keystore, None)
        .await
        .submit
        .unwrap()
        .unwrap();

    wait_for_transaction(
        &mut client,
        &response.transaction_identifier.hash.to_string(),
    )
    .await
    .unwrap();

    // Fetch using gRPC
    let grpc_request = GetTransactionRequest::default()
        .with_digest(response.transaction_identifier.hash.to_string())
        .with_read_mask(FieldMask::from_paths([
            "digest",
            "transaction",
            "effects",
            "balance_changes",
            "events",
        ]));

    let grpc_response = client
        .clone()
        .ledger_client()
        .get_transaction(grpc_request)
        .await
        .unwrap()
        .into_inner();

    let tx = grpc_response
        .transaction
        .expect("Response transaction should not be empty");

    assert!(
        tx.effects().status().success(),
        "Transaction failed: {:?}",
        tx.effects().status().error()
    );
    let coin_cache = CoinMetadataCache::new(client.clone(), NonZeroUsize::new(2).unwrap());
    let ops2 = fetch_transaction_and_get_operations(
        &test_cluster,
        tx.digest().parse().unwrap(),
        &coin_cache,
    )
    .await
    .unwrap();
    assert!(
        ops2.contains(&ops),
        "Operation mismatch. expecting:{}, got:{}",
        serde_json::to_string(&ops).unwrap(),
        serde_json::to_string(&ops2).unwrap()
    );
}

#[tokio::test]
async fn test_pay_sui_multiple_times() {
    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(36000000)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let recipient = test_cluster.get_address_1();
    let keystore = &test_cluster.wallet.config.keystore;

    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;
    let coin_cache = CoinMetadataCache::new(client.clone(), NonZeroUsize::new(2).unwrap());

    for _i in 1..20 {
        let ops = serde_json::from_value(json!(
            [{
                "operation_identifier":{"index":0},
                "type":"PaySui",
                "account": { "address" : recipient.to_string() },
                "amount" : { "value": "1000000000" }
            },{
                "operation_identifier":{"index":1},
                "type":"PaySui",
                "account": { "address" : sender.to_string() },
                "amount" : { "value": "-1000000000" }
            }]
        ))
        .unwrap();

        let response = rosetta_client
            .rosetta_flow(&ops, keystore, None)
            .await
            .submit
            .unwrap()
            .unwrap();

        // Wait for transaction to be indexed
        wait_for_transaction(
            &mut client,
            &response.transaction_identifier.hash.to_string(),
        )
        .await
        .unwrap();

        // Fetch using gRPC
        let grpc_request = GetTransactionRequest::default()
            .with_digest(response.transaction_identifier.hash.to_string())
            .with_read_mask(FieldMask::from_paths([
                "digest",
                "transaction",
                "effects",
                "balance_changes",
                "events",
            ]));

        let grpc_response = client
            .ledger_client()
            .get_transaction(grpc_request)
            .await
            .unwrap()
            .into_inner();

        let tx = grpc_response
            .transaction
            .expect("Response transaction should not be empty");
        assert!(
            tx.effects().status().success(),
            "Transaction failed: {:?}",
            tx.effects().status().error()
        );
        let ops2 = fetch_transaction_and_get_operations(
            &test_cluster,
            tx.digest().parse().unwrap(),
            &coin_cache,
        )
        .await
        .unwrap();
        assert!(
            ops2.contains(&ops),
            "Operation mismatch. expecting:{}, got:{}",
            serde_json::to_string(&ops).unwrap(),
            serde_json::to_string(&ops2).unwrap()
        );
    }
}

#[tokio::test]
async fn test_transfer_single_gas_coin() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let recipient = test_cluster.get_address_1();
    let keystore = &test_cluster.wallet.config.keystore;
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();

    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        builder.transfer_arg(recipient, Argument::GasCoin);
        builder.finish()
    };

    let input_objects = pt
        .input_objects()
        .unwrap_or_default()
        .iter()
        .flat_map(|obj| {
            if let InputObjectKind::ImmOrOwnedMoveObject((id, ..)) = obj {
                Some(*id)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let gas = vec![get_random_sui(&mut client.clone(), sender, input_objects).await];
    let gas_price = client.get_reference_gas_price().await.unwrap();

    let data = TransactionData::new_programmable(
        sender,
        gas,
        pt,
        TEST_ONLY_GAS_UNIT_FOR_TRANSFER * gas_price,
        gas_price,
    );

    let signature = keystore
        .sign_secure(&sender, &data, Intent::sui_transaction())
        .await
        .unwrap();

    let signed_transaction = Transaction::from_data(data.clone(), vec![signature]);
    let response = execute_transaction(&mut client.clone(), &signed_transaction)
        .await
        .map_err(|e| anyhow!("TX execution failed for {data:#?}, error : {e}"))
        .unwrap();

    let coin_cache = CoinMetadataCache::new(client.clone(), NonZeroUsize::new(2).unwrap());
    let operations = fetch_transaction_and_get_operations(
        &test_cluster,
        response.transaction().digest().parse().unwrap(),
        &coin_cache,
    )
    .await
    .unwrap();

    let mut balance = 0;
    operations.into_iter().for_each(|op| {
        if op.type_ == OperationType::Gas {
            assert_eq!(op.account.unwrap().address, sender);
        }
        if op.type_ == OperationType::PaySui {
            balance += op.amount.unwrap().value;
        }
    });
    assert_eq!(balance, 0);
}

/// The below SuiTransactionBlockResponse is created by using the below contract:
/// ```move
/// module vault::vault;
///
/// use sui::balance::Balance;
/// use sui::coin::Coin;
/// use sui::sui::SUI;
///
/// public struct Vault has key, store {
///     id: UID,
///     balance: Balance<SUI>
/// }
///
/// public fun from_coin(coin: Coin<SUI>, ctx: &mut TxContext): Vault {
///     Vault {
///         id: object::new(ctx),
///         balance: coin.into_balance()
///     }
/// }
///
/// public fun to_coin(self: Vault, ctx: &mut TxContext): Coin<SUI> {
///     let Vault { id, balance } = self;
///     id.delete();
///     balance.into_coin(ctx)
/// }
///
/// public fun amount_to_coin(self: &mut Vault, amount: u64, ctx: &mut TxContext): Coin<SUI> {
///     self.balance.split(amount).into_coin(ctx)
/// }
///
/// The sender has a `Vault` under their account and they convert to a `Coin`, merge it with gas
/// and transfer it to recipient. `Vault` splits balance to extract
/// amount double the gas-cost. Then gas-object is merged with the coin equal to gas-cost and is
/// returned to sender.
/// This checks to see when GAS_COST is transferred back to the sender, which is an edge case.
/// In this case `process_gascoin_transfer` should be not processed.
///
/// ptb:
/// ```bash
/// gas_cost=$((1000000+4294000-2294820))
/// amount=$((2*$gas_cost))
///
/// res=$(sui client ptb \
///     --move-call $PACKAGE_ID::vault::amount_to_coin \
///         @$VAULT_ID \
///         $amount \
///     --assign coin \
///     --split-coins coin [$gas_cost] \
///     --assign coin_to_transfer \
///     --transfer-objects \
///         [coin_to_transfer] @$RECIPIENT \
///     --transfer-objects \
///         [gas, coin] @$sender \
///     --json)
/// ```
#[tokio::test]
async fn test_balance_from_obj_paid_eq_gas() {
    let test_cluster = TestClusterBuilder::new().build().await;
    const SENDER: &str = "0x6293e2b4434265fa60ac8ed96342b7a288c0e43ffe737ba40feb24f06fed305d";
    const RECIPIENT: &str = "0x0e3225553e3b945b4cde5621a980297c45b96002f33c95d3306e58013129ee7c";
    const AMOUNT: i128 = 2999180;
    let client = GrpcClient::new(test_cluster.rpc_url()).unwrap();

    use sui_rpc::proto::sui::rpc::v2::{
        BalanceChange, Bcs, ExecutedTransaction, GetTransactionResponse, Transaction,
        TransactionEffects,
    };
    use sui_test_transaction_builder::TestTransactionBuilder;
    use sui_types::base_types::ObjectDigest;
    use sui_types::crypto::{AccountKeyPair, get_key_pair};
    use sui_types::effects::TestEffectsBuilder;
    use sui_types::utils::to_sender_signed_transaction;

    // Build a mock transaction data using TestTransactionBuilder
    let sender_address = SuiAddress::from_str(SENDER).unwrap();
    let recipient_address = SuiAddress::from_str(RECIPIENT).unwrap();
    let (_, sender_key): (_, AccountKeyPair) = get_key_pair();

    // Create a simple transaction using TestTransactionBuilder
    let gas_ref = (
        ObjectID::from_hex_literal(
            "0x08d6f5f85a55933fff977c94a2d1d94e8e2fff241c19c20bc5c032e0989f16a4",
        )
        .unwrap(),
        8.into(),
        ObjectDigest::from_str("dsk2WjBAbXh8oEppwavnwWmEsqRbBkSGDmVZGBaZHY6").unwrap(),
    );

    let tx_data = TestTransactionBuilder::new(sender_address, gas_ref, 1000)
        .transfer_sui(Some(AMOUNT as u64), recipient_address)
        .build();

    // Convert to SenderSignedData for TestEffectsBuilder
    let signed_tx = to_sender_signed_transaction(tx_data.clone(), &sender_key);

    // Build effects using TestEffectsBuilder
    let effects = TestEffectsBuilder::new(&signed_tx)
        .with_status(sui_types::execution_status::ExecutionStatus::Success)
        .build();

    // Serialize data to BCS
    let tx_data_bcs = bcs::to_bytes(&tx_data).unwrap();
    let effects_bcs = bcs::to_bytes(&effects).unwrap();

    // Create the gRPC response
    let mut response = GetTransactionResponse::default();

    let mut executed_transaction = ExecutedTransaction::default();
    executed_transaction.digest = Some("HavKhwo1K4QNXvvRPE8AhSYKEJSS7tmVq66Eb5Woj4ut".to_string());

    let mut transaction: Transaction = tx_data.clone().into();
    let mut tx_bcs = Bcs::default();
    tx_bcs.name = None;
    tx_bcs.value = Some(tx_data_bcs.into());
    transaction.bcs = Some(tx_bcs);
    executed_transaction.transaction = Some(transaction);

    executed_transaction.signatures = vec![];

    let mut transaction_effects: TransactionEffects = effects.clone().into();
    let mut effects_bcs_struct = Bcs::default();
    effects_bcs_struct.name = None;
    effects_bcs_struct.value = Some(effects_bcs.into());
    transaction_effects.bcs = Some(effects_bcs_struct);
    executed_transaction.effects = Some(transaction_effects);

    executed_transaction.events = None;
    executed_transaction.checkpoint = Some(1300);
    executed_transaction.timestamp = Some(::prost_types::Timestamp {
        seconds: 1736949830,
        nanos: 409000000,
    });

    let mut balance_change = BalanceChange::default();
    balance_change.address = Some(RECIPIENT.to_string());
    balance_change.coin_type = Some(
        "0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI".to_string(),
    );
    balance_change.amount = Some(AMOUNT.to_string());
    executed_transaction.balance_changes = vec![balance_change];

    response.transaction = Some(executed_transaction);

    let coin_cache = CoinMetadataCache::new(client.clone(), NonZeroUsize::new(2).unwrap());
    let executed_tx = response
        .transaction
        .expect("Response transaction should not be empty");
    let operations = Operations::try_from_executed_transaction(executed_tx, &coin_cache)
        .await
        .unwrap();

    let mut balance_changes: HashMap<SuiAddress, i128> = HashMap::new();
    operations.into_iter().for_each(|op| {
        let Some(account) = op.account else { return };
        let addr = account.address;
        let value = op.amount.map(|a| a.value).unwrap_or(0);
        if let Some(v) = balance_changes.get_mut(&addr) {
            *v += value;
            return;
        };
        balance_changes.insert(account.address, value);
    });

    assert_eq!(
        *balance_changes
            .get(&SuiAddress::from_str(RECIPIENT).unwrap())
            .unwrap(),
        AMOUNT
    );
    assert_eq!(
        *balance_changes
            .get(&SuiAddress::from_str(SENDER).unwrap())
            .unwrap_or(&0),
        0
    );
}

#[tokio::test]
async fn test_stake_with_party_objects() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;

    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    // Create party objects - convert first 3 coins to party objects
    let coins = get_all_coins(&mut client, sender).await.unwrap();
    let mut coins_to_convert = Vec::new();
    for coin in coins.iter().take(3) {
        let obj_ref = get_object_ref(&mut client, coin.id()).await.unwrap();
        coins_to_convert.push(obj_ref.as_object_ref());
    }

    create_party_objects(&mut client, sender, sender, keystore, &coins_to_convert)
        .await
        .unwrap();

    let request = GetEpochRequest::latest().with_read_mask(FieldMask::from_paths(["system_state"]));
    let response = client
        .ledger_client()
        .get_epoch(request)
        .await
        .unwrap()
        .into_inner();
    let system_state = response.epoch.and_then(|epoch| epoch.system_state).unwrap();
    let validator = system_state.validators.unwrap().active_validators[0]
        .address()
        .parse::<SuiAddress>()
        .unwrap();

    // Stake with party objects present
    // Stake amount must exceed regular coins (60M SUI) to force party coin usage
    let stake_amount = "70000000000000000"; // 70M SUI in MIST
    let ops = serde_json::from_value(json!(
        [{
            "operation_identifier":{"index":0},
            "type":"Stake",
            "account": { "address" : sender.to_string() },
            "amount" : { "value": format!("-{}", stake_amount) },
            "metadata": { "Stake" : {"validator": validator.to_string()} }
        }]
    ))
    .unwrap();

    let flow_result = rosetta_client.rosetta_flow(&ops, keystore, None).await;

    if let Some(Err(e)) = &flow_result.preprocess {
        panic!("Preprocess failed: {:?}", e);
    }
    if let Some(Err(e)) = &flow_result.metadata {
        panic!("Metadata failed: {:?}", e);
    }
    if let Some(Err(e)) = &flow_result.payloads {
        panic!("Payloads failed: {:?}", e);
    }
    if let Some(Err(e)) = &flow_result.combine {
        panic!("Combine failed: {:?}", e);
    }

    let response: TransactionIdentifierResponse = flow_result
        .submit
        .expect("Submit was None")
        .expect("Submit failed");

    wait_for_transaction(
        &mut client,
        &response.transaction_identifier.hash.to_string(),
    )
    .await
    .unwrap();

    // Verify transaction succeeded
    let grpc_request = GetTransactionRequest::default()
        .with_digest(response.transaction_identifier.hash.to_string())
        .with_read_mask(FieldMask::from_paths(["effects"]));

    let grpc_response = client
        .ledger_client()
        .get_transaction(grpc_request)
        .await
        .unwrap()
        .into_inner();

    let tx = grpc_response
        .transaction
        .expect("Response transaction should not be empty");

    assert!(
        tx.effects().status().success(),
        "Staking with party objects failed: {:?}",
        tx.effects().status().error()
    );
}

#[tokio::test]
async fn test_pay_sui_with_party_objects() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let recipient = test_cluster.get_address_1();
    let keystore = &test_cluster.wallet.config.keystore;

    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    // Create party objects - convert first 3 coins to party objects
    let coins = get_all_coins(&mut client, sender).await.unwrap();
    let mut coins_to_convert = Vec::new();
    for coin in coins.iter().take(3) {
        let obj_ref = get_object_ref(&mut client, coin.id()).await.unwrap();
        coins_to_convert.push(obj_ref.as_object_ref());
    }

    create_party_objects(&mut client, sender, sender, keystore, &coins_to_convert)
        .await
        .unwrap();

    // Pay SUI with party objects present
    // Payment amount must exceed regular coins (60M SUI) to force party coin usage
    let payment_amount = "70000000000000000"; // 70M SUI in MIST
    let ops = serde_json::from_value(json!(
        [{
            "operation_identifier":{"index":0},
            "type":"PaySui",
            "account": { "address" : recipient.to_string() },
            "amount" : { "value": payment_amount }
        },{
            "operation_identifier":{"index":1},
            "type":"PaySui",
            "account": { "address" : sender.to_string() },
            "amount" : { "value": format!("-{}", payment_amount) }
        }]
    ))
    .unwrap();

    let flow_result = rosetta_client.rosetta_flow(&ops, keystore, None).await;

    if let Some(Err(e)) = &flow_result.preprocess {
        panic!("Preprocess failed: {:?}", e);
    }
    if let Some(Err(e)) = &flow_result.metadata {
        panic!("Metadata failed: {:?}", e);
    }
    if let Some(Err(e)) = &flow_result.payloads {
        panic!("Payloads failed: {:?}", e);
    }
    if let Some(Err(e)) = &flow_result.combine {
        panic!("Combine failed: {:?}", e);
    }

    let response = flow_result
        .submit
        .expect("Submit was None")
        .expect("Submit failed");

    wait_for_transaction(
        &mut client,
        &response.transaction_identifier.hash.to_string(),
    )
    .await
    .unwrap();

    // Verify transaction succeeded
    let grpc_request = GetTransactionRequest::default()
        .with_digest(response.transaction_identifier.hash.to_string())
        .with_read_mask(FieldMask::from_paths(["effects"]));

    let grpc_response = client
        .ledger_client()
        .get_transaction(grpc_request)
        .await
        .unwrap()
        .into_inner();

    let tx = grpc_response
        .transaction
        .expect("Response transaction should not be empty");

    assert!(
        tx.effects().status().success(),
        "Pay SUI with party objects failed: {:?}",
        tx.effects().status().error()
    );
}

#[allow(dead_code)]
#[path = "custom_coins/test_coin_utils.rs"]
mod test_coin_utils_paycoin;

#[tokio::test]
async fn test_pay_coin_with_party_objects() {
    use std::path::Path;
    use test_coin_utils_paycoin::{TEST_COIN_DECIMALS, init_package, mint};

    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(36000000)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let recipient = test_cluster.get_address_1();
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let keystore = &test_cluster.wallet.config.keystore;

    // Initialize custom coin package
    let init_ret = init_package(
        &test_cluster,
        &mut client,
        keystore,
        sender,
        Path::new("tests/custom_coins/test_coin"),
    )
    .await
    .unwrap();
    let coin_type = init_ret.coin_tag.to_canonical_string(true);

    let coin_balance = 1_000_000_u64;
    let n_coins = 5_usize;
    let total_balance = n_coins as i128 * coin_balance as i128;
    let balances_to = vec![(coin_balance, sender); n_coins];

    let _mint_res = mint(
        &test_cluster,
        &mut client,
        keystore,
        init_ret.clone(),
        balances_to,
    )
    .await
    .unwrap();

    // Get the custom coins and convert some to party objects
    let coin_type_tag: sui_sdk_types::TypeTag = coin_type.parse().unwrap();
    let custom_coins = client
        .select_up_to_n_largest_coins(
            &sui_sdk_types::Address::from(sender),
            &coin_type_tag,
            5,
            &[],
        )
        .await
        .unwrap();

    let mut coins_to_convert = Vec::new();
    for coin in custom_coins.iter().take(2) {
        let obj_ref = get_object_ref(&mut client, ObjectID::from_str(coin.object_id()).unwrap())
            .await
            .unwrap();
        coins_to_convert.push(obj_ref.as_object_ref());
    }

    // Create party objects with custom coin type
    let custom_coin_type = format!("0x2::coin::Coin<{}>", coin_type);
    create_party_objects_with_type(
        &mut client,
        sender,
        sender,
        keystore,
        &coins_to_convert,
        &custom_coin_type,
    )
    .await
    .unwrap();

    // Test PayCoin with party objects present
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let ops = serde_json::from_value(json!(
        [{
            "operation_identifier":{"index":0},
            "type":"PayCoin",
            "account": { "address" : recipient.to_string() },
            "amount" : {
                "value": total_balance.to_string(),
                "currency": {
                    "symbol": "TEST_COIN",
                    "decimals": TEST_COIN_DECIMALS,
                    "metadata": {
                        "coin_type": coin_type.clone(),
                    }
                }
            },
        },
        {
            "operation_identifier":{"index":1},
            "type":"PayCoin",
            "account": { "address" : sender.to_string() },
            "amount" : {
                "value": (-total_balance).to_string(),
                "currency": {
                    "symbol": "TEST_COIN",
                    "decimals": TEST_COIN_DECIMALS,
                    "metadata": {
                        "coin_type": coin_type.clone(),
                    }
                }
            },
        }]
    ))
    .unwrap();

    let flow_result = rosetta_client.rosetta_flow(&ops, keystore, None).await;

    if let Some(Err(e)) = &flow_result.preprocess {
        panic!("Preprocess failed: {:?}", e);
    }
    if let Some(Err(e)) = &flow_result.metadata {
        panic!("Metadata failed: {:?}", e);
    }
    if let Some(Err(e)) = &flow_result.payloads {
        panic!("Payloads failed: {:?}", e);
    }
    if let Some(Err(e)) = &flow_result.combine {
        panic!("Combine failed: {:?}", e);
    }

    let response = flow_result
        .submit
        .expect("Submit was None")
        .expect("Submit failed");

    wait_for_transaction(
        &mut client,
        &response.transaction_identifier.hash.to_string(),
    )
    .await
    .unwrap();

    // Verify transaction succeeded
    let grpc_request = GetTransactionRequest::default()
        .with_digest(response.transaction_identifier.hash.to_string())
        .with_read_mask(FieldMask::from_paths(["effects"]));

    let grpc_response = client
        .ledger_client()
        .get_transaction(grpc_request)
        .await
        .unwrap()
        .into_inner();

    let tx = grpc_response
        .transaction
        .expect("Response transaction should not be empty");

    assert!(
        tx.effects().status().success(),
        "Pay custom coin with party objects failed: {:?}",
        tx.effects().status().error()
    );
}

// Helper function to fetch transaction via gRPC and parse operations
async fn fetch_transaction_and_get_operations(
    test_cluster: &test_cluster::TestCluster,
    tx_digest: sui_types::digests::TransactionDigest,
    coin_cache: &CoinMetadataCache,
) -> anyhow::Result<Operations> {
    let client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let grpc_request = GetTransactionRequest::default()
        .with_digest(tx_digest.to_string())
        .with_read_mask(FieldMask::from_paths([
            "digest",
            "transaction",
            "effects",
            "balance_changes",
            "events.events.event_type",
            "events.events.json",
            "events.events.contents",
        ]));

    let mut client = client;
    let grpc_response = client
        .ledger_client()
        .get_transaction(grpc_request)
        .await?
        .into_inner();

    let executed_tx = grpc_response
        .transaction
        .ok_or_else(|| anyhow::anyhow!("Response transaction should not be empty"))?;
    Operations::try_from_executed_transaction(executed_tx, coin_cache)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to parse operations: {}", e))
}

/// Helper function to create party objects for an address
/// Converts multiple coins to party objects owned by the specified address
async fn create_party_objects(
    client: &mut GrpcClient,
    sender: SuiAddress,
    recipient: SuiAddress,
    keystore: &impl AccountKeystore,
    coins_to_convert: &[ObjectRef],
) -> Result<()> {
    create_party_objects_with_type(
        client,
        sender,
        recipient,
        keystore,
        coins_to_convert,
        "0x2::coin::Coin<0x2::sui::SUI>",
    )
    .await
}

async fn create_party_objects_with_type(
    client: &mut GrpcClient,
    sender: SuiAddress,
    recipient: SuiAddress,
    keystore: &impl AccountKeystore,
    coins_to_convert: &[ObjectRef],
    coin_type: &str,
) -> Result<()> {
    let gas_price = client.get_reference_gas_price().await?;

    // Convert ObjectIDs to Addresses for the exclude list
    let exclude_addrs: Vec<sui_sdk_types::Address> = coins_to_convert
        .iter()
        .map(|coin_ref| sui_sdk_types::Address::from(coin_ref.0.into_bytes()))
        .collect();

    let sui_type = sui_sdk_types::TypeTag::from_str("0x2::sui::SUI")?;
    let gas_coins = client
        .select_coins(
            &sui_sdk_types::Address::from(sender),
            &sui_type,
            1,
            &exclude_addrs,
        )
        .await?;

    let gas_coin = gas_coins
        .first()
        .ok_or_else(|| anyhow!("No gas coin available outside conversion list"))?;

    let gas_object_ref = gas_coin.object_reference().try_to_object_ref()?;

    let mut builder = ProgrammableTransactionBuilder::new();

    let recipient_arg = builder.input(CallArg::Pure(bcs::to_bytes(&recipient)?))?;
    let party_owner = builder.programmable_move_call(
        "0x2".parse()?,
        Identifier::new("party")?,
        Identifier::new("single_owner")?,
        vec![],
        vec![recipient_arg],
    );

    for coin_ref in coins_to_convert {
        let coin_arg = builder.input(CallArg::Object(ObjectArg::ImmOrOwnedObject(*coin_ref)))?;
        builder.programmable_move_call(
            "0x2".parse()?,
            Identifier::new("transfer")?,
            Identifier::new("public_party_transfer")?,
            vec![coin_type.parse()?],
            vec![coin_arg, party_owner],
        );
    }

    let ptb = builder.finish();
    let tx_data = TransactionData::new_programmable(
        sender,
        vec![gas_object_ref],
        ptb,
        100_000_000,
        gas_price,
    );

    let tx = to_sender_signed_transaction(tx_data, keystore.export(&sender)?);
    let response = execute_transaction(client, &tx).await?;

    if !response.effects().status().success() {
        return Err(anyhow!(
            "Failed to create party objects: {:?}",
            response.effects().status().error()
        ));
    }

    Ok(())
}

#[tokio::test]
async fn test_network_status() -> Result<()> {
    let test_cluster = TestClusterBuilder::new().build().await;
    let mut client = GrpcClient::new(test_cluster.rpc_url())?;
    let keystore = &test_cluster.wallet.config.keystore;

    // Execute a transaction to advance past genesis checkpoint
    let sender = test_cluster.get_address_0();
    let recipient = test_cluster.get_address_1();
    let coins = get_all_coins(&mut client, sender).await?;
    let gas_object = coins.first().unwrap().compute_object_reference();

    let tx_data = TransactionData::new_transfer_sui(
        recipient,
        sender,
        Some(1),
        gas_object,
        1_000_000,
        test_cluster.get_reference_gas_price().await,
    );
    let tx = to_sender_signed_transaction(tx_data, keystore.export(&sender)?);
    execute_transaction(&mut client, &tx).await?;

    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let request = serde_json::json!({
        "network_identifier": {
            "blockchain": "sui",
            "network": "localnet"
        }
    });

    let response: serde_json::Value = rosetta_client
        .call(RosettaEndpoint::Status, &request)
        .await
        .unwrap();

    let current_block = &response["current_block_identifier"];
    assert!(current_block["index"].as_i64().unwrap() >= 0);
    assert!(response["current_block_timestamp"].as_u64().unwrap() > 0);

    let genesis_block = &response["genesis_block_identifier"];
    assert_eq!(genesis_block["index"].as_i64().unwrap(), 0);

    assert!(response["oldest_block_identifier"].is_object());

    if let Some(sync_status) = response["sync_status"].as_object() {
        let current = sync_status["current_index"].as_i64().unwrap();
        let target = sync_status["target_index"].as_i64().unwrap();
        assert!(current <= target);
        assert_eq!(sync_status["synced"].as_bool().unwrap(), current == target);
    }

    let peers = response["peers"].as_array().unwrap();
    assert!(!peers.is_empty());

    for peer in peers {
        let metadata = peer["metadata"].as_object().unwrap();
        assert!(metadata.contains_key("public_key"));
        assert!(metadata.contains_key("stake_amount"));
    }

    Ok(())
}

#[tokio::test]
async fn test_block() -> Result<()> {
    let test_cluster = TestClusterBuilder::new().build().await;
    let mut client = GrpcClient::new(test_cluster.rpc_url())?;
    let keystore = &test_cluster.wallet.config.keystore;

    let sender = test_cluster.get_address_0();
    let recipient = test_cluster.get_address_1();
    let coins = get_all_coins(&mut client, sender).await?;
    let gas_object = coins.first().unwrap().compute_object_reference();

    let tx_data = TransactionData::new_transfer_sui(
        recipient,
        sender,
        Some(1),
        gas_object,
        1_000_000,
        test_cluster.get_reference_gas_price().await,
    );
    let tx = to_sender_signed_transaction(tx_data, keystore.export(&sender)?);
    execute_transaction(&mut client, &tx).await?;

    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let request = serde_json::json!({
        "network_identifier": {
            "blockchain": "sui",
            "network": "localnet"
        },
        "block_identifier": {
            "index": 1
        }
    });

    let response: serde_json::Value = rosetta_client
        .call(RosettaEndpoint::Block, &request)
        .await
        .unwrap();

    let block = &response["block"];
    assert_eq!(block["block_identifier"]["index"].as_i64().unwrap(), 1);
    assert!(
        !block["block_identifier"]["hash"]
            .as_str()
            .unwrap()
            .is_empty()
    );
    assert!(block["timestamp"].as_u64().unwrap() > 0);

    let parent = &block["parent_block_identifier"];
    assert_eq!(parent["index"].as_i64().unwrap(), 0);

    let transactions = block["transactions"].as_array().unwrap();
    assert!(!transactions.is_empty());

    for tx in transactions {
        assert!(
            !tx["transaction_identifier"]["hash"]
                .as_str()
                .unwrap()
                .is_empty()
        );
        let operations = tx["operations"].as_array().unwrap();
        assert!(!operations.is_empty());
    }

    Ok(())
}

#[tokio::test]
async fn test_network_list() -> Result<()> {
    let test_cluster = TestClusterBuilder::new().build().await;
    let client = GrpcClient::new(test_cluster.rpc_url())?;
    let (rosetta_client, _handle) = start_rosetta_test_server(client).await;

    let request = serde_json::json!({});
    let response: serde_json::Value = rosetta_client
        .call(RosettaEndpoint::List, &request)
        .await
        .unwrap();

    let networks = response["network_identifiers"].as_array().unwrap();
    assert!(!networks.is_empty());
    assert_eq!(networks[0]["blockchain"].as_str().unwrap(), "sui");

    Ok(())
}

#[tokio::test]
async fn test_network_options() -> Result<()> {
    let test_cluster = TestClusterBuilder::new().build().await;
    let client = GrpcClient::new(test_cluster.rpc_url())?;
    let (rosetta_client, _handle) = start_rosetta_test_server(client).await;

    let request = serde_json::json!({
        "network_identifier": {
            "blockchain": "sui",
            "network": "localnet"
        }
    });

    let response: serde_json::Value = rosetta_client
        .call(RosettaEndpoint::Options, &request)
        .await
        .unwrap();

    assert!(
        !response["version"]["rosetta_version"]
            .as_str()
            .unwrap()
            .is_empty()
    );
    assert!(
        !response["allow"]["operation_statuses"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(
        !response["allow"]["operation_types"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    Ok(())
}

#[tokio::test]
async fn test_account_coins() -> Result<()> {
    let test_cluster = TestClusterBuilder::new().build().await;
    let mut client = GrpcClient::new(test_cluster.rpc_url())?;
    let keystore = &test_cluster.wallet.config.keystore;

    let sender = test_cluster.get_address_0();
    let recipient = test_cluster.get_address_1();
    let coins_before = get_all_coins(&mut client, sender).await?;
    let gas_object = coins_before.first().unwrap().compute_object_reference();

    let tx_data = TransactionData::new_transfer_sui(
        recipient,
        sender,
        Some(1),
        gas_object,
        1_000_000,
        test_cluster.get_reference_gas_price().await,
    );
    let tx = to_sender_signed_transaction(tx_data, keystore.export(&sender)?);
    execute_transaction(&mut client, &tx).await?;

    let (rosetta_client, _handle) = start_rosetta_test_server(client).await;

    let request = serde_json::json!({
        "network_identifier": {
            "blockchain": "sui",
            "network": "localnet"
        },
        "account_identifier": {
            "address": sender.to_string()
        },
        "include_mempool": false
    });

    let response: serde_json::Value = rosetta_client
        .call(RosettaEndpoint::Coins, &request)
        .await
        .unwrap();

    assert!(response.is_object());

    let block_identifier = &response["block_identifier"];
    assert!(block_identifier["index"].as_u64().unwrap() > 0);
    assert!(!block_identifier["hash"].as_str().unwrap().is_empty());

    let coins = response["coins"].as_array().unwrap();
    assert!(!coins.is_empty());

    for coin in coins {
        let coin_identifier = coin["coin_identifier"]["identifier"].as_str().unwrap();
        let coin_value = coin["amount"]["value"].as_str().unwrap();
        let currency_symbol = coin["amount"]["currency"]["symbol"].as_str().unwrap();
        let currency_decimals = coin["amount"]["currency"]["decimals"].as_u64().unwrap();

        assert!(coin_identifier.contains(':'));
        let parts: Vec<&str> = coin_identifier.split(':').collect();
        assert_eq!(parts.len(), 2);
        let object_id = parts[0];
        let version = parts[1];

        assert!(ObjectID::from_str(object_id).is_ok());
        assert!(version.parse::<u64>().is_ok());

        let value_num = coin_value.parse::<u64>().unwrap();
        assert!(value_num > 0);
        assert!(value_num <= DEFAULT_GAS_AMOUNT);

        assert_eq!(currency_symbol, "SUI");
        assert_eq!(currency_decimals, 9);
    }

    Ok(())
}

#[tokio::test]
async fn test_block_transaction() -> Result<()> {
    let test_cluster = TestClusterBuilder::new().build().await;
    let mut client = GrpcClient::new(test_cluster.rpc_url())?;
    let keystore = &test_cluster.wallet.config.keystore;

    let sender = test_cluster.get_address_0();
    let recipient = test_cluster.get_address_1();
    let coins = get_all_coins(&mut client, sender).await?;
    let gas_object = coins.first().unwrap().compute_object_reference();

    let tx_data = TransactionData::new_transfer_sui(
        recipient,
        sender,
        Some(1),
        gas_object,
        1_000_000,
        test_cluster.get_reference_gas_price().await,
    );
    let tx = to_sender_signed_transaction(tx_data, keystore.export(&sender)?);
    let executed_tx = execute_transaction(&mut client, &tx).await?;
    let tx_digest = executed_tx.transaction().digest();

    let checkpoint_1 = client
        .clone()
        .ledger_client()
        .get_checkpoint(GetCheckpointRequest::by_sequence_number(1))
        .await?
        .into_inner();
    let block_hash = checkpoint_1.checkpoint().digest();

    let (rosetta_client, _handle) = start_rosetta_test_server(client).await;

    let request = serde_json::json!({
        "network_identifier": {
            "blockchain": "sui",
            "network": "localnet"
        },
        "block_identifier": {
            "index": 1,
            "hash": block_hash
        },
        "transaction_identifier": {
            "hash": tx_digest
        }
    });

    let response: serde_json::Value = rosetta_client
        .call(RosettaEndpoint::Transaction, &request)
        .await
        .unwrap();

    assert!(response.is_object());
    assert!(response.get("transaction").is_some());
    if let Some(tx) = response["transaction"]["transaction_identifier"]["hash"].as_str() {
        assert_eq!(tx, tx_digest);
    }

    Ok(())
}

#[tokio::test]
async fn test_consolidate_all_staked_sui_to_fungible() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;

    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    // Get validator address
    let request = GetEpochRequest::latest().with_read_mask(FieldMask::from_paths(["system_state"]));
    let response = client
        .ledger_client()
        .get_epoch(request)
        .await
        .unwrap()
        .into_inner();
    let system_state = response.epoch.and_then(|epoch| epoch.system_state).unwrap();
    let validator = system_state.validators.unwrap().active_validators[0]
        .address()
        .parse::<SuiAddress>()
        .unwrap();

    // Stake 1 SUI
    let ops = serde_json::from_value(json!(
        [{
            "operation_identifier":{"index":0},
            "type":"Stake",
            "account": { "address" : sender.to_string() },
            "amount" : { "value": "-1000000000" },
            "metadata": { "Stake" : {"validator": validator.to_string()} }
        }]
    ))
    .unwrap();
    let response: TransactionIdentifierResponse = rosetta_client
        .rosetta_flow(&ops, keystore, None)
        .await
        .submit
        .unwrap()
        .unwrap();
    wait_for_transaction(
        &mut client,
        &response.transaction_identifier.hash.to_string(),
    )
    .await
    .unwrap();

    // Advance epoch so stake becomes active
    test_cluster.trigger_reconfiguration().await;

    // Consolidate: convert StakedSui → FungibleStakedSui via Rosetta construction flow
    let consolidate_ops = serde_json::from_value(json!(
        [{
            "operation_identifier": {"index": 0},
            "type": "ConsolidateAllStakedSuiToFungible",
            "account": {"address": sender.to_string()},
            "metadata": {
                "ConsolidateAllStakedSuiToFungible": {
                    "validator": validator.to_string()
                }
            }
        }]
    ))
    .unwrap();

    let response: TransactionIdentifierResponse = rosetta_client
        .rosetta_flow(&consolidate_ops, keystore, None)
        .await
        .submit
        .unwrap()
        .unwrap();

    wait_for_transaction(
        &mut client,
        &response.transaction_identifier.hash.to_string(),
    )
    .await
    .unwrap();

    // Verify: StakedSui should be gone, FungibleStakedSui should exist
    use futures::TryStreamExt;
    use sui_rpc::proto::sui::rpc::v2::ListOwnedObjectsRequest;

    let staked_sui_request = ListOwnedObjectsRequest::default()
        .with_owner(sender.to_string())
        .with_object_type("0x3::staking_pool::StakedSui".to_string())
        .with_page_size(10u32)
        .with_read_mask(FieldMask::from_paths(["object_id"]));
    let staked_sui: Vec<_> = client
        .clone()
        .list_owned_objects(staked_sui_request)
        .try_collect()
        .await
        .unwrap();
    assert!(
        staked_sui.is_empty(),
        "Expected no StakedSui objects after consolidation, found {}",
        staked_sui.len()
    );

    let fss_request = ListOwnedObjectsRequest::default()
        .with_owner(sender.to_string())
        .with_object_type("0x3::staking_pool::FungibleStakedSui".to_string())
        .with_page_size(10u32)
        .with_read_mask(FieldMask::from_paths(["object_id"]));
    let fss_objects: Vec<_> = client
        .clone()
        .list_owned_objects(fss_request)
        .try_collect()
        .await
        .unwrap();
    assert_eq!(
        fss_objects.len(),
        1,
        "Expected exactly 1 FungibleStakedSui after consolidation, found {}",
        fss_objects.len()
    );
}

/// Stake 3 times with the same validator, advance epoch, then consolidate.
/// Verifies all StakedSui objects are converted and merged into a single FSS.
#[tokio::test]
async fn test_consolidate_multiple_staked_sui() {
    use futures::TryStreamExt;
    use sui_rpc::proto::sui::rpc::v2::ListOwnedObjectsRequest;

    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;

    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let request = GetEpochRequest::latest().with_read_mask(FieldMask::from_paths(["system_state"]));
    let response = client
        .ledger_client()
        .get_epoch(request)
        .await
        .unwrap()
        .into_inner();
    let system_state = response.epoch.and_then(|epoch| epoch.system_state).unwrap();
    let validator = system_state.validators.unwrap().active_validators[0]
        .address()
        .parse::<SuiAddress>()
        .unwrap();

    // Stake 3 times with the same validator
    for _ in 0..3 {
        let ops = serde_json::from_value(json!(
            [{
                "operation_identifier":{"index":0},
                "type":"Stake",
                "account": { "address" : sender.to_string() },
                "amount" : { "value": "-1000000000" },
                "metadata": { "Stake" : {"validator": validator.to_string()} }
            }]
        ))
        .unwrap();
        let response: TransactionIdentifierResponse = rosetta_client
            .rosetta_flow(&ops, keystore, None)
            .await
            .submit
            .unwrap()
            .unwrap();
        wait_for_transaction(
            &mut client,
            &response.transaction_identifier.hash.to_string(),
        )
        .await
        .unwrap();
    }

    // Advance epoch so all 3 stakes become active
    test_cluster.trigger_reconfiguration().await;

    // Verify we have 3 StakedSui before consolidation
    let staked_sui_request = ListOwnedObjectsRequest::default()
        .with_owner(sender.to_string())
        .with_object_type("0x3::staking_pool::StakedSui".to_string())
        .with_page_size(10u32)
        .with_read_mask(FieldMask::from_paths(["object_id"]));
    let staked_sui_before: Vec<_> = client
        .clone()
        .list_owned_objects(staked_sui_request)
        .try_collect()
        .await
        .unwrap();
    assert_eq!(
        staked_sui_before.len(),
        3,
        "Expected 3 StakedSui before consolidation, found {}",
        staked_sui_before.len()
    );

    // Consolidate
    let consolidate_ops = serde_json::from_value(json!(
        [{
            "operation_identifier": {"index": 0},
            "type": "ConsolidateAllStakedSuiToFungible",
            "account": {"address": sender.to_string()},
            "metadata": {
                "ConsolidateAllStakedSuiToFungible": {
                    "validator": validator.to_string()
                }
            }
        }]
    ))
    .unwrap();

    let response: TransactionIdentifierResponse = rosetta_client
        .rosetta_flow(&consolidate_ops, keystore, None)
        .await
        .submit
        .unwrap()
        .unwrap();
    wait_for_transaction(
        &mut client,
        &response.transaction_identifier.hash.to_string(),
    )
    .await
    .unwrap();

    // Verify: no StakedSui remaining
    let staked_sui_request = ListOwnedObjectsRequest::default()
        .with_owner(sender.to_string())
        .with_object_type("0x3::staking_pool::StakedSui".to_string())
        .with_page_size(10u32)
        .with_read_mask(FieldMask::from_paths(["object_id"]));
    let staked_sui_after: Vec<_> = client
        .clone()
        .list_owned_objects(staked_sui_request)
        .try_collect()
        .await
        .unwrap();
    assert!(
        staked_sui_after.is_empty(),
        "Expected no StakedSui after consolidation, found {}",
        staked_sui_after.len()
    );

    // Verify: exactly 1 FSS
    let fss_request = ListOwnedObjectsRequest::default()
        .with_owner(sender.to_string())
        .with_object_type("0x3::staking_pool::FungibleStakedSui".to_string())
        .with_page_size(10u32)
        .with_read_mask(FieldMask::from_paths(["object_id"]));
    let fss_objects: Vec<_> = client
        .clone()
        .list_owned_objects(fss_request)
        .try_collect()
        .await
        .unwrap();
    assert_eq!(
        fss_objects.len(),
        1,
        "Expected exactly 1 FungibleStakedSui after consolidation, found {}",
        fss_objects.len()
    );
}

/// Stake once, advance epoch, manually convert to FSS, stake again, advance epoch,
/// then consolidate. Verifies the pre-existing FSS is merged with the newly
/// converted one into a single FSS.
#[tokio::test]
async fn test_consolidate_with_preexisting_fss() {
    use futures::TryStreamExt;
    use sui_rpc::proto::sui::rpc::v2::ListOwnedObjectsRequest;

    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;

    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let request = GetEpochRequest::latest().with_read_mask(FieldMask::from_paths(["system_state"]));
    let response = client
        .ledger_client()
        .get_epoch(request)
        .await
        .unwrap()
        .into_inner();
    let system_state = response.epoch.and_then(|epoch| epoch.system_state).unwrap();
    let validator = system_state.validators.unwrap().active_validators[0]
        .address()
        .parse::<SuiAddress>()
        .unwrap();

    // Stake 1 SUI
    let ops = serde_json::from_value(json!(
        [{
            "operation_identifier":{"index":0},
            "type":"Stake",
            "account": { "address" : sender.to_string() },
            "amount" : { "value": "-1000000000" },
            "metadata": { "Stake" : {"validator": validator.to_string()} }
        }]
    ))
    .unwrap();
    let response: TransactionIdentifierResponse = rosetta_client
        .rosetta_flow(&ops, keystore, None)
        .await
        .submit
        .unwrap()
        .unwrap();
    wait_for_transaction(
        &mut client,
        &response.transaction_identifier.hash.to_string(),
    )
    .await
    .unwrap();

    // Advance epoch to activate the stake
    test_cluster.trigger_reconfiguration().await;

    // Manually convert the StakedSui to FSS via a direct PTB
    let staked_sui_request = ListOwnedObjectsRequest::default()
        .with_owner(sender.to_string())
        .with_object_type("0x3::staking_pool::StakedSui".to_string())
        .with_page_size(10u32)
        .with_read_mask(FieldMask::from_paths(["object_id", "version", "digest"]));
    let staked_objs: Vec<_> = client
        .clone()
        .list_owned_objects(staked_sui_request)
        .try_collect()
        .await
        .unwrap();
    assert_eq!(staked_objs.len(), 1, "Expected 1 StakedSui to convert");

    let staked_obj = &staked_objs[0];
    let staked_ref = (
        ObjectID::from_str(staked_obj.object_id()).unwrap(),
        staked_obj.version().into(),
        staked_obj.digest().parse().unwrap(),
    );

    let gas_price = client.get_reference_gas_price().await.unwrap();
    let coins = get_all_coins(&mut client.clone(), sender).await.unwrap();
    let gas_object = get_object_ref(&mut client.clone(), coins[0].id())
        .await
        .unwrap()
        .as_object_ref();

    let mut ptb = ProgrammableTransactionBuilder::new();
    let system_state_arg = ptb.input(CallArg::SUI_SYSTEM_MUT).unwrap();
    let staked_sui_arg = ptb.obj(ObjectArg::ImmOrOwnedObject(staked_ref)).unwrap();
    let fss_result = ptb.command(Command::move_call(
        SUI_SYSTEM_PACKAGE_ID,
        SUI_SYSTEM_MODULE_NAME.to_owned(),
        Identifier::new("convert_to_fungible_staked_sui").unwrap(),
        vec![],
        vec![system_state_arg, staked_sui_arg],
    ));
    let sender_arg = ptb.pure(sender).unwrap();
    ptb.command(Command::TransferObjects(vec![fss_result], sender_arg));

    let tx_data = TransactionData::new_programmable(
        sender,
        vec![gas_object],
        ptb.finish(),
        1_000_000_000,
        gas_price,
    );
    let tx = to_sender_signed_transaction(tx_data, keystore.export(&sender).unwrap());
    execute_transaction(&mut client.clone(), &tx).await.unwrap();

    // Verify we now have 1 FSS and 0 StakedSui
    let fss_request = ListOwnedObjectsRequest::default()
        .with_owner(sender.to_string())
        .with_object_type("0x3::staking_pool::FungibleStakedSui".to_string())
        .with_page_size(10u32)
        .with_read_mask(FieldMask::from_paths(["object_id"]));
    let fss_before: Vec<_> = client
        .clone()
        .list_owned_objects(fss_request)
        .try_collect()
        .await
        .unwrap();
    assert_eq!(fss_before.len(), 1, "Expected 1 pre-existing FSS");

    // Stake again
    let ops = serde_json::from_value(json!(
        [{
            "operation_identifier":{"index":0},
            "type":"Stake",
            "account": { "address" : sender.to_string() },
            "amount" : { "value": "-1000000000" },
            "metadata": { "Stake" : {"validator": validator.to_string()} }
        }]
    ))
    .unwrap();
    let response: TransactionIdentifierResponse = rosetta_client
        .rosetta_flow(&ops, keystore, None)
        .await
        .submit
        .unwrap()
        .unwrap();
    wait_for_transaction(
        &mut client,
        &response.transaction_identifier.hash.to_string(),
    )
    .await
    .unwrap();

    // Advance epoch again
    test_cluster.trigger_reconfiguration().await;

    // Now consolidate: should merge the pre-existing FSS with the new converted StakedSui
    let consolidate_ops = serde_json::from_value(json!(
        [{
            "operation_identifier": {"index": 0},
            "type": "ConsolidateAllStakedSuiToFungible",
            "account": {"address": sender.to_string()},
            "metadata": {
                "ConsolidateAllStakedSuiToFungible": {
                    "validator": validator.to_string()
                }
            }
        }]
    ))
    .unwrap();

    let response: TransactionIdentifierResponse = rosetta_client
        .rosetta_flow(&consolidate_ops, keystore, None)
        .await
        .submit
        .unwrap()
        .unwrap();
    wait_for_transaction(
        &mut client,
        &response.transaction_identifier.hash.to_string(),
    )
    .await
    .unwrap();

    // Verify: no StakedSui remaining
    let staked_sui_request = ListOwnedObjectsRequest::default()
        .with_owner(sender.to_string())
        .with_object_type("0x3::staking_pool::StakedSui".to_string())
        .with_page_size(10u32)
        .with_read_mask(FieldMask::from_paths(["object_id"]));
    let staked_sui_after: Vec<_> = client
        .clone()
        .list_owned_objects(staked_sui_request)
        .try_collect()
        .await
        .unwrap();
    assert!(
        staked_sui_after.is_empty(),
        "Expected no StakedSui after consolidation, found {}",
        staked_sui_after.len()
    );

    // Verify: exactly 1 FSS (pre-existing merged with newly converted)
    let fss_request = ListOwnedObjectsRequest::default()
        .with_owner(sender.to_string())
        .with_object_type("0x3::staking_pool::FungibleStakedSui".to_string())
        .with_page_size(10u32)
        .with_read_mask(FieldMask::from_paths(["object_id"]));
    let fss_after: Vec<_> = client
        .clone()
        .list_owned_objects(fss_request)
        .try_collect()
        .await
        .unwrap();
    assert_eq!(
        fss_after.len(),
        1,
        "Expected exactly 1 FungibleStakedSui after consolidation, found {}",
        fss_after.len()
    );
}

/// Create 2 FSS objects by staking twice, advancing, and converting each
/// separately. Then consolidate to merge them into a single FSS (no StakedSui
/// conversion needed, only FSS merging).
#[tokio::test]
async fn test_consolidate_fss_only_merge() {
    use futures::TryStreamExt;
    use sui_rpc::proto::sui::rpc::v2::ListOwnedObjectsRequest;

    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;

    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let request = GetEpochRequest::latest().with_read_mask(FieldMask::from_paths(["system_state"]));
    let response = client
        .ledger_client()
        .get_epoch(request)
        .await
        .unwrap()
        .into_inner();
    let system_state = response.epoch.and_then(|epoch| epoch.system_state).unwrap();
    let validator = system_state.validators.unwrap().active_validators[0]
        .address()
        .parse::<SuiAddress>()
        .unwrap();

    // Stake twice
    for _ in 0..2 {
        let ops = serde_json::from_value(json!(
            [{
                "operation_identifier":{"index":0},
                "type":"Stake",
                "account": { "address" : sender.to_string() },
                "amount" : { "value": "-1000000000" },
                "metadata": { "Stake" : {"validator": validator.to_string()} }
            }]
        ))
        .unwrap();
        let response: TransactionIdentifierResponse = rosetta_client
            .rosetta_flow(&ops, keystore, None)
            .await
            .submit
            .unwrap()
            .unwrap();
        wait_for_transaction(
            &mut client,
            &response.transaction_identifier.hash.to_string(),
        )
        .await
        .unwrap();
    }

    // Advance epoch
    test_cluster.trigger_reconfiguration().await;

    // Convert each StakedSui to FSS manually, one at a time
    for _ in 0..2 {
        let staked_sui_request = ListOwnedObjectsRequest::default()
            .with_owner(sender.to_string())
            .with_object_type("0x3::staking_pool::StakedSui".to_string())
            .with_page_size(10u32)
            .with_read_mask(FieldMask::from_paths(["object_id", "version", "digest"]));
        let staked_objs: Vec<_> = client
            .clone()
            .list_owned_objects(staked_sui_request)
            .try_collect()
            .await
            .unwrap();
        if staked_objs.is_empty() {
            break;
        }

        let staked_obj = &staked_objs[0];
        let staked_ref = (
            ObjectID::from_str(staked_obj.object_id()).unwrap(),
            staked_obj.version().into(),
            staked_obj.digest().parse().unwrap(),
        );

        let gas_price = client.get_reference_gas_price().await.unwrap();
        let coins = get_all_coins(&mut client.clone(), sender).await.unwrap();
        let gas_object = get_object_ref(&mut client.clone(), coins[0].id())
            .await
            .unwrap()
            .as_object_ref();

        let mut ptb = ProgrammableTransactionBuilder::new();
        let system_state_arg = ptb.input(CallArg::SUI_SYSTEM_MUT).unwrap();
        let staked_sui_arg = ptb.obj(ObjectArg::ImmOrOwnedObject(staked_ref)).unwrap();
        let fss_result = ptb.command(Command::move_call(
            SUI_SYSTEM_PACKAGE_ID,
            SUI_SYSTEM_MODULE_NAME.to_owned(),
            Identifier::new("convert_to_fungible_staked_sui").unwrap(),
            vec![],
            vec![system_state_arg, staked_sui_arg],
        ));
        let sender_arg = ptb.pure(sender).unwrap();
        ptb.command(Command::TransferObjects(vec![fss_result], sender_arg));

        let tx_data = TransactionData::new_programmable(
            sender,
            vec![gas_object],
            ptb.finish(),
            1_000_000_000,
            gas_price,
        );
        let tx = to_sender_signed_transaction(tx_data, keystore.export(&sender).unwrap());
        execute_transaction(&mut client.clone(), &tx).await.unwrap();
    }

    // Verify we have 2 FSS and 0 StakedSui
    let fss_request = ListOwnedObjectsRequest::default()
        .with_owner(sender.to_string())
        .with_object_type("0x3::staking_pool::FungibleStakedSui".to_string())
        .with_page_size(10u32)
        .with_read_mask(FieldMask::from_paths(["object_id"]));
    let fss_before: Vec<_> = client
        .clone()
        .list_owned_objects(fss_request)
        .try_collect()
        .await
        .unwrap();
    assert_eq!(
        fss_before.len(),
        2,
        "Expected 2 FSS before consolidation, found {}",
        fss_before.len()
    );

    // Consolidate: should only merge FSS, no StakedSui to convert
    let consolidate_ops = serde_json::from_value(json!(
        [{
            "operation_identifier": {"index": 0},
            "type": "ConsolidateAllStakedSuiToFungible",
            "account": {"address": sender.to_string()},
            "metadata": {
                "ConsolidateAllStakedSuiToFungible": {
                    "validator": validator.to_string()
                }
            }
        }]
    ))
    .unwrap();

    let response: TransactionIdentifierResponse = rosetta_client
        .rosetta_flow(&consolidate_ops, keystore, None)
        .await
        .submit
        .unwrap()
        .unwrap();
    wait_for_transaction(
        &mut client,
        &response.transaction_identifier.hash.to_string(),
    )
    .await
    .unwrap();

    // Verify: exactly 1 FSS
    let fss_request = ListOwnedObjectsRequest::default()
        .with_owner(sender.to_string())
        .with_object_type("0x3::staking_pool::FungibleStakedSui".to_string())
        .with_page_size(10u32)
        .with_read_mask(FieldMask::from_paths(["object_id"]));
    let fss_after: Vec<_> = client
        .clone()
        .list_owned_objects(fss_request)
        .try_collect()
        .await
        .unwrap();
    assert_eq!(
        fss_after.len(),
        1,
        "Expected exactly 1 FungibleStakedSui after consolidation, found {}",
        fss_after.len()
    );
}

/// Stake with two different validators (A and B), advance epoch, then
/// consolidate only for validator A. Verifies only A's StakedSui is converted,
/// while B's StakedSui remains untouched.
#[tokio::test]
async fn test_consolidate_multi_validator_isolation() {
    use futures::TryStreamExt;
    use sui_rpc::proto::sui::rpc::v2::ListOwnedObjectsRequest;

    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;

    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let request = GetEpochRequest::latest().with_read_mask(FieldMask::from_paths(["system_state"]));
    let response = client
        .ledger_client()
        .get_epoch(request)
        .await
        .unwrap()
        .into_inner();
    let system_state = response.epoch.and_then(|epoch| epoch.system_state).unwrap();
    let active_validators = &system_state.validators.unwrap().active_validators;
    assert!(
        active_validators.len() >= 2,
        "Need at least 2 validators for multi-validator test, found {}",
        active_validators.len()
    );
    let validator_a = active_validators[0]
        .address()
        .parse::<SuiAddress>()
        .unwrap();
    let validator_b = active_validators[1]
        .address()
        .parse::<SuiAddress>()
        .unwrap();

    // Stake with validator A
    let ops = serde_json::from_value(json!(
        [{
            "operation_identifier":{"index":0},
            "type":"Stake",
            "account": { "address" : sender.to_string() },
            "amount" : { "value": "-1000000000" },
            "metadata": { "Stake" : {"validator": validator_a.to_string()} }
        }]
    ))
    .unwrap();
    let response: TransactionIdentifierResponse = rosetta_client
        .rosetta_flow(&ops, keystore, None)
        .await
        .submit
        .unwrap()
        .unwrap();
    wait_for_transaction(
        &mut client,
        &response.transaction_identifier.hash.to_string(),
    )
    .await
    .unwrap();

    // Stake with validator B
    let ops = serde_json::from_value(json!(
        [{
            "operation_identifier":{"index":0},
            "type":"Stake",
            "account": { "address" : sender.to_string() },
            "amount" : { "value": "-1000000000" },
            "metadata": { "Stake" : {"validator": validator_b.to_string()} }
        }]
    ))
    .unwrap();
    let response: TransactionIdentifierResponse = rosetta_client
        .rosetta_flow(&ops, keystore, None)
        .await
        .submit
        .unwrap()
        .unwrap();
    wait_for_transaction(
        &mut client,
        &response.transaction_identifier.hash.to_string(),
    )
    .await
    .unwrap();

    // Advance epoch so both stakes become active
    test_cluster.trigger_reconfiguration().await;

    // Verify we have 2 StakedSui total (one per validator)
    let staked_sui_request = ListOwnedObjectsRequest::default()
        .with_owner(sender.to_string())
        .with_object_type("0x3::staking_pool::StakedSui".to_string())
        .with_page_size(10u32)
        .with_read_mask(FieldMask::from_paths(["object_id"]));
    let staked_before: Vec<_> = client
        .clone()
        .list_owned_objects(staked_sui_request)
        .try_collect()
        .await
        .unwrap();
    assert_eq!(
        staked_before.len(),
        2,
        "Expected 2 StakedSui (one per validator), found {}",
        staked_before.len()
    );

    // Consolidate only for validator A
    let consolidate_ops = serde_json::from_value(json!(
        [{
            "operation_identifier": {"index": 0},
            "type": "ConsolidateAllStakedSuiToFungible",
            "account": {"address": sender.to_string()},
            "metadata": {
                "ConsolidateAllStakedSuiToFungible": {
                    "validator": validator_a.to_string()
                }
            }
        }]
    ))
    .unwrap();

    let response: TransactionIdentifierResponse = rosetta_client
        .rosetta_flow(&consolidate_ops, keystore, None)
        .await
        .submit
        .unwrap()
        .unwrap();
    wait_for_transaction(
        &mut client,
        &response.transaction_identifier.hash.to_string(),
    )
    .await
    .unwrap();

    // Verify: 1 StakedSui remains (validator B's), 1 FSS created (from validator A)
    let staked_sui_request = ListOwnedObjectsRequest::default()
        .with_owner(sender.to_string())
        .with_object_type("0x3::staking_pool::StakedSui".to_string())
        .with_page_size(10u32)
        .with_read_mask(FieldMask::from_paths(["object_id"]));
    let staked_after: Vec<_> = client
        .clone()
        .list_owned_objects(staked_sui_request)
        .try_collect()
        .await
        .unwrap();
    assert_eq!(
        staked_after.len(),
        1,
        "Expected 1 StakedSui remaining (validator B's), found {}",
        staked_after.len()
    );

    let fss_request = ListOwnedObjectsRequest::default()
        .with_owner(sender.to_string())
        .with_object_type("0x3::staking_pool::FungibleStakedSui".to_string())
        .with_page_size(10u32)
        .with_read_mask(FieldMask::from_paths(["object_id"]));
    let fss_objects: Vec<_> = client
        .clone()
        .list_owned_objects(fss_request)
        .try_collect()
        .await
        .unwrap();
    assert_eq!(
        fss_objects.len(),
        1,
        "Expected exactly 1 FungibleStakedSui (from validator A), found {}",
        fss_objects.len()
    );
}

/// No staking at all, then attempt consolidation. The metadata step should
/// return an error because there is nothing to consolidate.
#[tokio::test]
async fn test_consolidate_noop_no_stakes() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;

    let client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let request = GetEpochRequest::latest().with_read_mask(FieldMask::from_paths(["system_state"]));
    let response = client
        .clone()
        .ledger_client()
        .get_epoch(request)
        .await
        .unwrap()
        .into_inner();
    let system_state = response.epoch.and_then(|epoch| epoch.system_state).unwrap();
    let validator = system_state.validators.unwrap().active_validators[0]
        .address()
        .parse::<SuiAddress>()
        .unwrap();

    // Attempt to consolidate with no stakes at all
    let consolidate_ops = serde_json::from_value(json!(
        [{
            "operation_identifier": {"index": 0},
            "type": "ConsolidateAllStakedSuiToFungible",
            "account": {"address": sender.to_string()},
            "metadata": {
                "ConsolidateAllStakedSuiToFungible": {
                    "validator": validator.to_string()
                }
            }
        }]
    ))
    .unwrap();

    let flow_result = rosetta_client
        .rosetta_flow(&consolidate_ops, keystore, None)
        .await;

    // The metadata step should return an error
    assert!(
        flow_result.metadata.as_ref().is_some_and(|r| r.is_err()),
        "Expected metadata error when no stakes exist, got: {:?}",
        flow_result.metadata
    );
}

/// Convert 1 StakedSui to FSS, then attempt consolidation. With only a single
/// FSS and no StakedSui, there is nothing to consolidate, so metadata should
/// return an error.
#[tokio::test]
async fn test_consolidate_noop_single_fss() {
    use futures::TryStreamExt;
    use sui_rpc::proto::sui::rpc::v2::ListOwnedObjectsRequest;

    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;

    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let request = GetEpochRequest::latest().with_read_mask(FieldMask::from_paths(["system_state"]));
    let response = client
        .ledger_client()
        .get_epoch(request)
        .await
        .unwrap()
        .into_inner();
    let system_state = response.epoch.and_then(|epoch| epoch.system_state).unwrap();
    let validator = system_state.validators.unwrap().active_validators[0]
        .address()
        .parse::<SuiAddress>()
        .unwrap();

    // Stake 1 SUI
    let ops = serde_json::from_value(json!(
        [{
            "operation_identifier":{"index":0},
            "type":"Stake",
            "account": { "address" : sender.to_string() },
            "amount" : { "value": "-1000000000" },
            "metadata": { "Stake" : {"validator": validator.to_string()} }
        }]
    ))
    .unwrap();
    let response: TransactionIdentifierResponse = rosetta_client
        .rosetta_flow(&ops, keystore, None)
        .await
        .submit
        .unwrap()
        .unwrap();
    wait_for_transaction(
        &mut client,
        &response.transaction_identifier.hash.to_string(),
    )
    .await
    .unwrap();

    // Advance epoch
    test_cluster.trigger_reconfiguration().await;

    // Convert to FSS manually
    let staked_sui_request = ListOwnedObjectsRequest::default()
        .with_owner(sender.to_string())
        .with_object_type("0x3::staking_pool::StakedSui".to_string())
        .with_page_size(10u32)
        .with_read_mask(FieldMask::from_paths(["object_id", "version", "digest"]));
    let staked_objs: Vec<_> = client
        .clone()
        .list_owned_objects(staked_sui_request)
        .try_collect()
        .await
        .unwrap();
    assert_eq!(staked_objs.len(), 1);

    let staked_obj = &staked_objs[0];
    let staked_ref = (
        ObjectID::from_str(staked_obj.object_id()).unwrap(),
        staked_obj.version().into(),
        staked_obj.digest().parse().unwrap(),
    );

    let gas_price = client.get_reference_gas_price().await.unwrap();
    let coins = get_all_coins(&mut client.clone(), sender).await.unwrap();
    let gas_object = get_object_ref(&mut client.clone(), coins[0].id())
        .await
        .unwrap()
        .as_object_ref();

    let mut ptb = ProgrammableTransactionBuilder::new();
    let system_state_arg = ptb.input(CallArg::SUI_SYSTEM_MUT).unwrap();
    let staked_sui_arg = ptb.obj(ObjectArg::ImmOrOwnedObject(staked_ref)).unwrap();
    let fss_result = ptb.command(Command::move_call(
        SUI_SYSTEM_PACKAGE_ID,
        SUI_SYSTEM_MODULE_NAME.to_owned(),
        Identifier::new("convert_to_fungible_staked_sui").unwrap(),
        vec![],
        vec![system_state_arg, staked_sui_arg],
    ));
    let sender_arg = ptb.pure(sender).unwrap();
    ptb.command(Command::TransferObjects(vec![fss_result], sender_arg));

    let tx_data = TransactionData::new_programmable(
        sender,
        vec![gas_object],
        ptb.finish(),
        1_000_000_000,
        gas_price,
    );
    let tx = to_sender_signed_transaction(tx_data, keystore.export(&sender).unwrap());
    execute_transaction(&mut client.clone(), &tx).await.unwrap();

    // Verify we have exactly 1 FSS and 0 StakedSui
    let fss_request = ListOwnedObjectsRequest::default()
        .with_owner(sender.to_string())
        .with_object_type("0x3::staking_pool::FungibleStakedSui".to_string())
        .with_page_size(10u32)
        .with_read_mask(FieldMask::from_paths(["object_id"]));
    let fss_objects: Vec<_> = client
        .clone()
        .list_owned_objects(fss_request)
        .try_collect()
        .await
        .unwrap();
    assert_eq!(fss_objects.len(), 1, "Expected exactly 1 FSS");

    // Attempt to consolidate: single FSS + 0 StakedSui = nothing to do
    let consolidate_ops = serde_json::from_value(json!(
        [{
            "operation_identifier": {"index": 0},
            "type": "ConsolidateAllStakedSuiToFungible",
            "account": {"address": sender.to_string()},
            "metadata": {
                "ConsolidateAllStakedSuiToFungible": {
                    "validator": validator.to_string()
                }
            }
        }]
    ))
    .unwrap();

    let flow_result = rosetta_client
        .rosetta_flow(&consolidate_ops, keystore, None)
        .await;

    // The metadata step should return an error
    assert!(
        flow_result.metadata.as_ref().is_some_and(|r| r.is_err()),
        "Expected metadata error when only 1 FSS exists, got: {:?}",
        flow_result.metadata
    );
}

/// Stake without advancing epoch (stakes remain pending/unactivated), then
/// attempt consolidation. The metadata step should return an error because
/// unactivated stakes cannot be converted to FSS.
#[tokio::test]
async fn test_consolidate_unactivated_stakes_only() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;

    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let request = GetEpochRequest::latest().with_read_mask(FieldMask::from_paths(["system_state"]));
    let response = client
        .ledger_client()
        .get_epoch(request)
        .await
        .unwrap()
        .into_inner();
    let system_state = response.epoch.and_then(|epoch| epoch.system_state).unwrap();
    let validator = system_state.validators.unwrap().active_validators[0]
        .address()
        .parse::<SuiAddress>()
        .unwrap();

    // Stake 1 SUI but do NOT advance epoch
    let ops = serde_json::from_value(json!(
        [{
            "operation_identifier":{"index":0},
            "type":"Stake",
            "account": { "address" : sender.to_string() },
            "amount" : { "value": "-1000000000" },
            "metadata": { "Stake" : {"validator": validator.to_string()} }
        }]
    ))
    .unwrap();
    let response: TransactionIdentifierResponse = rosetta_client
        .rosetta_flow(&ops, keystore, None)
        .await
        .submit
        .unwrap()
        .unwrap();
    wait_for_transaction(
        &mut client,
        &response.transaction_identifier.hash.to_string(),
    )
    .await
    .unwrap();

    // Attempt to consolidate without advancing epoch
    let consolidate_ops = serde_json::from_value(json!(
        [{
            "operation_identifier": {"index": 0},
            "type": "ConsolidateAllStakedSuiToFungible",
            "account": {"address": sender.to_string()},
            "metadata": {
                "ConsolidateAllStakedSuiToFungible": {
                    "validator": validator.to_string()
                }
            }
        }]
    ))
    .unwrap();

    let flow_result = rosetta_client
        .rosetta_flow(&consolidate_ops, keystore, None)
        .await;

    // The metadata step should return an error because unactivated stakes
    // are filtered out, leaving nothing to consolidate
    assert!(
        flow_result.metadata.as_ref().is_some_and(|r| r.is_err()),
        "Expected metadata error for unactivated stakes, got: {:?}",
        flow_result.metadata
    );
}

#[tokio::test]
async fn test_fungible_staked_sui_value() -> Result<()> {
    let test_cluster = TestClusterBuilder::new().build().await;
    let address = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;

    let mut client = GrpcClient::new(test_cluster.rpc_url())?;
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let network_identifier = NetworkIdentifier {
        blockchain: "sui".to_string(),
        network: SuiEnv::LocalNet,
    };

    // Query FungibleStakedSuiValue with no FSS — should return 0
    let response: AccountBalanceResponse = rosetta_client
        .call(
            RosettaEndpoint::Balance,
            &AccountBalanceRequest {
                network_identifier: network_identifier.clone(),
                account_identifier: AccountIdentifier {
                    address,
                    sub_account: Some(SubAccount {
                        account_type: SubAccountType::FungibleStakedSuiValue,
                    }),
                },
                block_identifier: Default::default(),
                currencies: Currencies(vec![Currency::default()]),
            },
        )
        .await
        .map_err(|e| anyhow!("Rosetta client error: {e:?}"))?;
    assert_eq!(
        response.balances[0].value, 0,
        "Expected 0 FSS value for address with no FSS"
    );

    // Verify epoch timing metadata is present in sub-account response (even with zero balance)
    let metadata = response.balances[0]
        .metadata
        .as_ref()
        .expect("Expected metadata with epoch timing on zero-balance sub-account");
    assert!(
        metadata.latest_epoch.is_some(),
        "Expected latest_epoch in sub-account metadata"
    );
    assert!(
        metadata.latest_epoch_start_timestamp_ms.is_some(),
        "Expected latest_epoch_start_timestamp_ms in sub-account metadata"
    );
    assert!(
        metadata.latest_epoch_duration_ms.is_some(),
        "Expected latest_epoch_duration_ms in sub-account metadata"
    );

    // Verify epoch timing NOT in main balance
    let main_response: AccountBalanceResponse = rosetta_client
        .call(
            RosettaEndpoint::Balance,
            &AccountBalanceRequest {
                network_identifier: network_identifier.clone(),
                account_identifier: AccountIdentifier {
                    address,
                    sub_account: None,
                },
                block_identifier: Default::default(),
                currencies: Currencies(vec![Currency::default()]),
            },
        )
        .await
        .map_err(|e| anyhow!("Rosetta client error: {e:?}"))?;
    assert!(
        main_response.balances[0].metadata.is_none(),
        "Expected no metadata in main balance response"
    );

    // Stake SUI
    let epoch_request =
        GetEpochRequest::latest().with_read_mask(FieldMask::from_paths(["system_state"]));
    let epoch_response = client
        .ledger_client()
        .get_epoch(epoch_request)
        .await?
        .into_inner();
    let system_state = epoch_response
        .epoch
        .and_then(|epoch| epoch.system_state)
        .ok_or_else(|| anyhow!("Failed to get system state"))?;
    let validator = system_state
        .validators
        .ok_or_else(|| anyhow!("No validators in system state"))?
        .active_validators[0]
        .address()
        .parse::<SuiAddress>()?;

    let coins = get_all_coins(&mut client.clone(), address).await?;
    let gas_price = client.get_reference_gas_price().await?;

    let staking_coin_ref = get_object_ref(&mut client.clone(), coins[0].id()).await?;
    let gas_object = get_object_ref(&mut client.clone(), coins[1].id())
        .await?
        .as_object_ref();

    let mut ptb = ProgrammableTransactionBuilder::new();
    let arguments = vec![
        ptb.input(CallArg::SUI_SYSTEM_MUT)?,
        ptb.make_obj_vec(vec![ObjectArg::ImmOrOwnedObject(
            staking_coin_ref.as_object_ref(),
        )])?,
        ptb.pure(Some(1_000_000_000u64))?,
        ptb.pure(validator)?,
    ];
    ptb.command(Command::move_call(
        SUI_SYSTEM_PACKAGE_ID,
        SUI_SYSTEM_MODULE_NAME.to_owned(),
        ADD_STAKE_MUL_COIN_FUN_NAME.to_owned(),
        vec![],
        arguments,
    ));
    let delegation_tx = TransactionData::new_programmable(
        address,
        vec![gas_object],
        ptb.finish(),
        1_000_000_000,
        gas_price,
    );
    let tx = to_sender_signed_transaction(delegation_tx, keystore.export(&address)?);
    execute_transaction(&mut client.clone(), &tx).await?;

    // Verify activation_epoch is present in PendingStake sub-account
    let pending_response: AccountBalanceResponse = rosetta_client
        .call(
            RosettaEndpoint::Balance,
            &AccountBalanceRequest {
                network_identifier: network_identifier.clone(),
                account_identifier: AccountIdentifier {
                    address,
                    sub_account: Some(SubAccount {
                        account_type: SubAccountType::PendingStake,
                    }),
                },
                block_identifier: Default::default(),
                currencies: Currencies(vec![Currency::default()]),
            },
        )
        .await
        .map_err(|e| anyhow!("Rosetta client error: {e:?}"))?;
    assert_eq!(pending_response.balances[0].value, 1_000_000_000);
    let metadata = pending_response.balances[0]
        .metadata
        .as_ref()
        .expect("Expected metadata on PendingStake sub-account");
    assert!(
        metadata.sub_balances[0].activation_epoch.is_some(),
        "Expected activation_epoch in PendingStake sub-balance"
    );

    // Advance epoch so stake becomes active
    test_cluster.trigger_reconfiguration().await;

    // Verify activation_epoch is present in Stake sub-account
    let stake_response: AccountBalanceResponse = rosetta_client
        .call(
            RosettaEndpoint::Balance,
            &AccountBalanceRequest {
                network_identifier: network_identifier.clone(),
                account_identifier: AccountIdentifier {
                    address,
                    sub_account: Some(SubAccount {
                        account_type: SubAccountType::Stake,
                    }),
                },
                block_identifier: Default::default(),
                currencies: Currencies(vec![Currency::default()]),
            },
        )
        .await
        .map_err(|e| anyhow!("Rosetta client error: {e:?}"))?;
    assert_eq!(stake_response.balances[0].value, 1_000_000_000);
    let metadata = stake_response.balances[0]
        .metadata
        .as_ref()
        .expect("Expected metadata on Stake sub-account");
    assert!(
        metadata.sub_balances[0].activation_epoch.is_some(),
        "Expected activation_epoch in Stake sub-balance"
    );

    // Convert StakedSui to FungibleStakedSui
    // First, find the StakedSui object
    use futures::TryStreamExt;
    use sui_rpc::proto::sui::rpc::v2::ListOwnedObjectsRequest;

    let list_request = ListOwnedObjectsRequest::default()
        .with_owner(address.to_string())
        .with_object_type("0x3::staking_pool::StakedSui".to_string())
        .with_page_size(10u32)
        .with_read_mask(FieldMask::from_paths(["object_id", "version", "digest"]));

    let staked_sui_objects: Vec<_> = client
        .clone()
        .list_owned_objects(list_request)
        .map_err(|e| anyhow!("List error: {e}"))
        .try_collect()
        .await?;
    assert!(
        !staked_sui_objects.is_empty(),
        "Expected at least one StakedSui object"
    );

    let staked_obj = &staked_sui_objects[0];
    let staked_ref = (
        ObjectID::from_str(staked_obj.object_id())?,
        staked_obj.version().into(),
        staked_obj.digest().parse()?,
    );

    // Build PTB to convert to FSS
    let gas_coins = get_all_coins(&mut client.clone(), address).await?;
    let gas_ref = get_object_ref(&mut client.clone(), gas_coins[0].id())
        .await?
        .as_object_ref();

    let mut ptb = ProgrammableTransactionBuilder::new();
    let system_state_arg = ptb.input(CallArg::SUI_SYSTEM_MUT)?;
    let staked_sui_arg = ptb.obj(ObjectArg::ImmOrOwnedObject(staked_ref))?;
    let fss_result = ptb.command(Command::move_call(
        SUI_SYSTEM_PACKAGE_ID,
        SUI_SYSTEM_MODULE_NAME.to_owned(),
        Identifier::new("convert_to_fungible_staked_sui")?,
        vec![],
        vec![system_state_arg, staked_sui_arg],
    ));
    let sender_arg = ptb.pure(address)?;
    ptb.command(Command::TransferObjects(vec![fss_result], sender_arg));

    let convert_tx = TransactionData::new_programmable(
        address,
        vec![gas_ref],
        ptb.finish(),
        1_000_000_000,
        gas_price,
    );
    let tx = to_sender_signed_transaction(convert_tx, keystore.export(&address)?);
    let response = execute_transaction(&mut client.clone(), &tx).await?;
    assert!(
        response.effects().status().success(),
        "Convert to FSS failed: {:?}",
        response.effects().status().error()
    );

    // Now query FungibleStakedSuiValue — should be > 0
    let fss_response: AccountBalanceResponse = rosetta_client
        .call(
            RosettaEndpoint::Balance,
            &AccountBalanceRequest {
                network_identifier: network_identifier.clone(),
                account_identifier: AccountIdentifier {
                    address,
                    sub_account: Some(SubAccount {
                        account_type: SubAccountType::FungibleStakedSuiValue,
                    }),
                },
                block_identifier: Default::default(),
                currencies: Currencies(vec![Currency::default()]),
            },
        )
        .await
        .map_err(|e| anyhow!("Rosetta client error: {e:?}"))?;

    assert!(
        fss_response.balances[0].value > 0,
        "Expected positive FungibleStakedSuiValue, got {}",
        fss_response.balances[0].value
    );
    // The value should be approximately 1 SUI (1_000_000_000 MIST) since this is a fresh pool with rate ~1.0
    assert!(
        fss_response.balances[0].value >= 999_000_000
            && fss_response.balances[0].value <= 1_100_000_000,
        "Expected FSS value close to 1 SUI, got {}",
        fss_response.balances[0].value
    );

    // Verify epoch timing is also present
    let metadata = fss_response.balances[0]
        .metadata
        .as_ref()
        .expect("Expected metadata on FungibleStakedSuiValue sub-account");
    assert!(metadata.latest_epoch.is_some());
    assert!(metadata.latest_epoch_start_timestamp_ms.is_some());
    assert!(metadata.latest_epoch_duration_ms.is_some());

    // Verify existing Stake sub-account now has 0 (all converted)
    let stake_after_response: AccountBalanceResponse = rosetta_client
        .call(
            RosettaEndpoint::Balance,
            &AccountBalanceRequest {
                network_identifier: network_identifier.clone(),
                account_identifier: AccountIdentifier {
                    address,
                    sub_account: Some(SubAccount {
                        account_type: SubAccountType::Stake,
                    }),
                },
                block_identifier: Default::default(),
                currencies: Currencies(vec![Currency::default()]),
            },
        )
        .await
        .map_err(|e| anyhow!("Rosetta client error: {e:?}"))?;
    assert_eq!(
        stake_after_response.balances[0].value, 0,
        "Expected 0 Stake balance after converting all to FSS"
    );

    Ok(())
}

#[tokio::test]
async fn test_merge_and_redeem_fungible_staked_sui() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;

    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    // Get validator
    let request = GetEpochRequest::latest().with_read_mask(FieldMask::from_paths(["system_state"]));
    let response = client
        .ledger_client()
        .get_epoch(request)
        .await
        .unwrap()
        .into_inner();
    let system_state = response.epoch.and_then(|epoch| epoch.system_state).unwrap();
    let validator = system_state.validators.unwrap().active_validators[0]
        .address()
        .parse::<SuiAddress>()
        .unwrap();

    // Stake 2 SUI
    let ops = serde_json::from_value(json!(
        [{
            "operation_identifier":{"index":0},
            "type":"Stake",
            "account": { "address" : sender.to_string() },
            "amount" : { "value": "-2000000000" },
            "metadata": { "Stake" : {"validator": validator.to_string()} }
        }]
    ))
    .unwrap();
    let response: TransactionIdentifierResponse = rosetta_client
        .rosetta_flow(&ops, keystore, None)
        .await
        .submit
        .unwrap()
        .unwrap();
    wait_for_transaction(
        &mut client,
        &response.transaction_identifier.hash.to_string(),
    )
    .await
    .unwrap();

    // Advance epoch
    test_cluster.trigger_reconfiguration().await;

    // Convert StakedSui -> FSS
    use futures::TryStreamExt;
    use sui_rpc::proto::sui::rpc::v2::ListOwnedObjectsRequest;

    let list_request = ListOwnedObjectsRequest::default()
        .with_owner(sender.to_string())
        .with_object_type("0x3::staking_pool::StakedSui".to_string())
        .with_page_size(10u32)
        .with_read_mask(FieldMask::from_paths(["object_id", "version", "digest"]));
    let staked_sui_objects: Vec<_> = client
        .clone()
        .list_owned_objects(list_request)
        .map_err(|e| panic!("List error: {e}"))
        .try_collect()
        .await
        .unwrap();
    assert!(!staked_sui_objects.is_empty());

    let staked_obj = &staked_sui_objects[0];
    let staked_ref = (
        ObjectID::from_str(staked_obj.object_id()).unwrap(),
        staked_obj.version().into(),
        staked_obj.digest().parse().unwrap(),
    );

    let gas_coins = get_all_coins(&mut client.clone(), sender).await.unwrap();
    let gas_ref = get_object_ref(&mut client.clone(), gas_coins[0].id())
        .await
        .unwrap()
        .as_object_ref();
    let gas_price = client.get_reference_gas_price().await.unwrap();

    let mut ptb = ProgrammableTransactionBuilder::new();
    let system_state_arg = ptb.input(CallArg::SUI_SYSTEM_MUT).unwrap();
    let staked_sui_arg = ptb.obj(ObjectArg::ImmOrOwnedObject(staked_ref)).unwrap();
    let fss_result = ptb.command(Command::move_call(
        SUI_SYSTEM_PACKAGE_ID,
        SUI_SYSTEM_MODULE_NAME.to_owned(),
        Identifier::new("convert_to_fungible_staked_sui").unwrap(),
        vec![],
        vec![system_state_arg, staked_sui_arg],
    ));
    let sender_arg = ptb.pure(sender).unwrap();
    ptb.command(Command::TransferObjects(vec![fss_result], sender_arg));

    let convert_tx = TransactionData::new_programmable(
        sender,
        vec![gas_ref],
        ptb.finish(),
        1_000_000_000,
        gas_price,
    );
    let tx = sui_types::utils::to_sender_signed_transaction(
        convert_tx,
        keystore.export(&sender).unwrap(),
    );
    let convert_response = execute_transaction(&mut client.clone(), &tx).await.unwrap();
    assert!(
        convert_response.effects().status().success(),
        "Convert to FSS failed"
    );

    // Redeem all FSS via Rosetta
    let redeem_ops = serde_json::from_value(json!(
        [{
            "operation_identifier": {"index": 0},
            "type": "MergeAndRedeemFungibleStakedSui",
            "account": {"address": sender.to_string()},
            "metadata": {
                "MergeAndRedeemFungibleStakedSui": {
                    "validator": validator.to_string(),
                    "redeem_mode": "All"
                }
            }
        }]
    ))
    .unwrap();

    let flow_result = rosetta_client
        .rosetta_flow(&redeem_ops, keystore, None)
        .await;
    if let Some(Err(e)) = &flow_result.preprocess {
        panic!("Redeem preprocess step failed: {:?}", e);
    }
    if let Some(Err(e)) = &flow_result.metadata {
        panic!("Redeem metadata step failed: {:?}", e);
    }
    if let Some(Err(e)) = &flow_result.payloads {
        panic!("Redeem payloads step failed: {:?}", e);
    }
    if let Some(Err(e)) = &flow_result.combine {
        panic!("Redeem combine step failed: {:?}", e);
    }
    let response: TransactionIdentifierResponse = flow_result
        .submit
        .unwrap_or_else(|| {
            panic!(
                "Submit was None. preprocess: {:?}, metadata: {:?}, payloads: {:?}, combine: {:?}",
                flow_result.preprocess,
                flow_result.metadata,
                flow_result.payloads,
                flow_result.combine
            )
        })
        .expect("Submit should succeed");

    wait_for_transaction(
        &mut client,
        &response.transaction_identifier.hash.to_string(),
    )
    .await
    .unwrap();

    // Verify: no FSS remaining
    let fss_request = ListOwnedObjectsRequest::default()
        .with_owner(sender.to_string())
        .with_object_type("0x3::staking_pool::FungibleStakedSui".to_string())
        .with_page_size(10u32)
        .with_read_mask(FieldMask::from_paths(["object_id"]));
    let fss_remaining: Vec<_> = client
        .clone()
        .list_owned_objects(fss_request)
        .try_collect()
        .await
        .unwrap();
    assert!(
        fss_remaining.is_empty(),
        "Expected no FSS after full redeem, found {}",
        fss_remaining.len()
    );
}

async fn setup_fss_for_validator(
    stake_amount: u64,
) -> (
    test_cluster::TestCluster,
    GrpcClient,
    rosetta_client::RosettaClient,
    SuiAddress,
    SuiAddress,
    Vec<tokio::task::JoinHandle<()>>,
) {
    use futures::TryStreamExt;
    use sui_rpc::proto::sui::rpc::v2::ListOwnedObjectsRequest;

    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, handles) = start_rosetta_test_server(client.clone()).await;

    let request = GetEpochRequest::latest().with_read_mask(FieldMask::from_paths(["system_state"]));
    let response = client
        .ledger_client()
        .get_epoch(request)
        .await
        .unwrap()
        .into_inner();
    let system_state = response.epoch.and_then(|epoch| epoch.system_state).unwrap();
    let validator = system_state.validators.unwrap().active_validators[0]
        .address()
        .parse::<SuiAddress>()
        .unwrap();

    let ops = serde_json::from_value(json!(
        [{
            "operation_identifier":{"index":0},
            "type":"Stake",
            "account": { "address" : sender.to_string() },
            "amount" : { "value": format!("-{}", stake_amount) },
            "metadata": { "Stake" : {"validator": validator.to_string()} }
        }]
    ))
    .unwrap();
    let response: TransactionIdentifierResponse = rosetta_client
        .rosetta_flow(&ops, keystore, None)
        .await
        .submit
        .unwrap()
        .unwrap();
    wait_for_transaction(
        &mut client,
        &response.transaction_identifier.hash.to_string(),
    )
    .await
    .unwrap();

    test_cluster.trigger_reconfiguration().await;

    let list_request = ListOwnedObjectsRequest::default()
        .with_owner(sender.to_string())
        .with_object_type("0x3::staking_pool::StakedSui".to_string())
        .with_page_size(10u32)
        .with_read_mask(FieldMask::from_paths(["object_id", "version", "digest"]));
    let staked_sui_objects: Vec<_> = client
        .clone()
        .list_owned_objects(list_request)
        .map_err(|e| panic!("List error: {e}"))
        .try_collect()
        .await
        .unwrap();
    assert!(!staked_sui_objects.is_empty());

    let staked_obj = &staked_sui_objects[0];
    let staked_ref = (
        ObjectID::from_str(staked_obj.object_id()).unwrap(),
        staked_obj.version().into(),
        staked_obj.digest().parse().unwrap(),
    );

    let gas_coins = get_all_coins(&mut client.clone(), sender).await.unwrap();
    let gas_ref = get_object_ref(&mut client.clone(), gas_coins[0].id())
        .await
        .unwrap()
        .as_object_ref();
    let gas_price = client.get_reference_gas_price().await.unwrap();

    let mut ptb = ProgrammableTransactionBuilder::new();
    let system_state_arg = ptb.input(CallArg::SUI_SYSTEM_MUT).unwrap();
    let staked_sui_arg = ptb.obj(ObjectArg::ImmOrOwnedObject(staked_ref)).unwrap();
    let fss_result = ptb.command(Command::move_call(
        SUI_SYSTEM_PACKAGE_ID,
        SUI_SYSTEM_MODULE_NAME.to_owned(),
        Identifier::new("convert_to_fungible_staked_sui").unwrap(),
        vec![],
        vec![system_state_arg, staked_sui_arg],
    ));
    let sender_arg = ptb.pure(sender).unwrap();
    ptb.command(Command::TransferObjects(vec![fss_result], sender_arg));

    let convert_tx = TransactionData::new_programmable(
        sender,
        vec![gas_ref],
        ptb.finish(),
        1_000_000_000,
        gas_price,
    );
    let tx = to_sender_signed_transaction(convert_tx, keystore.export(&sender).unwrap());
    let convert_response = execute_transaction(&mut client.clone(), &tx).await.unwrap();
    assert!(
        convert_response.effects().status().success(),
        "Convert to FSS failed"
    );

    (
        test_cluster,
        client,
        rosetta_client,
        sender,
        validator,
        handles,
    )
}

async fn run_redeem_flow(
    client: &mut GrpcClient,
    rosetta_client: &rosetta_client::RosettaClient,
    keystore: &sui_keys::keystore::Keystore,
    sender: SuiAddress,
    validator: SuiAddress,
    redeem_mode: &str,
    amount: Option<u64>,
) -> TransactionIdentifierResponse {
    let metadata = if let Some(amt) = amount {
        json!({
            "MergeAndRedeemFungibleStakedSui": {
                "validator": validator.to_string(),
                "amount": amt.to_string(),
                "redeem_mode": redeem_mode
            }
        })
    } else {
        json!({
            "MergeAndRedeemFungibleStakedSui": {
                "validator": validator.to_string(),
                "redeem_mode": redeem_mode
            }
        })
    };

    let redeem_ops = serde_json::from_value(json!(
        [{
            "operation_identifier": {"index": 0},
            "type": "MergeAndRedeemFungibleStakedSui",
            "account": {"address": sender.to_string()},
            "metadata": metadata
        }]
    ))
    .unwrap();

    let flow_result = rosetta_client
        .rosetta_flow(&redeem_ops, keystore, None)
        .await;
    if let Some(Err(e)) = &flow_result.preprocess {
        panic!("Redeem preprocess failed: {:?}", e);
    }
    if let Some(Err(e)) = &flow_result.metadata {
        panic!("Redeem metadata failed: {:?}", e);
    }
    if let Some(Err(e)) = &flow_result.payloads {
        panic!("Redeem payloads failed: {:?}", e);
    }
    if let Some(Err(e)) = &flow_result.combine {
        panic!("Redeem combine failed: {:?}", e);
    }
    let response: TransactionIdentifierResponse = flow_result
        .submit
        .unwrap_or_else(|| {
            panic!(
                "Submit was None. preprocess: {:?}, metadata: {:?}, payloads: {:?}, combine: {:?}",
                flow_result.preprocess,
                flow_result.metadata,
                flow_result.payloads,
                flow_result.combine
            )
        })
        .expect("Submit should succeed");

    wait_for_transaction(client, &response.transaction_identifier.hash.to_string())
        .await
        .unwrap();

    response
}

async fn count_fss_objects(client: &mut GrpcClient, owner: SuiAddress) -> (usize, u64) {
    use futures::TryStreamExt;
    use sui_rpc::proto::sui::rpc::v2::ListOwnedObjectsRequest;

    let fss_request = ListOwnedObjectsRequest::default()
        .with_owner(owner.to_string())
        .with_object_type("0x3::staking_pool::FungibleStakedSui".to_string())
        .with_page_size(100u32)
        .with_read_mask(FieldMask::from_paths(["object_id", "contents"]));
    let objects: Vec<_> = client
        .clone()
        .list_owned_objects(fss_request)
        .map_err(|e| panic!("List FSS error: {e}"))
        .try_collect()
        .await
        .unwrap();

    let mut total_value: u64 = 0;
    for obj in &objects {
        if let Some(contents) = &obj.contents {
            #[derive(serde::Deserialize)]
            struct FssBcs {
                _id: sui_sdk_types::Address,
                _pool_id: sui_sdk_types::Address,
                value: u64,
            }
            if let Ok(fss) = contents.deserialize::<FssBcs>() {
                total_value += fss.value;
            }
        }
    }
    (objects.len(), total_value)
}

async fn get_sui_balance(client: &mut GrpcClient, address: SuiAddress) -> u64 {
    use sui_rpc::proto::sui::rpc::v2::GetBalanceRequest;

    let request = GetBalanceRequest::default()
        .with_owner(address.to_string())
        .with_coin_type(
            "0x0000000000000000000000000000000000000000000000000000000000000002::sui::SUI"
                .to_string(),
        );
    client
        .state_client()
        .get_balance(request)
        .await
        .unwrap()
        .into_inner()
        .balance
        .and_then(|b| b.balance)
        .unwrap_or(0)
}

#[tokio::test]
async fn test_redeem_at_least_mode() {
    let (test_cluster, mut client, rosetta_client, sender, validator, _handles) =
        setup_fss_for_validator(2_000_000_000).await;
    let keystore = &test_cluster.wallet.config.keystore;

    let balance_before = get_sui_balance(&mut client, sender).await;

    run_redeem_flow(
        &mut client,
        &rosetta_client,
        keystore,
        sender,
        validator,
        "AtLeast",
        Some(1_000_000_000),
    )
    .await;

    let balance_after = get_sui_balance(&mut client, sender).await;
    assert!(
        balance_after > balance_before,
        "Balance should have increased after AtLeast redeem: before={}, after={}",
        balance_before,
        balance_after
    );

    let (fss_count, fss_value) = count_fss_objects(&mut client, sender).await;
    assert!(fss_count > 0, "FSS should still exist after partial redeem");
    assert!(fss_value > 0, "Remaining FSS should have non-zero value");
}

#[tokio::test]
async fn test_redeem_at_most_mode() {
    let (test_cluster, mut client, rosetta_client, sender, validator, _handles) =
        setup_fss_for_validator(2_000_000_000).await;
    let keystore = &test_cluster.wallet.config.keystore;

    let balance_before = get_sui_balance(&mut client, sender).await;

    run_redeem_flow(
        &mut client,
        &rosetta_client,
        keystore,
        sender,
        validator,
        "AtMost",
        Some(1_000_000_000),
    )
    .await;

    let balance_after = get_sui_balance(&mut client, sender).await;
    assert!(
        balance_after > balance_before,
        "Balance should have increased after AtMost redeem: before={}, after={}",
        balance_before,
        balance_after
    );

    let (fss_count, _fss_value) = count_fss_objects(&mut client, sender).await;
    assert!(
        fss_count > 0,
        "FSS should still exist after partial AtMost redeem"
    );
}

#[tokio::test]
async fn test_redeem_single_fss_partial() {
    let (test_cluster, mut client, rosetta_client, sender, validator, _handles) =
        setup_fss_for_validator(2_000_000_000).await;
    let keystore = &test_cluster.wallet.config.keystore;

    let (fss_count_before, fss_value_before) = count_fss_objects(&mut client, sender).await;
    assert_eq!(
        fss_count_before, 1,
        "Should start with exactly 1 FSS object"
    );
    assert!(fss_value_before > 0, "FSS should have non-zero value");

    run_redeem_flow(
        &mut client,
        &rosetta_client,
        keystore,
        sender,
        validator,
        "AtLeast",
        Some(1_000_000_000),
    )
    .await;

    let (fss_count_after, fss_value_after) = count_fss_objects(&mut client, sender).await;
    assert!(
        fss_count_after > 0,
        "FSS should still exist after partial redeem of single FSS"
    );
    assert!(
        fss_value_after > 0 && fss_value_after < fss_value_before,
        "Remaining FSS value ({}) should be between 0 and original value ({})",
        fss_value_after,
        fss_value_before
    );
}

/// Redeem FSS from a validator that has been moved to `inactive_validators`.
///
/// Exercises the full inactive code path: FSS-first pool resolution, the
/// `inactive_validators[pool_id]` dynamic field walk, the `Versioned →
/// ValidatorV1` decode, and the `pool_token_exchange_rate_at_epoch` walk-back
/// against the pool's `exchange_rates` table. The chain accepts inactive
/// pools at the same `redeem_fungible_staked_sui` entry point
/// (`validator_set.move:346-364`) so any layout/derive bug in the Rosetta
/// inactive code surfaces as a metadata or submit failure here.
///
/// Default genesis sets `min_validator_count = 4`, and `request_remove_validator`
/// asserts `next_epoch_validator_count() > min_validator_count` (strictly
/// greater). We start with 6 validators so removing one leaves 5 > 4 and
/// the remove succeeds.
#[tokio::test]
async fn test_redeem_fss_from_inactive_validator() {
    use futures::TryStreamExt;
    use sui_rpc::proto::sui::rpc::v2::ListOwnedObjectsRequest;
    use sui_test_transaction_builder::TestTransactionBuilder;

    let test_cluster = TestClusterBuilder::new()
        .with_num_validators(6)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handles) = start_rosetta_test_server(client.clone()).await;
    let validator = first_validator(&mut client).await;

    // Stake → activate → convert StakedSui to FSS so the sender holds an
    // FSS object whose pool_id == `validator`'s staking pool.
    stake_via_rosetta(
        &rosetta_client,
        &mut client,
        keystore,
        sender,
        validator,
        2_000_000_000,
    )
    .await;
    test_cluster.trigger_reconfiguration().await;

    let staked_req = ListOwnedObjectsRequest::default()
        .with_owner(sender.to_string())
        .with_object_type("0x3::staking_pool::StakedSui".to_string())
        .with_page_size(10u32)
        .with_read_mask(FieldMask::from_paths(["object_id", "version", "digest"]));
    let staked_objects: Vec<_> = client
        .clone()
        .list_owned_objects(staked_req)
        .map_err(|e| panic!("list error: {e}"))
        .try_collect()
        .await
        .unwrap();
    let staked_obj = &staked_objects[0];
    let staked_ref = (
        ObjectID::from_str(staked_obj.object_id()).unwrap(),
        staked_obj.version().into(),
        staked_obj.digest().parse().unwrap(),
    );
    let gas_coins = get_all_coins(&mut client.clone(), sender).await.unwrap();
    let gas_ref = get_object_ref(&mut client.clone(), gas_coins[0].id())
        .await
        .unwrap()
        .as_object_ref();
    let gas_price = client.get_reference_gas_price().await.unwrap();
    let mut ptb = ProgrammableTransactionBuilder::new();
    let sys = ptb.input(CallArg::SUI_SYSTEM_MUT).unwrap();
    let staked_arg = ptb.obj(ObjectArg::ImmOrOwnedObject(staked_ref)).unwrap();
    let fss_result = ptb.command(Command::move_call(
        SUI_SYSTEM_PACKAGE_ID,
        SUI_SYSTEM_MODULE_NAME.to_owned(),
        Identifier::new("convert_to_fungible_staked_sui").unwrap(),
        vec![],
        vec![sys, staked_arg],
    ));
    let sender_arg = ptb.pure(sender).unwrap();
    ptb.command(Command::TransferObjects(vec![fss_result], sender_arg));
    let convert_tx = TransactionData::new_programmable(
        sender,
        vec![gas_ref],
        ptb.finish(),
        1_000_000_000,
        gas_price,
    );
    let tx = to_sender_signed_transaction(convert_tx, keystore.export(&sender).unwrap());
    execute_transaction(&mut client.clone(), &tx)
        .await
        .expect("convert StakedSui → FSS");

    // Have the validator request its own removal (signed by validator key).
    let validator_handle = test_cluster
        .swarm
        .validator_node_handles()
        .into_iter()
        .find(|h| h.with(|n| n.get_config().sui_address()) == validator)
        .expect("validator node handle for active_validators[0]");
    let validator_addr = validator_handle.with(|n| n.get_config().sui_address());
    let validator_gas = test_cluster
        .wallet
        .get_one_gas_object_owned_by_address(validator_addr)
        .await
        .unwrap()
        .unwrap();
    let rgp = test_cluster.get_reference_gas_price().await;
    let remove_tx = validator_handle.with(|n| {
        TestTransactionBuilder::new(validator_addr, validator_gas, rgp)
            .call_request_remove_validator()
            .build_and_sign(n.get_config().account_key_pair.keypair())
    });
    test_cluster.execute_transaction(remove_tx).await;

    // Trigger reconfiguration so the validator moves to inactive_validators.
    // After this, `system_state.validators.active_validators` no longer
    // contains it; only `inactive_validators[pool_id]` does.
    test_cluster.trigger_reconfiguration().await;

    let balance_before = get_sui_balance(&mut client, sender).await;

    // AtLeast is the strictest mode — it forces the inactive path to fetch
    // FSS data via dynamic-field walk, mirror the chain's payout formula
    // (which itself reads exchange_rates[deactivation_epoch]), and bind the
    // resulting transaction to the quote epoch. If any of those layers is
    // wrong this fails before submission.
    let min_sui = 500_000_000i128;
    let response = run_redeem_flow(
        &mut client,
        &rosetta_client,
        keystore,
        sender,
        validator,
        "AtLeast",
        Some(min_sui as u64),
    )
    .await;

    // The chain's AtLeast guarantee is on the *gross* SUI delivered by
    // `redeem_fungible_staked_sui`, not the net wallet delta — gas comes out
    // of the same address and would push the net delta below `min_sui` even
    // when the protocol behaved correctly. Reconstruct the gross amount as
    // `(balance_after - balance_before) + (computation_cost + storage_cost - storage_rebate)`.
    let balance_after = get_sui_balance(&mut client, sender).await;
    let net_delta = balance_after as i128 - balance_before as i128;
    let gas_net_cost = transaction_net_gas_cost(
        &mut client,
        &response.transaction_identifier.hash.to_string(),
    )
    .await;
    let gross_redeemed = net_delta + gas_net_cost;
    assert!(
        gross_redeemed >= min_sui,
        "AtLeast guarantee violated for inactive-pool redeem: \
         gross_redeemed = {gross_redeemed} MIST (net_delta = {net_delta}, \
         gas_net_cost = {gas_net_cost}), expected >= {min_sui}"
    );
}

/// Read `effects.gas_used` for a transaction and return the net gas cost
/// (`computation_cost + storage_cost - storage_rebate`) as i128.
///
/// `i128` matters because storage_rebate can in principle exceed
/// `computation_cost + storage_cost` (rare but allowed by the gas model);
/// using u64 with saturating subtraction would silently round to zero.
async fn transaction_net_gas_cost(client: &mut GrpcClient, tx_hash: &str) -> i128 {
    let request = GetTransactionRequest::default()
        .with_digest(tx_hash.to_string())
        .with_read_mask(FieldMask::from_paths(["effects.gas_used"]));
    let tx = client
        .clone()
        .ledger_client()
        .get_transaction(request)
        .await
        .expect("get_transaction RPC")
        .into_inner()
        .transaction
        .expect("transaction should be present in response");
    let gas = tx.effects().gas_used();
    gas.computation_cost_opt().unwrap_or(0) as i128 + gas.storage_cost_opt().unwrap_or(0) as i128
        - gas.storage_rebate_opt().unwrap_or(0) as i128
}

#[tokio::test]
async fn test_redeem_multi_validator_isolation() {
    use futures::TryStreamExt;
    use sui_rpc::proto::sui::rpc::v2::ListOwnedObjectsRequest;

    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handles) = start_rosetta_test_server(client.clone()).await;

    let request = GetEpochRequest::latest().with_read_mask(FieldMask::from_paths(["system_state"]));
    let response = client
        .ledger_client()
        .get_epoch(request)
        .await
        .unwrap()
        .into_inner();
    let system_state = response.epoch.and_then(|epoch| epoch.system_state).unwrap();
    let active_validators = &system_state.validators.unwrap().active_validators;
    assert!(
        active_validators.len() >= 2,
        "Need at least 2 validators for this test, found {}",
        active_validators.len()
    );
    let validator_a = active_validators[0]
        .address()
        .parse::<SuiAddress>()
        .unwrap();
    let validator_b = active_validators[1]
        .address()
        .parse::<SuiAddress>()
        .unwrap();

    let ops_a = serde_json::from_value(json!(
        [{
            "operation_identifier":{"index":0},
            "type":"Stake",
            "account": { "address" : sender.to_string() },
            "amount" : { "value": "-2000000000" },
            "metadata": { "Stake" : {"validator": validator_a.to_string()} }
        }]
    ))
    .unwrap();
    let resp_a: TransactionIdentifierResponse = rosetta_client
        .rosetta_flow(&ops_a, keystore, None)
        .await
        .submit
        .unwrap()
        .unwrap();
    wait_for_transaction(&mut client, &resp_a.transaction_identifier.hash.to_string())
        .await
        .unwrap();

    let ops_b = serde_json::from_value(json!(
        [{
            "operation_identifier":{"index":0},
            "type":"Stake",
            "account": { "address" : sender.to_string() },
            "amount" : { "value": "-2000000000" },
            "metadata": { "Stake" : {"validator": validator_b.to_string()} }
        }]
    ))
    .unwrap();
    let resp_b: TransactionIdentifierResponse = rosetta_client
        .rosetta_flow(&ops_b, keystore, None)
        .await
        .submit
        .unwrap()
        .unwrap();
    wait_for_transaction(&mut client, &resp_b.transaction_identifier.hash.to_string())
        .await
        .unwrap();

    test_cluster.trigger_reconfiguration().await;

    let list_request = ListOwnedObjectsRequest::default()
        .with_owner(sender.to_string())
        .with_object_type("0x3::staking_pool::StakedSui".to_string())
        .with_page_size(10u32)
        .with_read_mask(FieldMask::from_paths(["object_id", "version", "digest"]));
    let staked_sui_objects: Vec<_> = client
        .clone()
        .list_owned_objects(list_request)
        .map_err(|e| panic!("List error: {e}"))
        .try_collect()
        .await
        .unwrap();
    assert!(staked_sui_objects.len() >= 2);

    let gas_price = client.get_reference_gas_price().await.unwrap();

    for staked_obj in &staked_sui_objects {
        let staked_ref = (
            ObjectID::from_str(staked_obj.object_id()).unwrap(),
            staked_obj.version().into(),
            staked_obj.digest().parse().unwrap(),
        );
        let gas_coins = get_all_coins(&mut client.clone(), sender).await.unwrap();
        let gas_ref = get_object_ref(&mut client.clone(), gas_coins[0].id())
            .await
            .unwrap()
            .as_object_ref();

        let mut ptb = ProgrammableTransactionBuilder::new();
        let system_state_arg = ptb.input(CallArg::SUI_SYSTEM_MUT).unwrap();
        let staked_sui_arg = ptb.obj(ObjectArg::ImmOrOwnedObject(staked_ref)).unwrap();
        let fss_result = ptb.command(Command::move_call(
            SUI_SYSTEM_PACKAGE_ID,
            SUI_SYSTEM_MODULE_NAME.to_owned(),
            Identifier::new("convert_to_fungible_staked_sui").unwrap(),
            vec![],
            vec![system_state_arg, staked_sui_arg],
        ));
        let sender_arg = ptb.pure(sender).unwrap();
        ptb.command(Command::TransferObjects(vec![fss_result], sender_arg));

        let convert_tx = TransactionData::new_programmable(
            sender,
            vec![gas_ref],
            ptb.finish(),
            1_000_000_000,
            gas_price,
        );
        let tx = to_sender_signed_transaction(convert_tx, keystore.export(&sender).unwrap());
        let convert_response = execute_transaction(&mut client.clone(), &tx).await.unwrap();
        assert!(convert_response.effects().status().success());
    }

    let (fss_count_before, _) = count_fss_objects(&mut client, sender).await;
    assert!(fss_count_before >= 2);

    run_redeem_flow(
        &mut client,
        &rosetta_client,
        keystore,
        sender,
        validator_a,
        "All",
        None,
    )
    .await;

    let fss_request = ListOwnedObjectsRequest::default()
        .with_owner(sender.to_string())
        .with_object_type("0x3::staking_pool::FungibleStakedSui".to_string())
        .with_page_size(100u32)
        .with_read_mask(FieldMask::from_paths(["object_id", "contents"]));
    let remaining_fss: Vec<_> = client
        .clone()
        .list_owned_objects(fss_request)
        .map_err(|e| panic!("List FSS error: {e}"))
        .try_collect()
        .await
        .unwrap();
    assert!(
        !remaining_fss.is_empty(),
        "Validator B's FSS should still exist"
    );

    let request = GetEpochRequest::latest().with_read_mask(FieldMask::from_paths(["system_state"]));
    let response = client
        .ledger_client()
        .get_epoch(request)
        .await
        .unwrap()
        .into_inner();
    let system_state = response.epoch.and_then(|epoch| epoch.system_state).unwrap();
    let active_validators = &system_state.validators.unwrap().active_validators;
    let pool_b_id = active_validators
        .iter()
        .find(|v| v.address().parse::<SuiAddress>().ok() == Some(validator_b))
        .unwrap()
        .staking_pool()
        .id()
        .to_string();

    for obj in &remaining_fss {
        if let Some(contents) = &obj.contents {
            #[derive(serde::Deserialize)]
            struct FssBcs {
                _id: sui_sdk_types::Address,
                pool_id: sui_sdk_types::Address,
                _value: u64,
            }
            let fss: FssBcs = contents.deserialize().unwrap();
            assert_eq!(
                fss.pool_id.to_string(),
                pool_b_id,
                "Remaining FSS should belong to validator B's pool"
            );
        }
    }
}

#[tokio::test]
async fn test_redeem_no_fss_error() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handles) = start_rosetta_test_server(client.clone()).await;

    let request = GetEpochRequest::latest().with_read_mask(FieldMask::from_paths(["system_state"]));
    let response = client
        .ledger_client()
        .get_epoch(request)
        .await
        .unwrap()
        .into_inner();
    let system_state = response.epoch.and_then(|epoch| epoch.system_state).unwrap();
    let validator = system_state.validators.unwrap().active_validators[0]
        .address()
        .parse::<SuiAddress>()
        .unwrap();

    let redeem_ops = serde_json::from_value(json!(
        [{
            "operation_identifier": {"index": 0},
            "type": "MergeAndRedeemFungibleStakedSui",
            "account": {"address": sender.to_string()},
            "metadata": {
                "MergeAndRedeemFungibleStakedSui": {
                    "validator": validator.to_string(),
                    "redeem_mode": "All"
                }
            }
        }]
    ))
    .unwrap();

    let flow_result = rosetta_client
        .rosetta_flow(&redeem_ops, keystore, None)
        .await;
    assert!(
        flow_result.metadata.as_ref().is_some_and(|r| r.is_err()),
        "Expected metadata error when no FSS exists, got: {:?}",
        flow_result.metadata
    );
}

#[tokio::test]
async fn test_redeem_invalid_validator() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;
    let client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handles) = start_rosetta_test_server(client.clone()).await;

    let fake_validator =
        SuiAddress::from_str("0x0000000000000000000000000000000000000000000000000000000000000099")
            .unwrap();

    let redeem_ops = serde_json::from_value(json!(
        [{
            "operation_identifier": {"index": 0},
            "type": "MergeAndRedeemFungibleStakedSui",
            "account": {"address": sender.to_string()},
            "metadata": {
                "MergeAndRedeemFungibleStakedSui": {
                    "validator": fake_validator.to_string(),
                    "redeem_mode": "All"
                }
            }
        }]
    ))
    .unwrap();

    let flow_result = rosetta_client
        .rosetta_flow(&redeem_ops, keystore, None)
        .await;
    assert!(
        flow_result.metadata.as_ref().is_some_and(|r| r.is_err()),
        "Expected metadata error for invalid validator, got: {:?}",
        flow_result.metadata
    );
}

// ================================================================================
// PR 1 e2e tests — ConsolidateAllStakedSuiToFungible parser
// ================================================================================

#[derive(serde::Deserialize)]
struct ParseResp {
    operations: Operations,
    #[allow(dead_code)]
    account_identifier_signers: Vec<AccountIdentifier>,
}

/// POST unsigned TX bytes to /construction/parse (offline endpoint).
/// Note: SuiEnv serializes as lowercase (e.g., `localnet`), not `LocalNet`.
async fn parse_unsigned(
    rosetta_client: &rosetta_client::RosettaClient,
    unsigned_tx: impl serde::Serialize,
) -> ParseResp {
    let request = serde_json::json!({
        "network_identifier": {
            "blockchain": "sui",
            "network": serde_json::to_value(SuiEnv::LocalNet).unwrap(),
        },
        "signed": false,
        "transaction": serde_json::to_value(&unsigned_tx).unwrap(),
    });
    rosetta_client
        .call(rosetta_client::RosettaEndpoint::Parse, &request)
        .await
        .expect("parse failed")
}

async fn first_validator(client: &mut GrpcClient) -> SuiAddress {
    let request = GetEpochRequest::latest().with_read_mask(FieldMask::from_paths(["system_state"]));
    let response = client
        .clone()
        .ledger_client()
        .get_epoch(request)
        .await
        .unwrap()
        .into_inner();
    response
        .epoch
        .and_then(|e| e.system_state)
        .unwrap()
        .validators
        .unwrap()
        .active_validators[0]
        .address()
        .parse()
        .unwrap()
}

/// Stake `amount_mist` SUI with `validator` via the Rosetta flow.
async fn stake_via_rosetta(
    rosetta_client: &rosetta_client::RosettaClient,
    client: &mut GrpcClient,
    keystore: &sui_keys::keystore::Keystore,
    sender: SuiAddress,
    validator: SuiAddress,
    amount_mist: u64,
) {
    let ops = serde_json::from_value(json!([{
        "operation_identifier":{"index":0},
        "type":"Stake",
        "account": { "address": sender.to_string() },
        "amount": { "value": format!("-{}", amount_mist) },
        "metadata": { "Stake": {"validator": validator.to_string()} }
    }]))
    .unwrap();
    let resp: TransactionIdentifierResponse = rosetta_client
        .rosetta_flow(&ops, keystore, None)
        .await
        .submit
        .unwrap()
        .unwrap();
    wait_for_transaction(client, &resp.transaction_identifier.hash.to_string())
        .await
        .unwrap();
}

/// Convert every owned `StakedSui` to a `FungibleStakedSui` via a direct PTB.
/// Requires epoch to already be advanced so stakes are activated.
async fn convert_all_staked_to_fss_directly(
    client: &mut GrpcClient,
    keystore: &sui_keys::keystore::Keystore,
    sender: SuiAddress,
) {
    use futures::TryStreamExt;
    use sui_rpc::proto::sui::rpc::v2::ListOwnedObjectsRequest;

    let staked_req = ListOwnedObjectsRequest::default()
        .with_owner(sender.to_string())
        .with_object_type("0x3::staking_pool::StakedSui".to_string())
        .with_page_size(100u32)
        .with_read_mask(FieldMask::from_paths(["object_id", "version", "digest"]));
    let staked_objs: Vec<_> = client
        .clone()
        .list_owned_objects(staked_req)
        .try_collect()
        .await
        .unwrap();

    for staked_obj in &staked_objs {
        let staked_ref = (
            ObjectID::from_str(staked_obj.object_id()).unwrap(),
            staked_obj.version().into(),
            staked_obj.digest().parse().unwrap(),
        );
        let gas_price = client.get_reference_gas_price().await.unwrap();
        let coins = get_all_coins(&mut client.clone(), sender).await.unwrap();
        let gas_object = get_object_ref(&mut client.clone(), coins[0].id())
            .await
            .unwrap()
            .as_object_ref();

        let mut ptb = ProgrammableTransactionBuilder::new();
        let sys = ptb.input(CallArg::SUI_SYSTEM_MUT).unwrap();
        let staked_arg = ptb.obj(ObjectArg::ImmOrOwnedObject(staked_ref)).unwrap();
        let fss_result = ptb.command(Command::move_call(
            SUI_SYSTEM_PACKAGE_ID,
            SUI_SYSTEM_MODULE_NAME.to_owned(),
            Identifier::new("convert_to_fungible_staked_sui").unwrap(),
            vec![],
            vec![sys, staked_arg],
        ));
        let sender_arg = ptb.pure(sender).unwrap();
        ptb.command(Command::TransferObjects(vec![fss_result], sender_arg));

        let tx_data = TransactionData::new_programmable(
            sender,
            vec![gas_object],
            ptb.finish(),
            1_000_000_000,
            gas_price,
        );
        let tx = to_sender_signed_transaction(tx_data, keystore.export(&sender).unwrap());
        execute_transaction(&mut client.clone(), &tx).await.unwrap();
    }
}

/// S=1, F=0 — single StakedSui conversion, no pre-existing FSS.
#[tokio::test]
async fn test_e2e_parse_consolidate_1_stake_0_fss() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let validator = first_validator(&mut client).await;
    stake_via_rosetta(
        &rosetta_client,
        &mut client,
        keystore,
        sender,
        validator,
        1_000_000_000,
    )
    .await;
    test_cluster.trigger_reconfiguration().await;

    let consolidate_ops = serde_json::from_value(json!([{
        "operation_identifier": {"index": 0},
        "type": "ConsolidateAllStakedSuiToFungible",
        "account": {"address": sender.to_string()},
        "metadata": {
            "ConsolidateAllStakedSuiToFungible": {"validator": validator.to_string()}
        }
    }]))
    .unwrap();
    let flow = rosetta_client
        .rosetta_flow(&consolidate_ops, keystore, None)
        .await;
    let payloads = flow.payloads.expect("payloads").expect("payloads errored");
    let parsed = parse_unsigned(&rosetta_client, &payloads.unsigned_transaction).await;

    let ops: Vec<_> = parsed.operations.into_iter().collect();
    assert_eq!(ops.len(), 1);
    assert_eq!(
        ops[0].type_,
        OperationType::ConsolidateAllStakedSuiToFungible
    );
    assert_eq!(ops[0].account.as_ref().unwrap().address, sender);
    let Some(sui_rosetta::operations::OperationMetadata::ConsolidateAllStakedSuiToFungible {
        validator: v,
        staked_sui_ids,
        fss_ids,
    }) = ops[0].metadata.clone()
    else {
        panic!("wrong metadata variant");
    };
    assert!(v.is_none(), "validator must be None from parser");
    assert_eq!(staked_sui_ids.len(), 1);
    assert!(fss_ids.is_empty());
}

/// S=0, F=2 — pure FSS merge (no conversion).
#[tokio::test]
async fn test_e2e_parse_consolidate_0_stakes_2_fss() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let validator = first_validator(&mut client).await;
    // Stake twice, advance epoch, convert both to FSS.
    stake_via_rosetta(
        &rosetta_client,
        &mut client,
        keystore,
        sender,
        validator,
        1_000_000_000,
    )
    .await;
    stake_via_rosetta(
        &rosetta_client,
        &mut client,
        keystore,
        sender,
        validator,
        1_000_000_000,
    )
    .await;
    test_cluster.trigger_reconfiguration().await;
    convert_all_staked_to_fss_directly(&mut client, keystore, sender).await;

    let consolidate_ops = serde_json::from_value(json!([{
        "operation_identifier": {"index": 0},
        "type": "ConsolidateAllStakedSuiToFungible",
        "account": {"address": sender.to_string()},
        "metadata": {
            "ConsolidateAllStakedSuiToFungible": {"validator": validator.to_string()}
        }
    }]))
    .unwrap();
    let flow = rosetta_client
        .rosetta_flow(&consolidate_ops, keystore, None)
        .await;
    let payloads = flow.payloads.expect("payloads").expect("payloads errored");
    let parsed = parse_unsigned(&rosetta_client, &payloads.unsigned_transaction).await;

    let ops: Vec<_> = parsed.operations.into_iter().collect();
    assert_eq!(ops.len(), 1);
    assert_eq!(
        ops[0].type_,
        OperationType::ConsolidateAllStakedSuiToFungible
    );
    let Some(sui_rosetta::operations::OperationMetadata::ConsolidateAllStakedSuiToFungible {
        staked_sui_ids,
        fss_ids,
        ..
    }) = ops[0].metadata.clone()
    else {
        panic!();
    };
    assert!(
        staked_sui_ids.is_empty(),
        "pure merge should have no StakedSui IDs"
    );
    assert_eq!(fss_ids.len(), 2);
}

/// S=1, F=1 — mixed case: 1 stake + 1 pre-existing FSS, cross-merge path.
#[tokio::test]
async fn test_e2e_parse_consolidate_1_stake_1_fss() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let validator = first_validator(&mut client).await;
    // Stake 1, advance, convert to FSS (= 1 FSS, 0 stakes).
    stake_via_rosetta(
        &rosetta_client,
        &mut client,
        keystore,
        sender,
        validator,
        1_000_000_000,
    )
    .await;
    test_cluster.trigger_reconfiguration().await;
    convert_all_staked_to_fss_directly(&mut client, keystore, sender).await;
    // Stake again, advance again. Now we have 1 activated stake + 1 FSS.
    stake_via_rosetta(
        &rosetta_client,
        &mut client,
        keystore,
        sender,
        validator,
        1_000_000_000,
    )
    .await;
    test_cluster.trigger_reconfiguration().await;

    let consolidate_ops = serde_json::from_value(json!([{
        "operation_identifier": {"index": 0},
        "type": "ConsolidateAllStakedSuiToFungible",
        "account": {"address": sender.to_string()},
        "metadata": {
            "ConsolidateAllStakedSuiToFungible": {"validator": validator.to_string()}
        }
    }]))
    .unwrap();
    let flow = rosetta_client
        .rosetta_flow(&consolidate_ops, keystore, None)
        .await;
    let payloads = flow.payloads.expect("payloads").expect("payloads errored");
    let parsed = parse_unsigned(&rosetta_client, &payloads.unsigned_transaction).await;

    let ops: Vec<_> = parsed.operations.into_iter().collect();
    let Some(sui_rosetta::operations::OperationMetadata::ConsolidateAllStakedSuiToFungible {
        staked_sui_ids,
        fss_ids,
        ..
    }) = ops[0].metadata.clone()
    else {
        panic!();
    };
    assert_eq!(staked_sui_ids.len(), 1);
    assert_eq!(fss_ids.len(), 1);
}

/// Submit + wait, then verify the block/transaction endpoint exposes the typed op.
#[tokio::test]
async fn test_e2e_block_consolidate_1_stake_0_fss() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let validator = first_validator(&mut client).await;
    stake_via_rosetta(
        &rosetta_client,
        &mut client,
        keystore,
        sender,
        validator,
        1_000_000_000,
    )
    .await;
    test_cluster.trigger_reconfiguration().await;

    let consolidate_ops = serde_json::from_value(json!([{
        "operation_identifier": {"index": 0},
        "type": "ConsolidateAllStakedSuiToFungible",
        "account": {"address": sender.to_string()},
        "metadata": {
            "ConsolidateAllStakedSuiToFungible": {"validator": validator.to_string()}
        }
    }]))
    .unwrap();
    let submit_resp: TransactionIdentifierResponse = rosetta_client
        .rosetta_flow(&consolidate_ops, keystore, None)
        .await
        .submit
        .unwrap()
        .unwrap();
    let tx_hash = submit_resp.transaction_identifier.hash.to_string();
    wait_for_transaction(&mut client, &tx_hash).await.unwrap();

    // Read back the executed TX and reparse through the same code path.
    let get_tx = GetTransactionRequest::default()
        .with_digest(tx_hash.clone())
        .with_read_mask(FieldMask::from_paths([
            "transaction",
            "effects",
            "events",
            "balance_changes",
        ]));
    let executed = client
        .clone()
        .ledger_client()
        .get_transaction(get_tx)
        .await
        .unwrap()
        .into_inner();
    let cache = CoinMetadataCache::new(client.clone(), NonZeroUsize::new(100).unwrap());
    let ops = Operations::try_from_executed_transaction(executed.transaction.unwrap(), &cache)
        .await
        .unwrap();
    let ops_vec: Vec<_> = ops.into_iter().collect();
    let typed = ops_vec
        .iter()
        .find(|o| o.type_ == OperationType::ConsolidateAllStakedSuiToFungible)
        .expect("expected typed Consolidate op");
    assert_eq!(typed.account.as_ref().unwrap().address, sender);
    // Also confirm a Gas op is present.
    assert!(ops_vec.iter().any(|o| o.type_ == OperationType::Gas));
    // And NO SuiBalanceChange for sender (Consolidate is object-only, no net SUI delta).
    let sender_balance_changes: Vec<_> = ops_vec
        .iter()
        .filter(|o| {
            o.type_ == OperationType::SuiBalanceChange
                && o.account.as_ref().map(|a| a.address) == Some(sender)
        })
        .collect();
    assert!(
        sender_balance_changes.is_empty(),
        "Consolidate should have no SuiBalanceChange for sender, got {:?}",
        sender_balance_changes
    );
}

/// Invalid validator address (not in active set) should error at /metadata.
#[tokio::test]
async fn test_e2e_consolidate_errors_invalid_validator() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;
    let client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client).await;

    let fake_validator = SuiAddress::random_for_testing_only();
    let ops = serde_json::from_value(json!([{
        "operation_identifier": {"index": 0},
        "type": "ConsolidateAllStakedSuiToFungible",
        "account": {"address": sender.to_string()},
        "metadata": {
            "ConsolidateAllStakedSuiToFungible": {"validator": fake_validator.to_string()}
        }
    }]))
    .unwrap();
    let flow = rosetta_client.rosetta_flow(&ops, keystore, None).await;
    assert!(
        flow.metadata.as_ref().is_some_and(|r| r.is_err()),
        "Expected metadata error for invalid validator, got: {:?}",
        flow.metadata
    );
}

/// Submitting a Consolidate op without the validator field should NOT produce a valid
/// preprocess response. Note: the test helper's `RosettaAPIResult` untagged enum may
/// deserialize a server error response as an empty-Ok (since `ConstructionPreprocessResponse`
/// has all-optional fields), so we accept either an explicit `Err` OR an `Ok` with no
/// `options` / `required_public_keys` as evidence of rejection.
#[tokio::test]
async fn test_e2e_consolidate_errors_missing_validator_in_metadata() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;
    let client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client).await;

    let ops = serde_json::from_value(json!([{
        "operation_identifier": {"index": 0},
        "type": "ConsolidateAllStakedSuiToFungible",
        "account": {"address": sender.to_string()},
        "metadata": {
            "ConsolidateAllStakedSuiToFungible": {}
        }
    }]))
    .unwrap();
    let flow = rosetta_client.rosetta_flow(&ops, keystore, None).await;
    let preprocess = flow.preprocess.as_ref().expect("preprocess attempted");
    match preprocess {
        Err(_) => { /* expected */ }
        Ok(resp) => {
            // Masked error: options must be None AND required_public_keys empty.
            assert!(
                resp.options.is_none() && resp.required_public_keys.is_empty(),
                "expected rejection for missing validator, got success: {:?}",
                resp
            );
        }
    }
}

/// Garbage bytes to /construction/parse should return an error response, not panic.
/// Note: the test helper's untagged `RosettaAPIResult` with `T = serde_json::Value` will
/// accept ANY JSON as `Ok`. We look for Rosetta error shape (a `code` field) instead.
#[tokio::test]
async fn test_e2e_parse_garbage_bytes() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client).await;

    let request = serde_json::json!({
        "network_identifier": {
            "blockchain": "sui",
            "network": serde_json::to_value(SuiEnv::LocalNet).unwrap(),
        },
        "signed": false,
        "transaction": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef",
    });
    let result: Result<serde_json::Value, _> = rosetta_client
        .call(rosetta_client::RosettaEndpoint::Parse, &request)
        .await;
    let json = match result {
        Ok(v) => v,
        Err(e) => serde_json::to_value(e.code).unwrap_or_default(),
    };
    let has_error_shape = json.get("code").is_some() || json.get("message").is_some();
    assert!(
        has_error_shape,
        "expected Rosetta error response for garbage bytes, got: {:?}",
        json
    );
}

/// Truncated TX bytes to /construction/parse should return an error response, not panic.
#[tokio::test]
async fn test_e2e_parse_truncated_tx_data() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client).await;

    let request = serde_json::json!({
        "network_identifier": {
            "blockchain": "sui",
            "network": serde_json::to_value(SuiEnv::LocalNet).unwrap(),
        },
        "signed": false,
        "transaction": "00",
    });
    let result: Result<serde_json::Value, _> = rosetta_client
        .call(rosetta_client::RosettaEndpoint::Parse, &request)
        .await;
    let json = match result {
        Ok(v) => v,
        Err(e) => serde_json::to_value(e.code).unwrap_or_default(),
    };
    let has_error_shape = json.get("code").is_some() || json.get("message").is_some();
    assert!(
        has_error_shape,
        "expected Rosetta error response for truncated bytes, got: {:?}",
        json
    );
}

/// S=3, F=0 — 3 stakes, no pre-existing FSS.
#[tokio::test]
async fn test_e2e_parse_consolidate_3_stakes_0_fss() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let validator = first_validator(&mut client).await;
    for _ in 0..3 {
        stake_via_rosetta(
            &rosetta_client,
            &mut client,
            keystore,
            sender,
            validator,
            1_000_000_000,
        )
        .await;
    }
    test_cluster.trigger_reconfiguration().await;

    let consolidate_ops = serde_json::from_value(json!([{
        "operation_identifier": {"index": 0},
        "type": "ConsolidateAllStakedSuiToFungible",
        "account": {"address": sender.to_string()},
        "metadata": {
            "ConsolidateAllStakedSuiToFungible": {"validator": validator.to_string()}
        }
    }]))
    .unwrap();
    let flow = rosetta_client
        .rosetta_flow(&consolidate_ops, keystore, None)
        .await;
    let payloads = flow.payloads.expect("payloads").expect("payloads errored");
    let parsed = parse_unsigned(&rosetta_client, &payloads.unsigned_transaction).await;

    let ops: Vec<_> = parsed.operations.into_iter().collect();
    assert_eq!(ops.len(), 1);
    assert_eq!(
        ops[0].type_,
        OperationType::ConsolidateAllStakedSuiToFungible
    );
    let Some(sui_rosetta::operations::OperationMetadata::ConsolidateAllStakedSuiToFungible {
        staked_sui_ids,
        fss_ids,
        ..
    }) = ops[0].metadata.clone()
    else {
        panic!();
    };
    assert_eq!(staked_sui_ids.len(), 3);
    assert!(fss_ids.is_empty());
}

/// S=0, F=3 — three pre-existing FSS, pure merge.
#[tokio::test]
async fn test_e2e_parse_consolidate_0_stakes_3_fss() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let validator = first_validator(&mut client).await;
    for _ in 0..3 {
        stake_via_rosetta(
            &rosetta_client,
            &mut client,
            keystore,
            sender,
            validator,
            1_000_000_000,
        )
        .await;
    }
    test_cluster.trigger_reconfiguration().await;
    convert_all_staked_to_fss_directly(&mut client, keystore, sender).await;

    let consolidate_ops = serde_json::from_value(json!([{
        "operation_identifier": {"index": 0},
        "type": "ConsolidateAllStakedSuiToFungible",
        "account": {"address": sender.to_string()},
        "metadata": {
            "ConsolidateAllStakedSuiToFungible": {"validator": validator.to_string()}
        }
    }]))
    .unwrap();
    let flow = rosetta_client
        .rosetta_flow(&consolidate_ops, keystore, None)
        .await;
    let payloads = flow.payloads.expect("payloads").expect("payloads errored");
    let parsed = parse_unsigned(&rosetta_client, &payloads.unsigned_transaction).await;

    let ops: Vec<_> = parsed.operations.into_iter().collect();
    let Some(sui_rosetta::operations::OperationMetadata::ConsolidateAllStakedSuiToFungible {
        staked_sui_ids,
        fss_ids,
        ..
    }) = ops[0].metadata.clone()
    else {
        panic!();
    };
    assert!(staked_sui_ids.is_empty());
    assert_eq!(fss_ids.len(), 3);
}

/// S=2, F=2 — full flow with both existing FSS and activated StakedSui.
#[tokio::test]
async fn test_e2e_parse_consolidate_2_stakes_2_fss() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let validator = first_validator(&mut client).await;
    // First: stake twice, advance, convert to 2 FSS.
    stake_via_rosetta(
        &rosetta_client,
        &mut client,
        keystore,
        sender,
        validator,
        1_000_000_000,
    )
    .await;
    stake_via_rosetta(
        &rosetta_client,
        &mut client,
        keystore,
        sender,
        validator,
        1_000_000_000,
    )
    .await;
    test_cluster.trigger_reconfiguration().await;
    convert_all_staked_to_fss_directly(&mut client, keystore, sender).await;
    // Then: stake twice more, advance, leaving 2 activated stakes + 2 FSS.
    stake_via_rosetta(
        &rosetta_client,
        &mut client,
        keystore,
        sender,
        validator,
        1_000_000_000,
    )
    .await;
    stake_via_rosetta(
        &rosetta_client,
        &mut client,
        keystore,
        sender,
        validator,
        1_000_000_000,
    )
    .await;
    test_cluster.trigger_reconfiguration().await;

    let consolidate_ops = serde_json::from_value(json!([{
        "operation_identifier": {"index": 0},
        "type": "ConsolidateAllStakedSuiToFungible",
        "account": {"address": sender.to_string()},
        "metadata": {
            "ConsolidateAllStakedSuiToFungible": {"validator": validator.to_string()}
        }
    }]))
    .unwrap();
    let flow = rosetta_client
        .rosetta_flow(&consolidate_ops, keystore, None)
        .await;
    let payloads = flow.payloads.expect("payloads").expect("payloads errored");
    let parsed = parse_unsigned(&rosetta_client, &payloads.unsigned_transaction).await;

    let ops: Vec<_> = parsed.operations.into_iter().collect();
    let Some(sui_rosetta::operations::OperationMetadata::ConsolidateAllStakedSuiToFungible {
        staked_sui_ids,
        fss_ids,
        ..
    }) = ops[0].metadata.clone()
    else {
        panic!();
    };
    assert_eq!(staked_sui_ids.len(), 2);
    assert_eq!(fss_ids.len(), 2);
}

/// Stake with two different validators; consolidate validator A and verify only A's
/// objects appear in the parse output.
#[tokio::test]
async fn test_e2e_parse_consolidate_multi_validator_isolation() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let request = GetEpochRequest::latest().with_read_mask(FieldMask::from_paths(["system_state"]));
    let response = client
        .clone()
        .ledger_client()
        .get_epoch(request)
        .await
        .unwrap()
        .into_inner();
    let validators = response
        .epoch
        .and_then(|e| e.system_state)
        .unwrap()
        .validators
        .unwrap();
    let validator_a: SuiAddress = validators.active_validators[0].address().parse().unwrap();
    let validator_b: SuiAddress = validators.active_validators[1].address().parse().unwrap();

    stake_via_rosetta(
        &rosetta_client,
        &mut client,
        keystore,
        sender,
        validator_a,
        1_000_000_000,
    )
    .await;
    stake_via_rosetta(
        &rosetta_client,
        &mut client,
        keystore,
        sender,
        validator_b,
        1_000_000_000,
    )
    .await;
    test_cluster.trigger_reconfiguration().await;

    // Consolidate only validator_a's stakes.
    let consolidate_ops = serde_json::from_value(json!([{
        "operation_identifier": {"index": 0},
        "type": "ConsolidateAllStakedSuiToFungible",
        "account": {"address": sender.to_string()},
        "metadata": {
            "ConsolidateAllStakedSuiToFungible": {"validator": validator_a.to_string()}
        }
    }]))
    .unwrap();
    let flow = rosetta_client
        .rosetta_flow(&consolidate_ops, keystore, None)
        .await;
    let payloads = flow.payloads.expect("payloads").expect("payloads errored");
    let parsed = parse_unsigned(&rosetta_client, &payloads.unsigned_transaction).await;

    let ops: Vec<_> = parsed.operations.into_iter().collect();
    let Some(sui_rosetta::operations::OperationMetadata::ConsolidateAllStakedSuiToFungible {
        staked_sui_ids,
        fss_ids,
        ..
    }) = ops[0].metadata.clone()
    else {
        panic!();
    };
    // Exactly 1 staked (from A), 0 FSS.
    assert_eq!(staked_sui_ids.len(), 1);
    assert!(fss_ids.is_empty());
}

/// Helper: submit a Consolidate op and verify /block/transaction exposes the typed op.
async fn submit_and_assert_block_consolidate(
    rosetta_client: &rosetta_client::RosettaClient,
    client: &mut GrpcClient,
    keystore: &sui_keys::keystore::Keystore,
    sender: SuiAddress,
    validator: SuiAddress,
    expected_staked: usize,
    expected_fss: usize,
) {
    let consolidate_ops = serde_json::from_value(json!([{
        "operation_identifier": {"index": 0},
        "type": "ConsolidateAllStakedSuiToFungible",
        "account": {"address": sender.to_string()},
        "metadata": {
            "ConsolidateAllStakedSuiToFungible": {"validator": validator.to_string()}
        }
    }]))
    .unwrap();
    let submit_resp: TransactionIdentifierResponse = rosetta_client
        .rosetta_flow(&consolidate_ops, keystore, None)
        .await
        .submit
        .unwrap()
        .unwrap();
    let tx_hash = submit_resp.transaction_identifier.hash.to_string();
    wait_for_transaction(client, &tx_hash).await.unwrap();

    let get_tx = GetTransactionRequest::default()
        .with_digest(tx_hash.clone())
        .with_read_mask(FieldMask::from_paths([
            "transaction",
            "effects",
            "events",
            "balance_changes",
        ]));
    let executed = client
        .clone()
        .ledger_client()
        .get_transaction(get_tx)
        .await
        .unwrap()
        .into_inner();
    let cache = CoinMetadataCache::new(client.clone(), NonZeroUsize::new(100).unwrap());
    let ops = Operations::try_from_executed_transaction(executed.transaction.unwrap(), &cache)
        .await
        .unwrap();
    let ops_vec: Vec<_> = ops.into_iter().collect();
    let typed = ops_vec
        .iter()
        .find(|o| o.type_ == OperationType::ConsolidateAllStakedSuiToFungible)
        .expect("expected typed Consolidate op");
    assert_eq!(typed.account.as_ref().unwrap().address, sender);
    let Some(sui_rosetta::operations::OperationMetadata::ConsolidateAllStakedSuiToFungible {
        staked_sui_ids,
        fss_ids,
        ..
    }) = typed.metadata.clone()
    else {
        panic!();
    };
    assert_eq!(staked_sui_ids.len(), expected_staked);
    assert_eq!(fss_ids.len(), expected_fss);
    // Must have a Gas op.
    assert!(ops_vec.iter().any(|o| o.type_ == OperationType::Gas));
    // No SuiBalanceChange for sender (Consolidate is object-only).
    let sender_balance_changes: Vec<_> = ops_vec
        .iter()
        .filter(|o| {
            o.type_ == OperationType::SuiBalanceChange
                && o.account.as_ref().map(|a| a.address) == Some(sender)
        })
        .collect();
    assert!(
        sender_balance_changes.is_empty(),
        "Consolidate should have no sender SuiBalanceChange, got {:?}",
        sender_balance_changes
    );
}

#[tokio::test]
async fn test_e2e_block_consolidate_0_stakes_2_fss() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let validator = first_validator(&mut client).await;
    for _ in 0..2 {
        stake_via_rosetta(
            &rosetta_client,
            &mut client,
            keystore,
            sender,
            validator,
            1_000_000_000,
        )
        .await;
    }
    test_cluster.trigger_reconfiguration().await;
    convert_all_staked_to_fss_directly(&mut client, keystore, sender).await;

    submit_and_assert_block_consolidate(
        &rosetta_client,
        &mut client,
        keystore,
        sender,
        validator,
        0,
        2,
    )
    .await;
}

#[tokio::test]
async fn test_e2e_block_consolidate_0_stakes_3_fss() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let validator = first_validator(&mut client).await;
    for _ in 0..3 {
        stake_via_rosetta(
            &rosetta_client,
            &mut client,
            keystore,
            sender,
            validator,
            1_000_000_000,
        )
        .await;
    }
    test_cluster.trigger_reconfiguration().await;
    convert_all_staked_to_fss_directly(&mut client, keystore, sender).await;

    submit_and_assert_block_consolidate(
        &rosetta_client,
        &mut client,
        keystore,
        sender,
        validator,
        0,
        3,
    )
    .await;
}

#[tokio::test]
async fn test_e2e_block_consolidate_3_stakes_0_fss() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let validator = first_validator(&mut client).await;
    for _ in 0..3 {
        stake_via_rosetta(
            &rosetta_client,
            &mut client,
            keystore,
            sender,
            validator,
            1_000_000_000,
        )
        .await;
    }
    test_cluster.trigger_reconfiguration().await;

    submit_and_assert_block_consolidate(
        &rosetta_client,
        &mut client,
        keystore,
        sender,
        validator,
        3,
        0,
    )
    .await;
}

#[tokio::test]
async fn test_e2e_block_consolidate_1_stake_1_fss() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let validator = first_validator(&mut client).await;
    stake_via_rosetta(
        &rosetta_client,
        &mut client,
        keystore,
        sender,
        validator,
        1_000_000_000,
    )
    .await;
    test_cluster.trigger_reconfiguration().await;
    convert_all_staked_to_fss_directly(&mut client, keystore, sender).await;
    stake_via_rosetta(
        &rosetta_client,
        &mut client,
        keystore,
        sender,
        validator,
        1_000_000_000,
    )
    .await;
    test_cluster.trigger_reconfiguration().await;

    submit_and_assert_block_consolidate(
        &rosetta_client,
        &mut client,
        keystore,
        sender,
        validator,
        1,
        1,
    )
    .await;
}

#[tokio::test]
async fn test_e2e_block_consolidate_2_stakes_2_fss() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let validator = first_validator(&mut client).await;
    for _ in 0..2 {
        stake_via_rosetta(
            &rosetta_client,
            &mut client,
            keystore,
            sender,
            validator,
            1_000_000_000,
        )
        .await;
    }
    test_cluster.trigger_reconfiguration().await;
    convert_all_staked_to_fss_directly(&mut client, keystore, sender).await;
    for _ in 0..2 {
        stake_via_rosetta(
            &rosetta_client,
            &mut client,
            keystore,
            sender,
            validator,
            1_000_000_000,
        )
        .await;
    }
    test_cluster.trigger_reconfiguration().await;

    submit_and_assert_block_consolidate(
        &rosetta_client,
        &mut client,
        keystore,
        sender,
        validator,
        2,
        2,
    )
    .await;
}

#[tokio::test]
async fn test_e2e_block_consolidate_multi_validator_isolation() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let request = GetEpochRequest::latest().with_read_mask(FieldMask::from_paths(["system_state"]));
    let response = client
        .clone()
        .ledger_client()
        .get_epoch(request)
        .await
        .unwrap()
        .into_inner();
    let validators = response
        .epoch
        .and_then(|e| e.system_state)
        .unwrap()
        .validators
        .unwrap();
    let validator_a: SuiAddress = validators.active_validators[0].address().parse().unwrap();
    let validator_b: SuiAddress = validators.active_validators[1].address().parse().unwrap();

    stake_via_rosetta(
        &rosetta_client,
        &mut client,
        keystore,
        sender,
        validator_a,
        1_000_000_000,
    )
    .await;
    stake_via_rosetta(
        &rosetta_client,
        &mut client,
        keystore,
        sender,
        validator_b,
        1_000_000_000,
    )
    .await;
    test_cluster.trigger_reconfiguration().await;

    // Consolidate only validator_a's stake — expect 1 staked, 0 fss.
    submit_and_assert_block_consolidate(
        &rosetta_client,
        &mut client,
        keystore,
        sender,
        validator_a,
        1,
        0,
    )
    .await;
}

// ================================================================================
// PR 2 e2e tests — MergeAndRedeemFungibleStakedSui parser
// ================================================================================

/// Scaffold: stake `n` times, advance epoch, convert each StakedSui to FSS via direct PTB.
/// Produces `n` FSS objects all on the same validator's pool.
async fn setup_n_fss(
    rosetta_client: &rosetta_client::RosettaClient,
    client: &mut GrpcClient,
    keystore: &sui_keys::keystore::Keystore,
    test_cluster: &test_cluster::TestCluster,
    sender: SuiAddress,
    validator: SuiAddress,
    n: usize,
) {
    for _ in 0..n {
        stake_via_rosetta(
            rosetta_client,
            client,
            keystore,
            sender,
            validator,
            1_000_000_000,
        )
        .await;
    }
    test_cluster.trigger_reconfiguration().await;
    convert_all_staked_to_fss_directly(client, keystore, sender).await;
}

/// Run a MergeAndRedeem through /preprocess → /metadata → /payloads, then POST the
/// unsigned bytes to /construction/parse. Returns the parsed operations.
async fn run_merge_redeem_and_parse(
    rosetta_client: &rosetta_client::RosettaClient,
    keystore: &sui_keys::keystore::Keystore,
    sender: SuiAddress,
    validator: SuiAddress,
    amount: Option<u64>,
    mode: &str,
) -> Vec<sui_rosetta::operations::Operation> {
    let mut metadata_value = serde_json::json!({
        "validator": validator.to_string(),
        "redeem_mode": mode,
    });
    if let Some(a) = amount {
        metadata_value["amount"] = serde_json::Value::String(a.to_string());
    }
    let ops = serde_json::from_value(json!([{
        "operation_identifier": {"index": 0},
        "type": "MergeAndRedeemFungibleStakedSui",
        "account": {"address": sender.to_string()},
        "metadata": {
            "MergeAndRedeemFungibleStakedSui": metadata_value,
        }
    }]))
    .unwrap();
    let flow = rosetta_client.rosetta_flow(&ops, keystore, None).await;
    let payloads = flow.payloads.expect("payloads").expect("payloads errored");
    let parsed = parse_unsigned(rosetta_client, &payloads.unsigned_transaction).await;
    parsed.operations.into_iter().collect()
}

fn assert_merge_redeem_parse_ops(
    ops: &[sui_rosetta::operations::Operation],
    expected_sender: SuiAddress,
    expected_fss_count: usize,
    expected_mode: Option<sui_rosetta::types::RedeemMode>,
) {
    assert_eq!(ops.len(), 1, "expected exactly one parsed op");
    assert_eq!(ops[0].type_, OperationType::MergeAndRedeemFungibleStakedSui);
    assert_eq!(ops[0].account.as_ref().unwrap().address, expected_sender);
    let Some(sui_rosetta::operations::OperationMetadata::MergeAndRedeemFungibleStakedSui {
        validator,
        amount,
        redeem_mode,
        fss_ids,
    }) = ops[0].metadata.clone()
    else {
        panic!("wrong metadata variant: {:?}", ops[0].metadata);
    };
    assert!(validator.is_none(), "validator must be None from parser");
    // AtLeast carries `min_sui` decoded from the on-chain `balance::split`
    // guard; All and the unknown-partial mode emit no amount.
    match expected_mode {
        Some(sui_rosetta::types::RedeemMode::AtLeast) => {
            assert!(amount.is_some(), "AtLeast parse should recover min_sui");
        }
        _ => {
            assert!(amount.is_none(), "amount must be None from parser");
        }
    }
    assert_eq!(redeem_mode, expected_mode);
    assert_eq!(fss_ids.len(), expected_fss_count);
}

/// F=1, All mode — single FSS fully redeemed.
#[tokio::test]
async fn test_e2e_parse_merge_redeem_single_fss_all() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let validator = first_validator(&mut client).await;
    setup_n_fss(
        &rosetta_client,
        &mut client,
        keystore,
        &test_cluster,
        sender,
        validator,
        1,
    )
    .await;

    let ops =
        run_merge_redeem_and_parse(&rosetta_client, keystore, sender, validator, None, "All").await;
    assert_merge_redeem_parse_ops(&ops, sender, 1, Some(sui_rosetta::types::RedeemMode::All));
}

/// F=1, AtLeast partial redeem.
#[tokio::test]
async fn test_e2e_parse_merge_redeem_single_fss_atleast() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let validator = first_validator(&mut client).await;
    setup_n_fss(
        &rosetta_client,
        &mut client,
        keystore,
        &test_cluster,
        sender,
        validator,
        1,
    )
    .await;

    let ops = run_merge_redeem_and_parse(
        &rosetta_client,
        keystore,
        sender,
        validator,
        Some(500_000_000),
        "AtLeast",
    )
    .await;
    // The PTB carries a balance::split + balance::join guard, so the parser
    // can distinguish AtLeast from AtMost.
    assert_merge_redeem_parse_ops(
        &ops,
        sender,
        1,
        Some(sui_rosetta::types::RedeemMode::AtLeast),
    );
}

/// F=1, AtMost partial redeem.
#[tokio::test]
async fn test_e2e_parse_merge_redeem_single_fss_atmost() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let validator = first_validator(&mut client).await;
    setup_n_fss(
        &rosetta_client,
        &mut client,
        keystore,
        &test_cluster,
        sender,
        validator,
        1,
    )
    .await;

    let ops = run_merge_redeem_and_parse(
        &rosetta_client,
        keystore,
        sender,
        validator,
        Some(500_000_000),
        "AtMost",
    )
    .await;
    assert_merge_redeem_parse_ops(&ops, sender, 1, None);
}

/// F=3, All mode.
#[tokio::test]
async fn test_e2e_parse_merge_redeem_three_fss_all() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let validator = first_validator(&mut client).await;
    setup_n_fss(
        &rosetta_client,
        &mut client,
        keystore,
        &test_cluster,
        sender,
        validator,
        3,
    )
    .await;

    let ops =
        run_merge_redeem_and_parse(&rosetta_client, keystore, sender, validator, None, "All").await;
    assert_merge_redeem_parse_ops(&ops, sender, 3, Some(sui_rosetta::types::RedeemMode::All));
}

/// F=3, AtLeast partial.
#[tokio::test]
async fn test_e2e_parse_merge_redeem_three_fss_atleast() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let validator = first_validator(&mut client).await;
    setup_n_fss(
        &rosetta_client,
        &mut client,
        keystore,
        &test_cluster,
        sender,
        validator,
        3,
    )
    .await;

    let ops = run_merge_redeem_and_parse(
        &rosetta_client,
        keystore,
        sender,
        validator,
        Some(500_000_000),
        "AtLeast",
    )
    .await;
    assert_merge_redeem_parse_ops(
        &ops,
        sender,
        3,
        Some(sui_rosetta::types::RedeemMode::AtLeast),
    );
}

/// F=3, AtMost partial.
#[tokio::test]
async fn test_e2e_parse_merge_redeem_three_fss_atmost() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let validator = first_validator(&mut client).await;
    setup_n_fss(
        &rosetta_client,
        &mut client,
        keystore,
        &test_cluster,
        sender,
        validator,
        3,
    )
    .await;

    let ops = run_merge_redeem_and_parse(
        &rosetta_client,
        keystore,
        sender,
        validator,
        Some(500_000_000),
        "AtMost",
    )
    .await;
    assert_merge_redeem_parse_ops(&ops, sender, 3, None);
}

/// Multi-validator: FSS on both A and B; redeem only from A.
#[tokio::test]
async fn test_e2e_parse_merge_redeem_multi_validator_isolation() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let request = GetEpochRequest::latest().with_read_mask(FieldMask::from_paths(["system_state"]));
    let response = client
        .clone()
        .ledger_client()
        .get_epoch(request)
        .await
        .unwrap()
        .into_inner();
    let validators = response
        .epoch
        .and_then(|e| e.system_state)
        .unwrap()
        .validators
        .unwrap();
    let validator_a: SuiAddress = validators.active_validators[0].address().parse().unwrap();
    let validator_b: SuiAddress = validators.active_validators[1].address().parse().unwrap();

    // Stake on A and B, advance, convert BOTH to FSS.
    stake_via_rosetta(
        &rosetta_client,
        &mut client,
        keystore,
        sender,
        validator_a,
        1_000_000_000,
    )
    .await;
    stake_via_rosetta(
        &rosetta_client,
        &mut client,
        keystore,
        sender,
        validator_b,
        1_000_000_000,
    )
    .await;
    test_cluster.trigger_reconfiguration().await;
    convert_all_staked_to_fss_directly(&mut client, keystore, sender).await;

    // Redeem only from validator_a.
    let ops =
        run_merge_redeem_and_parse(&rosetta_client, keystore, sender, validator_a, None, "All")
            .await;
    // Exactly 1 FSS (A's).
    assert_merge_redeem_parse_ops(&ops, sender, 1, Some(sui_rosetta::types::RedeemMode::All));
}

// ================================================================================
// /block/transaction tests — submit then reparse via try_from_executed_transaction
// ================================================================================

async fn submit_and_assert_block_merge_redeem(
    rosetta_client: &rosetta_client::RosettaClient,
    client: &mut GrpcClient,
    keystore: &sui_keys::keystore::Keystore,
    sender: SuiAddress,
    validator: SuiAddress,
    amount: Option<u64>,
    mode: &str,
    expected_fss_count: usize,
    expected_mode: Option<sui_rosetta::types::RedeemMode>,
) {
    let mut metadata_value = serde_json::json!({
        "validator": validator.to_string(),
        "redeem_mode": mode,
    });
    if let Some(a) = amount {
        metadata_value["amount"] = serde_json::Value::String(a.to_string());
    }
    let ops = serde_json::from_value(json!([{
        "operation_identifier": {"index": 0},
        "type": "MergeAndRedeemFungibleStakedSui",
        "account": {"address": sender.to_string()},
        "metadata": {
            "MergeAndRedeemFungibleStakedSui": metadata_value,
        }
    }]))
    .unwrap();
    let submit_resp: TransactionIdentifierResponse = rosetta_client
        .rosetta_flow(&ops, keystore, None)
        .await
        .submit
        .unwrap()
        .unwrap();
    let tx_hash = submit_resp.transaction_identifier.hash.to_string();
    wait_for_transaction(client, &tx_hash).await.unwrap();

    let get_tx = GetTransactionRequest::default()
        .with_digest(tx_hash.clone())
        .with_read_mask(FieldMask::from_paths([
            "transaction",
            "effects",
            "events",
            "balance_changes",
        ]));
    let executed = client
        .clone()
        .ledger_client()
        .get_transaction(get_tx)
        .await
        .unwrap()
        .into_inner();
    let cache = CoinMetadataCache::new(client.clone(), NonZeroUsize::new(100).unwrap());
    let ops = Operations::try_from_executed_transaction(executed.transaction.unwrap(), &cache)
        .await
        .unwrap();
    let ops_vec: Vec<_> = ops.into_iter().collect();
    let typed = ops_vec
        .iter()
        .find(|o| o.type_ == OperationType::MergeAndRedeemFungibleStakedSui)
        .expect("expected typed MergeAndRedeem op");
    assert_eq!(typed.account.as_ref().unwrap().address, sender);
    let Some(sui_rosetta::operations::OperationMetadata::MergeAndRedeemFungibleStakedSui {
        redeem_mode,
        fss_ids,
        ..
    }) = typed.metadata.clone()
    else {
        panic!();
    };
    assert_eq!(redeem_mode, expected_mode);
    assert_eq!(fss_ids.len(), expected_fss_count);
    // Gas op is present.
    assert!(ops_vec.iter().any(|o| o.type_ == OperationType::Gas));
    // And SuiBalanceChange for sender — redemption produces liquid SUI.
    let sender_balance_change = ops_vec.iter().find(|o| {
        o.type_ == OperationType::SuiBalanceChange
            && o.account.as_ref().map(|a| a.address) == Some(sender)
    });
    assert!(
        sender_balance_change.is_some(),
        "MergeAndRedeem should produce a SuiBalanceChange for sender: {:?}",
        ops_vec
    );
}

#[tokio::test]
async fn test_e2e_block_merge_redeem_single_fss_all() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let validator = first_validator(&mut client).await;
    setup_n_fss(
        &rosetta_client,
        &mut client,
        keystore,
        &test_cluster,
        sender,
        validator,
        1,
    )
    .await;

    submit_and_assert_block_merge_redeem(
        &rosetta_client,
        &mut client,
        keystore,
        sender,
        validator,
        None,
        "All",
        1,
        Some(sui_rosetta::types::RedeemMode::All),
    )
    .await;
}

#[tokio::test]
async fn test_e2e_block_merge_redeem_single_fss_atleast() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let validator = first_validator(&mut client).await;
    setup_n_fss(
        &rosetta_client,
        &mut client,
        keystore,
        &test_cluster,
        sender,
        validator,
        1,
    )
    .await;

    submit_and_assert_block_merge_redeem(
        &rosetta_client,
        &mut client,
        keystore,
        sender,
        validator,
        Some(500_000_000),
        "AtLeast",
        1,
        Some(sui_rosetta::types::RedeemMode::AtLeast),
    )
    .await;
}

#[tokio::test]
async fn test_e2e_block_merge_redeem_single_fss_atmost() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let validator = first_validator(&mut client).await;
    setup_n_fss(
        &rosetta_client,
        &mut client,
        keystore,
        &test_cluster,
        sender,
        validator,
        1,
    )
    .await;

    submit_and_assert_block_merge_redeem(
        &rosetta_client,
        &mut client,
        keystore,
        sender,
        validator,
        Some(500_000_000),
        "AtMost",
        1,
        None,
    )
    .await;
}

#[tokio::test]
async fn test_e2e_block_merge_redeem_three_fss_all() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let validator = first_validator(&mut client).await;
    setup_n_fss(
        &rosetta_client,
        &mut client,
        keystore,
        &test_cluster,
        sender,
        validator,
        3,
    )
    .await;

    submit_and_assert_block_merge_redeem(
        &rosetta_client,
        &mut client,
        keystore,
        sender,
        validator,
        None,
        "All",
        3,
        Some(sui_rosetta::types::RedeemMode::All),
    )
    .await;
}

#[tokio::test]
async fn test_e2e_block_merge_redeem_three_fss_atleast() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let validator = first_validator(&mut client).await;
    setup_n_fss(
        &rosetta_client,
        &mut client,
        keystore,
        &test_cluster,
        sender,
        validator,
        3,
    )
    .await;

    submit_and_assert_block_merge_redeem(
        &rosetta_client,
        &mut client,
        keystore,
        sender,
        validator,
        Some(500_000_000),
        "AtLeast",
        3,
        Some(sui_rosetta::types::RedeemMode::AtLeast),
    )
    .await;
}

#[tokio::test]
async fn test_e2e_block_merge_redeem_three_fss_atmost() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let validator = first_validator(&mut client).await;
    setup_n_fss(
        &rosetta_client,
        &mut client,
        keystore,
        &test_cluster,
        sender,
        validator,
        3,
    )
    .await;

    submit_and_assert_block_merge_redeem(
        &rosetta_client,
        &mut client,
        keystore,
        sender,
        validator,
        Some(500_000_000),
        "AtMost",
        3,
        None,
    )
    .await;
}

#[tokio::test]
async fn test_e2e_block_merge_redeem_multi_validator_isolation() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let request = GetEpochRequest::latest().with_read_mask(FieldMask::from_paths(["system_state"]));
    let response = client
        .clone()
        .ledger_client()
        .get_epoch(request)
        .await
        .unwrap()
        .into_inner();
    let validators = response
        .epoch
        .and_then(|e| e.system_state)
        .unwrap()
        .validators
        .unwrap();
    let validator_a: SuiAddress = validators.active_validators[0].address().parse().unwrap();
    let validator_b: SuiAddress = validators.active_validators[1].address().parse().unwrap();

    stake_via_rosetta(
        &rosetta_client,
        &mut client,
        keystore,
        sender,
        validator_a,
        1_000_000_000,
    )
    .await;
    stake_via_rosetta(
        &rosetta_client,
        &mut client,
        keystore,
        sender,
        validator_b,
        1_000_000_000,
    )
    .await;
    test_cluster.trigger_reconfiguration().await;
    convert_all_staked_to_fss_directly(&mut client, keystore, sender).await;

    submit_and_assert_block_merge_redeem(
        &rosetta_client,
        &mut client,
        keystore,
        sender,
        validator_a,
        None,
        "All",
        1,
        Some(sui_rosetta::types::RedeemMode::All),
    )
    .await;
}

// ================================================================================
// PR 2 write-error tests — 5 new (2 cases covered by existing tests)
// ================================================================================
// Existing coverage:
// - test_redeem_no_fss_error (line 3936) covers #81 no_fss error
// - test_redeem_invalid_validator (line 3982) covers #84 invalid_validator

/// AtLeast amount exceeds available FSS SUI value → server should error.
#[tokio::test]
async fn test_e2e_merge_redeem_errors_atleast_exceeds_balance() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let validator = first_validator(&mut client).await;
    setup_n_fss(
        &rosetta_client,
        &mut client,
        keystore,
        &test_cluster,
        sender,
        validator,
        1,
    )
    .await;

    // Ask for much more SUI than 1 FSS can provide.
    let ops = serde_json::from_value(json!([{
        "operation_identifier": {"index": 0},
        "type": "MergeAndRedeemFungibleStakedSui",
        "account": {"address": sender.to_string()},
        "metadata": {
            "MergeAndRedeemFungibleStakedSui": {
                "validator": validator.to_string(),
                "amount": "100000000000000",
                "redeem_mode": "AtLeast",
            }
        }
    }]))
    .unwrap();
    let flow = rosetta_client.rosetta_flow(&ops, keystore, None).await;
    assert!(
        flow.metadata.as_ref().is_some_and(|r| r.is_err()),
        "expected metadata error for AtLeast exceeding balance, got: {:?}",
        flow.metadata
    );
}

/// AtMost amount = 0 → rejected before binary search runs.
#[tokio::test]
async fn test_e2e_merge_redeem_errors_atmost_zero_amount() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client.clone()).await;

    let validator = first_validator(&mut client).await;
    setup_n_fss(
        &rosetta_client,
        &mut client,
        keystore,
        &test_cluster,
        sender,
        validator,
        1,
    )
    .await;

    let ops = serde_json::from_value(json!([{
        "operation_identifier": {"index": 0},
        "type": "MergeAndRedeemFungibleStakedSui",
        "account": {"address": sender.to_string()},
        "metadata": {
            "MergeAndRedeemFungibleStakedSui": {
                "validator": validator.to_string(),
                "amount": "0",
                "redeem_mode": "AtMost",
            }
        }
    }]))
    .unwrap();
    let flow = rosetta_client.rosetta_flow(&ops, keystore, None).await;
    assert!(
        flow.metadata.as_ref().is_some_and(|r| r.is_err()),
        "expected metadata error for AtMost amount=0, got: {:?}",
        flow.metadata
    );
}

/// Missing amount in AtLeast/AtMost mode → write-side error at preprocess (MissingInput).
#[tokio::test]
async fn test_e2e_merge_redeem_errors_missing_amount() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;
    let client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client).await;

    let ops = serde_json::from_value(json!([{
        "operation_identifier": {"index": 0},
        "type": "MergeAndRedeemFungibleStakedSui",
        "account": {"address": sender.to_string()},
        "metadata": {
            "MergeAndRedeemFungibleStakedSui": {
                "validator": SuiAddress::random_for_testing_only().to_string(),
                "redeem_mode": "AtLeast",
            }
        }
    }]))
    .unwrap();
    let flow = rosetta_client.rosetta_flow(&ops, keystore, None).await;
    let preprocess = flow.preprocess.as_ref().expect("preprocess attempted");
    match preprocess {
        Err(_) => {}
        Ok(resp) => {
            // Masked error: empty response (untagged enum quirk in test harness).
            assert!(
                resp.options.is_none() && resp.required_public_keys.is_empty(),
                "expected rejection for missing amount, got success: {:?}",
                resp
            );
        }
    }
}

/// Zero amount for AtLeast/AtMost → write-side error "must be at least 1 MIST".
#[tokio::test]
async fn test_e2e_merge_redeem_errors_zero_amount() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;
    let client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client).await;

    let ops = serde_json::from_value(json!([{
        "operation_identifier": {"index": 0},
        "type": "MergeAndRedeemFungibleStakedSui",
        "account": {"address": sender.to_string()},
        "metadata": {
            "MergeAndRedeemFungibleStakedSui": {
                "validator": SuiAddress::random_for_testing_only().to_string(),
                "amount": "0",
                "redeem_mode": "AtLeast",
            }
        }
    }]))
    .unwrap();
    let flow = rosetta_client.rosetta_flow(&ops, keystore, None).await;
    let preprocess = flow.preprocess.as_ref().expect("preprocess attempted");
    match preprocess {
        Err(_) => {}
        Ok(resp) => {
            assert!(
                resp.options.is_none() && resp.required_public_keys.is_empty(),
                "expected rejection for zero amount, got success: {:?}",
                resp
            );
        }
    }
}

/// Missing redeem_mode in metadata → write-side error (MissingInput).
#[tokio::test]
async fn test_e2e_merge_redeem_errors_missing_redeem_mode() {
    let test_cluster = TestClusterBuilder::new().build().await;
    let sender = test_cluster.get_address_0();
    let keystore = &test_cluster.wallet.config.keystore;
    let client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let (rosetta_client, _handle) = start_rosetta_test_server(client).await;

    let ops = serde_json::from_value(json!([{
        "operation_identifier": {"index": 0},
        "type": "MergeAndRedeemFungibleStakedSui",
        "account": {"address": sender.to_string()},
        "metadata": {
            "MergeAndRedeemFungibleStakedSui": {
                "validator": SuiAddress::random_for_testing_only().to_string(),
            }
        }
    }]))
    .unwrap();
    let flow = rosetta_client.rosetta_flow(&ops, keystore, None).await;
    let preprocess = flow.preprocess.as_ref().expect("preprocess attempted");
    match preprocess {
        Err(_) => {}
        Ok(resp) => {
            assert!(
                resp.options.is_none() && resp.required_public_keys.is_empty(),
                "expected rejection for missing redeem_mode, got success: {:?}",
                resp
            );
        }
    }
}
