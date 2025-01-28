// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use sui_json_rpc_types::{SuiObjectDataOptions, SuiPastObjectResponse};
use sui_open_rpc::Module;
use sui_open_rpc_macros::open_rpc;
use sui_types::base_types::{ObjectID, SequenceNumber};

use crate::context::Context;

use super::rpc_module::RpcModule;

mod response;

#[open_rpc(namespace = "sui", tag = "Objects API")]
#[rpc(server, namespace = "sui")]
trait ObjectsApi {
    /// Return the object information for a specified version.
    ///
    /// Note that past versions of an object may be pruned from the system, even if they once
    /// existed. Different RPC services may return different responses for the same request as a
    /// result, based on their pruning policies.
    #[method(name = "tryGetPastObject")]
    async fn try_get_past_object(
        &self,
        /// The ID of the queried object
        object_id: ObjectID,
        /// The version of the queried object.
        version: SequenceNumber,
        /// Options for specifying the content to be returned
        options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<SuiPastObjectResponse>;
}

pub(crate) struct Objects(pub Context);

#[async_trait::async_trait]
impl ObjectsApiServer for Objects {
    async fn try_get_past_object(
        &self,
        object_id: ObjectID,
        version: SequenceNumber,
        options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<SuiPastObjectResponse> {
        let Self(ctx) = self;
        let options = options.unwrap_or_default();
        Ok(response::past_object(ctx, object_id, version, &options).await?)
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
