// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// FIXME: Add tests.
// TODO: Migrate to use BigTable for KV storage.

use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use sui_indexer_alt_schema::objects::StoredObject;
use sui_json_rpc_types::{
    SuiGetPastObjectRequest, SuiObjectData, SuiObjectDataOptions, SuiObjectRef,
    SuiPastObjectResponse,
};
use sui_open_rpc::Module;
use sui_open_rpc_macros::open_rpc;
use sui_types::{
    base_types::{ObjectID, SequenceNumber},
    digests::ObjectDigest,
    object::Object,
};

use crate::{
    context::Context,
    error::{internal_error, invalid_params},
};

use super::rpc_module::RpcModule;

#[open_rpc(namespace = "sui", tag = "Objects API")]
#[rpc(server, namespace = "sui")]
trait ObjectsApi {
    /// Note there is no software-level guarantee/SLA that objects with past versions
    /// can be retrieved by this API, even if the object and version exists/existed.
    /// The result may vary across nodes depending on their pruning policies.
    /// Return the object information for a specified version
    #[method(name = "tryGetPastObject")]
    async fn try_get_past_object(
        &self,
        /// the ID of the queried object
        object_id: ObjectID,
        /// the version of the queried object. If None, default to the latest known version
        version: SequenceNumber,
        /// options for specifying the content to be returned
        options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<SuiPastObjectResponse>;

    /// Note there is no software-level guarantee/SLA that objects with past versions
    /// can be retrieved by this API, even if the object and version exists/existed.
    /// The result may vary across nodes depending on their pruning policies.
    /// Return the object information for a specified version
    #[method(name = "tryMultiGetPastObjects")]
    async fn try_multi_get_past_objects(
        &self,
        /// a vector of object and versions to be queried
        past_objects: Vec<SuiGetPastObjectRequest>,
        /// options for specifying the content to be returned
        options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<Vec<SuiPastObjectResponse>>;
}

pub(crate) struct Objects(pub Context);

#[derive(thiserror::Error, Debug)]
pub(crate) enum Error {
    #[error("Object not found: {0} with version {1}")]
    NotFound(ObjectID, SequenceNumber),

    #[error("Error converting to response: {0}")]
    Conversion(anyhow::Error),

    #[error("Deserialization error: {0}")]
    Deserialization(#[from] bcs::Error),
}

#[async_trait::async_trait]
impl ObjectsApiServer for Objects {
    async fn try_get_past_object(
        &self,
        object_id: ObjectID,
        version: SequenceNumber,
        options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<SuiPastObjectResponse> {
        let Self(ctx) = self;
        let Some(stored) = ctx
            .loader()
            .load_one((object_id, version))
            .await
            .map_err(internal_error)?
        else {
            return Err(invalid_params(Error::NotFound(object_id, version)));
        };

        let options = options.unwrap_or_default();
        response(ctx, &stored, &options)
            .await
            .map_err(internal_error)
    }

    async fn try_multi_get_past_objects(
        &self,
        past_objects: Vec<SuiGetPastObjectRequest>,
        options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<Vec<SuiPastObjectResponse>> {
        let Self(ctx) = self;
        let stored_objects = ctx
            .loader()
            .load_many(past_objects.iter().map(|p| (p.object_id, p.version)))
            .await
            .map_err(internal_error)?;

        let mut responses = Vec::with_capacity(past_objects.len());
        let options = options.unwrap_or_default();
        for request in past_objects {
            if let Some(stored) = stored_objects.get(&(request.object_id, request.version)) {
                responses.push(
                    response(ctx, stored, &options)
                        .await
                        .map_err(internal_error)?,
                );
            } else {
                responses.push(SuiPastObjectResponse::VersionNotFound(
                    request.object_id,
                    request.version,
                ));
            }
        }

        Ok(responses)
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

/// Convert the representation of an object from the database into the response format,
/// including the fields requested in the `options`.
/// FIXME: Actually use the options.
pub(crate) async fn response(
    _ctx: &Context,
    stored: &StoredObject,
    _options: &SuiObjectDataOptions,
) -> Result<SuiPastObjectResponse, Error> {
    let object_id =
        ObjectID::from_bytes(&stored.object_id).map_err(|e| Error::Conversion(e.into()))?;
    let version = SequenceNumber::from_u64(stored.object_version as u64);

    let Some(serialized_object) = &stored.serialized_object else {
        return Ok(SuiPastObjectResponse::ObjectDeleted(SuiObjectRef {
            object_id,
            version,
            digest: ObjectDigest::OBJECT_DIGEST_DELETED,
        }));
    };
    let object: Object = bcs::from_bytes(serialized_object).map_err(Error::Deserialization)?;
    let object_data = SuiObjectData {
        object_id,
        version,
        digest: object.digest(),
        type_: None,
        owner: None,
        previous_transaction: None,
        storage_rebate: None,
        display: None,
        content: None,
        bcs: None,
    };

    Ok(SuiPastObjectResponse::VersionFound(object_data))
}
