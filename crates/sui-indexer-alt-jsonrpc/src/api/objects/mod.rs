// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use filter::SuiObjectResponseQuery;
use futures::future;
use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use serde::{Deserialize, Serialize};
use sui_json_rpc_types::{
    Page, SuiGetPastObjectRequest, SuiObjectDataOptions, SuiObjectResponse, SuiPastObjectResponse,
};
use sui_open_rpc::Module;
use sui_open_rpc_macros::open_rpc;
use sui_types::base_types::{ObjectID, SequenceNumber, SuiAddress};

use crate::{
    context::Context,
    error::{invalid_params, InternalContext},
};

use super::rpc_module::RpcModule;

use self::error::Error;

mod error;
mod filter;
pub(crate) mod response;

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

    /// Return the object information for the latest versions of multiple objects.
    #[method(name = "multiGetObjects")]
    async fn multi_get_objects(
        &self,
        /// the IDs of the queried objects
        object_ids: Vec<ObjectID>,
        /// Options for specifying the content to be returned
        options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<Vec<SuiObjectResponse>>;

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

#[open_rpc(namespace = "suix", tag = "Query Objects API")]
#[rpc(server, namespace = "suix")]
trait QueryObjectsApi {
    /// Query objects by their owner's address. Returns a paginated list of objects.
    ///
    /// If a cursor is provided, the query will start from the object after the one pointed to by
    /// this cursor, otherwise pagination starts from the first page of objects owned by the
    /// address.
    ///
    /// The definition of "first" page is somewhat arbitrary. It is a page such that continuing to
    /// paginate an address's objects from this page will eventually reach all objects owned by
    /// that address assuming that the owned object set does not change. If the owned object set
    /// does change, pagination may not be consistent (may not reflect a set of objects that the
    /// address owned at a single point in time).
    ///
    /// The size of each page is controlled by the `limit` parameter.
    #[method(name = "getOwnedObjects")]
    async fn get_owned_objects(
        &self,
        /// The owner's address.
        address: SuiAddress,
        /// Additional querying criteria for the object.
        query: Option<SuiObjectResponseQuery>,
        /// Cursor to start paginating from.
        cursor: Option<String>,
        /// Maximum number of objects to return per page.
        limit: Option<usize>,
    ) -> RpcResult<Page<SuiObjectResponse, String>>;
}

pub(crate) struct Objects(pub Context, pub ObjectsConfig);

pub(crate) struct QueryObjects(pub Context, pub ObjectsConfig);

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ObjectsConfig {
    /// The maximum number of keys that can be queried in a single multi-get request.
    pub max_multi_get_objects: usize,

    /// The default page size limit when querying objects, if none is provided.
    pub default_page_size: usize,

    /// The largest acceptable page size when querying transactions. Requesting a page larger than
    /// this is a user error.
    pub max_page_size: usize,
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

    async fn multi_get_objects(
        &self,
        object_ids: Vec<ObjectID>,
        options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<Vec<SuiObjectResponse>> {
        let Self(ctx, config) = self;
        if object_ids.len() > config.max_multi_get_objects {
            return Err(invalid_params(Error::TooManyKeys {
                requested: object_ids.len(),
                max: config.max_multi_get_objects,
            })
            .into());
        }

        let options = options.unwrap_or_default();

        let obj_futures = object_ids
            .iter()
            .map(|id| response::live_object(ctx, *id, &options));

        Ok(future::join_all(obj_futures)
            .await
            .into_iter()
            .zip(object_ids)
            .map(|(r, o)| {
                r.with_internal_context(|| format!("Failed to get object {o} at latest version"))
            })
            .collect::<Result<Vec<_>, _>>()?)
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
                let id = o.object_id;
                let v = o.version;
                r.with_internal_context(|| format!("Failed to get object {id} at version {v}"))
            })
            .collect::<Result<Vec<_>, _>>()?)
    }
}

#[async_trait::async_trait]
impl QueryObjectsApiServer for QueryObjects {
    async fn get_owned_objects(
        &self,
        address: SuiAddress,
        query: Option<SuiObjectResponseQuery>,
        cursor: Option<String>,
        limit: Option<usize>,
    ) -> RpcResult<Page<SuiObjectResponse, String>> {
        let Self(ctx, confige) = self;

        let query = query.unwrap_or_default();

        let Page {
            data: object_ids,
            next_cursor,
            has_next_page,
        } = filter::owned_objects(ctx, confige, address, &query.filter, cursor, limit).await?;

        let options = query.options.unwrap_or_default();

        let obj_futures = object_ids
            .iter()
            .map(|id| response::latest_object(ctx, *id, &options));

        let data = future::join_all(obj_futures)
            .await
            .into_iter()
            .zip(object_ids)
            .map(|(r, id)| {
                r.with_internal_context(|| format!("Failed to get object {id} at latest version"))
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Page {
            data,
            next_cursor,
            has_next_page,
        })
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

impl RpcModule for QueryObjects {
    fn schema(&self) -> Module {
        QueryObjectsApiOpenRpc::module_doc()
    }

    fn into_impl(self) -> jsonrpsee::RpcModule<Self> {
        self.into_rpc()
    }
}

impl Default for ObjectsConfig {
    fn default() -> Self {
        Self {
            max_multi_get_objects: 50,
            default_page_size: 50,
            max_page_size: 100,
        }
    }
}
