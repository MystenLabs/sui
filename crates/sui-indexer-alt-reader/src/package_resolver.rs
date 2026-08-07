// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::Arc;

use async_graphql::dataloader::DataLoader;
use async_graphql::dataloader::Loader;
use diesel::prelude::QueryableByName;
use diesel::sql_types::Array;
use diesel::sql_types::Bytea;
use move_core_types::account_address::AccountAddress;
use prost_types::FieldMask;
use sui_indexer_alt_schema::schema::kv_packages;
use sui_package_resolver::Package;
use sui_package_resolver::PackageStore;
use sui_package_resolver::PackageStoreWithLruCache;
use sui_package_resolver::Result;
use sui_package_resolver::error::Error;
use sui_rpc::field::FieldMaskUtil;
use sui_rpc::proto::sui::rpc::v2 as proto;
use sui_types::base_types::ObjectID;
use sui_types::object::Object;

use crate::ledger_grpc_reader::ChunkedLoader;
use crate::ledger_grpc_reader::LedgerGrpcReader;
use crate::pg_reader::PgReader;

const PG_STORE: &str = "PostgreSQL";
const GRPC_STORE: &str = "Ledger gRPC";

pub type PackageCache = PackageStoreWithLruCache<Box<dyn PackageStore>>;

pub struct DbPackageStore(Arc<DataLoader<PgReader>>);

pub struct GrpcPackageStore(Arc<DataLoader<LedgerGrpcReader>>);

#[derive(Copy, Clone, Hash, Eq, PartialEq, Debug)]
struct PackageKey(AccountAddress);

impl DbPackageStore {
    pub fn new(loader: Arc<DataLoader<PgReader>>) -> Self {
        Self(loader)
    }
}

impl GrpcPackageStore {
    pub fn new(reader: &LedgerGrpcReader) -> Self {
        Self(Arc::new(reader.as_data_loader()))
    }
}

#[async_trait::async_trait]
impl PackageStore for DbPackageStore {
    async fn fetch(&self, id: AccountAddress) -> Result<Arc<Package>> {
        let Self(loader) = self;
        let Some(package) = loader.load_one(PackageKey(id)).await? else {
            return Err(Error::PackageNotFound(id));
        };

        Ok(package)
    }
}

#[async_trait::async_trait]
impl PackageStore for GrpcPackageStore {
    async fn fetch(&self, id: AccountAddress) -> Result<Arc<Package>> {
        let Self(loader) = self;
        let Some(package) = loader.load_one(PackageKey(id)).await? else {
            return Err(Error::PackageNotFound(id));
        };

        Ok(package)
    }
}

#[async_trait::async_trait]
impl Loader<PackageKey> for PgReader {
    type Value = Arc<Package>;
    type Error = Error;

    async fn load(&self, keys: &[PackageKey]) -> Result<HashMap<PackageKey, Arc<Package>>> {
        let mut id_to_package = HashMap::new();
        if keys.is_empty() {
            return Ok(id_to_package);
        }

        let mut conn = self.connect().await.map_err(|e| store_err(PG_STORE, e))?;

        #[derive(QueryableByName)]
        #[diesel(table_name = kv_packages)]
        struct SerializedPackage {
            serialized_object: Vec<u8>,
        }

        let ids: Vec<_> = keys.iter().map(|PackageKey(id)| id.to_vec()).collect();
        let query = diesel::sql_query(
            r#"
                SELECT
                    v.serialized_object
                FROM (
                    SELECT UNNEST($1) package_id
                ) k
                CROSS JOIN LATERAL (
                    SELECT
                        serialized_object
                    FROM
                        kv_packages
                    WHERE
                        kv_packages.package_id = k.package_id
                    ORDER BY
                        package_version DESC
                    LIMIT 1
                ) v
            "#,
        )
        .bind::<Array<Bytea>, _>(ids);

        let stored_packages: Vec<SerializedPackage> = conn
            .results(query)
            .await
            .map_err(|e| store_err(PG_STORE, e))?;

        for stored in stored_packages {
            let object: Object = bcs::from_bytes(&stored.serialized_object)?;
            let Some(move_package) = object.data.try_as_package() else {
                return Err(Error::NotAPackage(object.id().into()));
            };

            let package = Package::read_from_package(move_package)?;
            id_to_package.insert(PackageKey(*move_package.id()), Arc::new(package));
        }

        Ok(id_to_package)
    }
}

#[async_trait::async_trait]
impl ChunkedLoader<PackageKey> for LedgerGrpcReader {
    type Value = Arc<Package>;
    type Error = Error;

    fn chunk_size(&self) -> usize {
        self.max_batch_get_objects()
    }

    async fn load_chunk(&self, keys: &[PackageKey]) -> Result<HashMap<PackageKey, Arc<Package>>> {
        let mut id_to_package = HashMap::new();
        if keys.is_empty() {
            return Ok(id_to_package);
        }

        let requests = keys
            .iter()
            .map(|PackageKey(id)| proto::GetObjectRequest::new(&ObjectID::from(*id).into()))
            .collect();

        let mut request = proto::BatchGetObjectsRequest::default();
        request.requests = requests;
        request.read_mask = Some(FieldMask::from_paths(["bcs"]));

        let batch_response = self
            .batch_get_objects(request)
            .await
            .map_err(|e| store_err(GRPC_STORE, e))?;

        for obj_result in batch_response.objects {
            let Some(proto::get_object_result::Result::Object(object)) = obj_result.result else {
                // Misses come back as per-object errors; leave the key unmapped so the store
                // reports `PackageNotFound`.
                continue;
            };

            let object: Object = object
                .bcs
                .as_ref()
                .ok_or_else(|| store_err(GRPC_STORE, "Missing bcs in object"))?
                .deserialize()
                .map_err(|e| store_err(GRPC_STORE, format!("Failed to deserialize object: {e}")))?;
            let Some(move_package) = object.data.try_as_package() else {
                return Err(Error::NotAPackage(object.id().into()));
            };

            let package = Package::read_from_package(move_package)?;
            id_to_package.insert(PackageKey(*move_package.id()), Arc::new(package));
        }

        Ok(id_to_package)
    }
}

fn store_err(store: &'static str, e: impl ToString) -> Error {
    Error::Store {
        store,
        error: e.to_string(),
    }
}
