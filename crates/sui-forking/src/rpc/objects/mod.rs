// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use filter::SuiObjectResponseQuery;
use futures::future;
use jsonrpsee::{core::RpcResult, proc_macros::rpc};
use sui_json_rpc_types::{
    Page, SuiGetPastObjectRequest, SuiObjectDataOptions, SuiObjectResponse, SuiPastObjectResponse,
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
use std::collections::{BTreeMap, BTreeSet};
use sui_data_store::{ObjectKey, ObjectStore};
use tracing::{error, info};

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
            at_checkpoint,
            ..
        }) = self;

        let options = options.unwrap_or_default();
        let mut simulacrum = simulacrum.write().await;
        let data_store: &mut ForkingStore = simulacrum.store_1_mut();
        let obj = data_store.get_object(&object_id);
        if obj.is_none() {
            info!("Object not found locally: {:?}", object_id);

            // try fetching from indexer first
            let object = response::live_object(ctx, object_id, &options).await?;

            // If the object does not exist locally, try to fetch it from the RPC data store
            if let Some(SuiObjectResponseError::NotExists { object_id }) = &object.error {
                info!("Need to fetch object `{object_id}` from rpc ");
                {
                    let obj = data_store
                        .get_rpc_data_store()
                        .get_objects(&[ObjectKey {
                            object_id: *object_id,
                            version_query: sui_data_store::VersionQuery::AtCheckpoint(
                                *at_checkpoint,
                            ),
                        }])
                        .unwrap();
                    let obj = obj.into_iter().next().unwrap();

                    if let Some((ref object, _version)) = obj {
                        info!("Fetched object from rpc: {:?}", object.id());
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
                        // objects need to be available in the network for execution
                        let written_objects = BTreeMap::from([(*object_id, object.clone())]);
                        data_store.update_objects(written_objects, vec![]);

                        // If this is a package, insert it into kv_packages table
                        // TODO maybe have a better way to do this
                        if object.is_package() {
                            let Some(package) = object.data.try_as_package() else {
                                panic!("Object {} is not a package", object.id());
                            };

                            // when we find a package, we need to download all related packages
                            // that define types used by this package and also add them to the
                            // forked network store
                            let packages_to_add = package
                                .type_origin_map()
                                .values()
                                .cloned()
                                .collect::<BTreeSet<_>>();

                            let mut downloaded_packages =
                                download_packages(packages_to_add, data_store, at_checkpoint)
                                    .await
                                    .expect("Failed to download packages");

                            let written_objects = downloaded_packages
                                .clone()
                                .into_iter()
                                .map(|o| (o.id(), o.clone()))
                                .collect();
                            data_store.update_objects(written_objects, vec![]);

                            downloaded_packages.push(object.clone());

                            let Self(Context { db_writer, .. }) = self;

                            if let Err(e) = insert_package_into_db(
                                db_writer,
                                &downloaded_packages,
                                *at_checkpoint,
                            )
                            .await
                            {
                                eprintln!("Failed to insert package into DB: {:?}", e);
                            }
                        }
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
    }

    async fn multi_get_objects(
        &self,
        object_ids: Vec<ObjectID>,
        options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<Vec<SuiObjectResponse>> {
        let Self(Context {
            pg_context: ctx, ..
        }) = self;
        let config = &ctx.config().objects;
        if object_ids.len() > config.max_multi_get_objects {
            return Err(invalid_params(Error::TooManyKeys {
                requested: object_ids.len(),
                max: config.max_multi_get_objects,
            })
            .into());
        }

        println!("multi_get_objects {:?}", object_ids);

        let obj_futures = object_ids
            .iter()
            .map(|id| self.get_object(*id, options.clone()));

        Ok(future::join_all(obj_futures)
            .await
            .into_iter()
            .zip(object_ids)
            .map(|(r, _)| r)
            .collect::<Result<Vec<_>, _>>()?)
    }

    async fn try_get_past_object(
        &self,
        object_id: ObjectID,
        version: SequenceNumber,
        options: Option<SuiObjectDataOptions>,
    ) -> RpcResult<SuiPastObjectResponse> {
        let Self(Context {
            pg_context: ctx, ..
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
            pg_context: ctx, ..
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
        _cursor: Option<String>,
        _limit: Option<usize>,
    ) -> RpcResult<Page<SuiObjectResponse, String>> {
        let Self(Context {
            pg_context: ctx,
            simulacrum,
            ..
        }) = self;

        let simulacrum = simulacrum.read().await;

        let query = query.unwrap_or_default();
        let options = query.options.unwrap_or_default();

        // TODO: this only works if all owned objects are stored locally in the simulacrum
        // we probably need to fetch from the RPC data store if not found locally, but it's tricky
        // if we're trying to get owned objects at a past checkpoint (older than 1h)
        let owned_objs = simulacrum.store_1().owned_objects(address);

        let mut data = vec![];
        for object in owned_objs {
            // Apply filter if provided
            if let Some(ref filter) = query.filter
                && !filter.matches(object)
            {
                continue;
            }

            let obj_data =
                response::object_data_with_options(ctx, object.clone(), &options).await?;
            data.push(SuiObjectResponse::new_with_data(obj_data));
        }

        Ok(Page {
            data,
            next_cursor: None,
            has_next_page: false,
        })
    }
}

/// Download package objects from the RPC data store given a set of package IDs
async fn download_packages(
    package_ids: BTreeSet<ObjectID>,
    data_store: &mut ForkingStore,
    at_checkpoint: &u64,
) -> anyhow::Result<Vec<Object>> {
    let mut output = Vec::with_capacity(package_ids.len());
    let objects_to_retrieve = package_ids
        .into_iter()
        .map(|id| ObjectKey {
            object_id: id,
            version_query: sui_data_store::VersionQuery::AtCheckpoint(*at_checkpoint),
        })
        .collect::<Vec<_>>();
    let obj = data_store
        .get_rpc_data_store()
        .get_objects(&objects_to_retrieve)
        .unwrap();

    for o in obj.into_iter().by_ref().flatten() {
        output.push(o.0);
    }

    Ok(output)
}

/// Insert a package object into the kv_packages table
pub(crate) async fn insert_package_into_db(
    db_writer: &sui_pg_db::Db,
    object: &[Object],
    checkpoint: u64,
) -> anyhow::Result<()> {
    use diesel::prelude::*;
    use diesel_async::RunQueryDsl;
    use sui_indexer_alt_schema::schema::kv_packages;

    for object in object.iter() {
        // Ensure the object is a package
        let Some(package) = object.data.try_as_package() else {
            error!("Object {} is not a package", object.id());
            anyhow::bail!("Object is not a package");
        };

        let package_id = package.id().to_vec();
        let package_version = object.version().value() as i64;
        let original_id = package.original_package_id().to_vec();
        let is_system_package = sui_types::is_system_package(package.id());
        let serialized_object = bcs::to_bytes(object)?;
        let cp_sequence_number = checkpoint as i64;

        let mut conn = db_writer.connect().await?;

        diesel::insert_into(kv_packages::table)
            .values((
                kv_packages::package_id.eq(package_id),
                kv_packages::package_version.eq(package_version),
                kv_packages::original_id.eq(original_id),
                kv_packages::is_system_package.eq(is_system_package),
                kv_packages::serialized_object.eq(serialized_object),
                kv_packages::cp_sequence_number.eq(cp_sequence_number),
            ))
            .on_conflict((kv_packages::package_id, kv_packages::package_version))
            .do_nothing()
            .execute(&mut conn)
            .await?;

        info!(
            "Inserted package {} version {} into kv_packages table",
            package.id(),
            package_version
        );
    }

    Ok(())
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
