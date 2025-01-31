// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::future;
use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use serde::{Deserialize, Serialize};
use sui_json_rpc_types::{
    SuiGetPastObjectRequest, SuiObjectDataOptions, SuiObjectResponse, SuiPastObjectResponse,
};
use sui_open_rpc::Module;
use sui_open_rpc_macros::open_rpc;
use sui_types::base_types::{ObjectID, SequenceNumber};

use crate::{
    context::Context,
    error::{invalid_params, InternalContext},
};

use super::rpc_module::RpcModule;

use self::error::Error;

mod error;
mod response;

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

    /// Return the object information for multiple specified objects and versions.
    ///
    /// Note that past versions of an object may be pruned from the system, even if they once
    /// existed. Different RPC services may return different responses for the same request as a
    /// result, based on their pruning policies.
    #[method(name = "tryMultiGetPastObjects")]
    async fn try_multi_get_past_objects(
        &self,
        /// A vector of object and versions to be queried
        past_objects: Vec<SuiGetPastObjectRequest>,
        /// Options for specifying the content to be returned
        options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<Vec<SuiPastObjectResponse>>;
}

pub(crate) struct Objects(pub Context, pub ObjectsConfig);

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ObjectsConfig {
    /// The maximum number of keys that can be queried in a single multi-get request.
    pub max_multi_get_objects: usize,
}

#[async_trait::async_trait]
impl ObjectsApiServer for Objects {
    async fn get_object(
        &self,
        object_id: ObjectID,
        options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<SuiObjectResponse> {
        let Self(ctx, _) = self;
        let options = options.unwrap_or_default();
        Ok(response::live_object(ctx, object_id, &options)
            .await
            .with_internal_context(|| {
                format!("Failed to get object {object_id} at latest version")
            })?)
    }

    async fn try_get_past_object(
        &self,
        object_id: ObjectID,
        version: SequenceNumber,
        options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<SuiPastObjectResponse> {
        let Self(ctx, _) = self;
        let options = options.unwrap_or_default();
        Ok(response::past_object(ctx, object_id, version, &options)
            .await
            .with_internal_context(|| {
                format!(
                    "Failed to get object {object_id} at version {}",
                    version.value()
                )
            })?)
    }

    async fn try_multi_get_past_objects(
        &self,
        past_objects: Vec<SuiGetPastObjectRequest>,
        options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<Vec<SuiPastObjectResponse>> {
        let Self(ctx, config) = self;
        if past_objects.len() > config.max_multi_get_objects {
            return Err(invalid_params(Error::TooManyKeys {
                requested: past_objects.len(),
                max: config.max_multi_get_objects,
            })
            .into());
        }

        let options = options.unwrap_or_default();

        let obj_futures = past_objects
            .iter()
            .map(|obj| response::past_object(ctx, obj.object_id, obj.version, &options));

        Ok(future::join_all(obj_futures)
            .await
            .into_iter()
            .zip(past_objects)
            .map(|(r, o)| {
                let id = o.object_id.to_canonical_display(/* with_prefix */ true);
                let v = o.version;
                r.with_internal_context(|| format!("Failed to get object {id} at version {v}"))
            })
            .collect::<Result<Vec<_>, _>>()?)
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

impl Default for ObjectsConfig {
    fn default() -> Self {
        Self {
            max_multi_get_objects: 50,
        }
    }
}
