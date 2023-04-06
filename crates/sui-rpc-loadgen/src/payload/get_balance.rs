// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::payload::{GetBalance, ProcessPayload, RpcCommandProcessor, SignerInfo};
use anyhow::Result;
use async_trait::async_trait;
use futures::future::join_all;
use sui_json_rpc_types::Balance;
use sui_sdk::SuiClient;
use sui_types::base_types::SuiAddress;

use super::validation::chunk_entities;

#[async_trait]
impl<'a> ProcessPayload<'a, &'a GetBalance> for RpcCommandProcessor {
    async fn process(
        &'a self,
        op: &'a GetBalance,
        _signer_info: &Option<SignerInfo>,
    ) -> Result<()> {
        if op.addresses_with_coin_types.is_empty() {
            panic!("No addresses provided, skipping query");
        }

        // Map address to each coin_type in coin_types
        let mapped: Vec<(SuiAddress, String)> = op
            .addresses_with_coin_types
            .iter()
            .flat_map(|awct| {
                awct.coin_types
                    .iter()
                    .map(move |coin_type| (awct.address, coin_type.clone()))
            })
            .collect();

        let clients = self.get_clients().await?;
        let chunked = chunk_entities(&mapped, Some(op.chunk_size));

        // TODO: generate this beforehand
        for chunk in chunked {
            let mut tasks = Vec::new();
            for (owner_address, coin_type) in chunk {
                for client in clients.iter() {
                    let with_coin_type = coin_type.clone();
                    let task = async move {
                        get_balance(client, owner_address, with_coin_type)
                            .await
                            .unwrap()
                    };
                    tasks.push(task);
                }
            }
            join_all(tasks).await;
        }
        Ok(())
    }
}

async fn get_balance(
    client: &SuiClient,
    owner_address: SuiAddress,
    coin_type: String,
) -> Result<Balance> {
    let balance = client
        .coin_read_api()
        .get_balance(owner_address, Some(coin_type))
        .await
        .unwrap();
    Ok(balance)
}
