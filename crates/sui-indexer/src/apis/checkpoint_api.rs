// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use jsonrpsee::core::RpcResult;
use jsonrpsee::RpcModule;
use std::sync::Arc;
use sui_json_rpc::SuiRpcModule;
use sui_open_rpc::Module;

use crate::errors::IndexerError;
use crate::models::checkpoints::get_latest_checkpoint_sequence_number;
use crate::{get_pg_pool_connection, PgConnectionPool};

use super::api::CheckpointApiServer;

pub struct CheckpointApiImpl {
    pg_connection_pool: Arc<PgConnectionPool>,
}

impl CheckpointApiImpl {
    pub fn new(pg_connection_pool: Arc<PgConnectionPool>) -> Self {
        Self { pg_connection_pool }
    }

    async fn get_latest_checkpoint_sequence_number(&self) -> Result<i64, IndexerError> {
        let mut pg_pool_conn = get_pg_pool_connection(self.pg_connection_pool.clone())?;
        get_latest_checkpoint_sequence_number(&mut pg_pool_conn)
    }
}

#[async_trait]
impl CheckpointApiServer for CheckpointApiImpl {
    async fn get_latest_checkpoint_sequence_number(&self) -> RpcResult<i64> {
        let latest_checkpoint_sequence_number =
            self.get_latest_checkpoint_sequence_number().await?;
        Ok(latest_checkpoint_sequence_number)
    }
}

impl SuiRpcModule for CheckpointApiImpl {
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        crate::apis::api::CheckpointApiOpenRpc::module_doc()
    }
}
