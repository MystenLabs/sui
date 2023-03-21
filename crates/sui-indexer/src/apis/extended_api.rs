// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use jsonrpsee::core::RpcResult;
use jsonrpsee::RpcModule;

use sui_json_rpc::api::{validate_limit, ExtendedApiServer, QUERY_MAX_RESULT_LIMIT_CHECKPOINTS};
use sui_json_rpc::SuiRpcModule;
use sui_json_rpc_types::{EpochInfo, EpochPage, Page};
use sui_open_rpc::Module;
use sui_types::base_types::EpochId;

use crate::store::IndexerStore;

pub(crate) struct ExtendedApi<S> {
    state: S,
}

impl<S: IndexerStore> ExtendedApi<S> {
    pub fn new(state: S) -> Self {
        Self { state }
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
