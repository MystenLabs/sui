// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::objects::{test_gas_objects, test_gas_objects_with_owners, test_shared_object};
use crate::{test_account_keys, test_committee, test_validator_keys};
use move_core_types::account_address::AccountAddress;
use move_core_types::ident_str;
use move_core_types::language_storage::TypeTag;
use move_package::BuildConfig;
use std::path::PathBuf;
use sui::client_commands::WalletContext;
use sui::client_commands::{SuiClientCommandResult, SuiClientCommands};
use sui_adapter::genesis;
use sui_json_rpc_types::SuiObjectInfo;
use sui_sdk::crypto::SuiKeystore;
use sui_types::base_types::ObjectRef;
use sui_types::base_types::{ObjectDigest, ObjectID, SequenceNumber};
use sui_types::crypto::{
    get_key_pair, AccountKeyPair, AuthorityKeyPair, AuthorityPublicKeyBytes, KeypairTraits,
};
use sui_types::gas::GasCostSummary;
use sui_types::gas_coin::GasCoin;
use sui_types::messages::SignedTransactionEffects;
use sui_types::messages::{CallArg, ExecutionStatus, TransactionEffects};
use sui_types::messages::{
    CertifiedTransaction, ObjectArg, SignatureAggregator, SignedTransaction, Transaction,
    TransactionData,
};
use sui_types::object::Object;
use sui_types::object::Owner;
use sui_types::{base_types::SuiAddress, crypto::Signature};
/// The maximum gas per transaction.
pub const MAX_GAS: u64 = 10_000;

pub fn random_object_ref() -> ObjectRef {
    (
        ObjectID::random(),
        SequenceNumber::new(),
        ObjectDigest::new([0; 32]),
    )
}

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

/// get one available gas ObjectRef
pub async fn get_gas_object_with_wallet_context(
    context: &WalletContext,
    address: &SuiAddress,
) -> Option<ObjectRef> {
    let mut res = get_gas_objects_with_wallet_context(context, address).await;
    if res.is_empty() {
        None
    } else {
        Some(res.swap_remove(0).to_object_ref())
    }
}

pub async fn get_gas_objects_with_wallet_context(
    context: &WalletContext,
    address: &SuiAddress,
) -> Vec<SuiObjectInfo> {
    context
        .gas_objects(*address)
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
            .map(|account| get_gas_objects_with_wallet_context(context, account)),
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
            let sig = context.keystore.sign(address, &data.to_bytes()).unwrap();

            res.push(Transaction::new(data, sig));
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
    // let mut signers = Vec::new(); // Keys are not copiable, move them here.
    for address in keys.addresses() {
        addresses_two_by_two.push(address);
        addresses_two_by_two.push(address);
        // signers.push(keys.signer(address));
    }
    let copied = addresses_two_by_two.clone();
    let gas_objects = test_gas_objects_with_owners(addresses_two_by_two);

    // Make one transaction for every two gas objects.
    let mut transactions = Vec::new();
    for (i, objects) in gas_objects.chunks(2).enumerate() {
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
        let signature = keys.sign(copied.get(i).unwrap(), &data.to_bytes()).unwrap();

        // let signature = Signature::new_secure_default(&data, &signer);
        transactions.push(Transaction::new(data, signature));
    }
    (transactions, gas_objects)
}

/// Make a few different test transaction containing the same shared object.
pub fn test_shared_object_transactions() -> Vec<Transaction> {
    // The key pair of the sender of the transaction.
    let (sender, keypair) = test_account_keys().pop().unwrap();

    // Make one transaction per gas object (all containing the same shared object).
    let mut transactions = Vec::new();
    let shared_object_id = test_shared_object().id();
    for gas_object in test_gas_objects() {
        let module = "object_basics";
        let function = "create";
        let package_object_ref = genesis::get_framework_object_ref();

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
    let signature = Signature::new(&data, keypair);
    Transaction::new(data, signature)
}

pub fn make_transfer_sui_transaction(
    gas_object: ObjectRef,
    recipient: SuiAddress,
    amount: Option<u64>,
    sender: SuiAddress,
    keypair: &AccountKeyPair,
) -> Transaction {
    let data = TransactionData::new_transfer_sui(recipient, sender, amount, gas_object, MAX_GAS);
    let signature = Signature::new(&data, keypair);
    Transaction::new(data, signature)
}

pub fn make_transfer_object_transaction(
    object_ref: ObjectRef,
    gas_object: ObjectRef,
    sender: SuiAddress,
    keypair: &AccountKeyPair,
    recipient: SuiAddress,
) -> Transaction {
    let data = TransactionData::new_transfer(recipient, object_ref, sender, gas_object, MAX_GAS);
    let signature = Signature::new(&data, keypair);
    Transaction::new(data, signature)
}

pub fn make_transfer_object_transaction_with_wallet_context(
    object_ref: ObjectRef,
    gas_object: ObjectRef,
    context: &WalletContext,
    sender: SuiAddress,
    recipient: SuiAddress,
) -> Transaction {
    let data = TransactionData::new_transfer(recipient, object_ref, sender, gas_object, MAX_GAS);
    let sig = context.keystore.sign(&sender, &data.to_bytes()).unwrap();
    Transaction::new(data, sig)
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
    let signature = Signature::new(&data, &keypair);
    Transaction::new(data, signature)
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
    let signature = Signature::new(&data, keypair);
    Transaction::new(data, signature)
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
    let signature = Signature::new(&data, keypair);
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

    move_transaction_with_type_tags(gas_object, module, function, package_ref, &[], arguments)
}

/// Make a transaction calling a specific move module & function, with specific type tags
pub fn move_transaction_with_type_tags(
    gas_object: Object,
    module: &'static str,
    function: &'static str,
    package_ref: ObjectRef,
    type_args: &[TypeTag],
    arguments: Vec<CallArg>,
) -> Transaction {
    let (sender, keypair) = test_account_keys().pop().unwrap();

    // Make the transaction.
    let data = TransactionData::new_move_call(
        sender,
        package_ref,
        ident_str!(module).to_owned(),
        ident_str!(function).to_owned(),
        type_args.to_vec(),
        gas_object.compute_object_reference(),
        arguments,
        MAX_GAS,
    );
    let signature = Signature::new(&data, &keypair);
    Transaction::new(data, signature)
}

/// Make a test certificates for each input transaction.
pub fn make_tx_certs_and_signed_effects(
    transactions: Vec<Transaction>,
) -> (Vec<CertifiedTransaction>, Vec<SignedTransactionEffects>) {
    let committee = test_committee();
    let mut tx_certs = Vec::new();
    let mut effect_sigs = Vec::new();
    for tx in transactions {
        let mut signed_tx_aggregator =
            SignatureAggregator::try_new(tx.clone(), &committee).unwrap();
        for (key, _, _, _) in test_validator_keys() {
            let vote =
                SignedTransaction::new(/* epoch */ 0, tx.clone(), key.public().into(), &key);

            if let Some(tx_cert) = signed_tx_aggregator
                .append(vote.auth_sign_info.authority, vote.auth_sign_info.signature)
                .unwrap()
            {
                tx_certs.push(tx_cert);
                let effects = dummy_transaction_effects(&tx);
                let signed_effects = effects.to_sign_effects(
                    committee.epoch(),
                    &AuthorityPublicKeyBytes::from(key.public()),
                    &key,
                );
                effect_sigs.push(signed_effects);
                break;
            };
        }
    }
    (tx_certs, effect_sigs)
}

fn dummy_transaction_effects(tx: &Transaction) -> TransactionEffects {
    TransactionEffects {
        status: ExecutionStatus::Success,
        gas_used: GasCostSummary {
            computation_cost: 0,
            storage_cost: 0,
            storage_rebate: 0,
        },
        shared_objects: Vec::new(),
        transaction_digest: *tx.digest(),
        created: Vec::new(),
        mutated: Vec::new(),
        unwrapped: Vec::new(),
        deleted: Vec::new(),
        wrapped: Vec::new(),
        gas_object: (
            random_object_ref(),
            Owner::AddressOwner(tx.signed_data.data.signer()),
        ),
        events: Vec::new(),
        dependencies: Vec::new(),
    }
}
