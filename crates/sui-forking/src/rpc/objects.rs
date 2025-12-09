// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use sui_indexer_alt_jsonrpc::{api::rpc_module::RpcModule, error::invalid_params};
use sui_json_rpc_types::{
    Page, SuiGetPastObjectRequest, SuiObjectDataOptions, SuiObjectResponse, SuiPastObjectResponse,
};
use sui_open_rpc::Module;
use sui_open_rpc_macros::open_rpc;
use sui_types::{
    base_types::{ObjectID, SequenceNumber, SuiAddress},
    supported_protocol_versions::Chain,
};

use self::error::Error;
use simulacrum::Simulacrum;
use std::sync::{Arc, RwLock};

#[open_rpc(namespace = "sui", tag = "Objects API")]
#[rpc(server, namespace = "sui")]
trait ObjectsApi {
    /// Return the object information for the latest version of an object.
    #[method(name = "getObject")]
    async fn get_object(
        &self,
        /// The ID of the queried obect
        object_id: ObjectID,
        /// Options for specifying the content to be returned
        options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<SuiObjectResponse>;
}

pub(crate) struct Objects {
    pub simulacrum: Arc<RwLock<Simulacrum>>,
    pub protocol_version: u64,
    pub chain: Chain,
}

#[async_trait::async_trait]
impl ObjectsApiServer for Objects {
    async fn get_object(
        &self,
        object_id: ObjectID,
        options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<SuiObjectResponse> {
        // let Self(ctx) = self;
        let options = options.unwrap_or_default();

        let simulacrum = self.simulacrum.read().unwrap();
        let store: ForkingStore = simulacrum.store();

        let obj = store.get_object(&object_id).await;

        if let Ok(obj) = obj {
            return Ok(SuiObjectResponse::new_with_data(
                obj.into_sui_object_data(&options).await?,
            ));
        } else {
            return Ok(SuiObjectResponse::new_with_error(
                Error::NotExists { object_id }.into(),
            ));
        }
    }
}

impl RpcModule for Objects {
    fn schema(&self) -> Module {
        ObjectsApiOpenRpc::module_doc()
    }

    fn into_impl(self) -> jsonrpsee::RpcModule<Self> {
        self.into_rpc()
    }
}
