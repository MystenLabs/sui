// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Error, Result};
use std::collections::HashMap;
use sui_keys::keystore::{AccountKeystore, FileBasedKeystore};
use sui_types::{base_types::SuiAddress, coin, crypto::SuiKeyPair, SUI_FRAMEWORK_OBJECT_ID};

use crate::ValidatorProxy;
use itertools::Itertools;
use move_core_types::language_storage::TypeTag;
use std::path::PathBuf;
use std::sync::Arc;
use sui_types::base_types::ObjectRef;
use sui_types::messages::{
    CallArg, ObjectArg, TransactionData, VerifiedTransaction, DUMMY_GAS_PRICE,
};
use sui_types::utils::to_sender_signed_transaction;
use tracing::log::info;

use crate::workloads::{
    Gas, GasCoinConfig, WorkloadGasConfig, WorkloadInitGas, WorkloadPayloadGas,
};
use sui_types::crypto::{AccountKeyPair, KeypairTraits};

// This is the maximum gas we will transfer from primary coin into any gas coin
// for running the benchmark
pub const MAX_GAS_FOR_TESTING: u64 = 1_000_000_000;

pub type UpdatedAndNewlyMintedGasCoins = (Gas, Gas, Vec<Gas>);

pub fn get_ed25519_keypair_from_keystore(
    keystore_path: PathBuf,
    requested_address: &SuiAddress,
) -> Result<AccountKeyPair> {
    let keystore = FileBasedKeystore::new(&keystore_path)?;
    match keystore.get_key(requested_address) {
        Ok(SuiKeyPair::Ed25519(kp)) => Ok(kp.copy()),
        other => Err(anyhow::anyhow!("Invalid key type: {:?}", other)),
    }
}

pub fn make_split_coin_tx(
    sender: SuiAddress,
    coin: ObjectRef,
    coin_type_tag: TypeTag,
    split_amounts: Vec<u64>,
    gas: ObjectRef,
    keypair: &AccountKeyPair,
    gas_price: Option<u64>,
) -> Result<VerifiedTransaction> {
    let split_coin = TransactionData::new_move_call(
        sender,
        SUI_FRAMEWORK_OBJECT_ID,
        coin::PAY_MODULE_NAME.to_owned(),
        coin::PAY_SPLIT_VEC_FUNC_NAME.to_owned(),
        vec![coin_type_tag],
        gas,
        vec![
            CallArg::Object(ObjectArg::ImmOrOwnedObject(coin)),
            CallArg::Pure(bcs::to_bytes(&split_amounts).unwrap()),
        ],
        1000000,
        gas_price.unwrap_or(DUMMY_GAS_PRICE),
    );
    let verified_tx = to_sender_signed_transaction(split_coin, keypair);
    Ok(verified_tx)
}

pub fn make_pay_tx(
    input_coins: Vec<ObjectRef>,
    sender: SuiAddress,
    addresses: Vec<SuiAddress>,
    split_amounts: Vec<u64>,
    gas: ObjectRef,
    keypair: &AccountKeyPair,
    gas_price: Option<u64>,
) -> VerifiedTransaction {
    let pay = TransactionData::new_pay(
        sender,
        input_coins,
        addresses,
        split_amounts,
        gas,
        1000000,
        gas_price.unwrap_or(DUMMY_GAS_PRICE),
    );
    to_sender_signed_transaction(pay, keypair)
}

pub async fn split_coin_and_pay(
    proxy: Arc<dyn ValidatorProxy + Send + Sync>,
    coin: ObjectRef,
    coin_sender: SuiAddress,
    coin_type_tag: TypeTag,
    coin_configs: &[GasCoinConfig],
    gas: Gas,
    gas_price: u64,
) -> Result<UpdatedAndNewlyMintedGasCoins> {
    // split one coin into smaller coins of different amounts and send them to recipients
    let split_amounts: Vec<u64> = coin_configs.iter().map(|c| c.amount).collect();
    // TODO: Instead of splitting the coin and then using pay tx to transfer it to recipients,
    // we can do both in one tx with pay_sui which will split the coin out for us before
    // transferring it to recipients
    let verified_tx = make_split_coin_tx(
        coin_sender,
        coin,
        coin_type_tag,
        split_amounts.clone(),
        gas.0,
        &gas.2,
        Some(gas_price),
    )?;
    let (_, effects) = proxy.execute_transaction(verified_tx.into()).await?;
    let updated_gas = effects
        .mutated()
        .into_iter()
        .find(|(k, _)| k.0 == gas.0 .0)
        .ok_or("Input gas missing in the effects")
        .map_err(Error::msg)?;
    let created_coins: Vec<ObjectRef> = effects.created().into_iter().map(|c| c.0).collect();
    assert_eq!(created_coins.len(), split_amounts.len());
    let updated_coin = effects
        .mutated()
        .into_iter()
        .find(|(k, _)| k.0 == coin.0)
        .ok_or("Input gas missing in the effects")
        .map_err(Error::msg)?;
    let recipient_addresses: Vec<SuiAddress> = coin_configs.iter().map(|g| g.address).collect();
    let verified_tx = make_pay_tx(
        created_coins,
        gas.1.get_owner_address()?,
        recipient_addresses,
        split_amounts,
        updated_gas.0,
        &gas.2,
        Some(gas_price),
    );
    let (_, effects) = proxy.execute_transaction(verified_tx.into()).await?;
    let address_map: HashMap<SuiAddress, Arc<AccountKeyPair>> = coin_configs
        .iter()
        .map(|c| (c.address, c.keypair.clone()))
        .collect();
    let transferred_coins: Result<Vec<Gas>> = effects
        .created()
        .into_iter()
        .map(|c| {
            let address = c.1.get_owner_address()?;
            let keypair = address_map
                .get(&address)
                .ok_or("Owner address missing in the address map")
                .map_err(Error::msg)?;
            Ok((c.0, c.1, keypair.clone()))
        })
        .collect();
    let updated_gas = effects
        .mutated()
        .into_iter()
        .find(|(k, _)| k.0 == gas.0 .0)
        .ok_or("Input gas missing in the effects")
        .map_err(Error::msg)?;
    Ok((
        (updated_gas.0, updated_gas.1, gas.2.clone()),
        (updated_coin.0, updated_coin.1, gas.2),
        transferred_coins?,
    ))
}

pub async fn generate_all_gas_for_test(
    proxy: Arc<dyn ValidatorProxy + Send + Sync>,
    gas: Gas,
    coin: Gas,
    coin_type_tag: TypeTag,
    workload_gas_config: WorkloadGasConfig,
    gas_price: u64,
    chunk_size: u64,
) -> Result<(WorkloadInitGas, WorkloadPayloadGas)> {
    info!(
        "Generating gas with number of coins for shared counter init = {:?}, number of coins for \
    shared counter payloads = {:?}, number of transfer object token = {:?}, number of coins for \
    transfer object payloads = {:?}, number of coins for delegation payloads = {:?}",
        workload_gas_config
            .shared_counter_workload_init_gas_config
            .len(),
        workload_gas_config
            .shared_counter_workload_payload_gas_config
            .len(),
        workload_gas_config.transfer_object_workload_tokens.len(),
        workload_gas_config
            .transfer_object_workload_payload_gas_config
            .len(),
        workload_gas_config.delegation_gas_configs.len(),
    );
    let mut coin_configs = vec![];
    coin_configs.extend(
        workload_gas_config
            .shared_counter_workload_init_gas_config
            .iter()
            .cloned(),
    );
    coin_configs.extend(
        workload_gas_config
            .shared_counter_workload_payload_gas_config
            .iter()
            .cloned(),
    );
    coin_configs.extend(
        workload_gas_config
            .transfer_object_workload_tokens
            .iter()
            .cloned(),
    );
    coin_configs.extend(
        workload_gas_config
            .transfer_object_workload_payload_gas_config
            .iter()
            .cloned(),
    );
    coin_configs.extend(workload_gas_config.delegation_gas_configs.iter().cloned());
    let mut primary_gas = gas;
    let mut pay_coin = coin;
    let mut new_gas_coins: Vec<Gas> = vec![];
    let chunked_coin_configs = coin_configs.chunks(chunk_size as usize);
    eprintln!("Number of gas requests = {}", chunked_coin_configs.len());
    for chunk in chunked_coin_configs {
        let (updated_primary_gas, updated_coin, gas_coins) = split_coin_and_pay(
            proxy.clone(),
            pay_coin.0,
            pay_coin.1.get_owner_address()?,
            coin_type_tag.clone(),
            chunk,
            primary_gas,
            gas_price,
        )
        .await?;
        primary_gas = updated_primary_gas;
        pay_coin = updated_coin;
        new_gas_coins.extend(gas_coins);
    }
    let transfer_tokens: Vec<Gas> = workload_gas_config
        .transfer_object_workload_tokens
        .iter()
        .map(|c| {
            let (index, _) = new_gas_coins
                .iter()
                .find_position(|g| g.1.get_owner_address().unwrap() == c.address)
                .unwrap();
            new_gas_coins.remove(index)
        })
        .collect();
    let transfer_object_payload_gas: Vec<Gas> = workload_gas_config
        .transfer_object_workload_payload_gas_config
        .iter()
        .map(|c| {
            let (index, _) = new_gas_coins
                .iter()
                .find_position(|g| g.1.get_owner_address().unwrap() == c.address)
                .unwrap();
            new_gas_coins.remove(index)
        })
        .collect();
    let shared_counter_init_gas: Vec<Gas> = workload_gas_config
        .shared_counter_workload_init_gas_config
        .iter()
        .map(|c| {
            let (index, _) = new_gas_coins
                .iter()
                .find_position(|g| g.1.get_owner_address().unwrap() == c.address)
                .unwrap();
            new_gas_coins.remove(index)
        })
        .collect();
    let shared_counter_payload_gas: Vec<Gas> = workload_gas_config
        .shared_counter_workload_payload_gas_config
        .iter()
        .map(|c| {
            let (index, _) = new_gas_coins
                .iter()
                .find_position(|g| g.1.get_owner_address().unwrap() == c.address)
                .unwrap();
            new_gas_coins.remove(index)
        })
        .collect();
    let delegation_payload_gas: Vec<Gas> = workload_gas_config
        .delegation_gas_configs
        .iter()
        .map(|c| {
            let (index, _) = new_gas_coins
                .iter()
                .find_position(|g| g.1.get_owner_address().unwrap() == c.address)
                .unwrap();
            new_gas_coins.remove(index)
        })
        .collect();
    let workload_init_config = WorkloadInitGas {
        shared_counter_init_gas,
    };

    let workload_payload_config = WorkloadPayloadGas {
        transfer_tokens,
        transfer_object_payload_gas,
        shared_counter_payload_gas,
        delegation_payload_gas,
    };

    Ok((workload_init_config, workload_payload_config))
}
