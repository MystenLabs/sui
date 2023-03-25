// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::future::join_all;
use std::collections::HashSet;
use std::time::Instant;
use sui_json_rpc::api::QUERY_MAX_RESULT_LIMIT;
use sui_json_rpc_types::{
    SuiObjectDataOptions, SuiTransactionEffectsAPI, SuiTransactionResponse,
    SuiTransactionResponseOptions,
};
use sui_sdk::SuiClient;
use sui_types::base_types::{ObjectID, TransactionDigest};
use tracing::log::warn;
use tracing::{debug, error};

pub(crate) fn cross_validate_entities<T, U>(
    keys: &[T],
    first: &[U],
    second: &[U],
    key_name: &str,
    entity_name: &str,
) where
    T: std::fmt::Debug,
    U: PartialEq + std::fmt::Debug,
{
    if first.len() != second.len() {
        error!(
            "Entity: {} lengths do not match: {} vs {}",
            entity_name,
            first.len(),
            second.len()
        );
        return;
    }

    for (i, (a, b)) in first.iter().zip(second.iter()).enumerate() {
        if a != b {
            error!(
                "Entity: {} mismatch with index {}: {}: {:?}, first: {:?}, second: {:?}",
                entity_name, i, key_name, keys[i], a, b
            );
        }
    }
}

pub(crate) async fn check_transactions(
    clients: &[SuiClient],
    digests: &[TransactionDigest],
    cross_validate: bool,
    verify_objects: bool,
) {
    let transactions = join_all(clients.iter().enumerate().map(|(i, client)| async move {
        let start_time = Instant::now();
        let transactions = client
            .read_api()
            .multi_get_transactions_with_options(
                digests.to_vec(),
                SuiTransactionResponseOptions::full_content(), // todo(Will) support options for this
            )
            .await;
        let elapsed_time = start_time.elapsed();
        debug!(
            "MultiGetTransactions Request latency {:.4} for rpc at url {i}",
            elapsed_time.as_secs_f64()
        );
        transactions
    }))
    .await;

    // TODO: support more than 2 transactions
    if cross_validate && transactions.len() == 2 {
        if let (Some(t1), Some(t2)) = (transactions.get(0), transactions.get(1)) {
            let first = match t1 {
                Ok(vec) => vec.as_slice(),
                Err(err) => {
                    error!("Error unwrapping first vec of transactions: {:?}", err);
                    error!("Logging digests, {:?}", digests);
                    return;
                }
            };
            let second = match t2 {
                Ok(vec) => vec.as_slice(),
                Err(err) => {
                    error!("Error unwrapping second vec of transactions: {:?}", err);
                    error!("Logging digests, {:?}", digests);
                    return;
                }
            };

            cross_validate_entities(digests, first, second, "TransactionDigest", "Transaction");

            if verify_objects {
                let object_ids = first
                    .iter()
                    .chain(second.iter())
                    .flat_map(get_all_object_ids)
                    .collect::<HashSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>();

                check_objects(clients, &object_ids, cross_validate).await;
            }
        }
    }
}

pub(crate) fn get_all_object_ids(response: &SuiTransactionResponse) -> Vec<ObjectID> {
    let objects = match response.effects.as_ref() {
        // TODO: handle deleted and wrapped objects
        Some(effects) => effects.all_changed_objects(),
        None => {
            error!(
                "Effects for transaction digest {} should not be empty",
                response.digest
            );
            vec![]
        }
    };
    objects
        .iter()
        .map(|(owned_object_ref, _)| owned_object_ref.object_id())
        .collect::<Vec<_>>()
}

// todo: this and check_transactions can be generic
pub(crate) async fn check_objects(
    clients: &[SuiClient],
    object_ids: &[ObjectID],
    cross_validate: bool,
) {
    let objects = join_all(clients.iter().enumerate().map(|(i, client)| async move {
        // TODO: support chunking so that we don't exceed query limit
        let object_ids = if object_ids.len() > QUERY_MAX_RESULT_LIMIT {
            warn!(
                "The input size for multi_get_object_with_options has exceed the query limit\
             {QUERY_MAX_RESULT_LIMIT}: {}, time to implement chunking",
                object_ids.len()
            );
            &object_ids[0..QUERY_MAX_RESULT_LIMIT]
        } else {
            object_ids
        };
        let start_time = Instant::now();
        let transactions = client
            .read_api()
            .multi_get_object_with_options(
                object_ids.to_vec(),
                SuiObjectDataOptions::full_content(), // todo(Will) support options for this
            )
            .await;
        let elapsed_time = start_time.elapsed();
        debug!(
            "MultiGetObject Request latency {:.4} for rpc at url {i}",
            elapsed_time.as_secs_f64()
        );
        transactions
    }))
    .await;

    // TODO: support more than 2 transactions
    if cross_validate && objects.len() == 2 {
        if let (Some(t1), Some(t2)) = (objects.get(0), objects.get(1)) {
            let first = match t1 {
                Ok(vec) => vec.as_slice(),
                Err(err) => {
                    error!(
                        "Error unwrapping first vec of objects: {:?} for objectIDs {:?}",
                        err, object_ids
                    );
                    return;
                }
            };
            let second = match t2 {
                Ok(vec) => vec.as_slice(),
                Err(err) => {
                    error!(
                        "Error unwrapping second vec of objects: {:?} for objectIDs {:?}",
                        err, object_ids
                    );
                    return;
                }
            };

            cross_validate_entities(object_ids, first, second, "ObjectID", "Object");
        }
    }
}
