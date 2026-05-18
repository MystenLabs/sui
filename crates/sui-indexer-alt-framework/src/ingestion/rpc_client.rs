// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use anyhow::Context as _;
use anyhow::anyhow;
use async_trait::async_trait;
use prost_types::FieldMask;
use sui_rpc::Client as RpcClient;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2::GetCheckpointRequest;
use sui_rpc::proto::sui::rpc::v2::GetServiceInfoRequest;
use sui_rpc::proto::sui::rpc::v2::GetServiceInfoResponse;
use sui_types::digests::ChainIdentifier;
use sui_types::digests::CheckpointDigest;
use sui_types::full_checkpoint_content::Checkpoint;
use tonic::Code;

use crate::ingestion::decode::Error::ProtoConversion;
use crate::ingestion::ingestion_client::CheckpointError;
use crate::ingestion::ingestion_client::CheckpointResult;
use crate::ingestion::ingestion_client::IngestionClientTrait;

#[async_trait]
impl IngestionClientTrait for RpcClient {
    async fn chain_id(&self) -> anyhow::Result<ChainIdentifier> {
        let response = get_service_info_request(self).await?;
        Ok(CheckpointDigest::from_str(response.chain_id())?.into())
    }

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
                _ => CheckpointError::Fetch(anyhow!(status)),
            })?;

        // `total_ingested_bytes` is incremented directly by the
        // `ByteCountMakeCallbackHandler` request layer attached in
        // `IngestionClient::with_grpc`, so it does not need to be tracked here.
        let response = response.into_inner();
        // Proto -> Checkpoint conversion is multi-ms of CPU work; offload to the
        // blocking pool so it doesn't stall the reactor.
        tokio::task::spawn_blocking(move || {
            Checkpoint::try_from(response.checkpoint())
                .map_err(|e| CheckpointError::Decode(ProtoConversion(e)))
        })
        .await
        .map_err(|e| CheckpointError::Fetch(anyhow!("decode task panicked: {e}")))?
    }

    async fn latest_checkpoint_number(&self) -> anyhow::Result<u64> {
        get_service_info_request(self)
            .await?
            .checkpoint_height
            .context("Checkpoint height not found")
    }
}

async fn get_service_info_request(
    rpc_client: &RpcClient,
) -> anyhow::Result<GetServiceInfoResponse> {
    let request = GetServiceInfoRequest::const_default();
    Ok(rpc_client
        .clone()
        .ledger_client()
        .get_service_info(request)
        .await?
        .into_inner())
}
