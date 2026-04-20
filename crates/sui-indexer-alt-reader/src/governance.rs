// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use async_graphql::dataloader::Loader;
use mysten_common::ZipDebugEqIteratorExt;
use sui_sdk_types::Address;

use crate::error::Error;
use crate::fullnode_client::FullnodeClient;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RewardsKey(pub Address);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ValidatorAddressKey(pub Address);

#[async_trait::async_trait]
impl Loader<RewardsKey> for FullnodeClient {
    type Value = u64;
    type Error = Error;

    async fn load(&self, keys: &[RewardsKey]) -> Result<HashMap<RewardsKey, u64>, Self::Error> {
        let ids: Vec<Address> = keys.iter().map(|k| k.0).collect();
        let results = self.calculate_rewards(&ids).await?;
        Ok(keys
            .iter()
            .zip_debug_eq(results)
            .map(|(k, reward)| (k.clone(), reward))
            .collect())
    }
}

#[async_trait::async_trait]
impl Loader<ValidatorAddressKey> for FullnodeClient {
    type Value = Address;
    type Error = Error;

    async fn load(
        &self,
        keys: &[ValidatorAddressKey],
    ) -> Result<HashMap<ValidatorAddressKey, Address>, Self::Error> {
        let ids: Vec<Address> = keys.iter().map(|k| k.0).collect();
        let results = self.get_validator_address_by_pool_id(&ids).await?;
        Ok(keys
            .iter()
            .zip_debug_eq(results)
            .map(|(k, addr)| (k.clone(), addr))
            .collect())
    }
}
