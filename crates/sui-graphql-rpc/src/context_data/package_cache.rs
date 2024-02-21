// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use async_trait::async_trait;
use diesel::{ExpressionMethods, OptionalExtension, QueryDsl, RunQueryDsl};
use move_core_types::account_address::AccountAddress;
use sui_indexer::errors::IndexerError;
use sui_indexer::{indexer_reader::IndexerReader, schema::objects};
use sui_package_resolver::{
    error::Error as PackageResolverError, Package, PackageStore, PackageStoreWithLruCache, Result,
};
use sui_types::{base_types::SequenceNumber, object::Object};
use thiserror::Error;

const STORE: &str = "PostgresDB";
#[derive(Error, Debug)]
pub enum Error {
    #[error("{0}")]
    Indexer(#[from] IndexerError),
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

pub(crate) type PackageCache = PackageStoreWithLruCache<DbPackageStore>;

/// Store which fetches package for the given address from the backend db on every call
/// to `fetch`
pub struct DbPackageStore(pub IndexerReader);

#[async_trait]
impl PackageStore for DbPackageStore {
    async fn version(&self, id: AccountAddress) -> Result<SequenceNumber> {
        get_package_version_from_db(id, &self.0).await
    }

    async fn fetch(&self, id: AccountAddress) -> Result<Arc<Package>> {
        let package = get_package_from_db(id, &self.0).await?;
        Ok(Arc::new(package))
    }
}

async fn get_package_version_from_db(
    id: AccountAddress,
    sui_indexer: &IndexerReader,
) -> Result<SequenceNumber> {
    let query = objects::dsl::objects
        .select(objects::dsl::object_version)
        .filter(objects::dsl::object_id.eq(id.to_vec()));

    let Some(version) = sui_indexer
        .run_query_async(move |conn| query.get_result::<i64>(conn).optional())
        .await
        .map_err(Error::Indexer)?
    else {
        return Err(PackageResolverError::PackageNotFound(id));
    };

    Ok(SequenceNumber::from_u64(version as u64))
}

async fn get_package_from_db(id: AccountAddress, sui_indexer: &IndexerReader) -> Result<Package> {
    let query = objects::dsl::objects
        .select(objects::dsl::serialized_object)
        .filter(objects::dsl::object_id.eq(id.to_vec()));

    let Some(bcs) = sui_indexer
        .run_query_async(move |conn| query.get_result::<Vec<u8>>(conn).optional())
        .await
        .map_err(Error::Indexer)?
    else {
        return Err(PackageResolverError::PackageNotFound(id));
    };

    let object = bcs::from_bytes::<Object>(&bcs)?;
    Package::read(&object)
}
