// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use move_core_types::account_address::AccountAddress;
use sui_kvstore::{BigTableClient, KeyValueStoreReader};
use sui_package_resolver::Package;
use sui_package_resolver::PackageStore;
use sui_package_resolver::error::Error;
use sui_types::base_types::ObjectID;

const STORE: &str = "BigTable";

pub struct BigTablePackageStore {
    client: BigTableClient,
}

impl BigTablePackageStore {
    pub fn new(client: BigTableClient) -> Self {
        Self { client }
    }
}

#[async_trait::async_trait]
impl PackageStore for BigTablePackageStore {
    async fn fetch(&self, id: AccountAddress) -> sui_package_resolver::Result<Arc<Package>> {
        let mut client = self.client.clone();
        let object = client
            .get_latest_object(&ObjectID::from(id))
            .await
            .map_err(|e| Error::Store {
                store: STORE,
                error: e.to_string(),
            })?
            .ok_or(Error::PackageNotFound(id))?;
        let package = object
            .data
            .try_as_package()
            .ok_or(Error::NotAPackage(object.id().into()))?;
        Ok(Arc::new(Package::read_from_package(package)?))
    }
}
