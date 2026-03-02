// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use async_trait::async_trait;
use prost_types::FieldMask;
use sui_rpc::Client as RpcClient;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::GetCheckpointRequest;
use sui_rpc::proto::sui::rpc::v2::GetServiceInfoRequest;
use sui_types::full_checkpoint_content::Checkpoint;
use tonic::Code;

use crate::ingestion::ingestion_client::FetchCheckpointData;
use crate::ingestion::ingestion_client::FetchCheckpointResult;
use crate::ingestion::ingestion_client::FetchError;
use crate::ingestion::ingestion_client::FetchLatestCheckpointNumberResult;
use crate::ingestion::ingestion_client::IngestionClientTrait;

#[async_trait]
impl IngestionClientTrait for RpcClient {
    async fn fetch_latest_checkpoint_number(&self) -> FetchLatestCheckpointNumberResult {
        let request = GetServiceInfoRequest::default();

        let response = self
            .clone()
            .ledger_client()
            .get_service_info(request)
            .await
            .map_err(|status| match status.code() {
                Code::NotFound => FetchError::NotFound,
                _ => FetchError::Transient {
                    reason: "get_service_info",
                    error: anyhow!(status),
                },
            })?
            .into_inner();

        Ok(response.checkpoint_height)
    }

    async fn fetch_checkpoint(&self, checkpoint: u64) -> FetchCheckpointResult {
        let request: GetCheckpointRequest = GetCheckpointRequest::by_sequence_number(checkpoint)
            .with_read_mask(FieldMask::from_paths([
                "summary.bcs",
                "signature",
                "contents.bcs",
                "transactions.transaction.bcs",
                "transactions.effects.bcs",
                "transactions.effects.unchanged_loaded_runtime_objects",
                "transactions.events.bcs",
                "objects.objects.bcs",
            ]));

        let response = self
            .clone()
            .ledger_client()
            .get_checkpoint(request)
            .await
            .map_err(|status| match status.code() {
                Code::NotFound => FetchError::NotFound,
                _ => FetchError::Transient {
                    reason: "get_checkpoint",
                    error: anyhow!(status),
                },
            })?
            .into_inner();

        let checkpoint =
            Checkpoint::try_from(response.checkpoint()).map_err(|e| FetchError::Permanent {
                reason: "proto_conversion",
                error: e.into(),
            })?;

        Ok(FetchCheckpointData::Checkpoint(checkpoint))
    }
}
