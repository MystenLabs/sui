// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::grpc::consistent_store::ForkingConsistentStore;
use crate::store::object_store::ObjectStoreApi;
use sui_types::base_types::SuiAddress;

struct DataLayer {
    /// Checkpoint store for accessing checkpoint data
    checkpoint_store: CheckpointStore,

    /// Object store that maintains object versions and data. For objects that are not found, it
    /// will fallback to the RPC data store to retrieve them, except for owned objects. Owned
    /// objects need to be seeded at start up time.
    object_store: ObjectStore,

    /// Transaction store for accessing and updating transaction data
    transaction_store: TransactionStore,

    /// Fallback RPC data store based on `sui-data-store`, which uses a cache layer and GraphQL
    /// RPC to fetch data from GraphQL.
    rpc_data_store:
        Arc<ReadThroughStore<LruMemoryStore, ReadThroughStore<FileSystemStore, DataStore>>>,

    /// Checkpoint at which the forking service was initialized
    forked_at_checkpoint: u64,
}

impl DataLayer {
    /// Queries the PG DB for the object at the specified version. If not found, falls back to the
    /// RPC data store, and inserts the object into the DB and Consistent Store if found.
    fn get_object_by_version(&self, object_id: ObjectID, version: SequenceNumber) {
        todo!()
    }

    fn get_object(&mut self, object_id: ObjectID) {
        // find latest version of the object in consistent store
        // if not found, fall back to rpc data store
        // insert into db and consistent store if found
        let obj = self.object_store.get_object(object_id);

        if let Some((object, version)) = obj {
            return object;
        } else {
            // fall back to rpc data store
            let object_key = ObjectKey {
                object_id,
                version_query: sui_data_store::VersionQuery::Latest,
            };
            let obj = self
                .rpc_data_store
                .get_objects(&[object_key])
                .unwrap()
                .into_iter()
                .next()
                .flatten();

            if let Some((object, _)) = obj {
                // insert into db and consistent store
                self.object_store
                    .insert_object(object.clone(), object.version());
                return object;
            } else {
                return None;
            }
        }
    }

    /// Queries the ConsistentStore for all objects owned by the specified address. If not found, falls back to the RPC data store,
    /// and inserts the objects into the DB and Consistent Store if found.
    fn get_owned_objects(&self, address: SuiAddress) {
        // these live objects should be seeded at start up time; anything that happens after that
        // should be handled by the consistent store itself through the ingesting pipelines
        todo!()
    }
}

/// Download package objects from the RPC data store given a set of package IDs
pub(crate) async fn download_packages(
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
