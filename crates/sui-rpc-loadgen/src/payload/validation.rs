// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::future::join_all;
use itertools::Itertools;
use std::collections::HashSet;
use std::fmt::Debug;
use sui_json_rpc_types::{
    SuiObjectDataOptions, SuiObjectResponse, SuiTransactionBlockEffectsAPI,
    SuiTransactionBlockResponse, SuiTransactionBlockResponseOptions,
};
use sui_sdk::SuiClient;
use sui_types::base_types::{ObjectID, TransactionDigest};
use tracing::error;
use tracing::log::warn;

const LOADGEN_QUERY_MAX_RESULT_LIMIT: usize = 25;

pub(crate) fn cross_validate_entities<U>(entities: &[Vec<U>], entity_name: &str)
where
    U: PartialEq + Debug,
{
    if entities.len() < 2 {
        return;
    }

    let length = entities[0].len();
    if let Some((vec_index, v)) = entities.iter().enumerate().find(|(_, v)| v.len() != length) {
        error!(
            "Entity: {} lengths do not match at index {}: first vec has length {} vs vec {} has length {}",
            entity_name, vec_index, length, vec_index, v.len()
        );
        return;
    }

    // Iterate through all indices (from 0 to length - 1) of the inner vectors.
    for i in 0..length {
        // Create an iterator that produces references to elements at position i in each inner vector of entities.
        let mut iter = entities.iter().map(|v| &v[i]);

        // Compare first against rest of the iter (other inner vectors)
        if let Some(first) = iter.next() {
            for (j, other) in iter.enumerate() {
                if first != other {
                    // Example error: Entity: ExampleEntity mismatch at index 2: expected: 3, received ExampleEntity[1]: 4
                    error!(
                        "Entity: {} mismatch at index {}: expected: {:?}, received {}: {:?}",
                        entity_name,
                        i,
                        first,
                        format!("{}[{}]", entity_name, j + 1),
                        other
                    );
                }
            }
        }
    }
}

pub(crate) async fn check_transactions(
    clients: &[SuiClient],
    digests: &[TransactionDigest],
    cross_validate: bool,
    verify_objects: bool,
) -> Vec<Vec<SuiTransactionBlockResponse>> {
    let transactions: Vec<Vec<SuiTransactionBlockResponse>> =
        join_all(clients.iter().map(|client| async move {
            client
                .read_api()
                .multi_get_transactions_with_options(
                    digests.to_vec(),
                    SuiTransactionBlockResponseOptions::full_content(), // todo(Will) support options for this
                )
                .await
        }))
        .await
        .into_iter()
        .enumerate()
        .filter_map(|(i, result)| match result {
            Ok(transactions) => Some(transactions),
            Err(err) => {
                warn!(
                    "Failed to fetch transactions for vec {i}: {:?}. Logging digests, {:?}",
                    err, digests
                );
                None
            }
        })
        .collect();

    if cross_validate {
        cross_validate_entities(&transactions, "Transactions");
    }

    if verify_objects {
        let object_ids = transactions
            .iter()
            .flatten()
            .flat_map(get_all_object_ids)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();

        check_objects(clients, &object_ids, cross_validate).await;
    }
    transactions
}

pub(crate) fn get_all_object_ids(response: &SuiTransactionBlockResponse) -> Vec<ObjectID> {
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

pub(crate) fn chunk_entities<U>(entities: &[U], chunk_size: Option<usize>) -> Vec<Vec<U>>
where
    U: Clone + PartialEq + Debug,
{
    let chunk_size = chunk_size.unwrap_or(LOADGEN_QUERY_MAX_RESULT_LIMIT);
    entities
        .iter()
        .cloned()
        .chunks(chunk_size)
        .into_iter()
        .map(|chunk| chunk.collect())
        .collect()
}

pub(crate) async fn check_objects(
    clients: &[SuiClient],
    object_ids: &[ObjectID],
    cross_validate: bool,
) {
    let chunks = chunk_entities(object_ids, None);
    let results = join_all(chunks.iter().map(|chunk| multi_get_object(clients, chunk))).await;

    if cross_validate {
        for result in results {
            cross_validate_entities(&result, "Objects");
        }
    }
}

pub(crate) async fn multi_get_object(
    clients: &[SuiClient],
    object_ids: &[ObjectID],
) -> Vec<Vec<SuiObjectResponse>> {
    let objects: Vec<Vec<SuiObjectResponse>> = join_all(clients.iter().map(|client| async move {
        let object_ids = if object_ids.len() > LOADGEN_QUERY_MAX_RESULT_LIMIT {
            warn!(
                "The input size for multi_get_object_with_options has exceed the query limit\
         {LOADGEN_QUERY_MAX_RESULT_LIMIT}: {}, time to implement chunking",
                object_ids.len()
            );
            &object_ids[0..LOADGEN_QUERY_MAX_RESULT_LIMIT]
        } else {
            object_ids
        };

        client
            .read_api()
            .multi_get_object_with_options(
                object_ids.to_vec(),
                SuiObjectDataOptions::full_content(), // todo(Will) support options for this
            )
            .await
    }))
    .await
    .into_iter()
    .enumerate()
    .filter_map(|(i, result)| match result {
        Ok(obj_vec) => Some(obj_vec),
        Err(err) => {
            error!(
                "Failed to fetch objects for vec {i}: {:?}. Logging objectIDs, {:?}",
                err, object_ids
            );
            None
        }
    })
    .collect();
    objects
}
