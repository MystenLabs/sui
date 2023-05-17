// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use sui_keys::keystore::{AccountKeystore, FileBasedKeystore};
use sui_types::{base_types::SuiAddress, crypto::SuiKeyPair};

use crate::ValidatorProxy;
use std::path::PathBuf;
use std::sync::Arc;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::ObjectRef;
use sui_types::transaction::{
    TransactionData, VerifiedTransaction, TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
};
use sui_types::utils::to_sender_signed_transaction;

use crate::workloads::Gas;
use sui_types::crypto::{AccountKeyPair, KeypairTraits};
use test_utils::transaction::parse_package_ref;

// This is the maximum gas we will transfer from primary coin into any gas coin
// for running the benchmark

pub type UpdatedAndNewlyMintedGasCoins = Vec<Gas>;

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

pub fn make_pay_tx(
    input_coins: Vec<ObjectRef>,
    sender: SuiAddress,
    addresses: Vec<SuiAddress>,
    split_amounts: Vec<u64>,
    gas: ObjectRef,
    keypair: &AccountKeyPair,
    gas_price: u64,
) -> Result<VerifiedTransaction> {
    let pay = TransactionData::new_pay(
        sender,
        input_coins,
        addresses,
        split_amounts,
        gas,
        TEST_ONLY_GAS_UNIT_FOR_TRANSFER * gas_price,
        gas_price,
    )?;
    Ok(to_sender_signed_transaction(pay, keypair))
}

pub async fn publish_basics_package(
    gas: ObjectRef,
    proxy: Arc<dyn ValidatorProxy + Sync + Send>,
    sender: SuiAddress,
    keypair: &AccountKeyPair,
    gas_price: u64,
) -> ObjectRef {
    let transaction = TestTransactionBuilder::new(sender, gas, gas_price)
        .publish_examples("basics")
        .build_and_sign(keypair);
    let effects = proxy
        .execute_transaction_block(transaction.into())
        .await
        .unwrap();
    parse_package_ref(&effects.created()).unwrap()
}
