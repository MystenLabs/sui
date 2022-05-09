// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::objects::test_gas_objects;
use crate::objects::test_shared_object;
use crate::test_committee;
use crate::test_keys;
use move_core_types::account_address::AccountAddress;
use move_core_types::ident_str;
use move_package::BuildConfig;
use std::path::PathBuf;
use sui_adapter::genesis;
use sui_types::base_types::ObjectRef;
use sui_types::crypto::Signature;
use sui_types::messages::{CallArg, TransactionEffects};
use sui_types::messages::{
    CertifiedTransaction, SignatureAggregator, SignedTransaction, Transaction, TransactionData,
};
use sui_types::object::{Object, Owner};

/// The maximum gas per transaction.
pub const MAX_GAS: u64 = 10_000;

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
        let module = "ObjectBasics";
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
                CallArg::SharedObject(shared_object_id),
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
pub fn publish_move_package_transaction(gas_object: Object) -> Transaction {
    let build_config = BuildConfig::default();
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("../sui_programmability/examples/basics");
    let modules = sui_framework::build_move_package(&path, build_config, false).unwrap();

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
                *key.public_key_bytes(),
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

/// Extract the package reference from a transaction effect. This is useful to deduce the
/// authority-created package reference after attempting to publish a new Move package.
pub fn parse_package_ref(effects: &TransactionEffects) -> Option<ObjectRef> {
    effects
        .created
        .iter()
        .find(|(_, owner)| matches!(owner, Owner::Immutable))
        .map(|(reference, _)| *reference)
}
