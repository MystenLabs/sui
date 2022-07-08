// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::objects::{test_gas_objects, test_gas_objects_with_owners, test_shared_object};
use crate::{test_committee, test_keys};
use move_core_types::account_address::AccountAddress;
use move_core_types::ident_str;
use move_package::BuildConfig;
use std::path::PathBuf;
use sui_adapter::genesis;
use sui_types::base_types::ObjectRef;
use sui_types::crypto::NarwhalKeypair;
use sui_types::messages::{
    CertifiedTransaction, ObjectArg, SignatureAggregator, SignedTransaction, Transaction,
    TransactionData,
};
use sui_types::object::Object;
use sui_types::{base_types::SuiAddress, crypto::Signature};
use sui_types::{crypto::KeyPair, messages::CallArg};

/// The maximum gas per transaction.
pub const MAX_GAS: u64 = 10_000;

/// Make a few different single-writer test transactions owned by specific addresses.
pub fn test_transactions<K>(keys: K) -> (Vec<Transaction>, Vec<Object>)
where
    K: Iterator<Item = KeyPair>,
{
    // The key pair of the recipient of the transaction.
    let (recipient, _) = test_keys().pop().unwrap();

    // The gas objects and the objects used in the transfer transactions. Ever two
    // consecutive objects must have the same owner for the transaction to be valid.
    let mut addresses_two_by_two = Vec::new();
    let mut keypairs = Vec::new(); // Keys are not copiable, move them here.
    for keypair in keys {
        let address = SuiAddress::from(keypair.public_key_bytes());
        addresses_two_by_two.push(address);
        addresses_two_by_two.push(address);
        keypairs.push(keypair);
    }
    let gas_objects = test_gas_objects_with_owners(addresses_two_by_two);

    // Make one transaction for every two gas objects.
    let mut transactions = Vec::new();
    for (objects, keypair) in gas_objects.chunks(2).zip(keypairs) {
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
        let signature = Signature::new(&data, &keypair);
        transactions.push(Transaction::new(data, signature));
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
    let (sender, keypair) = test_keys().pop().unwrap();

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
        let signature = Signature::new(&data, &keypair);
        transactions.push(Transaction::new(data, signature));
    }
    transactions
}

/// Make a transaction to publish a test move contracts package.
pub fn create_publish_move_package_transaction(gas_object: Object, path: PathBuf) -> Transaction {
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

    let gas_object_ref = gas_object.compute_object_reference();
    let (sender, keypair) = test_keys().pop().unwrap();
    let data = TransactionData::new_module(sender, gas_object_ref, all_module_bytes, MAX_GAS);
    let signature = Signature::new(&data, &keypair);
    Transaction::new(data, signature)
}

pub fn make_transfer_sui_transaction(gas_object: Object, recipient: SuiAddress) -> Transaction {
    let (sender, keypair) = test_keys().pop().unwrap();
    let data = TransactionData::new_transfer_sui(
        recipient,
        sender,
        None,
        gas_object.compute_object_reference(),
        MAX_GAS,
    );
    let signature = Signature::new(&data, &keypair);
    Transaction::new(data, signature)
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
    let (sender, keypair) = test_keys().pop().unwrap();

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
    let signature = Signature::new(&data, &keypair);
    Transaction::new(data, signature)
}

/// Make a test certificates for each input transaction.
pub fn make_certificates(transactions: Vec<Transaction>) -> Vec<CertifiedTransaction> {
    let committee = test_committee();
    let mut certificates = Vec::new();
    for tx in transactions {
        let mut aggregator = SignatureAggregator::try_new(tx.clone(), &committee).unwrap();
        for (_, key) in test_keys() {
            let vote = SignedTransaction::new(
                /* epoch */ 0,
                tx.clone(),
                key.public_key_bytes(),
                &key,
            );
            if let Some(certificate) = aggregator
                .append(vote.auth_sign_info.authority, vote.auth_sign_info.signature)
                .unwrap()
            {
                certificates.push(certificate);
                break;
            }
        }
    }
    certificates
}
