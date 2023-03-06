// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::str::FromStr;

use anyhow::anyhow;
use move_core_types::identifier::Identifier;
use rand::seq::{IteratorRandom, SliceRandom};
use signature::rand_core::OsRng;

use crate::operations::Operations;
use sui_framework_build::compiled_package::BuildConfig;
use sui_keys::keystore::AccountKeystore;
use sui_keys::keystore::Keystore;
use sui_sdk::rpc_types::{
    OwnedObjectRef, SuiData, SuiEvent, SuiExecutionStatus, SuiObjectDataOptions,
    SuiTransactionEffects, SuiTransactionEffectsAPI, SuiTransactionEvents, SuiTransactionResponse,
};
use sui_sdk::SuiClient;
use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress};
use sui_types::gas_coin::GasCoin;
use sui_types::intent::Intent;
use sui_types::messages::{
    CallArg, ExecuteTransactionRequestType, InputObjectKind, MoveCall, MoveModulePublish,
    ObjectArg, Pay, PayAllSui, PaySui, SingleTransactionKind, Transaction, TransactionData,
    TransactionDataAPI, TransactionKind, TransferSui,
};
use test_utils::network::TestClusterBuilder;

use crate::state::extract_balance_changes_from_ops;
use crate::types::{ConstructionMetadata, TransactionMetadata};

#[tokio::test]
async fn test_transfer_sui() {
    let network = TestClusterBuilder::new().build().await.unwrap();
    let client = network.wallet.get_client().await.unwrap();
    let keystore = &network.wallet.config.keystore;

    // Test Transfer Sui
    let sender = get_random_address(&network.accounts, vec![]);
    let recipient = get_random_address(&network.accounts, vec![sender]);
    let tx = SingleTransactionKind::TransferSui(TransferSui {
        recipient,
        amount: Some(50000),
    });
    test_transaction(
        &client,
        keystore,
        vec![recipient],
        sender,
        tx,
        None,
        10000,
        false,
    )
    .await;
}

#[tokio::test]
async fn test_transfer_sui_whole_coin() {
    let network = TestClusterBuilder::new().build().await.unwrap();
    let client = network.wallet.get_client().await.unwrap();
    let keystore = &network.wallet.config.keystore;

    // Test transfer sui whole coin
    let sender = get_random_address(&network.accounts, vec![]);
    let recipient = get_random_address(&network.accounts, vec![sender]);
    let tx = SingleTransactionKind::TransferSui(TransferSui {
        recipient,
        amount: None,
    });
    test_transaction(
        &client,
        keystore,
        vec![recipient],
        sender,
        tx,
        None,
        10000,
        false,
    )
    .await;
}

#[tokio::test]
async fn test_transfer_object() {
    let network = TestClusterBuilder::new().build().await.unwrap();
    let client = network.wallet.get_client().await.unwrap();
    let keystore = &network.wallet.config.keystore;

    // Test transfer object
    let sender = get_random_address(&network.accounts, vec![]);
    let recipient = get_random_address(&network.accounts, vec![sender]);
    let object_ref = get_random_sui(&client, sender, vec![]).await;
    let tx = SingleTransactionKind::TransferObject(sui_types::messages::TransferObject {
        recipient,
        object_ref,
    });
    test_transaction(
        &client,
        keystore,
        vec![recipient],
        sender,
        tx,
        None,
        10000,
        false,
    )
    .await;
}

#[tokio::test]
async fn test_publish_and_move_call() {
    let network = TestClusterBuilder::new().build().await.unwrap();
    let client = network.wallet.get_client().await.unwrap();
    let keystore = &network.wallet.config.keystore;

    // Test publish
    let sender = get_random_address(&network.accounts, vec![]);
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../sui_programmability/examples/fungible_tokens");
    let package = sui_framework::build_move_package(&path, BuildConfig::new_for_testing()).unwrap();
    let compiled_module = package
        .get_modules()
        .map(|m| {
            let mut module_bytes = Vec::new();
            m.serialize(&mut module_bytes).unwrap();
            module_bytes
        })
        .collect::<Vec<_>>();

    let tx = SingleTransactionKind::Publish(MoveModulePublish {
        modules: compiled_module,
    });
    let response =
        test_transaction(&client, keystore, vec![], sender, tx, None, 10000, false).await;
    let events = response.events;

    // Test move call (reuse published module from above test)
    let effect = response.effects;
    let package = events
        .data
        .iter()
        .find_map(|event| {
            if let SuiEvent::Publish { package_id, .. } = event {
                Some(package_id)
            } else {
                None
            }
        })
        .unwrap();

    // TODO: Improve tx response to make it easier to find objects.
    let treasury = find_module_object(&effect, &events, "managed", "TreasuryCap");
    let treasury = treasury.clone().reference.to_object_ref();
    let recipient = *network.accounts.choose(&mut OsRng::default()).unwrap();
    let tx = SingleTransactionKind::Call(MoveCall {
        package: *package,
        module: Identifier::from_str("managed").unwrap(),
        function: Identifier::from_str("mint").unwrap(),
        type_arguments: vec![],
        arguments: vec![
            CallArg::Object(ObjectArg::ImmOrOwnedObject(treasury)),
            CallArg::Pure(bcs::to_bytes(&10000u64).unwrap()),
            CallArg::Pure(bcs::to_bytes(&recipient).unwrap()),
        ],
    });
    test_transaction(&client, keystore, vec![], sender, tx, None, 10000, false).await;
}

#[tokio::test]
async fn test_split_coin() {
    let network = TestClusterBuilder::new().build().await.unwrap();
    let client = network.wallet.get_client().await.unwrap();
    let keystore = &network.wallet.config.keystore;

    // Test spilt coin
    let sender = get_random_address(&network.accounts, vec![]);
    let coin = get_random_sui(&client, sender, vec![]).await;
    let tx = client
        .transaction_builder()
        .split_coin(sender, coin.0, vec![100000], None, 10000)
        .await
        .unwrap();
    let tx = tx.into_kind().single_transactions().next().unwrap().clone();
    test_transaction(&client, keystore, vec![], sender, tx, None, 10000, false).await;
}

#[tokio::test]
async fn test_merge_coin() {
    let network = TestClusterBuilder::new().build().await.unwrap();
    let client = network.wallet.get_client().await.unwrap();
    let keystore = &network.wallet.config.keystore;

    // Test merge coin
    let sender = get_random_address(&network.accounts, vec![]);
    let coin = get_random_sui(&client, sender, vec![]).await;
    let coin2 = get_random_sui(&client, sender, vec![coin.0]).await;
    let tx = client
        .transaction_builder()
        .merge_coins(sender, coin.0, coin2.0, None, 10000)
        .await
        .unwrap();
    let tx = tx.into_kind().single_transactions().next().unwrap().clone();
    test_transaction(&client, keystore, vec![], sender, tx, None, 10000, false).await;
}

#[tokio::test]
async fn test_pay() {
    let network = TestClusterBuilder::new().build().await.unwrap();
    let client = network.wallet.get_client().await.unwrap();
    let keystore = &network.wallet.config.keystore;

    // Test Pay
    let sender = get_random_address(&network.accounts, vec![]);
    let recipient = get_random_address(&network.accounts, vec![sender]);
    let coin = get_random_sui(&client, sender, vec![]).await;
    let tx = SingleTransactionKind::Pay(Pay {
        coins: vec![coin],
        recipients: vec![recipient],
        amounts: vec![100000],
    });
    test_transaction(
        &client,
        keystore,
        vec![recipient],
        sender,
        tx,
        None,
        10000,
        false,
    )
    .await;
}

#[tokio::test]
async fn test_pay_multiple_coin_multiple_recipient() {
    let network = TestClusterBuilder::new().build().await.unwrap();
    let client = network.wallet.get_client().await.unwrap();
    let keystore = &network.wallet.config.keystore;

    // Test Pay multiple coin multiple recipient
    let sender = get_random_address(&network.accounts, vec![]);
    let recipient1 = get_random_address(&network.accounts, vec![sender]);
    let recipient2 = get_random_address(&network.accounts, vec![sender, recipient1]);
    let coin1 = get_random_sui(&client, sender, vec![]).await;
    let coin2 = get_random_sui(&client, sender, vec![coin1.0]).await;
    let tx = SingleTransactionKind::Pay(Pay {
        coins: vec![coin1, coin2],
        recipients: vec![recipient1, recipient2],
        amounts: vec![100000, 200000],
    });
    test_transaction(
        &client,
        keystore,
        vec![recipient1, recipient2],
        sender,
        tx,
        None,
        10000,
        false,
    )
    .await;
}

#[tokio::test]
async fn test_pay_sui_multiple_coin_same_recipient() {
    let network = TestClusterBuilder::new().build().await.unwrap();
    let client = network.wallet.get_client().await.unwrap();
    let keystore = &network.wallet.config.keystore;

    // Test Pay multiple coin same recipient
    let sender = get_random_address(&network.accounts, vec![]);
    let recipient1 = get_random_address(&network.accounts, vec![sender]);
    let coin1 = get_random_sui(&client, sender, vec![]).await;
    let coin2 = get_random_sui(&client, sender, vec![coin1.0]).await;
    let tx = SingleTransactionKind::PaySui(PaySui {
        coins: vec![coin1, coin2],
        recipients: vec![recipient1, recipient1, recipient1],
        amounts: vec![100000, 100000, 100000],
    });
    test_transaction(
        &client,
        keystore,
        vec![recipient1],
        sender,
        tx,
        Some(coin1),
        10000,
        false,
    )
    .await;
}

#[tokio::test]
async fn test_pay_sui() {
    let network = TestClusterBuilder::new().build().await.unwrap();
    let client = network.wallet.get_client().await.unwrap();
    let keystore = &network.wallet.config.keystore;

    // Test Pay Sui
    let sender = get_random_address(&network.accounts, vec![]);
    let recipient1 = get_random_address(&network.accounts, vec![sender]);
    let recipient2 = get_random_address(&network.accounts, vec![sender, recipient1]);
    let coin1 = get_random_sui(&client, sender, vec![]).await;
    let coin2 = get_random_sui(&client, sender, vec![coin1.0]).await;
    let tx = SingleTransactionKind::PaySui(PaySui {
        coins: vec![coin1, coin2],
        recipients: vec![recipient1, recipient2],
        amounts: vec![1000000, 2000000],
    });
    test_transaction(
        &client,
        keystore,
        vec![recipient1, recipient2],
        sender,
        tx,
        Some(coin1),
        10000,
        false,
    )
    .await;
}

#[tokio::test]
async fn test_failed_pay_sui() {
    let network = TestClusterBuilder::new().build().await.unwrap();
    let client = network.wallet.get_client().await.unwrap();
    let keystore = &network.wallet.config.keystore;

    // Test failed Pay Sui
    let sender = get_random_address(&network.accounts, vec![]);
    let recipient1 = get_random_address(&network.accounts, vec![sender]);
    let recipient2 = get_random_address(&network.accounts, vec![sender, recipient1]);
    let coin1 = get_random_sui(&client, sender, vec![]).await;
    let coin2 = get_random_sui(&client, sender, vec![coin1.0]).await;
    let tx = SingleTransactionKind::PaySui(PaySui {
        coins: vec![coin1, coin2],
        recipients: vec![recipient1, recipient2],
        amounts: vec![1000000, 2000000],
    });
    test_transaction(
        &client,
        keystore,
        vec![],
        sender,
        tx,
        Some(coin1),
        110,
        true,
    )
    .await;
}

#[tokio::test]
async fn test_delegate_sui() {
    let network = TestClusterBuilder::new().build().await.unwrap();
    let client = network.wallet.get_client().await.unwrap();
    let keystore = &network.wallet.config.keystore;

    // Test Delegate Sui
    let sender = get_random_address(&network.accounts, vec![]);
    let coin1 = get_random_sui(&client, sender, vec![]).await;
    let coin2 = get_random_sui(&client, sender, vec![coin1.0]).await;
    let validator = client.governance_api().get_validators().await.unwrap()[0].sui_address;

    let tx = client
        .transaction_builder()
        .request_add_delegation(
            sender,
            vec![coin1.0, coin2.0],
            Some(1000000),
            validator,
            None,
            100000,
        )
        .await
        .unwrap();
    let tx = tx.into_kind().into_single_transactions().next().unwrap();

    test_transaction(&client, keystore, vec![], sender, tx, None, 10000, false).await;
}

#[tokio::test]
async fn test_delegate_sui_with_none_amount() {
    let network = TestClusterBuilder::new().build().await.unwrap();
    let client = network.wallet.get_client().await.unwrap();
    let keystore = &network.wallet.config.keystore;

    // Test Delegate Sui
    let sender = get_random_address(&network.accounts, vec![]);
    let coin1 = get_random_sui(&client, sender, vec![]).await;
    let coin2 = get_random_sui(&client, sender, vec![coin1.0]).await;
    let validator = client.governance_api().get_validators().await.unwrap()[0].sui_address;

    let tx = client
        .transaction_builder()
        .request_add_delegation(
            sender,
            vec![coin1.0, coin2.0],
            None,
            validator,
            None,
            100000,
        )
        .await
        .unwrap();
    let tx = tx.into_kind().into_single_transactions().next().unwrap();

    test_transaction(&client, keystore, vec![], sender, tx, None, 10000, false).await;
}

#[tokio::test]
async fn test_pay_all_sui() {
    let network = TestClusterBuilder::new().build().await.unwrap();
    let client = network.wallet.get_client().await.unwrap();
    let keystore = &network.wallet.config.keystore;

    // Test Pay All Sui
    let sender = get_random_address(&network.accounts, vec![]);
    let recipient = get_random_address(&network.accounts, vec![sender]);
    let coin1 = get_random_sui(&client, sender, vec![]).await;
    let coin2 = get_random_sui(&client, sender, vec![coin1.0]).await;
    let tx = SingleTransactionKind::PayAllSui(PayAllSui {
        coins: vec![coin1, coin2],
        recipient,
    });
    test_transaction(
        &client,
        keystore,
        vec![recipient],
        sender,
        tx,
        Some(coin1),
        10000,
        false,
    )
    .await;
}

#[tokio::test]
async fn test_delegation_parsing() -> Result<(), anyhow::Error> {
    let network = TestClusterBuilder::new().build().await.unwrap();
    let client = network.wallet.get_client().await.unwrap();
    let sender = get_random_address(&network.accounts, vec![]);
    let coin1 = get_random_sui(&client, sender, vec![]).await;
    let coin2 = get_random_sui(&client, sender, vec![coin1.0]).await;
    let gas = get_random_sui(&client, sender, vec![coin1.0, coin2.0]).await;
    let validator = client.governance_api().get_validators().await.unwrap()[0].sui_address;

    let data = client
        .transaction_builder()
        .request_add_delegation(
            sender,
            vec![coin1.0, coin2.0],
            Some(100000),
            validator,
            Some(gas.0),
            10000,
        )
        .await?;

    let ops: Operations = data.clone().try_into()?;
    let metadata = ConstructionMetadata {
        tx_metadata: TransactionMetadata::Delegation {
            coins: vec![coin1, coin2],
            locked_until_epoch: None,
        },
        sender,
        gas,
        gas_price: client.read_api().get_reference_gas_price().await?,
        budget: 10000,
    };
    let parsed_data = ops
        .into_internal(Some(metadata.tx_metadata.clone().into()))?
        .try_into_data(metadata)?;
    assert_eq!(data, parsed_data);

    Ok(())
}

fn find_module_object(
    effects: &SuiTransactionEffects,
    events: &SuiTransactionEvents,
    module: &str,
    object_type_name: &str,
) -> OwnedObjectRef {
    let mut results: Vec<_> = events
        .data
        .iter()
        .filter_map(|event| {
            if let SuiEvent::NewObject {
                transaction_module,
                object_id,
                object_type,
                ..
            } = event
            {
                if transaction_module == module && object_type.contains(object_type_name) {
                    return effects
                        .created()
                        .iter()
                        .find(|obj| &obj.reference.object_id == object_id);
                }
            };
            None
        })
        .cloned()
        .collect();
    // Check that there is only one object found, and hence no ambiguity.
    assert_eq!(results.len(), 1);
    results.pop().unwrap()
}

// Record current Sui balance of an address then execute the transaction,
// and compare the balance change reported by the event against the actual balance change.
async fn test_transaction(
    client: &SuiClient,
    keystore: &Keystore,
    addr_to_check: Vec<SuiAddress>,
    sender: SuiAddress,
    tx: SingleTransactionKind,
    gas: Option<ObjectRef>,
    budget: u64,
    expect_fail: bool,
) -> SuiTransactionResponse {
    let gas = if let Some(gas) = gas {
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
        get_random_sui(client, sender, input_objects).await
    };

    let data = TransactionData::new_with_dummy_gas_price(
        TransactionKind::Single(tx.clone()),
        sender,
        gas,
        budget,
    );

    let signature = keystore
        .sign_secure(&data.sender(), &data, Intent::default())
        .unwrap();

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
            Transaction::from_data(data.clone(), Intent::default(), vec![signature])
                .verify()
                .unwrap(),
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await
        .map_err(|e| anyhow!("TX execution failed for {data:#?}, error : {e}"))
        .unwrap();

    let effects = &response.effects;

    if !expect_fail {
        assert_eq!(
            SuiExecutionStatus::Success,
            *effects.status(),
            "TX execution failed for {:#?}",
            data
        );
    } else {
        assert!(matches!(
            effects.status(),
            SuiExecutionStatus::Failure { .. }
        ));
    }

    let ops = response.clone().try_into().unwrap();
    let balances_from_ops = extract_balance_changes_from_ops(ops);

    // get actual balance changed after transaction
    let mut actual_balance_change = HashMap::new();
    for (addr, balance) in balances {
        let new_balance = get_balance(client, addr).await as i128;
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

async fn get_random_sui(
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

fn get_random_address(addresses: &[SuiAddress], except: Vec<SuiAddress>) -> SuiAddress {
    *addresses
        .iter()
        .filter(|addr| !except.contains(*addr))
        .choose(&mut OsRng::default())
        .unwrap()
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
            let object = client
                .read_api()
                .get_object_with_options(coin.object_id, SuiObjectDataOptions::new().with_bcs())
                .await
                .unwrap();
            let coin: GasCoin = object
                .into_object()
                .unwrap()
                .bcs
                .unwrap()
                .try_as_move()
                .unwrap()
                .deserialize()
                .unwrap();
            balance += coin.value()
        }
    }
    balance
}
