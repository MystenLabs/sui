// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use async_trait::async_trait;
use diesel::{ExpressionMethods, OptionalExtension, QueryDsl, RunQueryDsl};
use move_core_types::account_address::AccountAddress;
use sui_indexer::errors::IndexerError;
use sui_indexer::{indexer_reader::IndexerReader, schema::objects};
use sui_package_resolver::Resolver;
use sui_package_resolver::{
    error::Error as PackageResolverError, Package, PackageStore, PackageStoreWithLruCache, Result,
};
use sui_types::base_types::SequenceNumber;
use sui_types::object::Object;
use thiserror::Error;

const STORE: &str = "PostgresDB";

pub(crate) type PackageCache = PackageStoreWithLruCache<DbPackageStore>;
pub(crate) type PackageResolver = Resolver<PackageCache>;

/// Store which fetches package for the given address from the backend db on every call
/// to `fetch`
pub struct DbPackageStore(pub IndexerReader);

#[derive(Error, Debug)]
pub enum Error {
    #[error("{0}")]
    Indexer(#[from] IndexerError),
}

#[async_trait]
impl PackageStore for DbPackageStore {
    async fn version(&self, id: AccountAddress) -> Result<SequenceNumber> {
        let query = objects::dsl::objects
            .select(objects::dsl::object_version)
            .filter(objects::dsl::object_id.eq(id.to_vec()));

        let Some(version) = self
            .0
            .run_query_async(move |conn| query.get_result::<i64>(conn).optional())
            .await
            .map_err(Error::Indexer)?
        else {
            return Err(PackageResolverError::PackageNotFound(id));
        };

        Ok(SequenceNumber::from_u64(version as u64))
    }

    async fn fetch(&self, id: AccountAddress) -> Result<Arc<Package>> {
        let query = objects::dsl::objects
            .select(objects::dsl::serialized_object)
            .filter(objects::dsl::object_id.eq(id.to_vec()));

        let Some(bcs) = self
            .0
            .run_query_async(move |conn| query.get_result::<Vec<u8>>(conn).optional())
            .await
            .map_err(Error::Indexer)?
        else {
            return Err(PackageResolverError::PackageNotFound(id));
        };

        let object = bcs::from_bytes::<Object>(&bcs)?;
        Ok(Arc::new(Package::read(&object)?))
    }
}

impl From<Error> for PackageResolverError {
    fn from(source: Error) -> Self {
        match source {
            Error::Indexer(indexer_error) => Self::Store {
                store: STORE,
                source: Box::new(indexer_error),
            },
        }
    }
}
