// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::get_client;
use crate::messages::{create_publish_move_package_transaction, make_certificates};
use std::collections::HashMap;
use std::path::PathBuf;
use sui_config::ValidatorInfo;
use sui_core::authority_client::AuthorityAPI;
use sui_types::base_types::ObjectRef;
use sui_types::error::SuiResult;
use sui_types::messages::{Transaction, TransactionEffects, TransactionInfoResponse};
use sui_types::object::{Object, Owner};

pub async fn publish_package(
    gas_object: Object,
    path: PathBuf,
    configs: &[ValidatorInfo],
) -> ObjectRef {
    let transaction = create_publish_move_package_transaction(gas_object, path);
    let effects = submit_single_owner_transaction(transaction, configs).await;
    parse_package_ref(&effects).unwrap()
}

/// Helper function to publish the move package of a simple shared counter.
pub async fn publish_counter_package(gas_object: Object, configs: &[ValidatorInfo]) -> ObjectRef {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("../../sui_programmability/examples/basics");
    publish_package(gas_object, path, configs).await
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
        let effects = reply.signed_effects.unwrap().effects;
        all_effects.insert(effects.digest(), effects);
    }
    assert_eq!(all_effects.len(), 1);
    all_effects.into_values().next().unwrap()
}

/// Extract the package reference from a transaction effect. This is useful to deduce the
/// authority-created package reference after attempting to publish a new Move package.
fn parse_package_ref(effects: &TransactionEffects) -> Option<ObjectRef> {
    effects
        .created
        .iter()
        .find(|(_, owner)| matches!(owner, Owner::Immutable))
        .map(|(reference, _)| *reference)
}
