// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use futures::future::join_all;
use sui_json_rpc_types::{SuiObjectDataOptions, SuiObjectResponse};
use sui_sdk::SuiClient;
use sui_types::base_types::ObjectID;

use crate::payload::{GetObject, ProcessPayload, RpcCommandProcessor, SignerInfo};
use async_trait::async_trait;

use super::validation::chunk_entities;

#[async_trait]
impl<'a> ProcessPayload<'a, &'a GetObject> for RpcCommandProcessor {
    async fn process(&'a self, op: &'a GetObject, _signer_info: &Option<SignerInfo>) -> Result<()> {
        if op.object_ids.is_empty() {
            panic!("No object ids provided, skipping query");
        };
        let clients = self.get_clients().await?;
        let chunked = chunk_entities(&op.object_ids, Some(op.chunk_size));

        for chunk in chunked {
            let mut tasks = Vec::new();
            for object_id in chunk {
                for client in clients.iter() {
                    let task = async move {
                        get_object(client, object_id).await.unwrap();
                    };
                    tasks.push(task);
                }
            }
            join_all(tasks).await;
        }
        Ok(())
    }
}

// TODO: should organize these into an api_calls.rs
pub(crate) async fn get_object(
    client: &SuiClient,
    object_id: ObjectID,
) -> Result<SuiObjectResponse> {
    let result = client
        .read_api()
        .get_object_with_options(object_id, SuiObjectDataOptions::full_content())
        .await
        .unwrap();
    Ok(result)
}
