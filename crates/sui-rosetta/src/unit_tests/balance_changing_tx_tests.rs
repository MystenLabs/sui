// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::str::FromStr;

use anyhow::anyhow;
use move_core_types::identifier::Identifier;
use move_package::BuildConfig;
use rand::seq::{IteratorRandom, SliceRandom};
use signature::rand_core::OsRng;

use sui_config::utils::get_available_port;
use sui_sdk::crypto::{AccountKeystore, Keystore};
use sui_sdk::rpc_types::{
    OwnedObjectRef, SuiData, SuiEvent, SuiExecutionStatus, SuiPastObjectRead, SuiTransactionEffects,
};
use sui_sdk::{SuiClient, TransactionExecutionResult};
use sui_types::base_types::{
    ObjectDigest, ObjectID, ObjectRef, SequenceNumber, SuiAddress, TransactionDigest,
};
use sui_types::event::Event::TransferObject;
use sui_types::event::TransferType;
use sui_types::gas::GasCostSummary;
use sui_types::gas_coin::GasCoin;
use sui_types::messages::{
    CallArg, ExecuteTransactionRequestType, ExecutionStatus, InputObjectKind, MoveCall,
    MoveModulePublish, ObjectArg, Pay, SingleTransactionKind, Transaction, TransactionData,
    TransactionEffects, TransactionKind, TransferSui,
};
use sui_types::object::Owner;
use sui_types::SUI_FRAMEWORK_OBJECT_ID;
use test_utils::network::TestClusterBuilder;

use crate::operations::Operation;
use crate::state::extract_balance_changes_from_ops;
use crate::types::SignedValue;

#[test]
fn test_transfer_sui_null_amount() {
    let sender = SuiAddress::random_for_testing_only();
    let recipient = SuiAddress::random_for_testing_only();
    let gas = (
        ObjectID::random(),
        SequenceNumber::new(),
        ObjectDigest::random(),
    );
    let data = TransactionData::new_transfer_sui(recipient, sender, None, gas, 1000);

    let effect = TransactionEffects {
        status: ExecutionStatus::Success,
        gas_used: GasCostSummary {
            computation_cost: 100,
            storage_cost: 100,
            storage_rebate: 50,
        },
        shared_objects: vec![],
        transaction_digest: TransactionDigest::random(),
        created: vec![],
        mutated: vec![],
        unwrapped: vec![],
        deleted: vec![],
        wrapped: vec![],
        gas_object: (gas, Owner::AddressOwner(sender)),
        events: vec![TransferObject {
            package_id: SUI_FRAMEWORK_OBJECT_ID,
            transaction_module: Identifier::from_str("test").unwrap(),
            sender,
            recipient: Owner::AddressOwner(recipient),
            object_id: ObjectID::random(),
            version: Default::default(),
            type_: TransferType::Coin,
            amount: Some(10000),
        }],
        dependencies: vec![],
    };
    let ops = Operation::from_data_and_events(
        &data,
        &effect.status,
        &effect.events,
        effect.gas_used.net_gas_usage(),
        effect.gas_object.1,
        &[],
    )
    .unwrap();
    let balances = extract_balance_changes_from_ops(ops).unwrap();

    assert_eq!(SignedValue::neg(10150), balances[&sender]);
    assert_eq!(SignedValue::from(10000u64), balances[&recipient]);
}

#[tokio::test]
async fn test_all_transaction_type() {
    let port = get_available_port();
    let network = TestClusterBuilder::new()
        .set_fullnode_rpc_port(port)
        .build()
        .await
        .unwrap();
    let client = network.wallet.client;
    let keystore = &network.wallet.config.keystore;

    // Test Transfer Sui
    let sender = *network.accounts.choose(&mut OsRng::default()).unwrap();
    let recipient = *network.accounts.choose(&mut OsRng::default()).unwrap();
    let tx = SingleTransactionKind::TransferSui(TransferSui {
        recipient,
        amount: Some(50000),
    });
    test_transaction(&client, keystore, vec![recipient], sender, tx).await;

    // Test transfer sui whole coin
    let sender = *network.accounts.choose(&mut OsRng::default()).unwrap();
    let recipient = *network.accounts.choose(&mut OsRng::default()).unwrap();
    let tx = SingleTransactionKind::TransferSui(TransferSui {
        recipient,
        amount: None,
    });
    test_transaction(&client, keystore, vec![recipient], sender, tx).await;

    // Test transfer object
    let sender = *network.accounts.choose(&mut OsRng::default()).unwrap();
    let recipient = *network.accounts.choose(&mut OsRng::default()).unwrap();
    let object_ref = get_random_gas(&client, sender, vec![]).await;
    let tx = SingleTransactionKind::TransferObject(sui_types::messages::TransferObject {
        recipient,
        object_ref,
    });
    test_transaction(&client, keystore, vec![recipient], sender, tx).await;

    // Test publish
    let sender = *network.accounts.choose(&mut OsRng::default()).unwrap();
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../sui_programmability/examples/fungible_tokens");
    let package = sui_framework::build_move_package(&path, BuildConfig::default()).unwrap();
    let compiled_module = package
        .iter()
        .map(|m| {
            let mut module_bytes = Vec::new();
            m.serialize(&mut module_bytes).unwrap();
            module_bytes
        })
        .collect::<Vec<_>>();

    let tx = SingleTransactionKind::Publish(MoveModulePublish {
        modules: compiled_module,
    });
    let response = test_transaction(&client, keystore, vec![], sender, tx).await;

    // Test move call (reuse published module from above test)
    let effect = response.effects.clone().unwrap();
    let package = effect
        .events
        .iter()
        .find_map(|event| {
            if let SuiEvent::Publish { package_id, .. } = event {
                Some(package_id)
            } else {
                None
            }
        })
        .unwrap();
    // Get object ref from effect
    let package = effect
        .created
        .iter()
        .find(|obj| &obj.reference.object_id == package)
        .unwrap();
    let package = package.clone().reference.to_object_ref();
    // TODO: Improve tx response to make it easier to find objects.
    let treasury = find_module_object(&effect, "managed").unwrap();
    let treasury = treasury.clone().reference.to_object_ref();
    let recipient = *network.accounts.choose(&mut OsRng::default()).unwrap();
    let tx = SingleTransactionKind::Call(MoveCall {
        package,
        module: Identifier::from_str("managed").unwrap(),
        function: Identifier::from_str("mint").unwrap(),
        type_arguments: vec![],
        arguments: vec![
            CallArg::Object(ObjectArg::ImmOrOwnedObject(treasury)),
            CallArg::Pure(bcs::to_bytes(&10000u64).unwrap()),
            CallArg::Pure(bcs::to_bytes(&recipient).unwrap()),
        ],
    });
    test_transaction(&client, keystore, vec![], sender, tx).await;

    // Test spilt coin
    let sender = recipient;
    let coin = get_random_gas(&client, sender, vec![]).await;
    let tx = client
        .transaction_builder()
        .split_coin(sender, coin.0, vec![10000], None, 10000)
        .await
        .unwrap();
    let tx = tx.kind.single_transactions().next().unwrap().clone();
    test_transaction(&client, keystore, vec![], sender, tx).await;

    // Test merge coin
    let sender = *network.accounts.choose(&mut OsRng::default()).unwrap();
    let coin = get_random_gas(&client, sender, vec![]).await;
    let coin2 = get_random_gas(&client, sender, vec![coin.0]).await;
    let tx = client
        .transaction_builder()
        .merge_coins(sender, coin.0, coin2.0, None, 10000)
        .await
        .unwrap();
    let tx = tx.kind.single_transactions().next().unwrap().clone();
    test_transaction(&client, keystore, vec![], sender, tx).await;

    // Test Pay
    let sender = *network.accounts.choose(&mut OsRng::default()).unwrap();
    let coin = get_random_gas(&client, sender, vec![]).await;
    let tx = SingleTransactionKind::Pay(Pay {
        coins: vec![coin],
        recipients: vec![recipient],
        amounts: vec![10000],
    });
    test_transaction(&client, keystore, vec![recipient], sender, tx).await;
}

fn find_module_object(effects: &SuiTransactionEffects, module: &str) -> Option<OwnedObjectRef> {
    effects
        .events
        .iter()
        .find_map(|event| {
            if let SuiEvent::NewObject {
                transaction_module,
                object_id,
                ..
            } = event
            {
                if transaction_module == module {
                    return effects
                        .created
                        .iter()
                        .find(|obj| &obj.reference.object_id == object_id);
                }
            };
            None
        })
        .cloned()
}

async fn test_transaction(
    client: &SuiClient,
    keystore: &Keystore,
    addr_to_check: Vec<SuiAddress>,
    sender: SuiAddress,
    tx: SingleTransactionKind,
) -> TransactionExecutionResult {
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
    let gas = get_random_gas(client, sender, input_objects).await;
    let data = TransactionData::new(TransactionKind::Single(tx.clone()), sender, gas, 10000);

    let signature = keystore.sign(&data.signer(), &data.to_bytes()).unwrap();

    // Balance before execution
    let mut balances = BTreeMap::new();
    let mut addr_to_check = addr_to_check;
    addr_to_check.push(sender);
    for addr in addr_to_check {
        balances.insert(addr, get_balance(client, addr).await);
    }

    let response = client
        .quorum_driver()
        .execute_transaction(
            Transaction::new(data.clone(), signature),
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await
        .map_err(|e| anyhow!("TX execution failed for {tx:#?}, error : {e}"))
        .unwrap();

    let effect = response.effects.clone().unwrap();

    assert_eq!(
        SuiExecutionStatus::Success,
        effect.status,
        "TX execution failed for {:#?}",
        tx
    );

    let events = effect
        .events
        .clone()
        .into_iter()
        .map(|event| event.try_into())
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let net_gas_used = effect.gas_used.computation_cost + effect.gas_used.storage_cost
        - effect.gas_used.storage_rebate;

    let mut new_coins: Vec<(GasCoin, ObjectRef)> = vec![];
    for oref in &effect.created {
        let oref = &oref.reference;
        if let Ok(SuiPastObjectRead::VersionFound(obj)) = client
            .read_api()
            .try_get_parsed_past_object(oref.object_id, oref.version)
            .await
        {
            if let Ok(coin) = (&obj).try_into() {
                new_coins.push((coin, oref.to_object_ref()))
            }
        }
    }

    let ops = Operation::from_data_and_events(
        &data,
        &ExecutionStatus::Success,
        &events,
        net_gas_used as i64,
        effect.gas_object.owner,
        &new_coins,
    )
    .unwrap();
    let balances_from_ops = extract_balance_changes_from_ops(ops).unwrap();

    // get actual balance changed after transaction
    let mut actual_balance_change = BTreeMap::new();
    for (addr, balance) in balances {
        let new_balance = get_balance(client, addr).await as i64;
        let balance_changed = new_balance - balance as i64;
        actual_balance_change.insert(addr, SignedValue::from(balance_changed));
    }
    assert_eq!(
        actual_balance_change, balances_from_ops,
        "balance check failed for tx: {}\neffect:{:#?}",
        tx, effect
    );
    response
}

async fn get_random_gas(
    client: &SuiClient,
    sender: SuiAddress,
    except: Vec<ObjectID>,
) -> ObjectRef {
    let coins = client
        .read_api()
        .get_objects_owned_by_address(sender)
        .await
        .unwrap();
    let coin = coins
        .iter()
        .filter(|object| {
            object.type_ == GasCoin::type_().to_string() && !except.contains(&object.object_id)
        })
        .choose(&mut OsRng::default())
        .unwrap();
    (coin.object_id, coin.version, coin.digest)
}

async fn get_balance(client: &SuiClient, address: SuiAddress) -> u64 {
    let coins = client
        .read_api()
        .get_objects_owned_by_address(address)
        .await
        .unwrap();
    let mut balance = 0u64;
    for coin in coins {
        if coin.type_ == GasCoin::type_().to_string() {
            let object = client.read_api().get_object(coin.object_id).await.unwrap();
            let coin: GasCoin = object
                .into_object()
                .unwrap()
                .data
                .try_as_move()
                .unwrap()
                .deserialize()
                .unwrap();
            balance += coin.value()
        }
    }
    balance
}
