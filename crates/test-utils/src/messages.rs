// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::account_address::AccountAddress;
use move_core_types::ident_str;
use move_core_types::language_storage::TypeTag;
use std::path::PathBuf;
use sui_core::test_utils::dummy_transaction_effects;
use sui_keys::keystore::AccountKeystore;
use sui_move_build::BuildConfig;
use sui_sdk::wallet_context::WalletContext;
use sui_types::base_types::ObjectRef;
use sui_types::base_types::SuiAddress;
use sui_types::base_types::{ObjectID, SequenceNumber};
use sui_types::committee::Committee;
use sui_types::crypto::{
    deterministic_random_account_key, get_key_pair, AccountKeyPair, AuthorityKeyPair,
    AuthorityPublicKeyBytes, AuthoritySignInfo, KeypairTraits, Signature, Signer,
};
use sui_types::effects::SignedTransactionEffects;
use sui_types::messages::CallArg;
use sui_types::messages::TEST_ONLY_GAS_UNIT_FOR_GENERIC;
use sui_types::messages::TEST_ONLY_GAS_UNIT_FOR_TRANSFER;
use sui_types::messages::{
    CertifiedTransaction, ObjectArg, TransactionData, VerifiedCertificate,
    VerifiedSignedTransaction, VerifiedTransaction,
};
use sui_types::object::{generate_test_gas_objects, Object};
use sui_types::sui_system_state::SUI_SYSTEM_MODULE_NAME;
use sui_types::utils::to_sender_signed_transaction;
use sui_types::{
    SUI_FRAMEWORK_OBJECT_ID, SUI_SYSTEM_OBJECT_ID, SUI_SYSTEM_STATE_OBJECT_ID,
    SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
};

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
) -> Vec<VerifiedTransaction> {
    let recipient = get_key_pair::<AuthorityKeyPair>().0;
    let accounts_and_objs = context.get_all_accounts_and_gas_objects().await.unwrap();
    let mut res = Vec::with_capacity(max_txn_num);

    let gas_price = context.get_reference_gas_price().await.unwrap();
    for (address, objs) in accounts_and_objs {
        for obj in objs {
            if res.len() >= max_txn_num {
                return res;
            }
            let data = TransactionData::new_transfer_sui(
                recipient,
                address,
                Some(2),
                obj,
                gas_price * TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
                gas_price,
            );
            let tx = to_sender_signed_transaction(
                data,
                context.config.keystore.get_key(&address).unwrap(),
            );
            res.push(tx);
        }
    }
    res
}

pub async fn make_staking_transaction_with_wallet_context(
    context: &mut WalletContext,
    validator_address: SuiAddress,
) -> VerifiedTransaction {
    let accounts_and_objs = context.get_all_accounts_and_gas_objects().await.unwrap();
    let sender = accounts_and_objs[0].0;
    let gas_object = accounts_and_objs[0].1[0];
    let stake_object = accounts_and_objs[0].1[1];
    let gas_price = context.get_reference_gas_price().await.unwrap();

    make_staking_transaction(
        gas_object,
        stake_object,
        validator_address,
        sender,
        context.config.keystore.get_key(&sender).unwrap(),
        gas_price,
    )
}

pub async fn make_counter_increment_transaction_with_wallet_context(
    context: &WalletContext,
    sender: SuiAddress,
    counter_id: ObjectID,
    counter_initial_shared_version: SequenceNumber,
    gas_object_ref: Option<ObjectRef>,
) -> VerifiedTransaction {
    let gas_object_ref = match gas_object_ref {
        Some(obj_ref) => obj_ref,
        None => context
            .get_one_gas_object_owned_by_address(sender)
            .await
            .unwrap()
            .unwrap(),
    };
    let rgp = context.get_reference_gas_price().await.unwrap();
    let data = TransactionData::new_move_call(
        sender,
        SUI_FRAMEWORK_OBJECT_ID,
        "counter".parse().unwrap(),
        "increment".parse().unwrap(),
        Vec::new(),
        gas_object_ref,
        vec![CallArg::Object(ObjectArg::SharedObject {
            id: counter_id,
            initial_shared_version: counter_initial_shared_version,
            mutable: true,
        })],
        rgp * TEST_ONLY_GAS_UNIT_FOR_GENERIC,
        rgp,
    )
    .unwrap();
    to_sender_signed_transaction(data, context.config.keystore.get_key(&sender).unwrap())
}

/// Make a few different test transaction containing the same shared object.
pub fn test_shared_object_transactions(
    shared_object: Option<Object>,
    gas_objects: Option<Vec<Object>>,
    gas_price: u64,
) -> Vec<VerifiedTransaction> {
    // The key pair of the sender of the transaction.
    let (sender, keypair) = deterministic_random_account_key();
    // Make one transaction per gas object (all containing the same shared object).
    let mut transactions = Vec::new();
    let shared_object = shared_object.unwrap_or_else(Object::shared_for_testing);
    let gas_objects = gas_objects.unwrap_or_else(generate_test_gas_objects);
    let shared_object_id = shared_object.id();
    let initial_shared_version = shared_object.version();
    let module = "object_basics";
    let function = "create";

    for gas_object in gas_objects {
        let data = TransactionData::new_move_call(
            sender,
            SUI_FRAMEWORK_OBJECT_ID,
            ident_str!(module).to_owned(),
            ident_str!(function).to_owned(),
            /* type_args */ vec![],
            gas_object.compute_object_reference(),
            /* args */
            vec![
                CallArg::Object(ObjectArg::SharedObject {
                    id: shared_object_id,
                    initial_shared_version,
                    mutable: true,
                }),
                CallArg::Pure(16u64.to_le_bytes().to_vec()),
                CallArg::Pure(bcs::to_bytes(&AccountAddress::from(sender)).unwrap()),
            ],
            gas_price * TEST_ONLY_GAS_UNIT_FOR_GENERIC,
            gas_price,
        )
        .unwrap();
        transactions.push(to_sender_signed_transaction(data, &keypair));
    }
    transactions
}

/// Make a transaction to publish a test move contracts package.
pub fn create_publish_move_package_transaction(
    gas_object_ref: ObjectRef,
    path: PathBuf,
    sender: SuiAddress,
    keypair: &AccountKeyPair,
    gas_budget: u64,
    gas_price: u64,
) -> VerifiedTransaction {
    let compiled_package = BuildConfig::new_for_testing().build(path).unwrap();
    let all_module_bytes =
        compiled_package.get_package_bytes(/* with_unpublished_deps */ false);
    let dependencies = compiled_package.get_dependency_original_package_ids();

    let data = TransactionData::new_module(
        sender,
        gas_object_ref,
        all_module_bytes,
        dependencies,
        gas_budget,
        gas_price,
    );
    to_sender_signed_transaction(data, keypair)
}

pub fn make_transfer_object_transaction_with_wallet_context(
    object_ref: ObjectRef,
    gas_object: ObjectRef,
    context: &WalletContext,
    sender: SuiAddress,
    recipient: SuiAddress,
    gas_price: u64,
) -> VerifiedTransaction {
    let data = TransactionData::new_transfer(
        recipient,
        object_ref,
        sender,
        gas_object,
        TEST_ONLY_GAS_UNIT_FOR_TRANSFER * gas_price,
        gas_price,
    );
    to_sender_signed_transaction(data, context.config.keystore.get_key(&sender).unwrap())
}

pub fn make_counter_create_transaction(
    gas_object: ObjectRef,
    package_id: ObjectID,
    sender: SuiAddress,
    keypair: &AccountKeyPair,
    gas_price: u64,
) -> VerifiedTransaction {
    let data = TransactionData::new_move_call(
        sender,
        package_id,
        "counter".parse().unwrap(),
        "create".parse().unwrap(),
        Vec::new(),
        gas_object,
        vec![],
        gas_price * TEST_ONLY_GAS_UNIT_FOR_GENERIC,
        gas_price,
    )
    .unwrap();
    to_sender_signed_transaction(data, keypair)
}

pub fn make_counter_increment_transaction(
    gas_object: ObjectRef,
    package_id: ObjectID,
    counter_id: ObjectID,
    counter_initial_shared_version: SequenceNumber,
    sender: SuiAddress,
    keypair: &AccountKeyPair,
    gas_price: u64,
) -> VerifiedTransaction {
    let data = TransactionData::new_move_call(
        sender,
        package_id,
        "counter".parse().unwrap(),
        "increment".parse().unwrap(),
        Vec::new(),
        gas_object,
        vec![CallArg::Object(ObjectArg::SharedObject {
            id: counter_id,
            initial_shared_version: counter_initial_shared_version,
            mutable: true,
        })],
        gas_price * TEST_ONLY_GAS_UNIT_FOR_GENERIC,
        gas_price,
    )
    .unwrap();
    to_sender_signed_transaction(data, keypair)
}

pub fn make_staking_transaction(
    gas_object: ObjectRef,
    coin: ObjectRef,
    validator: SuiAddress,
    sender: SuiAddress,
    keypair: &dyn Signer<Signature>,
    gas_price: u64,
) -> VerifiedTransaction {
    let data = TransactionData::new_move_call(
        sender,
        SUI_SYSTEM_OBJECT_ID,
        SUI_SYSTEM_MODULE_NAME.to_owned(),
        "request_add_stake".parse().unwrap(),
        vec![],
        gas_object,
        vec![
            CallArg::Object(ObjectArg::SharedObject {
                id: SUI_SYSTEM_STATE_OBJECT_ID,
                initial_shared_version: SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
                mutable: true,
            }),
            CallArg::Object(ObjectArg::ImmOrOwnedObject(coin)),
            CallArg::Pure(bcs::to_bytes(&validator).unwrap()),
        ],
        gas_price * TEST_ONLY_GAS_UNIT_FOR_GENERIC,
        gas_price,
    )
    .unwrap();
    to_sender_signed_transaction(data, keypair)
}

/// Make a transaction calling a specific move module & function.
pub fn move_transaction(
    gas_object: Object,
    module: &'static str,
    function: &'static str,
    package_id: ObjectID,
    arguments: Vec<CallArg>,
    gas_budget: u64,
    gas_price: u64,
) -> VerifiedTransaction {
    move_transaction_with_type_tags(
        gas_object,
        module,
        function,
        package_id,
        &[],
        arguments,
        gas_budget,
        gas_price,
    )
}

/// Make a transaction calling a specific move module & function, with specific type tags
pub fn move_transaction_with_type_tags(
    gas_object: Object,
    module: &'static str,
    function: &'static str,
    package_id: ObjectID,
    type_args: &[TypeTag],
    arguments: Vec<CallArg>,
    gas_budget: u64,
    gas_price: u64,
) -> VerifiedTransaction {
    let (sender, keypair) = deterministic_random_account_key();

    // Make the transaction.
    let data = TransactionData::new_move_call(
        sender,
        package_id,
        ident_str!(module).to_owned(),
        ident_str!(function).to_owned(),
        type_args.to_vec(),
        gas_object.compute_object_reference(),
        arguments,
        gas_budget,
        gas_price,
    )
    .unwrap();
    to_sender_signed_transaction(data, &keypair)
}

/// Make a test certificates for each input transaction.
pub fn make_tx_certs_and_signed_effects(
    transactions: Vec<VerifiedTransaction>,
) -> (Vec<VerifiedCertificate>, Vec<SignedTransactionEffects>) {
    let (committee, key_pairs) = Committee::new_simple_test_committee();
    make_tx_certs_and_signed_effects_with_committee(transactions, &committee, &key_pairs)
}

/// Make a test certificates for each input transaction.
pub fn make_tx_certs_and_signed_effects_with_committee(
    transactions: Vec<VerifiedTransaction>,
    committee: &Committee,
    key_pairs: &[AuthorityKeyPair],
) -> (Vec<VerifiedCertificate>, Vec<SignedTransactionEffects>) {
    let mut tx_certs = Vec::new();
    let mut effect_sigs = Vec::new();
    for tx in transactions {
        let mut sigs: Vec<AuthoritySignInfo> = Vec::new();
        for key in key_pairs {
            let vote = VerifiedSignedTransaction::new(
                committee.epoch,
                tx.clone(),
                key.public().into(),
                key,
            );
            sigs.push(vote.auth_sig().clone());
            if let Ok(tx_cert) =
                CertifiedTransaction::new(vote.into_inner().into_data(), sigs.clone(), committee)
            {
                tx_certs.push(tx_cert.verify(committee).unwrap());
                let effects = dummy_transaction_effects(&tx);
                let signed_effects = SignedTransactionEffects::new(
                    committee.epoch(),
                    effects,
                    key,
                    AuthorityPublicKeyBytes::from(key.public()),
                );
                effect_sigs.push(signed_effects);
                break;
            };
        }
    }
    (tx_certs, effect_sigs)
}
