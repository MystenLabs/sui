// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use anyhow::Context;
use async_graphql::dataloader::Loader;
use futures::future::try_join_all;
use prost_types::FieldMask;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2 as proto;
use sui_types::crypto::AuthorityQuorumSignInfo;
use sui_types::messages_checkpoint::CheckpointContents;
use sui_types::messages_checkpoint::CheckpointSummary;

use crate::error::Error;
use crate::ledger_grpc_reader::LedgerGrpcReader;

/// Key for fetching a checkpoint's content by its sequence number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CheckpointKey(pub u64);

#[async_trait::async_trait]
impl Loader<CheckpointKey> for LedgerGrpcReader {
    type Value = (
        CheckpointSummary,
        CheckpointContents,
        AuthorityQuorumSignInfo<true>,
    );
    type Error = Error;

    async fn load(
        &self,
        keys: &[CheckpointKey],
    ) -> Result<HashMap<CheckpointKey, Self::Value>, Error> {
        if keys.is_empty() {
            return Ok(HashMap::new());
        }

        let futures = keys.iter().map(|key| async {
            let request = proto::GetCheckpointRequest::by_sequence_number(key.0).with_read_mask(
                FieldMask::from_paths(["summary.bcs", "signature", "contents.bcs"]),
            );

            match self.get_checkpoint(request).await {
                Ok(response) => {
                    let checkpoint = response.checkpoint.context("No checkpoint returned")?;

                    let summary: CheckpointSummary = checkpoint
                        .summary
                        .as_ref()
                        .and_then(|s| s.bcs.as_ref())
                        .context("Missing summary.bcs")?
                        .deserialize()
                        .context("Failed to deserialize checkpoint summary")?;

                    let contents: CheckpointContents = checkpoint
                        .contents
                        .as_ref()
                        .and_then(|c| c.bcs.as_ref())
                        .context("Missing contents.bcs")?
                        .deserialize()
                        .context("Failed to deserialize checkpoint contents")?;

                    let signature: AuthorityQuorumSignInfo<true> = {
                        let sdk_sig = sui_sdk_types::ValidatorAggregatedSignature::try_from(
                            checkpoint.signature.as_ref().context("Missing signature")?,
                        )
                        .context("Failed to parse signature")?;
                        AuthorityQuorumSignInfo::from(sdk_sig)
                    };

                    Ok(Some((*key, (summary, contents, signature))))
                }
                Err(status) if status.code() == tonic::Code::NotFound => Ok(None),
                Err(e) => Err(Error::from(e)),
            }
        });

        let results: Vec<_> = try_join_all(futures).await?;
        Ok(results.into_iter().flatten().collect())
    }
}
