// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::objects::test_gas_objects;
use crate::objects::test_shared_object;
use crate::test_committee;
use crate::test_keys;
use move_core_types::account_address::AccountAddress;
use move_core_types::ident_str;
use sui_adapter::genesis;
use sui_types::base_types::ObjectRef;
use sui_types::crypto::Signature;
use sui_types::messages::CallArg;
use sui_types::messages::{
    CertifiedTransaction, SignatureAggregator, SignedTransaction, Transaction, TransactionData,
};
use sui_types::object::Object;

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
            /* max_gas */ 10_000,
        );
        let signature = Signature::new(&data, &keypair);
        transactions.push(Transaction::new(data, signature));
    }
    transactions
}

/// Make a test certificates for each shared-object transaction.
pub async fn test_shared_object_certificates() -> Vec<CertifiedTransaction> {
    let committee = test_committee();
    let mut certificates = Vec::new();
    for tx in test_shared_object_transactions() {
        let mut aggregator = SignatureAggregator::try_new(tx.clone(), &committee).unwrap();
        for (_, key) in test_keys() {
            let vote = SignedTransaction::new(
                /* epoch */ 0,
                tx.clone(),
                *key.public_key_bytes(),
                &key,
            );
            if let Some(certificate) = aggregator
                .append(vote.auth_signature.authority, vote.auth_signature.signature)
                .unwrap()
            {
                certificates.push(certificate);
                break;
            }
        }
    }
    certificates
}
