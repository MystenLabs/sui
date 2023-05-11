// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use sui_core::authority_client::AuthorityAPI;
pub use sui_core::test_utils::{wait_for_all_txes, wait_for_tx};
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::base_types::ObjectRef;
use sui_types::committee::Committee;
use sui_types::crypto::{deterministic_random_account_key, AuthorityKeyPair};
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI, TransactionEvents};
use sui_types::error::SuiResult;
use sui_types::message_envelope::Message;
use sui_types::messages_grpc::HandleCertificateResponseV2;
use sui_types::multiaddr::Multiaddr;
use sui_types::object::{Object, Owner};
use sui_types::transaction::{CertifiedTransaction, VerifiedTransaction};

use crate::authority::get_client;

fn make_publish_package(
    gas_object: ObjectRef,
    path: PathBuf,
    gas_price: u64,
) -> VerifiedTransaction {
    let (sender, keypair) = deterministic_random_account_key();
    TestTransactionBuilder::new(sender, gas_object, gas_price)
        .publish(path)
        .build_and_sign(&keypair)
}

pub async fn publish_package(
    gas_object: Object,
    path: PathBuf,
    net_addresses: &[Multiaddr],
    gas_price: u64,
) -> ObjectRef {
    let (effects, _, _) =
        publish_package_for_effects(gas_object, path, net_addresses, gas_price).await;
    parse_package_ref(effects.created()).unwrap()
}

async fn publish_package_for_effects(
    gas_object: Object,
    path: PathBuf,
    net_addresses: &[Multiaddr],
    gas_price: u64,
) -> (TransactionEffects, TransactionEvents, Vec<Object>) {
    submit_single_owner_transaction(
        make_publish_package(gas_object.compute_object_reference(), path, gas_price),
        net_addresses,
    )
    .await
}

/// Helper function to publish the move package of a simple shared counter.
pub async fn publish_counter_package(
    gas_object: Object,
    net_addresses: &[Multiaddr],
    gas_price: u64,
) -> ObjectRef {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("../../sui_programmability/examples/basics");
    publish_package(gas_object, path, net_addresses, gas_price).await
}

/// Submit a certificate containing only owned-objects to all authorities.
pub async fn submit_single_owner_transaction(
    transaction: VerifiedTransaction,
    net_addresses: &[Multiaddr],
) -> (TransactionEffects, TransactionEvents, Vec<Object>) {
    let (committee, key_pairs) = Committee::new_simple_test_committee();
    let certificate = CertifiedTransaction::new_from_keypairs_for_testing(
        transaction.into_message(),
        &key_pairs,
        &committee,
    )
    .verify(&committee)
    .unwrap();
    let mut responses = Vec::new();
    for addr in net_addresses {
        let client = get_client(addr);
        let reply = client
            .handle_certificate_v2(certificate.clone().into())
            .await
            .unwrap();
        responses.push(reply);
    }
    get_unique_effects(responses)
}

/// Keep submitting the certificates of a shared-object transaction until it is sequenced by
/// at least one consensus node. We use the loop since some consensus protocols (like Tusk)
/// may drop transactions. The certificate is submitted to every Sui authority.
pub async fn submit_shared_object_transaction(
    transaction: VerifiedTransaction,
    net_addresses: &[Multiaddr],
) -> SuiResult<(TransactionEffects, TransactionEvents, Vec<Object>)> {
    let (committee, key_pairs) = Committee::new_simple_test_committee();
    submit_shared_object_transaction_with_committee(
        transaction,
        net_addresses,
        &committee,
        &key_pairs,
    )
    .await
}

/// Keep submitting the certificates of a shared-object transaction until it is sequenced by
/// at least one consensus node. We use the loop since some consensus protocols (like Tusk)
/// may drop transactions. The certificate is submitted to every Sui authority.
async fn submit_shared_object_transaction_with_committee(
    transaction: VerifiedTransaction,
    net_addresses: &[Multiaddr],
    committee: &Committee,
    key_pairs: &[AuthorityKeyPair],
) -> SuiResult<(TransactionEffects, TransactionEvents, Vec<Object>)> {
    let certificate = CertifiedTransaction::new_from_keypairs_for_testing(
        transaction.into_message(),
        key_pairs,
        committee,
    )
    .verify(committee)
    .unwrap();

    let replies = loop {
        let futures: Vec<_> = net_addresses
            .iter()
            .map(|addr| {
                let client = get_client(addr);
                let cert = certificate.clone();
                async move { client.handle_certificate_v2(cert.into()).await }
            })
            .collect();

        let replies: Vec<_> = futures::future::join_all(futures)
            .await
            .into_iter()
            // Remove all `FailedToHearBackFromConsensus` replies. Note that the original Sui error type
            // `SuiError::FailedToHearBackFromConsensus(..)` is lost when the message is sent through the
            // network (it is replaced by `RpcError`). As a result, the following filter doesn't work:
            // `.filter(|result| !matches!(result, Err(SuiError::FailedToHearBackFromConsensus(..))))`.
            .filter(|result| match result {
                Err(e) => !e.to_string().contains("deadline has elapsed"),
                _ => true,
            })
            .collect();

        if !replies.is_empty() {
            break replies;
        }
    };
    let replies: SuiResult<Vec<_>> = replies.into_iter().collect();
    replies.map(get_unique_effects)
}

fn get_unique_effects(
    replies: Vec<HandleCertificateResponseV2>,
) -> (TransactionEffects, TransactionEvents, Vec<Object>) {
    let mut all_effects = HashMap::new();
    let mut all_events = HashMap::new();
    let mut all_objects = HashSet::new();
    for reply in replies {
        let effects = reply.signed_effects.into_data();
        all_effects.insert(effects.digest(), effects);
        all_events.insert(reply.events.digest(), reply.events);
        all_objects.insert(reply.fastpath_input_objects);
    }
    assert_eq!(all_effects.len(), 1);
    assert_eq!(all_events.len(), 1);
    assert_eq!(all_objects.len(), 1);
    (
        all_effects.into_values().next().unwrap(),
        all_events.into_values().next().unwrap(),
        all_objects.into_iter().next().unwrap(),
    )
}

/// Extract the package reference from a transaction effect. This is useful to deduce the
/// authority-created package reference after attempting to publish a new Move package.
pub fn parse_package_ref(created_objs: &[(ObjectRef, Owner)]) -> Option<ObjectRef> {
    created_objs
        .iter()
        .find(|(_, owner)| matches!(owner, Owner::Immutable))
        .map(|(reference, _)| *reference)
}
