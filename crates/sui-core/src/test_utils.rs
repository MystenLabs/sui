// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::{AuthorityState, EffectsNotifyRead};
use signature::Signer;
use std::sync::Arc;
use std::time::Duration;

use sui_types::{
    base_types::{
        dbg_addr, random_object_ref, AuthorityName, ExecutionDigests, ObjectID, TransactionDigest,
    },
    committee::Committee,
    crypto::{get_key_pair, AccountKeyPair, AuthoritySignInfo, AuthoritySignature, Signature},
    gas::GasCostSummary,
    intent::{Intent, IntentMessage},
    message_envelope::Message,
    messages::{
        CertifiedTransaction, ExecutionStatus, Transaction, TransactionData, TransactionEffects,
        VerifiedTransaction,
    },
    object::{Object, Owner},
};
use tokio::time::timeout;
use tracing::{info, warn};

const WAIT_FOR_TX_TIMEOUT: Duration = Duration::from_secs(15);

pub async fn wait_for_tx(digest: TransactionDigest, state: Arc<AuthorityState>) {
    match timeout(
        WAIT_FOR_TX_TIMEOUT,
        state.database.notify_read_effects(vec![digest]),
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
        state.database.notify_read_effects(digests.clone()),
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

// Creates a fake sender-signed transaction for testing. This transaction will
// not actually work.
pub fn create_fake_transaction() -> VerifiedTransaction {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let recipient = dbg_addr(2);
    let object_id = ObjectID::random();
    let object = Object::immutable_with_id_for_testing(object_id);
    let data = TransactionData::new_transfer_sui(
        recipient,
        sender,
        None,
        object.compute_object_reference(),
        10000,
    );
    to_sender_signed_transaction(data, &sender_key)
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
                AuthoritySignInfo::new(committee.epoch, transaction.data(), *name, signer)
            })
            .collect(),
        committee,
    )
    .unwrap();
    let effects = dummy_transaction_effects(&transaction);
    (
        ExecutionDigests::new(*transaction.digest(), effects.digest()),
        cert,
    )
}

// This is used to sign transaction with signer using default Intent.
pub fn to_sender_signed_transaction(
    data: TransactionData,
    signer: &dyn Signer<Signature>,
) -> VerifiedTransaction {
    VerifiedTransaction::new_unchecked(Transaction::from_data_and_signer(
        data,
        Intent::default(),
        signer,
    ))
}

// Workaround for benchmark setup.
pub fn to_sender_signed_transaction_arc(
    data: TransactionData,
    signer: &Arc<fastcrypto::ed25519::Ed25519KeyPair>,
) -> VerifiedTransaction {
    let data1 = data.clone();
    let intent_message = IntentMessage::new(Intent::default(), data);
    // OK to unwrap because this is used for benchmark only.
    let bytes = bcs::to_bytes(&intent_message).unwrap();
    let signature: Signature = signer.sign(&bytes);
    VerifiedTransaction::new_unchecked(Transaction::from_data(data1, Intent::default(), signature))
}

pub fn dummy_transaction_effects(tx: &Transaction) -> TransactionEffects {
    TransactionEffects {
        status: ExecutionStatus::Success,
        gas_used: GasCostSummary {
            computation_cost: 0,
            storage_cost: 0,
            storage_rebate: 0,
        },
        modified_at_versions: Vec::new(),
        shared_objects: Vec::new(),
        transaction_digest: *tx.digest(),
        created: Vec::new(),
        mutated: Vec::new(),
        unwrapped: Vec::new(),
        deleted: Vec::new(),
        wrapped: Vec::new(),
        gas_object: (
            random_object_ref(),
            Owner::AddressOwner(tx.data().intent_message.value.signer()),
        ),
        events: Vec::new(),
        dependencies: Vec::new(),
    }
}
