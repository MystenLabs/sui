// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use jsonrpsee::core::RpcResult;
use jsonrpsee::RpcModule;

use sui_json_rpc::api::{
    validate_limit, ExtendedApiServer, QUERY_MAX_RESULT_LIMIT_CHECKPOINTS,
    QUERY_MAX_RESULT_LIMIT_OBJECTS,
};
use sui_json_rpc::SuiRpcModule;
use sui_json_rpc_types::{
    CheckpointedObjectID, EpochInfo, EpochPage, MoveCallMetrics, NetworkMetrics, Page,
    QueryObjectsPage, SuiObjectDataFilter, SuiObjectResponse, SuiObjectResponseQuery,
};
use sui_open_rpc::Module;
use sui_types::sui_serde::BigInt;

use crate::errors::IndexerError;
use crate::store::IndexerStore;

pub(crate) struct ExtendedApi<S> {
    state: S,
}

impl<S: IndexerStore> ExtendedApi<S> {
    pub fn new(state: S) -> Self {
        Self { state }
    }

    async fn query_objects_internal(
        &self,
        query: SuiObjectResponseQuery,
        cursor: Option<CheckpointedObjectID>,
        limit: Option<usize>,
    ) -> Result<QueryObjectsPage, IndexerError> {
        let limit = validate_limit(limit, QUERY_MAX_RESULT_LIMIT_OBJECTS)?;

        let at_checkpoint = if let Some(CheckpointedObjectID {
            at_checkpoint: Some(cp),
            ..
        }) = cursor
        {
            cp
        } else {
            self.state.get_latest_checkpoint_sequence_number().await? as u64
        };

        let object_cursor = cursor.as_ref().map(|c| c.object_id);

        let SuiObjectResponseQuery { filter, options } = query;
        let filter = filter.unwrap_or_else(|| SuiObjectDataFilter::MatchAll(vec![]));

        let objects_from_db = self
            .state
            .query_objects_history(filter, at_checkpoint, object_cursor, limit + 1)
            .await?;

        let mut data = objects_from_db
            .into_iter()
            .map(|obj_read| {
                SuiObjectResponse::try_from((obj_read, options.clone().unwrap_or_default()))
            })
            .collect::<Result<Vec<SuiObjectResponse>, _>>()?;

        let has_next_page = data.len() > limit;
        data.truncate(limit);
        let next_cursor = data
            .last()
            .map(|obj| {
                obj.object().map(|o| CheckpointedObjectID {
                    object_id: o.object_id,
                    at_checkpoint: Some(at_checkpoint),
                })
            })
            .transpose()?;

        Ok(Page {
            data,
            next_cursor,
            has_next_page,
        })
    }
}

#[async_trait]
impl<S: IndexerStore + Sync + Send + 'static> ExtendedApiServer for ExtendedApi<S> {
    async fn get_epochs(
        &self,
        cursor: Option<BigInt<u64>>,
        limit: Option<usize>,
        descending_order: Option<bool>,
    ) -> RpcResult<EpochPage> {
        let limit = validate_limit(limit, QUERY_MAX_RESULT_LIMIT_CHECKPOINTS)?;
        let mut epochs = self
            .state
            .get_epochs(cursor.map(|c| *c), limit + 1, descending_order)
            .await?;

        let has_next_page = epochs.len() > limit;
        epochs.truncate(limit);
        let next_cursor = epochs.last().map(|e| e.epoch);
        Ok(Page {
            data: epochs,
            next_cursor: next_cursor.map(|id| id.into()),
            has_next_page,
        })
    }

    async fn get_current_epoch(&self) -> RpcResult<EpochInfo> {
        Ok(self.state.get_current_epoch().await?)
    }

    async fn query_objects(
        &self,
        query: SuiObjectResponseQuery,
        cursor: Option<CheckpointedObjectID>,
        limit: Option<usize>,
    ) -> RpcResult<QueryObjectsPage> {
        Ok(self.query_objects_internal(query, cursor, limit).await?)
    }

    async fn get_network_metrics(&self) -> RpcResult<NetworkMetrics> {
        Ok(self.state.get_network_metrics().await?)
    }

    async fn get_move_call_metrics(&self) -> RpcResult<MoveCallMetrics> {
        Ok(self.state.get_move_call_metrics().await?)
    }
}

impl<S> SuiRpcModule for ExtendedApi<S>
where
    S: IndexerStore + Sync + Send + 'static,
{
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        sui_json_rpc::api::ExtendedApiOpenRpc::module_doc()
    }
}
