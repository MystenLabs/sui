// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use bytes::Bytes;
use futures::stream::BoxStream;
use object_store::path::Path;
use object_store::{DynObjectStore, ObjectMeta};
use std::sync::Arc;

pub mod http;
pub mod util;

#[async_trait]
pub trait ObjectStoreGetExt: std::fmt::Display + Send + Sync + 'static {
    /// Return the bytes at given path in object store
    async fn get_bytes(&self, src: &Path) -> Result<Bytes>;
}

macro_rules! as_ref_get_ext_impl {
    ($type:ty) => {
        #[async_trait]
        impl ObjectStoreGetExt for $type {
            async fn get_bytes(&self, src: &Path) -> Result<Bytes> {
                self.as_ref().get_bytes(src).await
            }
        }
    };
}

as_ref_get_ext_impl!(Arc<dyn ObjectStoreGetExt>);
as_ref_get_ext_impl!(Box<dyn ObjectStoreGetExt>);

#[async_trait]
impl ObjectStoreGetExt for Arc<DynObjectStore> {
    async fn get_bytes(&self, src: &Path) -> Result<Bytes> {
        self.get(src)
            .await
            .map_err(|e| anyhow!("Failed to get file {} with error: {:?}", src, e))?
            .bytes()
            .await
            .map_err(|e| {
                anyhow!(
                    "Failed to collect GET result for file {} into bytes with error: {:?}",
                    src,
                    e
                )
            })
    }
}

#[async_trait]
pub trait ObjectStoreListExt: Send + Sync + 'static {
    /// List the objects at the given path in object store
    async fn list_objects(
        &self,
        src: Option<&Path>,
    ) -> BoxStream<'_, object_store::Result<ObjectMeta>>;
}

macro_rules! as_ref_list_ext_impl {
    ($type:ty) => {
        #[async_trait]
        impl ObjectStoreListExt for $type {
            async fn list_objects(
                &self,
                src: Option<&Path>,
            ) -> BoxStream<'_, object_store::Result<ObjectMeta>> {
                self.as_ref().list_objects(src).await
            }
        }
    };
}

as_ref_list_ext_impl!(Arc<dyn ObjectStoreListExt>);
as_ref_list_ext_impl!(Box<dyn ObjectStoreListExt>);

#[async_trait]
impl ObjectStoreListExt for Arc<DynObjectStore> {
    async fn list_objects(
        &self,
        src: Option<&Path>,
    ) -> BoxStream<'_, object_store::Result<ObjectMeta>> {
        self.list(src)
    }
}

#[async_trait]
pub trait ObjectStorePutExt: Send + Sync + 'static {
    /// Write the bytes at the given location in object store
    async fn put_bytes(&self, src: &Path, bytes: Bytes) -> Result<()>;
}

macro_rules! as_ref_put_ext_impl {
    ($type:ty) => {
        #[async_trait]
        impl ObjectStorePutExt for $type {
            async fn put_bytes(&self, src: &Path, bytes: Bytes) -> Result<()> {
                self.as_ref().put_bytes(src, bytes).await
            }
        }
    };
}

as_ref_put_ext_impl!(Arc<dyn ObjectStorePutExt>);
as_ref_put_ext_impl!(Box<dyn ObjectStorePutExt>);

#[async_trait]
impl ObjectStorePutExt for Arc<DynObjectStore> {
    async fn put_bytes(&self, src: &Path, bytes: Bytes) -> Result<()> {
        self.put(src, bytes.into()).await?;
        Ok(())
    }
}

#[async_trait]
pub trait ObjectStoreDeleteExt: Send + Sync + 'static {
    /// Delete the object at the given location in object store
    async fn delete_object(&self, src: &Path) -> Result<()>;
}

macro_rules! as_ref_delete_ext_impl {
    ($type:ty) => {
        #[async_trait]
        impl ObjectStoreDeleteExt for $type {
            async fn delete_object(&self, src: &Path) -> Result<()> {
                self.as_ref().delete_object(src).await
            }
        }
    };
}

as_ref_delete_ext_impl!(Arc<dyn ObjectStoreDeleteExt>);
as_ref_delete_ext_impl!(Box<dyn ObjectStoreDeleteExt>);

#[async_trait]

impl ObjectStoreDeleteExt for Arc<DynObjectStore> {
    async fn delete_object(&self, src: &Path) -> Result<()> {
        self.delete(src).await?;
        Ok(())
    }
}
