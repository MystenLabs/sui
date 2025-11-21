// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeSet, HashMap};

use anyhow::Context;
use async_graphql::dataloader::Loader;
use diesel::{ExpressionMethods, QueryDsl};
use prost_types::FieldMask;
use sui_indexer_alt_schema::{checkpoints::StoredCheckpoint, schema::kv_checkpoints};
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2 as proto;
use sui_types::{
    crypto::AuthorityQuorumSignInfo,
    messages_checkpoint::{CheckpointContents, CheckpointSummary},
};

use crate::{
    bigtable_reader::BigtableReader, error::Error, ledger_grpc_reader::LedgerGrpcReader,
    pg_reader::PgReader,
};

/// Key for fetching a checkpoint's content by its sequence number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CheckpointKey(pub u64);

#[async_trait::async_trait]
impl Loader<CheckpointKey> for PgReader {
    type Value = StoredCheckpoint;
    type Error = Error;

    async fn load(
        &self,
        keys: &[CheckpointKey],
    ) -> Result<HashMap<CheckpointKey, Self::Value>, Error> {
        use kv_checkpoints::dsl as c;

        if keys.is_empty() {
            return Ok(HashMap::new());
        }

        let mut conn = self.connect().await?;

        let seqs: BTreeSet<_> = keys.iter().map(|d| d.0 as i64).collect();
        let checkpoints: Vec<StoredCheckpoint> = conn
            .results(c::kv_checkpoints.filter(c::sequence_number.eq_any(seqs)))
            .await?;

        Ok(checkpoints
            .into_iter()
            .map(|c| (CheckpointKey(c.sequence_number as u64), c))
            .collect())
    }
}

#[async_trait::async_trait]
impl Loader<CheckpointKey> for BigtableReader {
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

        let checkpoint_keys: Vec<_> = keys.iter().map(|k| k.0).collect();

        Ok(self
            .checkpoints(&checkpoint_keys)
            .await?
            .into_iter()
            .map(|c| {
                (
                    CheckpointKey(c.summary.sequence_number),
                    (c.summary, c.contents, c.signatures),
                )
            })
            .collect())
    }
}

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

        let mut results = HashMap::new();
        for key in keys {
            let request = proto::GetCheckpointRequest::by_sequence_number(key.0).with_read_mask(
                FieldMask::from_paths(["summary.bcs", "signature", "contents.bcs"]),
            );

            match self.0.clone().get_checkpoint(request).await {
                Ok(response) => {
                    let checkpoint = response
                        .into_inner()
                        .checkpoint
                        .context("No checkpoint returned")?;

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

                    results.insert(*key, (summary, contents, signature));
                }
                Err(status) if status.code() == tonic::Code::NotFound => continue,
                Err(e) => return Err(e.into()),
            }
        }
        Ok(results)
    }
}
