// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod test_utils;

use anyhow::anyhow;
use move_core_types::identifier::Identifier;
use prost_types::FieldMask;
use rand::seq::{IteratorRandom, SliceRandom};
use serde_json::json;
use shared_crypto::intent::Intent;
use signature::rand_core::OsRng;
use std::collections::{BTreeMap, HashMap};
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::str::FromStr;
use sui_keys::keystore::AccountKeystore;
use sui_keys::keystore::Keystore;
use sui_move_build::BuildConfig;
use sui_rosetta::CoinMetadataCache;
use sui_rosetta::operations::Operations;
use sui_rosetta::types::{ConstructionMetadata, OperationStatus, OperationType};
use sui_rpc::client::Client as GrpcClient;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::{
    ExecutedTransaction, GetBalanceRequest, GetEpochRequest, GetTransactionRequest,
};
use sui_types::SUI_SYSTEM_PACKAGE_ID;
use sui_types::base_types::{FullObjectRef, ObjectRef, SuiAddress};
use sui_types::gas_coin::GAS;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::sui_system_state::SUI_SYSTEM_MODULE_NAME;
use sui_types::transaction::{
    CallArg, InputObjectKind, ObjectArg, ProgrammableTransaction, TEST_ONLY_GAS_UNIT_FOR_GENERIC,
    TEST_ONLY_GAS_UNIT_FOR_HEAVY_COMPUTATION_STORAGE, TEST_ONLY_GAS_UNIT_FOR_SPLIT_COIN,
    TEST_ONLY_GAS_UNIT_FOR_STAKING, TEST_ONLY_GAS_UNIT_FOR_TRANSFER, Transaction, TransactionData,
    TransactionDataAPI, TransactionKind,
};
use test_cluster::TestClusterBuilder;
use test_utils::{execute_transaction, find_module_object, find_published_package, get_random_sui};

#[tokio::test]
async fn test_transfer_sui() {
    let network = TestClusterBuilder::new().build().await;
    let keystore = &network.wallet.config.keystore;
    let mut client = GrpcClient::new(network.rpc_url()).unwrap();
    let rgp = client.get_reference_gas_price().await.unwrap();

    let addresses = network.get_addresses();
    let sender = get_random_address(&addresses, vec![]);
    let recipient = get_random_address(&addresses, vec![sender]);
    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        builder.transfer_sui(recipient, Some(50000));
        builder.finish()
    };
    test_transaction(
        &network,
        keystore,
        vec![recipient],
        sender,
        pt,
        vec![],
        rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
        rgp,
        false,
    )
    .await;
}

#[tokio::test]
async fn test_transfer_sui_whole_coin() {
    let network = TestClusterBuilder::new().build().await;
    let keystore = &network.wallet.config.keystore;
    let mut client = GrpcClient::new(network.rpc_url()).unwrap();
    let rgp = client.get_reference_gas_price().await.unwrap();

    let addresses = network.get_addresses();
    let sender = get_random_address(&addresses, vec![]);
    let recipient = get_random_address(&addresses, vec![sender]);
    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        builder.transfer_sui(recipient, None);
        builder.finish()
    };
    test_transaction(
        &network,
        keystore,
        vec![recipient],
        sender,
        pt,
        vec![],
        rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
        rgp,
        false,
    )
    .await;
}

#[tokio::test]
async fn test_transfer_object() {
    let network = TestClusterBuilder::new().build().await;
    let keystore = &network.wallet.config.keystore;
    let mut client = GrpcClient::new(network.rpc_url()).unwrap();
    let rgp = client.get_reference_gas_price().await.unwrap();

    let addresses = network.get_addresses();
    let sender = get_random_address(&addresses, vec![]);
    let recipient = get_random_address(&addresses, vec![sender]);
    let object_ref = get_random_sui(&mut client, sender, vec![]).await;
    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        builder
            .transfer_object(recipient, FullObjectRef::from_fastpath_ref(object_ref))
            .unwrap();
        builder.finish()
    };
    test_transaction(
        &network,
        keystore,
        vec![recipient],
        sender,
        pt,
        vec![],
        rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
        rgp,
        false,
    )
    .await;
}

#[tokio::test]
async fn test_publish_and_move_call() {
    let network = TestClusterBuilder::new().build().await;
    let keystore = &network.wallet.config.keystore;
    let mut client = GrpcClient::new(network.rpc_url()).unwrap();
    let rgp = client.get_reference_gas_price().await.unwrap();

    let addresses = network.get_addresses();
    let sender = get_random_address(&addresses, vec![]);
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["..", "..", "examples", "move", "coin"]);
    let compiled_package = BuildConfig::new_for_testing().build(&path).unwrap();
    let compiled_modules_bytes =
        compiled_package.get_package_bytes(/* with_unpublished_deps */ false);
    let dependencies = compiled_package.get_dependency_storage_package_ids();

    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        builder.publish_immutable(compiled_modules_bytes, dependencies);
        builder.finish()
    };
    let response = test_transaction(
        &network,
        keystore,
        vec![],
        sender,
        pt,
        vec![],
        rgp * TEST_ONLY_GAS_UNIT_FOR_HEAVY_COMPUTATION_STORAGE,
        rgp,
        false,
    )
    .await;
    let object_changes = response.effects().changed_objects.clone();

    let package = find_published_package(&object_changes).unwrap();

    let (_, treasury) = find_module_object(&object_changes, |type_str| {
        // Check if this is a TreasuryCap for MY_COIN (but not MY_COIN_NEW)
        type_str.contains("TreasuryCap") && type_str.contains("::my_coin::MY_COIN>")
    })
    .unwrap();
    let recipient = *addresses.choose(&mut OsRng).unwrap();
    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        builder
            .move_call(
                package,
                Identifier::from_str("my_coin").unwrap(),
                Identifier::from_str("mint").unwrap(),
                vec![],
                vec![
                    CallArg::Object(ObjectArg::ImmOrOwnedObject(treasury)),
                    CallArg::Pure(bcs::to_bytes(&10000u64).unwrap()),
                    CallArg::Pure(bcs::to_bytes(&recipient).unwrap()),
                ],
            )
            .unwrap();
        builder.finish()
    };

    test_transaction(
        &network,
        keystore,
        vec![],
        sender,
        pt,
        vec![],
        rgp * TEST_ONLY_GAS_UNIT_FOR_GENERIC,
        rgp,
        false,
    )
    .await;
}

#[tokio::test]
async fn test_split_coin() {
    let network = TestClusterBuilder::new().build().await;
    let mut client = GrpcClient::new(network.rpc_url()).unwrap();
    let keystore = &network.wallet.config.keystore;
    let rgp = client.get_reference_gas_price().await.unwrap();

    let sender = get_random_address(&network.get_addresses(), vec![]);
    let coin = get_random_sui(&mut client, sender, vec![]).await;
    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        builder.split_coin(sender, coin, vec![100000]);
        builder.finish()
    };
    test_transaction(
        &network,
        keystore,
        vec![],
        sender,
        pt,
        vec![],
        rgp * TEST_ONLY_GAS_UNIT_FOR_SPLIT_COIN,
        rgp,
        false,
    )
    .await;
}

#[tokio::test]
async fn test_merge_coin() {
    let network = TestClusterBuilder::new().build().await;
    let keystore = &network.wallet.config.keystore;
    let mut client = GrpcClient::new(network.rpc_url()).unwrap();
    let rgp = client.get_reference_gas_price().await.unwrap();

    let sender = get_random_address(&network.get_addresses(), vec![]);
    let coin = get_random_sui(&mut client, sender, vec![]).await;
    let coin2 = get_random_sui(&mut client, sender, vec![coin.0]).await;
    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        builder.merge_coins(coin, vec![coin2]).unwrap();
        builder.finish()
    };
    test_transaction(
        &network,
        keystore,
        vec![],
        sender,
        pt,
        vec![],
        rgp * TEST_ONLY_GAS_UNIT_FOR_GENERIC,
        rgp,
        false,
    )
    .await;
}

#[tokio::test]
async fn test_pay() {
    let network = TestClusterBuilder::new().build().await;
    let keystore = &network.wallet.config.keystore;
    let mut client = GrpcClient::new(network.rpc_url()).unwrap();
    let rgp = client.get_reference_gas_price().await.unwrap();

    let addresses = network.get_addresses();
    let sender = get_random_address(&addresses, vec![]);
    let recipient = get_random_address(&addresses, vec![sender]);
    let coin = get_random_sui(&mut client, sender, vec![]).await;
    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        builder
            .pay(vec![coin], vec![recipient], vec![100000])
            .unwrap();
        builder.finish()
    };
    test_transaction(
        &network,
        keystore,
        vec![recipient],
        sender,
        pt,
        vec![],
        rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
        rgp,
        false,
    )
    .await;
}

#[tokio::test]
async fn test_pay_multiple_coin_multiple_recipient() {
    let network = TestClusterBuilder::new().build().await;
    let keystore = &network.wallet.config.keystore;
    let mut client = GrpcClient::new(network.rpc_url()).unwrap();
    let rgp = client.get_reference_gas_price().await.unwrap();

    let addresses = network.get_addresses();
    let sender = get_random_address(&addresses, vec![]);
    let recipient1 = get_random_address(&addresses, vec![sender]);
    let recipient2 = get_random_address(&addresses, vec![sender, recipient1]);
    let coin1 = get_random_sui(&mut client, sender, vec![]).await;
    let coin2 = get_random_sui(&mut client, sender, vec![coin1.0]).await;
    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        builder
            .pay(
                vec![coin1, coin2],
                vec![recipient1, recipient2],
                vec![100000, 200000],
            )
            .unwrap();
        builder.finish()
    };
    test_transaction(
        &network,
        keystore,
        vec![recipient1, recipient2],
        sender,
        pt,
        vec![],
        rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
        rgp,
        false,
    )
    .await;
}

#[tokio::test]
async fn test_pay_sui_multiple_coin_same_recipient() {
    let network = TestClusterBuilder::new().build().await;
    let keystore = &network.wallet.config.keystore;
    let mut client = GrpcClient::new(network.rpc_url()).unwrap();
    let rgp = client.get_reference_gas_price().await.unwrap();

    let addresses = network.get_addresses();
    let sender = get_random_address(&addresses, vec![]);
    let recipient1 = get_random_address(&addresses, vec![sender]);
    let coin1 = get_random_sui(&mut client, sender, vec![]).await;
    let coin2 = get_random_sui(&mut client, sender, vec![coin1.0]).await;
    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        builder
            .pay_sui(
                vec![recipient1, recipient1, recipient1],
                vec![100000, 100000, 100000],
            )
            .unwrap();
        builder.finish()
    };
    test_transaction(
        &network,
        keystore,
        vec![recipient1],
        sender,
        pt,
        vec![coin1, coin2],
        rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
        rgp,
        false,
    )
    .await;
}

#[tokio::test]
async fn test_pay_sui() {
    let network = TestClusterBuilder::new().build().await;
    let keystore = &network.wallet.config.keystore;
    let mut client = GrpcClient::new(network.rpc_url()).unwrap();
    let rgp = client.get_reference_gas_price().await.unwrap();

    let addresses = network.get_addresses();
    let sender = get_random_address(&addresses, vec![]);
    let recipient1 = get_random_address(&addresses, vec![sender]);
    let recipient2 = get_random_address(&addresses, vec![sender, recipient1]);
    let coin1 = get_random_sui(&mut client, sender, vec![]).await;
    let coin2 = get_random_sui(&mut client, sender, vec![coin1.0]).await;
    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        builder
            .pay_sui(vec![recipient1, recipient2], vec![1000000, 2000000])
            .unwrap();
        builder.finish()
    };
    test_transaction(
        &network,
        keystore,
        vec![recipient1, recipient2],
        sender,
        pt,
        vec![coin1, coin2],
        rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
        rgp,
        false,
    )
    .await;
}

#[tokio::test]
async fn test_failed_pay_sui() {
    let network = TestClusterBuilder::new().build().await;
    let keystore = &network.wallet.config.keystore;
    let mut client = GrpcClient::new(network.rpc_url()).unwrap();
    let rgp = client.get_reference_gas_price().await.unwrap();

    let addresses = network.get_addresses();
    let sender = get_random_address(&addresses, vec![]);
    let recipient1 = get_random_address(&addresses, vec![sender]);
    let recipient2 = get_random_address(&addresses, vec![sender, recipient1]);
    let coin1 = get_random_sui(&mut client, sender, vec![]).await;
    let coin2 = get_random_sui(&mut client, sender, vec![coin1.0]).await;
    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        builder
            .pay_sui(vec![recipient1, recipient2], vec![1000000, 2000000])
            .unwrap();
        builder.finish()
    };
    test_transaction(
        &network,
        keystore,
        vec![],
        sender,
        pt,
        vec![coin1, coin2],
        2000000,
        rgp,
        true,
    )
    .await;
}

#[tokio::test]
async fn test_stake_sui() {
    let network = TestClusterBuilder::new().build().await;
    let keystore = &network.wallet.config.keystore;
    let mut client = GrpcClient::new(network.rpc_url()).unwrap();
    let rgp = client.get_reference_gas_price().await.unwrap();

    let sender = get_random_address(&network.get_addresses(), vec![]);
    let mut client = GrpcClient::new(network.rpc_url()).unwrap();
    let coin1 = get_random_sui(&mut client, sender, vec![]).await;
    let coin2 = get_random_sui(&mut client, sender, vec![coin1.0]).await;
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
    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        builder.merge_coins(coin1, vec![coin2]).unwrap();

        builder
            .move_call(
                SUI_SYSTEM_PACKAGE_ID,
                SUI_SYSTEM_MODULE_NAME.to_owned(),
                Identifier::from_str("request_add_stake").unwrap(),
                vec![],
                vec![
                    CallArg::SUI_SYSTEM_MUT,
                    CallArg::Object(ObjectArg::ImmOrOwnedObject(coin1)),
                    CallArg::Pure(bcs::to_bytes(&validator).unwrap()),
                ],
            )
            .unwrap();
        builder.finish()
    };
    test_transaction(
        &network,
        keystore,
        vec![],
        sender,
        pt,
        vec![],
        rgp * TEST_ONLY_GAS_UNIT_FOR_STAKING,
        rgp,
        false,
    )
    .await;
}

#[tokio::test]
async fn test_stake_sui_with_none_amount() {
    let network = TestClusterBuilder::new().build().await;
    let keystore = &network.wallet.config.keystore;
    let mut client = GrpcClient::new(network.rpc_url()).unwrap();
    let rgp = client.get_reference_gas_price().await.unwrap();

    let sender = get_random_address(&network.get_addresses(), vec![]);
    let coin1 = get_random_sui(&mut client, sender, vec![]).await;
    let coin2 = get_random_sui(&mut client, sender, vec![coin1.0]).await;
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
    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        builder.merge_coins(coin1, vec![coin2]).unwrap();

        builder
            .move_call(
                SUI_SYSTEM_PACKAGE_ID,
                SUI_SYSTEM_MODULE_NAME.to_owned(),
                Identifier::from_str("request_add_stake").unwrap(),
                vec![],
                vec![
                    CallArg::SUI_SYSTEM_MUT,
                    CallArg::Object(ObjectArg::ImmOrOwnedObject(coin1)),
                    CallArg::Pure(bcs::to_bytes(&validator).unwrap()),
                ],
            )
            .unwrap();
        builder.finish()
    };
    test_transaction(
        &network,
        keystore,
        vec![],
        sender,
        pt,
        vec![],
        rgp * TEST_ONLY_GAS_UNIT_FOR_STAKING,
        rgp,
        false,
    )
    .await;
}

#[tokio::test]
async fn test_pay_all_sui() {
    let network = TestClusterBuilder::new().build().await;
    let keystore = &network.wallet.config.keystore;
    let mut client = GrpcClient::new(network.rpc_url()).unwrap();
    let rgp = client.get_reference_gas_price().await.unwrap();

    let addresses = network.get_addresses();
    let sender = get_random_address(&addresses, vec![]);
    let recipient = get_random_address(&addresses, vec![sender]);
    let mut client = GrpcClient::new(network.rpc_url()).unwrap();
    let coin1 = get_random_sui(&mut client, sender, vec![]).await;
    let coin2 = get_random_sui(&mut client, sender, vec![coin1.0]).await;
    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        builder.pay_all_sui(recipient);
        builder.finish()
    };
    test_transaction(
        &network,
        keystore,
        vec![recipient],
        sender,
        pt,
        vec![coin1, coin2],
        rgp * TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
        rgp,
        false,
    )
    .await;
}

#[tokio::test]
async fn test_delegation_parsing() -> Result<(), anyhow::Error> {
    let network = TestClusterBuilder::new().build().await;
    let mut client = GrpcClient::new(network.rpc_url()).unwrap();
    let rgp = client.get_reference_gas_price().await.unwrap();
    let sender = get_random_address(&network.get_addresses(), vec![]);
    let mut client = GrpcClient::new(network.rpc_url()).unwrap();
    let gas = get_random_sui(&mut client, sender, vec![]).await;
    let total_coin_value = 0i128;
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

    let ops: Operations = serde_json::from_value(json!(
        [{
            "operation_identifier":{"index":0},
            "type":"Stake",
            "account": { "address" : sender.to_string() },
            "amount" : { "value": "-100000" , "currency": { "symbol": "SUI", "decimals": 9}},
            "metadata": { "Stake" : {"validator": validator.to_string()} }
        }]
    ))
    .unwrap();
    let metadata = ConstructionMetadata {
        sender,
        gas_coins: vec![gas],
        extra_gas_coins: vec![],
        objects: vec![],
        party_objects: vec![],
        total_coin_value,
        gas_price: rgp,
        budget: rgp * TEST_ONLY_GAS_UNIT_FOR_STAKING,
        currency: None,
    };
    let parsed_data = ops.clone().into_internal()?.try_into_data(metadata)?;

    let proto_tx: sui_rpc::proto::sui::rpc::v2::Transaction = parsed_data.clone().into();
    let parsed_ops = Operations::new(Operations::from_transaction(
        proto_tx
            .kind
            .ok_or_else(|| anyhow::anyhow!("Transaction missing kind"))?,
        parsed_data.sender(),
        None,
    )?);

    assert_eq!(
        ops, parsed_ops,
        "expected {:#?}, got: {:#?}",
        ops, parsed_ops
    );

    Ok(())
}

// Record current Sui balance of an address then execute the transaction,
// and compare the balance change reported by the event against the actual balance change.
async fn test_transaction(
    network: &test_cluster::TestCluster,
    keystore: &Keystore,
    addr_to_check: Vec<SuiAddress>,
    sender: SuiAddress,
    tx: ProgrammableTransaction,
    gas: Vec<ObjectRef>,
    gas_budget: u64,
    gas_price: u64,
    expect_fail: bool,
) -> ExecutedTransaction {
    let mut client = GrpcClient::new(network.rpc_url()).unwrap();
    let gas = if !gas.is_empty() {
        gas
    } else {
        let input_objects = tx
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
        vec![get_random_sui(&mut client, sender, input_objects).await]
    };

    let data = TransactionData::new_with_gas_coins(
        TransactionKind::programmable(tx.clone()),
        sender,
        gas,
        gas_budget,
        gas_price,
    );

    let signature = keystore
        .sign_secure(&data.sender(), &data, Intent::sui_transaction())
        .await
        .unwrap();

    let mut balances = BTreeMap::new();
    let mut addr_to_check = addr_to_check;
    addr_to_check.push(sender);
    for addr in addr_to_check {
        balances.insert(addr, get_balance(&mut client, addr).await);
    }

    let response = execute_transaction(
        &mut client.clone(),
        &Transaction::from_data(data.clone(), vec![signature]),
    )
    .await
    .map_err(|e| anyhow!("TX execution failed for {data:#?}, error : {e}"))
    .unwrap();

    let effects = response.effects();

    if !expect_fail {
        assert!(
            effects.status().success(),
            "TX execution failed for {:#?}",
            data
        );
    } else {
        assert!(!effects.status().success());
    }
    let client = GrpcClient::new(network.rpc_url()).unwrap();
    let tx_digest = response.digest().to_string();

    let grpc_request = GetTransactionRequest::default()
        .with_digest(tx_digest)
        .with_read_mask(FieldMask::from_paths([
            "digest",
            "transaction",
            "effects",
            "balance_changes",
            "events.events.event_type",
            "events.events.json",
            "events.events.contents",
        ]));

    let coin_cache = CoinMetadataCache::new(client.clone(), NonZeroUsize::new(2).unwrap());
    let mut client = client;
    let grpc_response = client
        .ledger_client()
        .get_transaction(grpc_request)
        .await
        .unwrap()
        .into_inner();
    let executed_tx = grpc_response
        .transaction
        .expect("Response transaction should not be empty");
    let ops = Operations::try_from_executed_transaction(executed_tx, &coin_cache)
        .await
        .unwrap();
    let balances_from_ops = extract_balance_changes_from_ops(ops);

    // get actual balance changed after transaction
    // Only check balances for addresses that appear in the operations
    let mut actual_balance_change = HashMap::new();
    for (addr, balance) in balances {
        let new_balance = get_balance(&mut client, addr).await as i128;
        let balance_changed = new_balance - balance as i128;
        actual_balance_change.insert(addr, balance_changed);
    }
    assert_eq!(
        actual_balance_change, balances_from_ops,
        "balance check failed for tx: {}\neffect:{:#?}",
        tx, effects
    );
    response
}

fn extract_balance_changes_from_ops(ops: Operations) -> HashMap<SuiAddress, i128> {
    ops.into_iter()
        .fold(HashMap::<SuiAddress, i128>::new(), |mut changes, op| {
            if let Some(OperationStatus::Success) = op.status {
                match op.type_ {
                    OperationType::SuiBalanceChange
                    | OperationType::Gas
                    | OperationType::PaySui
                    | OperationType::PayCoin
                    | OperationType::StakeReward
                    | OperationType::StakePrinciple
                    | OperationType::Stake => {
                        if let (Some(addr), Some(amount)) = (op.account, op.amount) {
                            // Todo: amend this method and tests to cover other coin types too (eg. test_publish_and_move_call also mints MY_COIN)
                            if amount.currency.metadata.coin_type
                                == sui_types::TypeTag::from(GAS::type_()).to_canonical_string(true)
                            {
                                *changes.entry(addr.address).or_default() += amount.value
                            }
                        }
                    }
                    _ => {}
                };
            }
            changes
        })
}

fn get_random_address(addresses: &[SuiAddress], except: Vec<SuiAddress>) -> SuiAddress {
    *addresses
        .iter()
        .filter(|addr| !except.contains(*addr))
        .choose(&mut OsRng)
        .unwrap()
}

async fn get_balance(client: &mut GrpcClient, address: SuiAddress) -> u64 {
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
