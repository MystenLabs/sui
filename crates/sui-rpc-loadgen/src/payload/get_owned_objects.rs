// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::payload::validation::cross_validate_entities;
use crate::payload::{GetOwnedObjects, ProcessPayload, RpcCommandProcessor, SignerInfo};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures::future::join_all;
use std::time::Instant;
use sui_json_rpc_types::{
    CheckpointedObjectID, ObjectsPage, SuiObjectDataOptions, SuiObjectResponse,
    SuiObjectResponseQuery,
};
use sui_sdk::SuiClient;
use sui_types::base_types::SuiAddress;
use tracing::debug;
use tracing::log::warn;

#[async_trait]
impl<'a> ProcessPayload<'a, &'a GetOwnedObjects> for RpcCommandProcessor {
    async fn process(
        &'a self,
        op: &'a GetOwnedObjects,
        _signer_info: &Option<SignerInfo>,
    ) -> Result<()> {
        let clients = self.get_clients().await?;
        println!("{:?}, {:?}", op.address, op.from_file);

        let addresses: Vec<SuiAddress> = match (op.address, op.from_file) {
            (Some(_address), Some(true)) => {
                return Err(anyhow!("Cannot specify both address and from_file"));
            }
            (Some(address), None) => vec![address],
            (None, Some(true)) => self
                .get_addresses()
                .iter()
                .map(|address| *address)
                .collect(),
            _ => {
                return Err(anyhow!(
                    "invalid combination of address and from_file: please provide either address or from_file = true"
                ));
            }
        };

        let query = SuiObjectResponseQuery::new_with_options(SuiObjectDataOptions::full_content());

        for address in addresses {
            let mut results: Vec<ObjectsPage> = Vec::new();
            // construct object cursor

            while results.is_empty() || results.iter().any(|r| r.has_next_page) {
                let cursor = if results.is_empty() {
                    None
                } else {
                    let some_cursor = results.get(0).unwrap().next_cursor;
                    for (idx, result) in results.iter().enumerate() {
                        if result.next_cursor != some_cursor {
                            warn!(
                                "Cursors are not the same, expected: {:?} received: {:?} at {idx}",
                                some_cursor, result.next_cursor
                            );
                        }
                    }
                    some_cursor
                };

                results =
                    get_owned_objects(&clients, address, Some(query.clone()), cursor, None).await;

                let owned_objects: Vec<Vec<SuiObjectResponse>> =
                    results.iter().map(|page| page.data.clone()).collect();
                cross_validate_entities(&owned_objects, "OwnedObjects");
            }
        }
        Ok(())
    }
}

pub(crate) async fn get_owned_objects(
    clients: &[SuiClient],
    address: SuiAddress,
    query: Option<SuiObjectResponseQuery>,
    cursor: Option<CheckpointedObjectID>,
    limit: Option<usize>,
) -> Vec<ObjectsPage> {
    let objects: Vec<ObjectsPage> = join_all(clients.iter().enumerate().map(|(i, client)| {
        let with_query = query.clone();
        async move {
            let start_time = Instant::now();
            let objects = client
                .read_api()
                .get_owned_objects(address, with_query, cursor, limit)
                .await
                .unwrap();
            let elapsed_time = start_time.elapsed();
            debug!(
                "GetOwnedObjects Request latency {:.4} for rpc at url {i}",
                elapsed_time.as_secs_f64()
            );
            objects
        }
    }))
    .await;
    objects
}
