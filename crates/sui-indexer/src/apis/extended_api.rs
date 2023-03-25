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
    BigInt, CheckpointId, EpochInfo, EpochPage, MoveCallMetrics, NetworkMetrics, ObjectsPage, Page,
    SuiObjectDataFilter, SuiObjectResponse, SuiObjectResponseQuery,
};
use sui_open_rpc::Module;
use sui_types::base_types::{EpochId, ObjectID};

use crate::errors::IndexerError;
use crate::store::IndexerStore;

pub(crate) struct ExtendedApi<S> {
    state: S,
}

impl<S: IndexerStore> ExtendedApi<S> {
    pub fn new(state: S) -> Self {
        Self { state }
    }

    fn query_objects_internal(
        &self,
        query: SuiObjectResponseQuery,
        cursor: Option<ObjectID>,
        limit: Option<usize>,
        at_checkpoint: Option<CheckpointId>,
    ) -> Result<ObjectsPage, IndexerError> {
        let limit = validate_limit(limit, QUERY_MAX_RESULT_LIMIT_OBJECTS)?;

        let at_checkpoint = match at_checkpoint {
            Some(CheckpointId::SequenceNumber(seq)) => seq,
            Some(CheckpointId::Digest(digest)) => {
                <BigInt>::from(self.state.get_checkpoint_sequence_number(digest)?)
            }
            None => <BigInt>::from(self.state.get_latest_checkpoint_sequence_number()? as u64),
        };

        let SuiObjectResponseQuery { filter, options } = query;
        let filter = filter.unwrap_or_else(|| SuiObjectDataFilter::MatchAll(vec![]));

        let objects_from_db =
            self.state
                .query_objects(filter, <u64>::from(at_checkpoint), cursor, limit + 1)?;

        let mut data = objects_from_db
            .into_iter()
            .map(|obj_read| {
                SuiObjectResponse::try_from((obj_read, options.clone().unwrap_or_default()))
            })
            .collect::<Result<Vec<SuiObjectResponse>, _>>()?;

        let has_next_page = data.len() > limit;
        data.truncate(limit);
        let next_cursor_result = data
            .last()
            .cloned()
            .map(|obj| obj.into_object().map(|o| o.object_id))
            .transpose();

        let next_cursor = next_cursor_result?;

        Ok(Page {
            data,
            next_cursor,
            has_next_page,
        })
    }
}

#[async_trait]
impl<S: IndexerStore + Sync + Send + 'static> ExtendedApiServer for ExtendedApi<S> {
    async fn get_epoch(
        &self,
        cursor: Option<EpochId>,
        limit: Option<usize>,
        descending_order: Option<bool>,
    ) -> RpcResult<EpochPage> {
        let limit = validate_limit(limit, QUERY_MAX_RESULT_LIMIT_CHECKPOINTS)?;
        let mut epochs = self.state.get_epochs(cursor, limit + 1, descending_order)?;

        let has_next_page = epochs.len() > limit;
        epochs.truncate(limit);
        let next_cursor = has_next_page
            .then_some(epochs.last().map(|e| e.epoch))
            .flatten();
        Ok(Page {
            data: epochs,
            next_cursor,
            has_next_page,
        })
    }

    async fn get_current_epoch(&self) -> RpcResult<EpochInfo> {
        Ok(self.state.get_current_epoch()?)
    }

    async fn query_objects(
        &self,
        query: SuiObjectResponseQuery,
        cursor: Option<ObjectID>,
        limit: Option<usize>,
        at_checkpoint: Option<CheckpointId>,
    ) -> RpcResult<ObjectsPage> {
        Ok(self.query_objects_internal(query, cursor, limit, at_checkpoint)?)
    }

    async fn get_network_metrics(&self) -> RpcResult<NetworkMetrics> {
        Ok(self.state.get_network_metrics()?)
    }

    async fn get_move_call_metrics(&self) -> RpcResult<MoveCallMetrics> {
        Ok(self.state.get_move_call_metrics()?)
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
