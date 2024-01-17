// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::read_auth_env;
use crate::storage::rocksdb::rocksdb_rpc_types::{
    ReserveGasStorageRequest, ReserveGasStorageResponse, UpdateGasStorageRequest,
    UpdateGasStorageResponse,
};
use crate::storage::Storage;
use crate::types::GasCoin;
use axum::headers::HeaderMap;
use axum::http::header::AUTHORIZATION;
use reqwest::Client;
use sui_types::base_types::{ObjectID, SuiAddress};

pub struct RocksDbRpcClient {
    client: Client,
    server_address: String,
}

impl RocksDbRpcClient {
    pub fn new(server_address: String) -> Self {
        let client = Client::new();
        Self {
            client,
            server_address,
        }
    }
}

#[async_trait::async_trait]
impl Storage for RocksDbRpcClient {
    async fn reserve_gas_coins(
        &self,
        request_sponsor: SuiAddress,
        gas_budget: u64,
    ) -> anyhow::Result<Vec<GasCoin>> {
        let request = ReserveGasStorageRequest {
            gas_budget,
            request_sponsor,
        };
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            format!("Bearer {}", read_auth_env()).parse().unwrap(),
        );
        let response = self
            .client
            .post(format!("{}/v1/reserve_gas_coins", self.server_address))
            .headers(headers)
            .json(&request)
            .send()
            .await?
            .json::<ReserveGasStorageResponse>()
            .await?;
        response
            .gas_coins
            .ok_or_else(|| {
                anyhow::anyhow!(response
                    .error
                    .unwrap_or_else(|| "Unknown error".to_string()))
            })
            .map(|gas_coins| gas_coins.into_iter().map(|c| c.into()).collect())
    }

    async fn update_gas_coins(
        &self,
        sponsor_address: SuiAddress,
        released_gas_coins: Vec<GasCoin>,
        deleted_gas_coins: Vec<ObjectID>,
    ) -> anyhow::Result<()> {
        let released_gas_coins = released_gas_coins.into_iter().map(|c| c.into()).collect();
        let request = UpdateGasStorageRequest {
            sponsor_address,
            released_gas_coins,
            deleted_gas_coins,
        };
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            format!("Bearer {}", read_auth_env()).parse().unwrap(),
        );
        let response = self
            .client
            .post(format!("{}/v1/update_gas_coins", self.server_address))
            .headers(headers)
            .json(&request)
            .send()
            .await?
            .json::<UpdateGasStorageResponse>()
            .await?;
        if let Some(err) = response.error {
            Err(anyhow::anyhow!(err))
        } else {
            Ok(())
        }
    }

    async fn check_health(&self) -> anyhow::Result<()> {
        let result = self
            .client
            .get(format!("{}/", self.server_address))
            .send()
            .await?
            .text()
            .await?;
        if result == "OK" {
            Ok(())
        } else {
            Err(anyhow::anyhow!("Invalid health check response: {}", result))
        }
    }

    #[cfg(test)]
    async fn get_available_coin_count(&self, sponsor_address: SuiAddress) -> usize {
        self.client
            .post(format!(
                "{}/v1/get_available_coin_count",
                self.server_address
            ))
            .json(&sponsor_address)
            .send()
            .await
            .unwrap()
            .json::<usize>()
            .await
            .unwrap()
    }

    #[cfg(test)]
    async fn get_total_available_coin_balance(&self, sponsor_address: SuiAddress) -> u64 {
        self.client
            .post(format!(
                "{}/v1/get_total_available_coin_balance",
                self.server_address
            ))
            .json(&sponsor_address)
            .send()
            .await
            .unwrap()
            .json::<u64>()
            .await
            .unwrap()
    }

    #[cfg(test)]
    async fn get_reserved_coin_count(&self) -> usize {
        self.client
            .post(format!(
                "{}/v1/get_reserved_coin_count",
                self.server_address
            ))
            .json(&())
            .send()
            .await
            .unwrap()
            .json::<usize>()
            .await
            .unwrap()
    }
}
