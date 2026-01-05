// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! A configurable ObjectStore wrapper for testing.
//!
//! Wraps an inner ObjectStore and can be configured to fail on specific operations
//! and record the order of PUT operations.

use std::ops::Range;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use futures::stream::BoxStream;
use object_store::{
    GetOptions, GetResult, ListResult, MultipartUpload, ObjectMeta, ObjectStore, PutMultipartOpts,
    PutOptions, PutPayload, PutResult, Result, path::Path,
};
use std::sync::RwLock;

/// Configuration for failure injection.
#[derive(Debug, Clone, Default)]
pub struct MockConfig {
    /// Fail on the Nth put operation (1-indexed). None = don't fail.
    pub fail_on_put: Option<usize>,
    /// Current put count.
    pub put_count: usize,
    /// Only fail puts to paths matching this prefix. None = match all.
    pub fail_path_prefix: Option<String>,
    /// Record the order of successful PUT operations (path strings).
    pub put_order: Vec<String>,
}

impl MockConfig {
    /// Reset the put count to zero and clear put order.
    pub fn reset_counts(&mut self) {
        self.put_count = 0;
        self.put_order.clear();
    }

    /// Disable all failures.
    pub fn disable_failures(&mut self) {
        self.fail_on_put = None;
        self.fail_path_prefix = None;
    }
}

/// An ObjectStore wrapper that can inject failures for testing.
#[derive(Debug)]
pub struct MockStore {
    inner: Arc<dyn ObjectStore>,
    config: Arc<RwLock<MockConfig>>,
}

impl MockStore {
    /// Create a new MockStore wrapping the given inner store.
    pub fn new(inner: Arc<dyn ObjectStore>) -> Self {
        Self {
            inner,
            config: Arc::new(RwLock::new(MockConfig::default())),
        }
    }

    /// Get access to the configuration for modification.
    pub fn config(&self) -> &Arc<RwLock<MockConfig>> {
        &self.config
    }

    /// Check if a put operation should fail and update counts.
    ///
    /// Only increments the put count for paths that match the optional prefix filter.
    fn should_fail_put(&self, path: &Path) -> bool {
        let mut config = self.config.write().unwrap();

        // Check path prefix filter first
        if let Some(ref prefix) = config.fail_path_prefix
            && !path.as_ref().starts_with(prefix)
        {
            // Path doesn't match filter - don't count and don't fail
            return false;
        }

        // Only increment count for matching paths (or all paths if no filter)
        config.put_count += 1;

        // Check if this is the Nth put that should fail
        if let Some(fail_on) = config.fail_on_put
            && config.put_count == fail_on
        {
            return true;
        }

        false
    }

    fn make_error(&self, path: &Path) -> object_store::Error {
        object_store::Error::Generic {
            store: "MockStore",
            source: format!("Injected failure for path: {}", path).into(),
        }
    }

    /// Record a successful PUT operation.
    fn record_put(&self, path: &Path) {
        self.config
            .write()
            .unwrap()
            .put_order
            .push(path.to_string());
    }
}

impl std::fmt::Display for MockStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MockStore({})", self.inner)
    }
}

#[async_trait]
impl ObjectStore for MockStore {
    async fn put(&self, location: &Path, payload: PutPayload) -> Result<PutResult> {
        if self.should_fail_put(location) {
            return Err(self.make_error(location));
        }
        let result = self.inner.put(location, payload).await?;
        self.record_put(location);
        Ok(result)
    }

    async fn put_opts(
        &self,
        location: &Path,
        payload: PutPayload,
        opts: PutOptions,
    ) -> Result<PutResult> {
        if self.should_fail_put(location) {
            return Err(self.make_error(location));
        }
        let result = self.inner.put_opts(location, payload, opts).await?;
        self.record_put(location);
        Ok(result)
    }

    async fn put_multipart(&self, location: &Path) -> Result<Box<dyn MultipartUpload>> {
        self.inner.put_multipart(location).await
    }

    async fn put_multipart_opts(
        &self,
        location: &Path,
        opts: PutMultipartOpts,
    ) -> Result<Box<dyn MultipartUpload>> {
        self.inner.put_multipart_opts(location, opts).await
    }

    async fn get(&self, location: &Path) -> Result<GetResult> {
        self.inner.get(location).await
    }

    async fn get_opts(&self, location: &Path, options: GetOptions) -> Result<GetResult> {
        self.inner.get_opts(location, options).await
    }

    async fn get_range(&self, location: &Path, range: Range<usize>) -> Result<Bytes> {
        self.inner.get_range(location, range).await
    }

    async fn get_ranges(&self, location: &Path, ranges: &[Range<usize>]) -> Result<Vec<Bytes>> {
        self.inner.get_ranges(location, ranges).await
    }

    async fn head(&self, location: &Path) -> Result<ObjectMeta> {
        self.inner.head(location).await
    }

    async fn delete(&self, location: &Path) -> Result<()> {
        self.inner.delete(location).await
    }

    fn list(&self, prefix: Option<&Path>) -> BoxStream<'_, Result<ObjectMeta>> {
        self.inner.list(prefix)
    }

    fn list_with_offset(
        &self,
        prefix: Option<&Path>,
        offset: &Path,
    ) -> BoxStream<'_, Result<ObjectMeta>> {
        self.inner.list_with_offset(prefix, offset)
    }

    async fn list_with_delimiter(&self, prefix: Option<&Path>) -> Result<ListResult> {
        self.inner.list_with_delimiter(prefix).await
    }

    async fn copy(&self, from: &Path, to: &Path) -> Result<()> {
        self.inner.copy(from, to).await
    }

    async fn rename(&self, from: &Path, to: &Path) -> Result<()> {
        self.inner.rename(from, to).await
    }

    async fn copy_if_not_exists(&self, from: &Path, to: &Path) -> Result<()> {
        self.inner.copy_if_not_exists(from, to).await
    }

    async fn rename_if_not_exists(&self, from: &Path, to: &Path) -> Result<()> {
        self.inner.rename_if_not_exists(from, to).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use object_store::memory::InMemory;

    #[tokio::test]
    async fn test_failing_store_basic() {
        let inner = Arc::new(InMemory::new());
        let store = MockStore::new(inner);

        // Normal put should work
        store.put(&Path::from("test"), "data".into()).await.unwrap();

        // Read back
        let result = store.get(&Path::from("test")).await.unwrap();
        let bytes = result.bytes().await.unwrap();
        assert_eq!(bytes.as_ref(), b"data");
    }

    #[tokio::test]
    async fn test_failing_store_fail_on_nth() {
        let inner = Arc::new(InMemory::new());
        let store = MockStore::new(inner);

        // Configure to fail on 2nd put
        store.config().write().unwrap().fail_on_put = Some(2);

        // First put succeeds
        store
            .put(&Path::from("file1"), "data1".into())
            .await
            .unwrap();

        // Second put fails
        let result = store.put(&Path::from("file2"), "data2".into()).await;
        assert!(result.is_err());

        // Third put succeeds
        store
            .put(&Path::from("file3"), "data3".into())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_failing_store_path_filter() {
        let inner = Arc::new(InMemory::new());
        let store = MockStore::new(inner);

        // Configure to fail on 1st put to _metadata paths
        {
            let mut config = store.config().write().unwrap();
            config.fail_on_put = Some(1);
            config.fail_path_prefix = Some("_metadata".to_string());
        }

        // Put to non-matching path succeeds
        store
            .put(&Path::from("data/file1"), "data1".into())
            .await
            .unwrap();

        // Put to matching path fails
        let result = store
            .put(&Path::from("_metadata/watermark"), "data2".into())
            .await;
        assert!(result.is_err());
    }
}
