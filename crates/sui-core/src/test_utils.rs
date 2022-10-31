// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::AuthorityState;
use signature::Signer;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use sui_types::{
    base_types::{dbg_addr, ObjectID, TransactionDigest},
    batch::UpdateItem,
    crypto::{get_key_pair, AccountKeyPair, Signature},
    intent::Intent,
    messages::{
        BatchInfoRequest, BatchInfoResponseItem, Transaction, TransactionData, VerifiedTransaction,
    },
    object::Object,
};

use futures::StreamExt;
use sui_types::base_types::{random_object_ref, AuthorityName, ExecutionDigests};
use sui_types::committee::Committee;
use sui_types::crypto::{AuthoritySignInfo, AuthoritySignature};
use sui_types::gas::GasCostSummary;
use sui_types::messages::{CertifiedTransaction, ExecutionStatus, TransactionEffects};
use sui_types::object::Owner;
use tokio::time::sleep;
use tracing::info;

pub async fn wait_for_tx(wait_digest: TransactionDigest, state: Arc<AuthorityState>) {
    wait_for_all_txes(vec![wait_digest], state).await
}

pub async fn wait_for_all_txes(wait_digests: Vec<TransactionDigest>, state: Arc<AuthorityState>) {
    let mut wait_digests: HashSet<_> = wait_digests.iter().collect();

    let mut timeout = Box::pin(sleep(Duration::from_millis(15_000)));

    let mut max_seq = Some(0);

    let mut stream = Box::pin(
        state
            .handle_batch_streaming(BatchInfoRequest {
                start: max_seq,
                length: 1000,
            })
            .await
            .unwrap(),
    );

    // TODO: duplicated code with transaction.rs
    loop {
        tokio::select! {
            _ = &mut timeout => panic!("wait_for_tx timed out"),

            items = &mut stream.next() => {
                match items {
                    // Upon receiving a batch
                    Some(Ok(BatchInfoResponseItem(UpdateItem::Batch(batch)) )) => {
                        max_seq = Some(batch.data().next_sequence_number);
                        info!(?max_seq, "Received Batch");
                    }
                    // Upon receiving a transaction digest we store it, if it is not processed already.
                    Some(Ok(BatchInfoResponseItem(UpdateItem::Transaction((_seq, digest))))) => {
                        info!(?digest, "Received Transaction");
                        if wait_digests.remove(&digest.transaction) {
                            info!(?digest, "Digest found");
                        }
                        if wait_digests.is_empty() {
                            info!(?digest, "all digests found");
                            break;
                        }
                    },

                    Some(Err( err )) => panic!("{}", err),
                    None => {
                        info!(?max_seq, "Restarting Batch");
                        stream = Box::pin(
                                state
                                    .handle_batch_streaming(BatchInfoRequest {
                                        start: max_seq,
                                        length: 1000,
                                    })
                                    .await
                                    .unwrap(),
                            );

                    }
                }
            },
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
    to_verified_transaction(data, &sender_key)
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
    let cert = CertifiedTransaction::new_with_auth_sign_infos(
        transaction.clone(),
        signers
            .map(|(name, signer)| {
                AuthoritySignInfo::new(committee.epoch, &transaction.signed_data, *name, signer)
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
) -> Transaction {
    let signature = Signature::new_secure(&data, Intent::default(), signer);
    Transaction::new(data, Intent::default(), signature)
}

// This is used to create a verified transaction from transaction data and signer signing with default intent.
pub fn to_verified_transaction(
    data: TransactionData,
    signer: &dyn Signer<Signature>,
) -> VerifiedTransaction {
    to_sender_signed_transaction(data, signer).verify().unwrap()
}

pub fn dummy_transaction_effects(tx: &Transaction) -> TransactionEffects {
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
