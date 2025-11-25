// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Result, anyhow};
use prost_types::FieldMask;

use crate::test_utils::{
    execute_transaction, extract_object_ref_from_changed_objects, get_all_coins, get_coin_value,
};
use shared_crypto::intent::Intent;
use sui_keys::keystore::{AccountKeystore, Keystore};
use sui_rpc::client::Client as GrpcClient;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::changed_object::IdOperation;
use sui_rpc::proto::sui::rpc::v2::get_object_result;
use sui_rpc::proto::sui::rpc::v2::{BatchGetObjectsRequest, ExecutedTransaction, GetObjectRequest};
use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress};
use sui_types::object::Object;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{
    Argument, Command, ObjectArg, Transaction, TransactionData, TransactionDataAPI,
};
use test_cluster::TestClusterBuilder;

pub const DEFAULT_GAS_BUDGET: u64 = 900_000_000;
const DEFAULT_INIT_COIN_BALANCE: u64 = 30_000_000_000_000_000;
const MAX_NEW_COINS: usize = 511; // maximum arguments in a programmable transaction command is 511

pub async fn split_coins(
    keystore: &Keystore,
    sender: SuiAddress,
    coin: ObjectRef,
    amounts: &[u64],
    gas: Option<ObjectRef>,
    reference_gas_price: Option<u64>,
    budget: Option<u64>,
    client: &mut GrpcClient,
) -> Result<ExecutedTransaction> {
    if amounts.len() > MAX_NEW_COINS {
        return Err(anyhow!("Max new coins: {}", MAX_NEW_COINS));
    }
    let budget = budget.unwrap_or(DEFAULT_GAS_BUDGET);
    let reference_gas_price = match reference_gas_price {
        Some(price) => price,
        None => client.get_reference_gas_price().await.unwrap(),
    };
    let mut ptb = ProgrammableTransactionBuilder::new();

    let amounts_len = amounts.len();
    let amounts = amounts
        .iter()
        .map(|amount| ptb.pure(amount))
        .collect::<Result<Vec<_>>>()?;
    let (split_coin, gas) = match gas {
        Some(gas) => (ptb.obj(ObjectArg::ImmOrOwnedObject(coin))?, gas),
        None => (Argument::GasCoin, coin),
    };
    ptb.command(Command::SplitCoins(split_coin, amounts));
    let sender_arg = ptb.pure(sender)?;
    let results = (0..amounts_len)
        .map(|i| Argument::NestedResult(0, i as u16))
        .collect::<Vec<_>>();
    ptb.command(Command::TransferObjects(results, sender_arg));
    let builder = ptb.finish();

    // Sign transaction
    let tx_data =
        TransactionData::new_programmable(sender, vec![gas], builder, budget, reference_gas_price);
    let sig = keystore
        .sign_secure(&tx_data.sender(), &tx_data, Intent::sui_transaction())
        .await?;

    let signed_transaction = Transaction::from_data(tx_data, vec![sig]);
    let grpc_resp = execute_transaction(client, &signed_transaction).await?;

    Ok(grpc_resp)
}

pub async fn make_change(
    client: &mut GrpcClient,
    keystore: &Keystore,
    sender: SuiAddress,
    coin_object: &Object,
    gas: Option<ObjectRef>,
    amount_per_change: u64,
) -> Result<Vec<ExecutedTransaction>> {
    let coin_value = get_coin_value(coin_object);
    let remainder = coin_value % amount_per_change;
    let n_new_coins = (coin_value / amount_per_change) as usize - (remainder == 0) as usize;
    let vecs_remainder = n_new_coins % MAX_NEW_COINS;
    let n_vecs = n_new_coins / MAX_NEW_COINS;
    assert!(n_new_coins as u64 * amount_per_change < coin_value);

    let mut amounts_vec = vec![vec![amount_per_change; MAX_NEW_COINS]; n_vecs];
    amounts_vec.push(vec![amount_per_change; vecs_remainder]);

    let mut responses = Vec::with_capacity(amounts_vec.len());
    let mut coin_ref = coin_object.compute_object_reference();
    let mut gas_ref = gas;
    let ref_gas_price = client.get_reference_gas_price().await.unwrap();
    for amounts in amounts_vec.into_iter() {
        let resp = split_coins(
            keystore,
            sender,
            coin_ref,
            &amounts,
            gas_ref,
            Some(ref_gas_price),
            None,
            client,
        )
        .await?;
        let effects = resp.effects();
        if !effects.status().success() {
            return Err(anyhow!("split_coins errored"));
        }

        // Get updated coin reference from transaction response changed_objects
        let changed_objects = &effects.changed_objects;
        let fresh_coin_ref =
            extract_object_ref_from_changed_objects(changed_objects, coin_object.id())?;
        coin_ref = fresh_coin_ref.as_object_ref();

        // Update gas reference if we're using separate gas
        gas_ref = match gas_ref {
            Some(old_gas_ref) => {
                // Get fresh gas object reference from transaction response changed_objects
                let fresh_gas_ref =
                    extract_object_ref_from_changed_objects(changed_objects, old_gas_ref.0)?;
                Some(fresh_gas_ref.as_object_ref())
            }
            None => None,
        };
        responses.push(resp);
    }
    Ok(responses)
}

#[tokio::test]
async fn test_make_change_exact_div() -> Result<()> {
    const SUI_100: u64 = 100_000_000_000;
    const SUI_10: u64 = 10_000_000_000;

    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(36000000)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let keystore = &test_cluster.wallet.config.keystore;

    let coin_object = get_all_coins(&mut client.clone(), sender)
        .await?
        .into_iter()
        .next()
        .ok_or(anyhow!("Expected 1 coin"))?;

    let coin_value = get_coin_value(&coin_object);
    assert!(
        coin_value == DEFAULT_INIT_COIN_BALANCE,
        "Coin did not match INIT_COIN_BALANCE"
    );

    let split_resp = split_coins(
        keystore,
        sender,
        coin_object.compute_object_reference(),
        &[SUI_100],
        None,
        None,
        None,
        &mut client,
    )
    .await?;

    // Check transaction success using gRPC response
    let effects = split_resp.effects();
    if !effects.status().success() {
        return Err(anyhow!("Transaction failed!: {:#?}", split_resp));
    }

    // Extract coin information from the transaction's changed_objects
    let changed_objects = &effects.changed_objects;

    // Find the newly created coin (should have IdOperation::Created)
    let new_coin_obj = changed_objects
        .iter()
        .find(|obj| obj.id_operation == Some(IdOperation::Created as i32))
        .ok_or(anyhow!(
            "Could not find newly created coin in changed_objects"
        ))?;

    let new_coin_id = ObjectID::from_hex_literal(
        new_coin_obj
            .object_id_opt()
            .ok_or(anyhow!("Missing object_id"))?,
    )?;

    // Get the updated reference for the original coin
    let original_coin_ref =
        extract_object_ref_from_changed_objects(changed_objects, coin_object.id())?;
    let initial = original_coin_ref.as_object_ref();

    // Now we need to fetch the actual new coin object to pass to make_change
    // We'll use get_object directly from the ledger instead of list_owned_objects
    let new_coin_request = GetObjectRequest::default()
        .with_object_id(new_coin_id.to_string())
        .with_read_mask(FieldMask::from_paths(["bcs"]));

    let new_coin_response = client
        .ledger_client()
        .get_object(new_coin_request)
        .await?
        .into_inner();

    let new_coin = new_coin_response
        .object
        .and_then(|obj| obj.bcs)
        .and_then(|bcs| bcs.deserialize::<Object>().ok())
        .ok_or(anyhow!("Could not deserialize new coin object"))?;

    // Verify it has the expected value
    if get_coin_value(&new_coin) != SUI_100 {
        return Err(anyhow!(
            "New coin has unexpected value: {} (expected {})",
            get_coin_value(&new_coin),
            SUI_100
        ));
    }

    let splitted_object = &new_coin;

    let txs = make_change(
        &mut client.clone(),
        keystore,
        sender,
        splitted_object,
        Some(initial),
        SUI_10,
    )
    .await?;

    assert!(txs.len() == 1, "Should only have 1 tx");

    // Get the transaction response and access changed_objects directly
    let tx_resp = &txs[0];
    let changed_objects = &tx_resp.effects().changed_objects;

    // Count created objects (those with IdOperation::Created)
    let new_coins_count = changed_objects
        .iter()
        .filter(|obj| obj.id_operation == Some(IdOperation::Created as i32))
        .count();

    assert!(
        new_coins_count == (SUI_100 / SUI_10) as usize - 1,
        "Expected {} new coins. Got: {}",
        SUI_100 / SUI_10 - 1,
        new_coins_count
    );

    Ok(())
}

#[tokio::test]
async fn test_make_change_remainder_div() -> Result<()> {
    const SUI_100: u64 = 100_000_000_000;
    const SUI_12: u64 = 12_000_000_000;

    let test_cluster = TestClusterBuilder::new()
        .with_epoch_duration_ms(36000000)
        .build()
        .await;
    let sender = test_cluster.get_address_0();
    let mut client = GrpcClient::new(test_cluster.rpc_url()).unwrap();
    let keystore = &test_cluster.wallet.config.keystore;

    let coin_object = get_all_coins(&mut client.clone(), sender)
        .await?
        .into_iter()
        .next()
        .ok_or(anyhow!("Expected 1 coin"))?;

    let coin_value = get_coin_value(&coin_object);
    assert!(
        coin_value == DEFAULT_INIT_COIN_BALANCE,
        "Coin did not match INIT_COIN_BALANCE"
    );

    let split_resp = split_coins(
        keystore,
        sender,
        coin_object.compute_object_reference(),
        &[SUI_100],
        None,
        None,
        None,
        &mut client,
    )
    .await?;

    // Check transaction success using gRPC response
    let effects = split_resp.effects();
    if !effects.status().success() {
        return Err(anyhow!("Transaction failed!: {:#?}", split_resp));
    }

    // Extract coin information from the transaction's changed_objects
    let changed_objects = &effects.changed_objects;

    // Find the newly created coin (should have IdOperation::Created)
    let new_coin_obj = changed_objects
        .iter()
        .find(|obj| obj.id_operation == Some(IdOperation::Created as i32))
        .ok_or(anyhow!(
            "Could not find newly created coin in changed_objects"
        ))?;

    let new_coin_id = ObjectID::from_hex_literal(
        new_coin_obj
            .object_id_opt()
            .ok_or(anyhow!("Missing object_id"))?,
    )?;

    // Get the updated reference for the original coin
    let original_coin_ref =
        extract_object_ref_from_changed_objects(changed_objects, coin_object.id())?;
    let initial = original_coin_ref.as_object_ref();

    // Now we need to fetch the actual new coin object to pass to make_change
    // We'll use get_object directly from the ledger instead of list_owned_objects
    let new_coin_request = GetObjectRequest::default()
        .with_object_id(new_coin_id.to_string())
        .with_read_mask(FieldMask::from_paths(["bcs"]));

    let new_coin_response = client
        .ledger_client()
        .get_object(new_coin_request)
        .await?
        .into_inner();

    let new_coin = new_coin_response
        .object
        .and_then(|obj| obj.bcs)
        .and_then(|bcs| bcs.deserialize::<Object>().ok())
        .ok_or(anyhow!("Could not deserialize new coin object"))?;

    // Verify it has the expected value
    if get_coin_value(&new_coin) != SUI_100 {
        return Err(anyhow!(
            "New coin has unexpected value: {} (expected {})",
            get_coin_value(&new_coin),
            SUI_100
        ));
    }

    let splitted_object = &new_coin;

    let splitted_id = *splitted_object.id();
    let txs = make_change(
        &mut client.clone(),
        keystore,
        sender,
        splitted_object,
        Some(initial),
        SUI_12,
    )
    .await?;

    assert!(txs.len() == 1, "Should only have 1 tx");

    // Get the transaction response and access changed_objects directly
    let tx_resp = &txs[0];
    let changed_objects = &tx_resp.effects().changed_objects;

    // Get created objects (those with IdOperation::Created)
    let new_coins: Vec<_> = changed_objects
        .iter()
        .filter(|obj| obj.id_operation == Some(IdOperation::Created as i32))
        .collect();

    assert!(
        new_coins.len() == (SUI_100 / SUI_12) as usize,
        "Expected {} new coins. Got: {}",
        SUI_100 / SUI_12,
        new_coins.len()
    );

    let mut all_coins: Vec<ObjectID> = new_coins
        .into_iter()
        .map(|c| ObjectID::from_hex_literal(c.object_id()).unwrap())
        .collect();
    all_coins.push(splitted_id.into());

    let requests: Vec<GetObjectRequest> = all_coins
        .iter()
        .map(|id| GetObjectRequest::default().with_object_id(id.to_string()))
        .collect();

    let batch_request = BatchGetObjectsRequest::default()
        .with_requests(requests)
        .with_read_mask(FieldMask::from_paths(["balance"]));

    let batch_response = client
        .clone()
        .ledger_client()
        .batch_get_objects(batch_request)
        .await?
        .into_inner();

    let (mut twelve_count, mut four_count) = (0, 0);
    for result in batch_response.objects {
        let object = match result.result {
            Some(get_object_result::Result::Object(obj)) => obj,
            Some(get_object_result::Result::Error(_)) => {
                return Err(anyhow!("Error retrieving object"));
            }
            Some(_) => {
                return Err(anyhow!("Unexpected result type"));
            }
            None => {
                return Err(anyhow!("No result in response"));
            }
        };
        let b = object
            .balance
            .ok_or(anyhow!("No balance field in object"))?;
        match b {
            SUI_12 => {
                twelve_count += 1;
            }
            4_000_000_000 => {
                four_count += 1;
            }
            b => {
                return Err(anyhow!(
                    "Did not expect anything else other than 12 or 4 SUI. Found {b}"
                ));
            }
        }
    }

    assert!(twelve_count == 8, "Expected 8 coins with 12 SUI");
    assert!(four_count == 1, "Expected 1 coin with 4 SUI");

    Ok(())
}
