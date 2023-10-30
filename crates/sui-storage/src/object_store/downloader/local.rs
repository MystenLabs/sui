// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::object_store::downloader::Downloader;
use crate::object_store::util::path_to_filesystem;
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use bytes::Bytes;
use object_store::path::Path;
use std::fs;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

pub struct LocalStorage {
    root: PathBuf,
}

impl LocalStorage {
    pub fn new(directory: &std::path::Path) -> Result<Self> {
        let path = fs::canonicalize(directory).context(anyhow!("Unable to canonicalize"))?;
        fs::create_dir_all(&path).context(anyhow!(
            "Failed to create local directory: {}",
            path.display()
        ))?;
        Ok(LocalStorage { root: path })
    }
}

#[async_trait]
impl Downloader for LocalStorage {
    async fn get(&self, location: &Path) -> Result<Bytes> {
        let path_to_filesystem = path_to_filesystem(self.root.clone(), location)?;
        let handle = tokio::task::spawn_blocking(move || {
            let mut f = File::open(path_to_filesystem)
                .map_err(|e| anyhow!("Failed to open file with error: {}", e.to_string()))?;
            let mut buf = vec![];
            f.read_to_end(&mut buf)
                .context(anyhow!("Failed to read file"))?;
            Ok(buf.into())
        });
        handle.await?
    }
}
