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
    let _client = test_cluster.wallet.get_client().await.unwrap();
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
