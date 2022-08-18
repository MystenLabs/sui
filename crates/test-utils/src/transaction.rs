// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::authority::get_client;
use crate::messages::{create_publish_move_package_transaction, make_certificates};
use crate::test_account_keys;
use move_package::BuildConfig;
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;
use sui::client_commands::WalletContext;
use sui_config::ValidatorInfo;
use sui_core::authority_client::AuthorityAPI;
use sui_json_rpc_types::{SuiParsedTransactionResponse, SuiTransactionResponse};
use sui_sdk::json::SuiJsonValue;
use sui_types::base_types::ObjectRef;
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::error::SuiResult;
use sui_types::message_envelope::Message;
use sui_types::messages::{Transaction, TransactionEffects, TransactionInfoResponse};
use sui_types::object::{Object, Owner};
use tracing::debug;

pub async fn publish_package(
    gas_object: Object,
    path: PathBuf,
    configs: &[ValidatorInfo],
) -> ObjectRef {
    let (sender, keypair) = test_account_keys().pop().unwrap();
    let transaction = create_publish_move_package_transaction(
        gas_object.compute_object_reference(),
        path,
        sender,
        &keypair,
    );
    let effects = submit_single_owner_transaction(transaction, configs).await;
    parse_package_ref(&effects).unwrap()
}

/// Helper function to publish the move package of a simple shared counter.
pub async fn publish_counter_package(gas_object: Object, configs: &[ValidatorInfo]) -> ObjectRef {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("../../sui_programmability/examples/basics");
    publish_package(gas_object, path, configs).await
}

/// A helper function to publish basic package using gateway API
pub async fn publish_basics_package(context: &WalletContext, sender: SuiAddress) -> ObjectRef {
    let transaction = {
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

        let data = context
            .gateway
            .publish(sender, all_module_bytes, None, 50000)
            .await
            .unwrap();

        Transaction::from_data(data, &context.keystore.signer(sender))
    };

    let resp = context
        .gateway
        .execute_transaction(transaction)
        .await
        .unwrap();

    if let Some(SuiParsedTransactionResponse::Publish(resp)) = resp.parsed_data {
        resp.package.to_object_ref()
    } else {
        panic!()
    }
}

/// A helper function to submit a move transaction using gateway API
pub async fn submit_move_transaction(
    context: &WalletContext,
    module: &'static str,
    function: &'static str,
    package_ref: ObjectRef,
    arguments: Vec<SuiJsonValue>,
    sender: SuiAddress,
    gas_object: Option<ObjectID>,
) -> SuiTransactionResponse {
    debug!(?package_ref, ?arguments, "move_transaction");

    let data = context
        .gateway
        .move_call(
            sender,
            package_ref.0,
            module.into(),
            function.into(),
            vec![], // type_args
            arguments,
            gas_object,
            50000,
        )
        .await
        .unwrap();

    let tx = Transaction::from_data(data, &context.keystore.signer(sender));
    context.gateway.execute_transaction(tx).await.unwrap()
}

/// A helper function to publish the basics package and make counter objects
pub async fn publish_basics_package_and_make_counter(
    context: &WalletContext,
    sender: SuiAddress,
) -> (ObjectRef, ObjectID) {
    let package_ref = publish_basics_package(context, sender).await;

    debug!(?package_ref);

    let create_shared_obj_resp = submit_move_transaction(
        context,
        "counter",
        "create",
        package_ref,
        vec![],
        sender,
        None,
    )
    .await;

    let counter_id = create_shared_obj_resp.effects.created[0]
        .clone()
        .reference
        .object_id;
    debug!(?counter_id);
    (package_ref, counter_id)
}

pub async fn increment_counter(
    context: &WalletContext,
    sender: SuiAddress,
    gas_object: Option<ObjectID>,
    package_ref: ObjectRef,
    counter_id: ObjectID,
) -> SuiTransactionResponse {
    submit_move_transaction(
        context,
        "counter",
        "increment",
        package_ref,
        vec![SuiJsonValue::new(json!(counter_id.to_hex_literal())).unwrap()],
        sender,
        gas_object,
    )
    .await
}

/// Submit a certificate containing only owned-objects to all authorities.
pub async fn submit_single_owner_transaction(
    transaction: Transaction,
    configs: &[ValidatorInfo],
) -> TransactionEffects {
    let certificate = make_certificates(vec![transaction]).pop().unwrap();

    let mut responses = Vec::new();
    for config in configs {
        let client = get_client(config);
        let reply = client
            .handle_certificate(certificate.clone())
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
    transaction: Transaction,
    configs: &[ValidatorInfo],
) -> SuiResult<TransactionEffects> {
    let certificate = make_certificates(vec![transaction]).pop().unwrap();

    let replies = loop {
        let futures: Vec<_> = configs
            .iter()
            .map(|config| {
                let client = get_client(config);
                let cert = certificate.clone();
                async move { client.handle_certificate(cert).await }
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

pub fn get_unique_effects(replies: Vec<TransactionInfoResponse>) -> TransactionEffects {
    let mut all_effects = HashMap::new();
    for reply in replies {
        let effects = reply.signed_effects.unwrap().effects().clone();
        all_effects.insert(effects.digest(), effects);
    }
    assert_eq!(all_effects.len(), 1);
    all_effects.into_values().next().unwrap()
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
