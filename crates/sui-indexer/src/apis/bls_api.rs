// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use jsonrpsee::core::RpcResult;
use jsonrpsee::http_client::HttpClient;
use jsonrpsee::RpcModule;
use sui_json_rpc::api::{ThresholdBlsApiClient, ThresholdBlsApiServer};
use sui_json_rpc::SuiRpcModule;
use sui_json_rpc_types::{SuiTBlsSignObjectCommitmentType, SuiTBlsSignRandomnessObjectResponse};
use sui_open_rpc::Module;
use sui_types::base_types::ObjectID;

pub(crate) struct ThresholdBlsApi {
    fullnode: HttpClient,
}

impl ThresholdBlsApi {
    pub fn new(fullnode_client: HttpClient) -> Self {
        Self {
            fullnode: fullnode_client,
        }
    }
}

#[async_trait]
impl ThresholdBlsApiServer for ThresholdBlsApi {
    async fn tbls_sign_randomness_object(
        &self,
        object_id: ObjectID,
        commitment_type: SuiTBlsSignObjectCommitmentType,
    ) -> RpcResult<SuiTBlsSignRandomnessObjectResponse> {
        self.fullnode
            .tbls_sign_randomness_object(object_id, commitment_type)
            .await
    }
}

impl SuiRpcModule for ThresholdBlsApi {
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        sui_json_rpc::api::ThresholdBlsApiOpenRpc::module_doc()
    }
}
