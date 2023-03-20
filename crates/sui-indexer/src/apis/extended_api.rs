// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use jsonrpsee::core::RpcResult;
use jsonrpsee::RpcModule;

use crate::errors::IndexerError;
use crate::store::IndexerStore;
use sui_json_rpc::api::{
    validate_limit, ExtendedApiServer, QUERY_MAX_RESULT_LIMIT_CHECKPOINTS,
    QUERY_MAX_RESULT_LIMIT_OBJECTS,
};
use sui_json_rpc::SuiRpcModule;
use sui_json_rpc_types::{
    CheckpointId, EpochInfo, EpochPage, ObjectsPage, Page, SuiObjectDataFilter, SuiObjectResponse,
    SuiObjectResponseQuery,
};
use sui_open_rpc::Module;
use sui_types::base_types::{EpochId, ObjectID};

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
        descending_order: Option<bool>,
        at_checkpoint: Option<CheckpointId>,
    ) -> Result<ObjectsPage, IndexerError> {
        let limit = validate_limit(limit, QUERY_MAX_RESULT_LIMIT_OBJECTS)?;
        let is_descending = descending_order.unwrap_or_default();
        let SuiObjectResponseQuery { filter, options } = query;

        let objects_from_db = match filter {
            None => self
                .state
                .get_all_objects_page(cursor, limit, is_descending, at_checkpoint),
            Some(SuiObjectDataFilter::AddressOwner(address)) => {
                self.state.get_all_objects_page_by_owner(
                    cursor,
                    address,
                    limit,
                    is_descending,
                    at_checkpoint,
                )
            }
            Some(SuiObjectDataFilter::StructType(struct_tag)) => {
                self.state.get_all_objects_page_by_type(
                    cursor,
                    struct_tag,
                    limit,
                    is_descending,
                    at_checkpoint,
                )
            }
            _ => Err(IndexerError::NotImplementedError(format!(
                "Filter type [{filter:?}] not supported by the Indexer."
            )))?,
        };

        let mut data = objects_from_db?
            .into_iter()
            .map(|obj_read| {
                SuiObjectResponse::try_from((obj_read, options.clone().unwrap_or_default()))
            })
            .collect::<Result<Vec<SuiObjectResponse>, _>>()?;

        let has_next_page = data.len() > limit;
        data.truncate(limit);
        let next_cursor_result =
            data.last()
                .cloned()
                .map_or(Ok(cursor), |obj| match obj.into_object() {
                    Ok(obj_data) => Ok(Some(obj_data.object_id)),
                    Err(e) => Err(IndexerError::UserInputError(e)),
                });

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
    ) -> RpcResult<EpochPage> {
        let limit = validate_limit(limit, QUERY_MAX_RESULT_LIMIT_CHECKPOINTS)?;
        let mut epochs = self.state.get_epochs(cursor, limit + 1)?;

        let has_next_page = epochs.len() > limit;
        epochs.truncate(limit);
        let next_cursor = has_next_page
            .then_some(epochs.last().map(|e| e.epoch))
            .flatten();
        Ok(Page {
            data: epochs,
            next_cursor,
            has_next_page: false,
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
        descending_order: Option<bool>,
        at_checkpoint: Option<CheckpointId>,
    ) -> RpcResult<ObjectsPage> {
        Ok(self.query_objects_internal(query, cursor, limit, descending_order, at_checkpoint)?)
    }

    async fn get_total_packages(&self) -> RpcResult<u64> {
        Ok(self.state.get_total_packages()?)
    }

    async fn get_total_addresses(&self) -> RpcResult<u64> {
        Ok(self.state.get_total_addresses()?)
    }

    async fn get_total_objects(&self) -> RpcResult<u64> {
        Ok(self.state.get_total_objects()?)
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
