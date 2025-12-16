// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use filter::SuiObjectResponseQuery;
use futures::future;
use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use sui_json_rpc_types::{
    Page, SuiData, SuiGetPastObjectRequest, SuiObjectData, SuiObjectDataOptions, SuiObjectResponse,
    SuiPastObjectResponse,
};
use sui_open_rpc::Module;
use sui_open_rpc_macros::open_rpc;
use sui_types::{
    base_types::{ObjectID, SequenceNumber, SuiAddress},
    error::SuiObjectResponseError,
    object::Object,
};

use sui_indexer_alt_jsonrpc::{
    api::rpc_module::RpcModule,
    error::{InternalContext, invalid_params},
};

use crate::{context::Context, store::ForkingStore};

use self::error::Error;
use std::collections::BTreeMap;
use sui_data_store::{ObjectKey, ObjectStore};

mod data;
mod error;
pub(crate) mod filter;
pub(crate) mod response;

#[open_rpc(namespace = "sui", tag = "Objects API")]
#[rpc(server, namespace = "sui")]
trait ObjectsApi {
    /// Return the object information for the latest version of an object.
    #[method(name = "getObject")]
    async fn get_object(
        &self,
        /// The ID of the queried object
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

pub(crate) struct Objects(pub Context);

pub(crate) struct QueryObjects(pub Context);

#[async_trait::async_trait]
impl ObjectsApiServer for Objects {
    async fn get_object(
        &self,
        object_id: ObjectID,
        options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<SuiObjectResponse> {
        let Self(Context {
            pg_context: ctx,
            simulacrum,
            protocol_version,
            chain,
            at_checkpoint,
        }) = self;

        let options = options.unwrap_or_default();
        let mut simulacrum = simulacrum.write().await;
        let mut data_store: &mut ForkingStore = simulacrum.store_1_mut();
        let obj = data_store.get_object(&object_id);
        if obj.is_none() {
            println!("Object not found locally: {:?}", object_id);

            let object = response::live_object(ctx, object_id, &options).await?;

            // If the object does not exist locally, try to fetch it from the RPC data store
            if let Some(SuiObjectResponseError::NotExists { object_id }) = &object.error {
                println!("Need to fetch object from rpc ");
                {
                    let obj = data_store
                        .get_rpc_data_store()
                        .get_objects(&[ObjectKey {
                            object_id: *object_id,
                            version_query: sui_data_store::VersionQuery::AtCheckpoint(
                                at_checkpoint.clone(),
                            ),
                        }])
                        .unwrap();
                    let obj = obj.into_iter().next().unwrap();

                    if let Some((ref object, _version)) = obj {
                        println!("Fetched object from rpc: {:?}", object.id());
                        let obj = SuiObjectResponse::new_with_data(
                            response::object_data_with_options(
                                ctx,
                                object.clone(),
                                &SuiObjectDataOptions {
                                    show_type: true,
                                    show_bcs: true,
                                    show_storage_rebate: true,
                                    show_content: true,
                                    show_owner: true,
                                    show_previous_transaction: true,
                                    ..Default::default()
                                },
                            )
                            .await?,
                        );
                        let written_objects = BTreeMap::from([(object_id.clone(), object.clone())]);
                        data_store.update_objects(written_objects, vec![]);
                        Ok(obj)
                    } else {
                        Ok(object)
                    }
                }
            } else {
                Ok(object)
            }
        } else {
            let obj = SuiObjectResponse::new_with_data(
                response::object_data_with_options(ctx, obj.unwrap().clone(), &options).await?,
            );
            Ok(obj)
        }
        // Ok(response::live_object(ctx, object_id, &options)
        //     .await
        //     .with_internal_context(|| {
        //         format!("Failed to get object {object_id} at latest version")
        //     })?)
    }

    async fn multi_get_objects(
        &self,
        object_ids: Vec<ObjectID>,
        options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<Vec<SuiObjectResponse>> {
        let Self(Context {
            pg_context: ctx,
            simulacrum,
            protocol_version,
            chain,
            at_checkpoint,
        }) = self;
        let config = &ctx.config().objects;
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
        let Self(Context {
            pg_context: ctx,
            simulacrum,
            protocol_version,
            chain,
            at_checkpoint,
        }) = self;

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
        let Self(Context {
            pg_context: ctx,
            simulacrum,
            protocol_version,
            chain,
            at_checkpoint,
        }) = self;

        let config = &ctx.config().objects;
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
        let Self(Context {
            pg_context: ctx,
            simulacrum,
            protocol_version,
            chain,
            at_checkpoint,
        }) = self;

        let simulacrum = simulacrum.read().await;
        let owned_objs = simulacrum.store_1().owned_objects(address);

        let mut data = vec![];
        for object in owned_objs {
            let object_id = object.id();
            let version = object.version();
            let digest = object.digest();
            let owner = object.owner().clone();
            let type_ = sui_types::base_types::ObjectType::from(object);

            let (bcs, content) = match &object.data {
                sui_types::object::Data::Move(move_obj) => {
                    let bcs = Some(sui_json_rpc_types::SuiRawData::MoveObject(
                        move_obj.clone().into(),
                    ));
                    let type_tag: sui_types::TypeTag = move_obj.type_().clone().into();
                    println!(
                        "Trying to fetch package {:?} from package resolver",
                        object_id
                    );
                    let layout_result = ctx.package_resolver().type_layout(type_tag.clone()).await;

                    let content = match layout_result {
                        Ok(move_core_types::annotated_value::MoveTypeLayout::Struct(layout)) => {
                            sui_json_rpc_types::SuiParsedData::try_from_object(
                                move_obj.clone(),
                                *layout,
                            )
                            .ok()
                        }
                        _ => None,
                    };

                    (bcs, content)
                }
                sui_types::object::Data::Package(pkg) => {
                    let bcs = Some(sui_json_rpc_types::SuiRawData::Package(pkg.clone().into()));
                    let content =
                        sui_json_rpc_types::SuiParsedData::try_from_package(pkg.clone()).ok();
                    (bcs, content)
                }
            };

            let obj_data = SuiObjectData {
                object_id,
                version,
                digest,
                type_: Some(type_),
                owner: Some(owner),
                previous_transaction: Some(object.previous_transaction),
                storage_rebate: Some(object.storage_rebate),
                display: None,
                content,
                bcs,
            };

            data.push(SuiObjectResponse::new_with_data(obj_data));
        }

        Ok(Page {
            data,
            next_cursor: None,
            has_next_page: false,
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
