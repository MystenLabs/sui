// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::objects::{test_gas_objects, test_gas_objects_with_owners, test_shared_object};
use crate::{test_account_keys, test_committee, test_keys};
use move_core_types::account_address::AccountAddress;
use move_core_types::ident_str;
use move_package::BuildConfig;
use std::path::PathBuf;
use sui::client_commands::WalletContext;
use sui::client_commands::{SuiClientCommandResult, SuiClientCommands};
use sui_adapter::genesis;
use sui_json_rpc_types::SuiObjectInfo;
use sui_sdk::crypto::SuiKeystore;
use sui_types::base_types::ObjectID;
use sui_types::base_types::ObjectRef;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::{get_key_pair, AccountKeyPair, AuthorityKeyPair, KeypairTraits};
use sui_types::gas_coin::GasCoin;
use sui_types::messages::CallArg;
use sui_types::messages::{
    CertifiedTransaction, ObjectArg, SignatureAggregator, SignedTransaction, Transaction,
    TransactionData,
};
use sui_types::object::Object;
/// The maximum gas per transaction.
pub const MAX_GAS: u64 = 10_000;

/// A helper function to get all accounts and their owned GasCoin
/// with a WalletContext
pub async fn get_account_and_gas_coins(
    context: &mut WalletContext,
) -> Result<Vec<(SuiAddress, Vec<GasCoin>)>, anyhow::Error> {
    let mut res = Vec::with_capacity(context.keystore.addresses().len());
    let accounts = context.keystore.addresses();
    for address in accounts {
        let result = SuiClientCommands::Gas {
            address: Some(address),
        }
        .execute(context)
        .await?;
        if let SuiClientCommandResult::Gas(coins) = result {
            res.push((address, coins))
        } else {
            panic!(
                "Failed to get owned objects result for address {address}: {:?}",
                result
            )
        }
    }
    Ok(res)
}

pub async fn get_gas_objects_with_wallet_context(
    context: &WalletContext,
    address: SuiAddress,
) -> Vec<SuiObjectInfo> {
    context
        .gas_objects(address)
        .await
        .unwrap()
        .into_iter()
        .map(|(_val, _object, object_ref)| object_ref)
        .collect()
}

/// A helper function to get all accounts and their owned gas objects
/// with a WalletContext.
pub async fn get_account_and_gas_objects(
    context: &WalletContext,
) -> Vec<(SuiAddress, Vec<SuiObjectInfo>)> {
    let owned_gas_objects = futures::future::join_all(
        context
            .keystore
            .addresses()
            .iter()
            .map(|account| get_gas_objects_with_wallet_context(context, *account)),
    )
    .await;
    context
        .keystore
        .addresses()
        .iter()
        .zip(owned_gas_objects.into_iter())
        .map(|(address, objects)| (*address, objects))
        .collect::<Vec<_>>()
}

/// A helper function to make Transactions with controlled accounts in WalletContext.
/// Particularly, the wallet needs to own gas objects for transactions.
/// However, if this function is called multiple times without any "sync" actions
/// on gas object management, txns may fail and objects may be locked.
///
/// The param is called `max_txn_num` because it does not always return the exact
/// same amount of Transactions, for example when there are not enough gas objects
/// controlled by the WalletContext. Caller should rely on the return value to
/// check the count.
pub async fn make_transactions_with_wallet_context(
    context: &mut WalletContext,
    max_txn_num: usize,
) -> Vec<Transaction> {
    let recipient = get_key_pair::<AuthorityKeyPair>().0;
    let accounts_and_objs = get_account_and_gas_objects(context).await;
    let mut res = Vec::with_capacity(max_txn_num);
    for (address, objs) in &accounts_and_objs {
        for obj in objs {
            if res.len() >= max_txn_num {
                return res;
            }
            let data = TransactionData::new_transfer_sui(
                recipient,
                *address,
                Some(2),
                obj.to_object_ref(),
                MAX_GAS,
            );
            let tx = Transaction::from_data(data, &context.keystore.signer(*address));
            res.push(tx);
        }
    }
    res
}

/// Make a few different single-writer test transactions owned by specific addresses.
pub fn make_transactions_with_pre_genesis_objects(
    keys: SuiKeystore,
) -> (Vec<Transaction>, Vec<Object>) {
    // The key pair of the recipient of the transaction.
    let recipient = get_key_pair::<AuthorityKeyPair>().0;

    // The gas objects and the objects used in the transfer transactions. Evert two
    // consecutive objects must have the same owner for the transaction to be valid.
    let mut addresses_two_by_two = Vec::new();
    let mut signers = Vec::new(); // Keys are not copiable, move them here.
    for keypair in keys.keys() {
        let address = (&keypair).into();
        addresses_two_by_two.push(address);
        addresses_two_by_two.push(address);
        signers.push(keys.signer(address));
    }
    let gas_objects = test_gas_objects_with_owners(addresses_two_by_two);

    // Make one transaction for every two gas objects.
    let mut transactions = Vec::new();
    for (objects, signer) in gas_objects.chunks(2).zip(signers) {
        let [o1, o2]: &[Object; 2] = match objects.try_into() {
            Ok(x) => x,
            Err(_) => break,
        };

        let data = TransactionData::new_transfer(
            recipient,
            o1.compute_object_reference(),
            /* sender */ o1.owner.get_owner_address().unwrap(),
            /* gas_object_ref */ o2.compute_object_reference(),
            MAX_GAS,
        );
        transactions.push(Transaction::from_data(data, &signer));
    }
    (transactions, gas_objects)
}

/// Make a few different test transaction containing the same shared object.
pub fn test_shared_object_transactions() -> Vec<Transaction> {
    // Helper function to load genesis packages.
    fn get_genesis_package_by_module(genesis_objects: &[Object], module: &str) -> ObjectRef {
        genesis_objects
            .iter()
            .find_map(|o| match o.data.try_as_package() {
                Some(p) => {
                    if p.serialized_module_map().keys().any(|name| name == module) {
                        Some(o.compute_object_reference())
                    } else {
                        None
                    }
                }
                None => None,
            })
            .unwrap()
    }

    // The key pair of the sender of the transaction.
    let (sender, keypair) = test_account_keys().pop().unwrap();

    // Make one transaction per gas object (all containing the same shared object).
    let mut transactions = Vec::new();
    let shared_object_id = test_shared_object().id();
    for gas_object in test_gas_objects() {
        let module = "object_basics";
        let function = "create";
        let genesis_package_objects = genesis::clone_genesis_packages();
        let package_object_ref = get_genesis_package_by_module(&genesis_package_objects, module);

        let data = TransactionData::new_move_call(
            sender,
            package_object_ref,
            ident_str!(module).to_owned(),
            ident_str!(function).to_owned(),
            /* type_args */ vec![],
            gas_object.compute_object_reference(),
            /* args */
            vec![
                CallArg::Object(ObjectArg::SharedObject(shared_object_id)),
                CallArg::Pure(16u64.to_le_bytes().to_vec()),
                CallArg::Pure(bcs::to_bytes(&AccountAddress::from(sender)).unwrap()),
            ],
            MAX_GAS,
        );
        transactions.push(Transaction::from_data(data, &keypair));
    }
    transactions
}

/// Make a transaction to publish a test move contracts package.
pub fn create_publish_move_package_transaction(
    gas_object_ref: ObjectRef,
    path: PathBuf,
    sender: SuiAddress,
    keypair: &AccountKeyPair,
) -> Transaction {
    let build_config = BuildConfig::default();
    let modules = sui_framework::build_move_package(&path, build_config).unwrap();

    let all_module_bytes = modules
        .iter()
        .map(|m| {
            let mut module_bytes = Vec::new();
            m.serialize(&mut module_bytes).unwrap();
            module_bytes
        })
        .collect();
    let data = TransactionData::new_module(sender, gas_object_ref, all_module_bytes, MAX_GAS);
    Transaction::from_data(data, keypair)
}

pub fn make_transfer_sui_transaction(
    gas_object: ObjectRef,
    recipient: SuiAddress,
    amount: Option<u64>,
    sender: SuiAddress,
    keypair: &AccountKeyPair,
) -> Transaction {
    let data = TransactionData::new_transfer_sui(recipient, sender, amount, gas_object, MAX_GAS);
    Transaction::from_data(data, keypair)
}

pub fn make_transfer_object_transaction(
    object_ref: ObjectRef,
    gas_object: ObjectRef,
    sender: SuiAddress,
    keypair: &AccountKeyPair,
    recipient: SuiAddress,
) -> Transaction {
    let data = TransactionData::new_transfer(recipient, object_ref, sender, gas_object, MAX_GAS);
    Transaction::from_data(data, keypair)
}

pub fn make_publish_basics_transaction(gas_object: ObjectRef) -> Transaction {
    let (sender, keypair) = test_account_keys().pop().unwrap();
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("../../sui_programmability/examples/basics");
    let build_config = BuildConfig::default();
    let modules = sui_framework::build_move_package(&path, build_config).unwrap();
    let all_module_bytes = modules
        .iter()
        .map(|m| {
            let mut module_bytes = Vec::new();
            m.serialize(&mut module_bytes).unwrap();
            module_bytes
        })
        .collect();
    let data = TransactionData::new_module(sender, gas_object, all_module_bytes, MAX_GAS);
    Transaction::from_data(data, &keypair)
}

pub fn make_counter_create_transaction(
    gas_object: ObjectRef,
    package_ref: ObjectRef,
    sender: SuiAddress,
    keypair: &AccountKeyPair,
) -> Transaction {
    let data = TransactionData::new_move_call(
        sender,
        package_ref,
        "counter".parse().unwrap(),
        "create".parse().unwrap(),
        Vec::new(),
        gas_object,
        vec![],
        MAX_GAS,
    );
    Transaction::from_data(data, keypair)
}

pub fn make_counter_increment_transaction(
    gas_object: ObjectRef,
    package_ref: ObjectRef,
    counter_id: ObjectID,
    sender: SuiAddress,
    keypair: &AccountKeyPair,
) -> Transaction {
    let data = TransactionData::new_move_call(
        sender,
        package_ref,
        "counter".parse().unwrap(),
        "increment".parse().unwrap(),
        Vec::new(),
        gas_object,
        vec![CallArg::Object(ObjectArg::SharedObject(counter_id))],
        MAX_GAS,
    );
    Transaction::from_data(data, keypair)
}

/// Make a transaction calling a specific move module & function.
pub fn move_transaction(
    gas_object: Object,
    module: &'static str,
    function: &'static str,
    package_ref: ObjectRef,
    arguments: Vec<CallArg>,
) -> Transaction {
    // The key pair of the sender of the transaction.
    let (sender, keypair) = test_account_keys().pop().unwrap();

    // Make the transaction.
    let data = TransactionData::new_move_call(
        sender,
        package_ref,
        ident_str!(module).to_owned(),
        ident_str!(function).to_owned(),
        /* type_args */ vec![],
        gas_object.compute_object_reference(),
        arguments,
        MAX_GAS,
    );
    Transaction::from_data(data, &keypair)
}

/// Make a test certificates for each input transaction.
pub fn make_certificates(transactions: Vec<Transaction>) -> Vec<CertifiedTransaction> {
    println!("0x123458");
    let committee = test_committee();
    println!("0x123457");
    let mut certificates = Vec::new();
    for tx in transactions {
        let mut aggregator = SignatureAggregator::try_new(tx.clone(), &committee).unwrap();
        for (_, key) in test_keys() {
            let vote = SignedTransaction::new(
                /* epoch */ 0,
                tx.data().clone(),
                &key,
                key.public().into(),
            );
            println!("0x11111");
            if let Some(certificate) = aggregator
                .append(vote.auth_sig().authority, vote.auth_sig().signature.clone())
                .unwrap()
            {
                certificates.push(certificate);
                break;
            }
        }
    }
    certificates
}
