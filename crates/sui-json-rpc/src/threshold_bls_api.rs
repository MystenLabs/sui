// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use crate::api::RpcBcsApiServer;
use crate::SuiRpcModule;
use anyhow::anyhow;
use async_trait::async_trait;
use jsonrpsee::core::RpcResult;
use jsonrpsee::RpcModule;
use sui_core::authority::AuthorityState;
use sui_json_rpc_types::GetRawObjectDataResponse;
use sui_open_rpc::Module;
use sui_types::base_types::ObjectID;

pub struct ThresholdBlsApiImpl {
    state: Arc<AuthorityState>,
}

impl ThresholdBlsApiImpl {
    pub fn new(client: Arc<AuthorityState>) -> Self {
        Self { state }
    }
}

#[async_trait]
impl ThresholdBlsApi for ThresholdBlsApiImpl {
    async fn tbls_sign_randomness_object(
        &self,
        object_id: ObjectID,
        epoch: EpochId,
        effects: Option<SuiCertifiedTransactionEffects>,
    )-> RpcResult<SuiTBlsSignRandomnessObjectResponse> {
        // get current epoch
        // if old -> compute sig and return
        // check signature on effects
        // check that obj id is there (created or modified)
        // compute the sig and return
        Err(Error::UnexpectedError)
    }
}

impl SuiRpcModule for ThresholdBlsApiImpl {
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        crate::api::RpcBcsApiOpenRpc::module_doc()
    }
}
