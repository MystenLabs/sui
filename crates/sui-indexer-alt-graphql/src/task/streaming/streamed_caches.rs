// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use super::EvictableCache;
use super::StreamedObjectStore;
use super::StreamedTransactionStore;
use super::StreamingPackageStore;

/// The in-memory caches a streamed checkpoint reads ahead of the durable index: packages for type
/// resolution, transaction contents for live `previousTransaction` lookups, and object contents for
/// live cross-checkpoint object lookups. Threaded as one handle from the stream task down to the
/// subscription resolvers, so a scope carries every streamed source it can read from in one place.
pub(crate) struct StreamedCaches {
    pub(crate) package_store: Arc<StreamingPackageStore>,
    pub(crate) transaction_store: Arc<StreamedTransactionStore>,
    pub(crate) object_store: Arc<StreamedObjectStore>,
}

impl StreamedCaches {
    pub(crate) fn new(
        package_store: Arc<StreamingPackageStore>,
        transaction_store: Arc<StreamedTransactionStore>,
        object_store: Arc<StreamedObjectStore>,
    ) -> Self {
        Self {
            package_store,
            transaction_store,
            object_store,
        }
    }

    /// Each cache as an [`EvictableCache`], for registration with the eviction task. Eviction stays
    /// per-store: each drains against its own watermark pipeline.
    pub(crate) fn to_evictable(&self) -> Vec<Arc<dyn EvictableCache>> {
        vec![
            self.package_store.clone(),
            self.transaction_store.clone(),
            self.object_store.clone(),
        ]
    }
}
