// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::indexer_reader::IndexerReader;
use jsonrpsee::{core::RpcResult, RpcModule};
use sui_json_rpc::SuiRpcModule;
use sui_json_rpc_api::{validate_limit, ExtendedApiServer, QUERY_MAX_RESULT_LIMIT_CHECKPOINTS};
use sui_json_rpc_types::{
    AddressMetrics, CheckpointedObjectID, EpochInfo, EpochMetrics, EpochMetricsPage, EpochPage,
    MoveCallMetrics, NetworkMetrics, Page, QueryObjectsPage, SuiObjectResponseQuery,
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
        let limit = validate_limit(limit, QUERY_MAX_RESULT_LIMIT_CHECKPOINTS)?;
        let mut epochs = self
            .inner
            .spawn_blocking(move |this| {
                this.get_epochs(
                    cursor.map(|x| *x),
                    limit + 1,
                    descending_order.unwrap_or(false),
                )
            })
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

    async fn get_epoch_metrics(
        &self,
        cursor: Option<BigInt<u64>>,
        limit: Option<usize>,
        descending_order: Option<bool>,
    ) -> RpcResult<EpochMetricsPage> {
        let limit = validate_limit(limit, QUERY_MAX_RESULT_LIMIT_CHECKPOINTS)?;
        let epochs = self
            .inner
            .spawn_blocking(move |this| {
                this.get_epochs(
                    cursor.map(|x| *x),
                    limit + 1,
                    descending_order.unwrap_or(false),
                )
            })
            .await?;

        let mut epoch_metrics = epochs
            .into_iter()
            .map(|e| EpochMetrics {
                epoch: e.epoch,
                epoch_total_transactions: e.epoch_total_transactions,
                first_checkpoint_id: e.first_checkpoint_id,
                epoch_start_timestamp: e.epoch_start_timestamp,
                end_of_epoch_info: e.end_of_epoch_info,
            })
            .collect::<Vec<_>>();

        let has_next_page = epoch_metrics.len() > limit;
        epoch_metrics.truncate(limit);
        let next_cursor = epoch_metrics.last().map(|e| e.epoch);
        Ok(Page {
            data: epoch_metrics,
            next_cursor: next_cursor.map(|id| id.into()),
            has_next_page,
        })
    }

    async fn get_current_epoch(&self) -> RpcResult<EpochInfo> {
        let stored_epoch = self
            .inner
            .spawn_blocking(|this| this.get_latest_epoch_info_from_db())
            .await?;
        EpochInfo::try_from(stored_epoch).map_err(Into::into)
    }

    async fn query_objects(
        &self,
        _query: SuiObjectResponseQuery,
        _cursor: Option<CheckpointedObjectID>,
        _limit: Option<usize>,
    ) -> RpcResult<QueryObjectsPage> {
        Err(jsonrpsee::types::error::CallError::Custom(
            jsonrpsee::types::error::ErrorCode::MethodNotFound.into(),
        )
        .into())
    }

    async fn get_network_metrics(&self) -> RpcResult<NetworkMetrics> {
        let network_metrics = self
            .inner
            .spawn_blocking(|this| this.get_latest_network_metrics())
            .await?;
        Ok(network_metrics)
    }

    async fn get_move_call_metrics(&self) -> RpcResult<MoveCallMetrics> {
        let move_call_metrics = self
            .inner
            .spawn_blocking(|this| this.get_latest_move_call_metrics())
            .await?;
        Ok(move_call_metrics)
    }

    async fn get_latest_address_metrics(&self) -> RpcResult<AddressMetrics> {
        let latest_address_metrics = self
            .inner
            .spawn_blocking(|this| this.get_latest_address_metrics())
            .await?;
        Ok(latest_address_metrics)
    }

    async fn get_checkpoint_address_metrics(&self, checkpoint: u64) -> RpcResult<AddressMetrics> {
        let checkpoint_address_metrics = self
            .inner
            .spawn_blocking(move |this| this.get_checkpoint_address_metrics(checkpoint))
            .await?;
        Ok(checkpoint_address_metrics)
    }

    async fn get_all_epoch_address_metrics(
        &self,
        descending_order: Option<bool>,
    ) -> RpcResult<Vec<AddressMetrics>> {
        let all_epoch_address_metrics = self
            .inner
            .spawn_blocking(move |this| this.get_all_epoch_address_metrics(descending_order))
            .await?;
        Ok(all_epoch_address_metrics)
    }

    async fn get_total_transactions(&self) -> RpcResult<BigInt<u64>> {
        let latest_checkpoint = self
            .inner
            .spawn_blocking(|this| this.get_latest_checkpoint())
            .await?;
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
