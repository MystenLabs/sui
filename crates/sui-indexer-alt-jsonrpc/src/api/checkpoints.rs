// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context as _;

use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use sui_indexer_alt_schema::checkpoints::StoredCheckpoint;
use sui_json_rpc_types::Checkpoint;
use sui_open_rpc::Module;
use sui_open_rpc_macros::open_rpc;
use sui_types::{
    crypto::AuthorityQuorumSignInfo,
    messages_checkpoint::{CheckpointContents, CheckpointSummary},
    sui_serde::BigInt,
};

use crate::{
    context::Context,
    data::checkpoints::CheckpointKey,
    error::{invalid_params, InternalContext, RpcError},
};

use super::rpc_module::RpcModule;

#[open_rpc(namespace = "sui", tag = "Checkpoints API")]
#[rpc(server, namespace = "sui")]
trait CheckpointsApi {
    /// Return a checkpoint by its sequence number
    #[method(name = "getCheckpoint")]
    async fn get_checkpoint(
        &self,
        /// Checkpoint sequence number.
        seq: BigInt<u64>,
    ) -> RpcResult<Checkpoint>;
}

pub(crate) struct Checkpoints(pub Context);

#[derive(thiserror::Error, Debug, Clone)]
enum Error {
    #[error("Checkpoint {0} not found")]
    NotFound(u64),
}

#[async_trait::async_trait]
impl CheckpointsApiServer for Checkpoints {
    async fn get_checkpoint(&self, seq: BigInt<u64>) -> RpcResult<Checkpoint> {
        let Self(ctx) = self;
        Ok(response(ctx, *seq).await.with_internal_context(|| {
            format!("Failed to fetch checkpoint at sequence number {seq:?}")
        })?)
    }
}

impl RpcModule for Checkpoints {
    fn schema(&self) -> Module {
        CheckpointsApiOpenRpc::module_doc()
    }

    fn into_impl(self) -> jsonrpsee::RpcModule<Self> {
        self.into_rpc()
    }
}

/// Load a checkpoint and prepare it for presentation as a JSON-RPC response.
async fn response(ctx: &Context, seq: u64) -> Result<Checkpoint, RpcError<Error>> {
    let stored: StoredCheckpoint = ctx
        .loader()
        .load_one(CheckpointKey(seq))
        .await
        .context("Failed to load checkpoint")?
        .ok_or_else(|| invalid_params(Error::NotFound(seq)))?;

    let summary: CheckpointSummary = bcs::from_bytes(&stored.checkpoint_summary)
        .context("Failed to deserialize checkpoint summary")?;

    let contents: CheckpointContents = bcs::from_bytes(&stored.checkpoint_contents)
        .context("Failed to deserialize checkpoint contents")?;

    let signature: AuthorityQuorumSignInfo<true> = bcs::from_bytes(&stored.validator_signatures)
        .context("Failed to deserialize validator signatures")?;

    Ok(Checkpoint::from((summary, contents, signature.signature)))
}
