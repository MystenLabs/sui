// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::payload::{GetAllBalances, ProcessPayload, RpcCommandProcessor, SignerInfo};
use anyhow::Result;
use async_trait::async_trait;
use futures::future::join_all;
use sui_json_rpc_types::Balance;
use sui_sdk::SuiClient;
use sui_types::base_types::SuiAddress;

use super::validation::chunk_entities;

#[async_trait]
impl<'a> ProcessPayload<'a, &'a GetAllBalances> for RpcCommandProcessor {
    async fn process(
        &'a self,
        op: &'a GetAllBalances,
        _signer_info: &Option<SignerInfo>,
    ) -> Result<()> {
        if op.addresses.is_empty() {
            panic!("No addresses provided, skipping query");
        }
        let clients = self.get_clients().await?;
        let chunked = chunk_entities(&op.addresses, Some(op.chunk_size));
        for chunk in chunked {
            let mut tasks = Vec::new();
            for address in chunk {
                for client in clients.iter() {
                    let owner_address = address;
                    let task = async move {
                        get_all_balances(client, owner_address).await.unwrap();
                    };
                    tasks.push(task);
                }
            }
            join_all(tasks).await;
        }
        Ok(())
    }
}

async fn get_all_balances(client: &SuiClient, owner_address: SuiAddress) -> Result<Vec<Balance>> {
    let balances = client
        .coin_read_api()
        .get_all_balances(owner_address)
        .await
        .unwrap();
    Ok(balances)
}
