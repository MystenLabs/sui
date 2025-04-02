// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use async_trait::async_trait;
use move_core_types::account_address::AccountAddress;
use std::collections::HashMap;
use std::sync::Arc;
use sui_package_resolver::{error::Error as PackageResolverError, Package, PackageStore};
use tokio::sync::Mutex;
use tracing::{error, info};

use crate::config::Config;
use crate::verifier::get_verified_object;

pub struct RemotePackageStore {
    config: Config,
    cache: Mutex<HashMap<AccountAddress, Arc<Package>>>,
}

impl RemotePackageStore {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            cache: Mutex::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl PackageStore for RemotePackageStore {
    async fn fetch(&self, id: AccountAddress) -> sui_package_resolver::Result<Arc<Package>> {
        // Check if we have it in the cache
        let res: Result<Arc<Package>> = async move {
            if let Some(package) = self.cache.lock().await.get(&id) {
                info!("Fetch Package: {} cache hit", id);
                return Ok(package.clone());
            }

            info!("Fetch Package: {}", id);

            let object = get_verified_object(&self.config, id.into()).await?;
            let package = Arc::new(Package::read_from_object(&object)?);

            // Add to the cache
            self.cache.lock().await.insert(id, package.clone());

            Ok(package)
        }
        .await;
        res.map_err(|e| {
            error!("Fetch Package: {} error: {:?}", id, e);
            PackageResolverError::PackageNotFound(id)
        })
    }
}
