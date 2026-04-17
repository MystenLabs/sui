// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::Result;
use async_trait::async_trait;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::sequential;
use sui_indexer_alt_framework::store::Store;
use sui_package_resolver::PackageStoreWithLruCache;
use sui_rpc_resolver::package_store::RpcPackageStore;
use sui_types::SYSTEM_PACKAGE_ADDRESSES;
use sui_types::full_checkpoint_content::Checkpoint;

use crate::store::AnalyticsStore;

pub const SYSTEM_PACKAGE_EVICTION_PIPELINE: &str = "SystemPackageEviction";

pub struct SystemPackageEviction {
    package_cache: Arc<PackageStoreWithLruCache<RpcPackageStore>>,
    last_epoch: AtomicU64,
}

impl SystemPackageEviction {
    pub fn new(package_cache: Arc<PackageStoreWithLruCache<RpcPackageStore>>) -> Self {
        Self {
            package_cache,
            last_epoch: AtomicU64::new(u64::MAX),
        }
    }
}

#[async_trait]
impl Processor for SystemPackageEviction {
    const NAME: &'static str = SYSTEM_PACKAGE_EVICTION_PIPELINE;
    type Value = ();

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> Result<Vec<()>> {
        let epoch = checkpoint.summary.data().epoch;
        if self.last_epoch.swap(epoch, Ordering::Relaxed) != epoch {
            self.package_cache
                .evict(SYSTEM_PACKAGE_ADDRESSES.iter().copied());
        }
        Ok(vec![])
    }
}

#[async_trait]
impl sequential::Handler for SystemPackageEviction {
    type Store = AnalyticsStore;
    type Batch = ();

    fn batch(&self, _batch: &mut (), _values: std::vec::IntoIter<()>) {}

    async fn commit<'a>(
        &self,
        _batch: &(),
        _conn: &mut <AnalyticsStore as Store>::Connection<'a>,
    ) -> Result<usize> {
        Ok(0)
    }
}
