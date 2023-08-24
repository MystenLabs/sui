// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::types::balance::Balance;
use crate::types::object::ObjectFilter;
use crate::types::protocol_config::ProtocolConfigs;
use crate::types::transaction_block::TransactionBlock;
use crate::types::{object::Object, sui_address::SuiAddress};
use async_graphql::connection::Connection;
use async_graphql::*;
use async_trait::async_trait;

#[async_trait]
pub(crate) trait DataProvider: Send + Sync {
    async fn fetch_obj(&self, address: SuiAddress, version: Option<u64>) -> Result<Option<Object>>;

    async fn fetch_owned_objs(
        &self,
        owner: &SuiAddress,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
        _filter: Option<ObjectFilter>,
    ) -> Result<Connection<String, Object>>;

    async fn fetch_balance(&self, address: &SuiAddress, type_: Option<String>) -> Result<Balance>;

    async fn fetch_balance_connection(
        &self,
        address: &SuiAddress,
        first: Option<u64>,
        after: Option<String>,
        last: Option<u64>,
        before: Option<String>,
    ) -> Result<Connection<String, Balance>>;

    async fn fetch_tx(&self, digest: &str) -> Result<Option<TransactionBlock>>;

    async fn fetch_chain_id(&self) -> Result<String>;

    async fn fetch_protocol_config(&self, version: Option<u64>) -> Result<ProtocolConfigs>;
}
