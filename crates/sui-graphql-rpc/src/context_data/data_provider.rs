// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::types::object::Object;
use crate::types::protocol_config::ProtocolConfigs;
use async_graphql::*;
use async_trait::async_trait;
use sui_json_rpc_types::SuiObjectDataOptions;
use sui_sdk::types::base_types::ObjectID;
use sui_sdk::types::sui_system_state::sui_system_state_summary::SuiSystemStateSummary;

#[async_trait]
pub(crate) trait DataProvider: Send + Sync {
    async fn get_object_with_options(
        &self,
        object_id: ObjectID,
        options: SuiObjectDataOptions,
    ) -> Result<Option<Object>>;

    async fn multi_get_object_with_options(
        &self,
        object_ids: Vec<ObjectID>,
        options: SuiObjectDataOptions,
    ) -> Result<Vec<Object>>;

    async fn fetch_protocol_config(&self, version: Option<u64>) -> Result<ProtocolConfigs>;

    async fn get_latest_sui_system_state(&self) -> Result<SuiSystemStateSummary>;
}
