// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{errors::IndexerError, indexer_reader::IndexerReader};
use jsonrpsee::{core::RpcResult, RpcModule};
use sui_json_rpc::SuiRpcModule;
use sui_json_rpc_api::{validate_limit, ExtendedApiServer, QUERY_MAX_RESULT_LIMIT_CHECKPOINTS};
use sui_json_rpc_types::{
    CheckpointedObjectID, EpochInfo, EpochPage, Page, QueryObjectsPage, SuiObjectResponseQuery,
};
use sui_open_rpc::Module;
use sui_types::sui_serde::BigInt;

pub(crate) struct ExtendedApi {
    inner: IndexerReader,
}

impl ExtendedApi {
    pub fn new(inner: IndexerReader) -> Self {
        Self { inner }
    }
}

#[async_trait::async_trait]
impl ExtendedApiServer for ExtendedApi {
    async fn get_epochs(
        &self,
        cursor: Option<BigInt<u64>>,
        limit: Option<usize>,
        descending_order: Option<bool>,
    ) -> RpcResult<EpochPage> {
        let limit = validate_limit(limit, QUERY_MAX_RESULT_LIMIT_CHECKPOINTS)
            .map_err(IndexerError::from)?;
        let mut epochs = self
            .inner
            .get_epochs(
                cursor.map(|x| *x),
                limit + 1,
                descending_order.unwrap_or(false),
            )
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
        let stored_epoch = self.inner.get_latest_epoch_info_from_db().await?;
        EpochInfo::try_from(stored_epoch).map_err(Into::into)
    }

    async fn query_objects(
        &self,
        _query: SuiObjectResponseQuery,
        _cursor: Option<CheckpointedObjectID>,
        _limit: Option<usize>,
    ) -> RpcResult<QueryObjectsPage> {
        Err(jsonrpsee::types::error::ErrorCode::MethodNotFound.into())
    }

    async fn get_total_transactions(&self) -> RpcResult<BigInt<u64>> {
        let latest_checkpoint = self.inner.get_latest_checkpoint().await?;
        Ok(latest_checkpoint.network_total_transactions.into())
    }
}

impl SuiRpcModule for ExtendedApi {
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        sui_json_rpc_api::ExtendedApiOpenRpc::module_doc()
    }
}
