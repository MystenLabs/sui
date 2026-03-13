// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use async_trait::async_trait;
use prost_types::FieldMask;
use sui_rpc::Client as RpcClient;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::GetCheckpointRequest;
use sui_types::full_checkpoint_content::Checkpoint;
use tonic::Code;

use crate::ingestion::ingestion_client::CheckpointData;
use crate::ingestion::ingestion_client::CheckpointError;
use crate::ingestion::ingestion_client::CheckpointResult;
use crate::ingestion::ingestion_client::IngestionClientTrait;

#[async_trait]
impl IngestionClientTrait for RpcClient {
    async fn checkpoint(&self, checkpoint: u64) -> CheckpointResult {
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
                Code::NotFound => CheckpointError::NotFound,
                _ => CheckpointError::Transient {
                    reason: "get_checkpoint",
                    error: anyhow!(status),
                },
            })?
            .into_inner();

        let checkpoint = Checkpoint::try_from(response.checkpoint()).map_err(|e| {
            CheckpointError::Permanent {
                reason: "proto_conversion",
                error: e.into(),
            }
        })?;

        Ok(CheckpointData::Checkpoint(checkpoint))
    }
}
