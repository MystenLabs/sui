// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde_json::json;
use shared_crypto::intent::Intent;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use sui::client_commands::{SuiClientCommandResult, SuiClientCommands};
use sui_core::authority_client::AuthorityAPI;
pub use sui_core::test_utils::{wait_for_all_txes, wait_for_tx};
use sui_json_rpc_types::SuiData;
use sui_json_rpc_types::SuiObjectResponse;
use sui_json_rpc_types::{
    SuiObjectDataOptions, SuiObjectResponseQuery, SuiTransactionBlockDataAPI,
    SuiTransactionBlockEffectsAPI, SuiTransactionBlockResponse, SuiTransactionBlockResponseOptions,
};
use sui_keys::keystore::AccountKeystore;
use sui_sdk::json::SuiJsonValue;
use sui_sdk::wallet_context::WalletContext;
use sui_types::base_types::ObjectRef;
use sui_types::base_types::{ObjectID, SuiAddress, TransactionDigest};
use sui_types::committee::Committee;
use sui_types::crypto::{deterministic_random_account_key, AuthorityKeyPair};
use sui_types::error::SuiResult;
use sui_types::message_envelope::Message;
use sui_types::transaction::TEST_ONLY_GAS_UNIT_FOR_TRANSFER;
use sui_types::transaction::{CertifiedTransaction, TEST_ONLY_GAS_UNIT_FOR_GENERIC};

use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI, TransactionEvents};
use sui_types::messages_grpc::HandleCertificateResponseV2;
use sui_types::multiaddr::Multiaddr;
use sui_types::object::{Object, Owner};
use sui_types::quorum_driver_types::ExecuteTransactionRequestType;
use sui_types::transaction::{Transaction, VerifiedTransaction};
use tracing::{debug, info};

use crate::authority::get_client;

pub fn make_publish_package(
    gas_object: ObjectRef,
    path: PathBuf,
    gas_price: u64,
) -> VerifiedTransaction {
    let (sender, keypair) = deterministic_random_account_key();
    TestTransactionBuilder::new(sender, gas_object, gas_price)
        .publish(path, false)
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

/// A helper function to submit a move transaction
async fn submit_move_transaction(
    context: &WalletContext,
    module: &'static str,
    function: &'static str,
    package_id: ObjectID,
    arguments: Vec<SuiJsonValue>,
    sender: SuiAddress,
    gas_object: Option<ObjectID>,
) -> SuiTransactionBlockResponse {
    debug!(?package_id, ?arguments, "move_transaction");
    let client = context.get_client().await.unwrap();
    let gas_price = context.get_reference_gas_price().await.unwrap();
    let data = client
        .transaction_builder()
        .move_call(
            sender,
            package_id,
            module,
            function,
            vec![], // type_args
            arguments,
            gas_object,
            TEST_ONLY_GAS_UNIT_FOR_GENERIC * gas_price,
        )
        .await
        .unwrap();

    let signature = context
        .config
        .keystore
        .sign_secure(&sender, &data, Intent::sui_transaction())
        .unwrap();

    let tx = Transaction::from_data(data, Intent::sui_transaction(), vec![signature])
        .verify()
        .unwrap();
    let tx_digest = tx.digest();
    debug!(?tx_digest, "submitting move transaction");

    let resp = client
        .quorum_driver_api()
        .execute_transaction_block(
            tx,
            SuiTransactionBlockResponseOptions::full_content(),
            Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await
        .unwrap();
    assert!(resp.confirmed_local_execution.unwrap());
    resp
}

/// A helper function to publish the basics package and make counter objects
pub async fn publish_basics_package_and_make_counter(
    context: &WalletContext,
    sender: SuiAddress,
) -> (ObjectRef, ObjectRef) {
    let package_ref = context.publish_basics_package().await;

    debug!(?package_ref);

    let response = submit_move_transaction(
        context,
        "counter",
        "create",
        package_ref.0,
        vec![],
        sender,
        None,
    )
    .await;

    let counter_ref = response
        .effects
        .unwrap()
        .created()
        .iter()
        .find(|obj_ref| matches!(obj_ref.owner, Owner::Shared { .. }))
        .unwrap()
        .reference
        .to_object_ref();
    debug!(?counter_ref);
    (package_ref, counter_ref)
}

pub async fn increment_counter(
    context: &WalletContext,
    sender: SuiAddress,
    gas_object: Option<ObjectID>,
    package_id: ObjectID,
    counter_id: ObjectID,
) -> SuiTransactionBlockResponse {
    submit_move_transaction(
        context,
        "counter",
        "increment",
        package_id,
        vec![SuiJsonValue::new(json!(counter_id.to_hex_literal())).unwrap()],
        sender,
        gas_object,
    )
    .await
}

pub async fn transfer_coin(
    context: &mut WalletContext,
) -> Result<
    (
        ObjectID,
        SuiAddress,
        SuiAddress,
        TransactionDigest,
        ObjectRef,
        u64,
    ),
    anyhow::Error,
> {
    let gas_price = context.get_reference_gas_price().await?;
    let sender = context.config.keystore.addresses().get(0).cloned().unwrap();
    let receiver = context.config.keystore.addresses().get(1).cloned().unwrap();
    let client = context.get_client().await.unwrap();
    let object_refs: Vec<SuiObjectResponse> = client
        .read_api()
        .get_owned_objects(
            sender,
            Some(SuiObjectResponseQuery::new_with_options(
                SuiObjectDataOptions::full_content().with_bcs(),
            )),
            None,
            None,
        )
        .await?
        .data;
    let object_to_send: ObjectID = object_refs
        .iter()
        .find(|resp| {
            resp.data
                .as_ref()
                .unwrap()
                .bcs
                .as_ref()
                .unwrap()
                .try_as_move()
                .map(|m| m.has_public_transfer)
                .unwrap_or(false)
        })
        .unwrap()
        .data
        .as_ref()
        .unwrap()
        .object_id;

    // Send an object
    info!(
        "transferring coin {:?} from {:?} -> {:?}",
        object_to_send, sender, receiver
    );
    let res = SuiClientCommands::Transfer {
        to: receiver,
        object_id: object_to_send,
        gas: None,
        gas_budget: TEST_ONLY_GAS_UNIT_FOR_TRANSFER * gas_price,
        serialize_unsigned_transaction: false,
        serialize_signed_transaction: false,
    }
    .execute(context)
    .await?;

    let (digest, gas, gas_used) = if let SuiClientCommandResult::Transfer(response) = res {
        let effects = response.effects.unwrap();
        assert!(effects.status().is_ok());
        let gas_used = effects.gas_cost_summary();
        (
            response.digest,
            response
                .transaction
                .unwrap()
                .data
                .gas_data()
                .payment
                .clone(),
            gas_used.computation_cost + gas_used.storage_cost - gas_used.storage_rebate,
        )
    } else {
        panic!("transfer command did not return WalletCommandResult::Transfer");
    };

    Ok((
        object_to_send,
        sender,
        receiver,
        digest,
        gas[0].to_object_ref(),
        gas_used,
    ))
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
