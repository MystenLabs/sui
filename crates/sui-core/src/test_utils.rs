// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::hash::MultisetHash;
use fastcrypto::traits::KeyPair;
use move_core_types::{account_address::AccountAddress, ident_str};
use shared_crypto::intent::{Intent, IntentScope};
use std::sync::Arc;
use std::time::Duration;
use sui_config::genesis::Genesis;
use sui_macros::nondeterministic;
use sui_types::base_types::{random_object_ref, ObjectID};
use sui_types::crypto::AuthorityKeyPair;
use sui_types::crypto::{AccountKeyPair, AuthorityPublicKeyBytes, Signer};
use sui_types::effects::{SignedTransactionEffects, TestEffectsBuilder};
use sui_types::error::SuiError;
use sui_types::signature_verification::VerifiedDigestCache;
use sui_types::transaction::ObjectArg;
use sui_types::transaction::{
    CallArg, SignedTransaction, Transaction, TransactionData, TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
};
use sui_types::utils::create_fake_transaction;
use sui_types::utils::to_sender_signed_transaction;
use sui_types::{
    base_types::{AuthorityName, ExecutionDigests, ObjectRef, SuiAddress, TransactionDigest},
    committee::Committee,
    crypto::{AuthoritySignInfo, AuthoritySignature},
    message_envelope::Message,
    transaction::CertifiedTransaction,
};
use tokio::time::timeout;
use tracing::{info, warn};

use crate::authority::AuthorityState;
use crate::state_accumulator::StateAccumulator;

const WAIT_FOR_TX_TIMEOUT: Duration = Duration::from_secs(15);

pub async fn send_and_confirm_transaction(
    authority: &AuthorityState,
    fullnode: Option<&AuthorityState>,
    transaction: Transaction,
) -> Result<(CertifiedTransaction, SignedTransactionEffects), SuiError> {
    // Make the initial request
    let epoch_store = authority.load_epoch_store_one_call_per_task();
    transaction.validity_check(epoch_store.protocol_config(), epoch_store.epoch())?;
    let transaction = epoch_store.verify_transaction(transaction)?;
    let response = authority
        .handle_transaction(&epoch_store, transaction.clone())
        .await?;
    let vote = response.status.into_signed_for_testing();

    // Collect signatures from a quorum of authorities
    let committee = authority.clone_committee_for_testing();
    let certificate =
        CertifiedTransaction::new(transaction.into_message(), vec![vote.clone()], &committee)
            .unwrap()
            .try_into_verified_for_testing(&committee, &Default::default())
            .unwrap();

    // Submit the confirmation. *Now* execution actually happens, and it should fail when we try to look up our dummy module.
    // we unfortunately don't get a very descriptive error message, but we can at least see that something went wrong inside the VM
    //
    // We also check the incremental effects of the transaction on the live object set against StateAccumulator
    // for testing and regression detection
    let state_acc = StateAccumulator::new_for_tests(authority.get_accumulator_store().clone());
    let include_wrapped_tombstone = !authority
        .epoch_store_for_testing()
        .protocol_config()
        .simplified_unwrap_then_delete();
    let mut state =
        state_acc.accumulate_cached_live_object_set_for_testing(include_wrapped_tombstone);
    let (result, _execution_error_opt) = authority.try_execute_for_test(&certificate).await?;
    let state_after =
        state_acc.accumulate_cached_live_object_set_for_testing(include_wrapped_tombstone);
    let effects_acc = state_acc.accumulate_effects(
        &[result.inner().data().clone()],
        epoch_store.protocol_config(),
    );
    state.union(&effects_acc);

    assert_eq!(state_after.digest(), state.digest());

    if let Some(fullnode) = fullnode {
        fullnode.try_execute_for_test(&certificate).await?;
    }
    Ok((certificate.into_inner(), result.into_inner()))
}

// note: clippy is confused about this being dead - it appears to only be used in cfg(test), but
// adding #[cfg(test)] causes other targets to fail
#[allow(dead_code)]
pub(crate) fn init_state_parameters_from_rng<R>(rng: &mut R) -> (Genesis, AuthorityKeyPair)
where
    R: rand::CryptoRng + rand::RngCore,
{
    let dir = nondeterministic!(tempfile::TempDir::new().unwrap());
    let network_config = sui_swarm_config::network_config_builder::ConfigBuilder::new(&dir)
        .rng(rng)
        .build();
    let genesis = network_config.genesis;
    let authority_key = network_config.validator_configs[0]
        .protocol_key_pair()
        .copy();

    (genesis, authority_key)
}

pub async fn wait_for_tx(digest: TransactionDigest, state: Arc<AuthorityState>) {
    match timeout(
        WAIT_FOR_TX_TIMEOUT,
        state
            .get_transaction_cache_reader()
            .notify_read_executed_effects(&[digest]),
    )
    .await
    {
        Ok(_) => info!(?digest, "digest found"),
        Err(e) => {
            warn!(?digest, "digest not found!");
            panic!("timed out waiting for effects of digest! {e}");
        }
    }
}

pub async fn wait_for_all_txes(digests: Vec<TransactionDigest>, state: Arc<AuthorityState>) {
    match timeout(
        WAIT_FOR_TX_TIMEOUT,
        state
            .get_transaction_cache_reader()
            .notify_read_executed_effects(&digests),
    )
    .await
    {
        Ok(_) => info!(?digests, "all digests found"),
        Err(e) => {
            warn!(?digests, "some digests not found!");
            panic!("timed out waiting for effects of digests! {e}");
        }
    }
}

pub fn create_fake_cert_and_effect_digest<'a>(
    signers: impl Iterator<
        Item = (
            &'a AuthorityName,
            &'a (dyn Signer<AuthoritySignature> + Send + Sync),
        ),
    >,
    committee: &Committee,
) -> (ExecutionDigests, CertifiedTransaction) {
    let transaction = create_fake_transaction();
    let cert = CertifiedTransaction::new(
        transaction.data().clone(),
        signers
            .map(|(name, signer)| {
                AuthoritySignInfo::new(
                    committee.epoch,
                    transaction.data(),
                    Intent::sui_app(IntentScope::SenderSignedTransaction),
                    *name,
                    signer,
                )
            })
            .collect(),
        committee,
    )
    .unwrap();
    let effects = TestEffectsBuilder::new(transaction.data()).build();
    (
        ExecutionDigests::new(*transaction.digest(), effects.digest()),
        cert,
    )
}

pub fn make_transfer_sui_transaction(
    gas_object: ObjectRef,
    recipient: SuiAddress,
    amount: Option<u64>,
    sender: SuiAddress,
    keypair: &AccountKeyPair,
    gas_price: u64,
) -> Transaction {
    let data = TransactionData::new_transfer_sui(
        recipient,
        sender,
        amount,
        gas_object,
        gas_price * TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
        gas_price,
    );
    to_sender_signed_transaction(data, keypair)
}

pub fn make_pay_sui_transaction(
    gas_object: ObjectRef,
    coins: Vec<ObjectRef>,
    recipients: Vec<SuiAddress>,
    amounts: Vec<u64>,
    sender: SuiAddress,
    keypair: &AccountKeyPair,
    gas_price: u64,
    gas_budget: u64,
) -> Transaction {
    let data = TransactionData::new_pay_sui(
        sender, coins, recipients, amounts, gas_object, gas_budget, gas_price,
    )
    .unwrap();
    to_sender_signed_transaction(data, keypair)
}

pub fn make_transfer_object_transaction(
    object_ref: ObjectRef,
    gas_object: ObjectRef,
    sender: SuiAddress,
    keypair: &AccountKeyPair,
    recipient: SuiAddress,
    gas_price: u64,
) -> Transaction {
    let data = TransactionData::new_transfer(
        recipient,
        object_ref,
        sender,
        gas_object,
        gas_price * TEST_ONLY_GAS_UNIT_FOR_TRANSFER * 10,
        gas_price,
    );
    to_sender_signed_transaction(data, keypair)
}

pub fn make_transfer_object_move_transaction(
    src: SuiAddress,
    keypair: &AccountKeyPair,
    dest: SuiAddress,
    object_ref: ObjectRef,
    framework_obj_id: ObjectID,
    gas_object_ref: ObjectRef,
    gas_budget_in_units: u64,
    gas_price: u64,
) -> Transaction {
    let args = vec![
        CallArg::Object(ObjectArg::ImmOrOwnedObject(object_ref)),
        CallArg::Pure(bcs::to_bytes(&AccountAddress::from(dest)).unwrap()),
    ];

    to_sender_signed_transaction(
        TransactionData::new_move_call(
            src,
            framework_obj_id,
            ident_str!("object_basics").to_owned(),
            ident_str!("transfer").to_owned(),
            Vec::new(),
            gas_object_ref,
            args,
            gas_budget_in_units * gas_price,
            gas_price,
        )
        .unwrap(),
        keypair,
    )
}

/// Make a dummy tx that uses random object refs.
pub fn make_dummy_tx(
    receiver: SuiAddress,
    sender: SuiAddress,
    sender_sec: &AccountKeyPair,
) -> Transaction {
    Transaction::from_data_and_signer(
        TransactionData::new_transfer(
            receiver,
            random_object_ref(),
            sender,
            random_object_ref(),
            TEST_ONLY_GAS_UNIT_FOR_TRANSFER * 10,
            10,
        ),
        vec![sender_sec],
    )
}

/// Make a cert using an arbitrarily large committee.
pub fn make_cert_with_large_committee(
    committee: &Committee,
    key_pairs: &[AuthorityKeyPair],
    transaction: &Transaction,
) -> CertifiedTransaction {
    // assumes equal weighting.
    let len = committee.voting_rights.len();
    assert_eq!(len, key_pairs.len());
    let count = (len * 2 + 2) / 3;

    let sigs: Vec<_> = key_pairs
        .iter()
        .take(count)
        .map(|key_pair| {
            SignedTransaction::new(
                committee.epoch(),
                transaction.clone().into_data(),
                key_pair,
                AuthorityPublicKeyBytes::from(key_pair.public()),
            )
            .auth_sig()
            .clone()
        })
        .collect();

    let cert = CertifiedTransaction::new(transaction.clone().into_data(), sigs, committee).unwrap();
    cert.verify_signatures_authenticated(
        committee,
        &Default::default(),
        Arc::new(VerifiedDigestCache::new_empty()),
    )
    .unwrap();
    cert
}
