// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context as _;

use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use sui_json_rpc_types::Checkpoint;
use sui_open_rpc::Module;
use sui_open_rpc_macros::open_rpc;
use sui_types::sui_serde::BigInt;

use crate::{
    context::Context,
    error::{InternalContext, RpcError, invalid_params},
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
    let (summary, contents, signature) = ctx
        .kv_loader()
        .load_one_checkpoint(seq)
        .await
        .context("Failed to load checkpoint")?
        .ok_or_else(|| invalid_params(Error::NotFound(seq)))?;

    Ok(Checkpoint::from((summary, contents, signature.signature)))
}
