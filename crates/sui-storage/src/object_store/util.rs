// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::object_store::{
    ObjectStoreDeleteExt, ObjectStoreGetExt, ObjectStoreListExt, ObjectStorePutExt,
};
use anyhow::{anyhow, Context, Result};
use backoff::future::retry;
use bytes::Bytes;
use futures::StreamExt;
use futures::TryStreamExt;
use indicatif::ProgressBar;
use itertools::Itertools;
use object_store::path::Path;
use object_store::{DynObjectStore, Error, ObjectStore};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::Instant;
use tracing::{error, warn};
use url::Url;

pub const MANIFEST_FILENAME: &str = "MANIFEST";

#[derive(Serialize, Deserialize)]

pub struct Manifest {
    pub available_epochs: Vec<u64>,
}

impl Manifest {
    pub fn new(available_epochs: Vec<u64>) -> Self {
        Manifest { available_epochs }
    }

    pub fn epoch_exists(&self, epoch: u64) -> bool {
        self.available_epochs.contains(&epoch)
    }
}

#[derive(Debug, Clone)]
pub struct PerEpochManifest {
    pub lines: Vec<String>,
}

impl PerEpochManifest {
    pub fn new(lines: Vec<String>) -> Self {
        PerEpochManifest { lines }
    }

    pub fn serialize_as_newline_delimited(&self) -> String {
        self.lines.join("\n")
    }

    pub fn deserialize_from_newline_delimited(s: &str) -> PerEpochManifest {
        PerEpochManifest {
            lines: s.lines().map(String::from).collect(),
        }
    }

    // Method to filter lines by a given prefix
    pub fn filter_by_prefix(&self, prefix: &str) -> PerEpochManifest {
        let filtered_lines = self
            .lines
            .iter()
            .filter(|line| line.starts_with(prefix))
            .cloned()
            .collect();

        PerEpochManifest {
            lines: filtered_lines,
        }
    }
}

pub async fn get<S: ObjectStoreGetExt>(store: &S, src: &Path) -> Result<Bytes> {
    let bytes = retry(backoff::ExponentialBackoff::default(), || async {
        store.get_bytes(src).await.map_err(|e| {
            error!("Failed to read file from object store with error: {:?}", &e);
            backoff::Error::transient(e)
        })
    })
    .await?;
    Ok(bytes)
}

pub async fn exists<S: ObjectStoreGetExt>(store: &S, src: &Path) -> bool {
    store.get_bytes(src).await.is_ok()
}

pub async fn put<S: ObjectStorePutExt>(store: &S, src: &Path, bytes: Bytes) -> Result<()> {
    retry(backoff::ExponentialBackoff::default(), || async {
        if !bytes.is_empty() {
            store.put_bytes(src, bytes.clone()).await.map_err(|e| {
                error!("Failed to write file to object store with error: {:?}", &e);
                backoff::Error::transient(e)
            })
        } else {
            warn!("Not copying empty file: {:?}", src);
            Ok(())
        }
    })
    .await?;
    Ok(())
}

pub async fn copy_file<S: ObjectStoreGetExt, D: ObjectStorePutExt>(
    src: &Path,
    dest: &Path,
    src_store: &S,
    dest_store: &D,
) -> Result<()> {
    let bytes = get(src_store, src).await?;
    if !bytes.is_empty() {
        put(dest_store, dest, bytes).await
    } else {
        warn!("Not copying empty file: {:?}", src);
        Ok(())
    }
}

pub async fn copy_files<S: ObjectStoreGetExt, D: ObjectStorePutExt>(
    src: &[Path],
    dest: &[Path],
    src_store: &S,
    dest_store: &D,
    concurrency: NonZeroUsize,
    progress_bar: Option<ProgressBar>,
) -> Result<Vec<()>> {
    let mut instant = Instant::now();
    let progress_bar_clone = progress_bar.clone();
    let results = futures::stream::iter(src.iter().zip(dest.iter()))
        .map(|(path_in, path_out)| async move {
            let ret = copy_file(path_in, path_out, src_store, dest_store).await;
            Ok((path_out.clone(), ret))
        })
        .boxed()
        .buffer_unordered(concurrency.get())
        .try_for_each(|(path, ret)| {
            if let Some(progress_bar_clone) = &progress_bar_clone {
                progress_bar_clone.inc(1);
                progress_bar_clone.set_message(format!("file: {}", path));
                instant = Instant::now();
            }
            futures::future::ready(ret)
        })
        .await;
    Ok(results.into_iter().collect())
}

pub async fn copy_recursively<S: ObjectStoreGetExt + ObjectStoreListExt, D: ObjectStorePutExt>(
    dir: &Path,
    src_store: &S,
    dest_store: &D,
    concurrency: NonZeroUsize,
) -> Result<Vec<()>> {
    let mut input_paths = vec![];
    let mut output_paths = vec![];
    let mut paths = src_store.list_objects(Some(dir)).await;
    while let Some(res) = paths.next().await {
        if let Ok(object_metadata) = res {
            input_paths.push(object_metadata.location.clone());
            output_paths.push(object_metadata.location);
        } else {
            return Err(res.err().unwrap().into());
        }
    }
    copy_files(
        &input_paths,
        &output_paths,
        src_store,
        dest_store,
        concurrency,
        None,
    )
    .await
}

pub async fn delete_files<S: ObjectStoreDeleteExt>(
    files: &[Path],
    store: &S,
    concurrency: NonZeroUsize,
) -> Result<Vec<()>> {
    let results: Vec<Result<()>> = futures::stream::iter(files)
        .map(|f| {
            retry(backoff::ExponentialBackoff::default(), || async {
                store.delete_object(f).await.map_err(|e| {
                    error!("Failed to delete file on object store with error: {:?}", &e);
                    backoff::Error::transient(e)
                })
            })
        })
        .boxed()
        .buffer_unordered(concurrency.get())
        .collect()
        .await;
    results.into_iter().collect()
}

pub async fn delete_recursively<S: ObjectStoreDeleteExt + ObjectStoreListExt>(
    path: &Path,
    store: &S,
    concurrency: NonZeroUsize,
) -> Result<Vec<()>> {
    let mut paths_to_delete = vec![];
    let mut paths = store.list_objects(Some(path)).await;
    while let Some(res) = paths.next().await {
        if let Ok(object_metadata) = res {
            paths_to_delete.push(object_metadata.location);
        } else {
            return Err(res.err().unwrap().into());
        }
    }
    delete_files(&paths_to_delete, store, concurrency).await
}

pub fn path_to_filesystem(local_dir_path: PathBuf, location: &Path) -> anyhow::Result<PathBuf> {
    // Convert an `object_store::path::Path` to `std::path::PathBuf`
    let path = std::fs::canonicalize(local_dir_path)?;
    let mut url = Url::from_file_path(&path)
        .map_err(|_| anyhow!("Failed to parse input path: {}", path.display()))?;
    url.path_segments_mut()
        .map_err(|_| anyhow!("Failed to get path segments: {}", path.display()))?
        .pop_if_empty()
        .extend(location.parts());
    let new_path = url
        .to_file_path()
        .map_err(|_| anyhow!("Failed to convert url to path: {}", url.as_str()))?;
    Ok(new_path)
}

/// This function will find all child directories in the input store which are of the form "epoch_num"
/// and return a map of epoch number to the directory path
pub async fn find_all_dirs_with_epoch_prefix(
    store: &Arc<DynObjectStore>,
    prefix: Option<&Path>,
) -> anyhow::Result<BTreeMap<u64, Path>> {
    let mut dirs = BTreeMap::new();
    let entries = store.list_with_delimiter(prefix).await?;
    for entry in entries.common_prefixes {
        if let Some(filename) = entry.filename() {
            if !filename.starts_with("epoch_") || filename.ends_with(".tmp") {
                continue;
            }
            let epoch = filename
                .split_once('_')
                .context("Failed to split dir name")
                .map(|(_, epoch)| epoch.parse::<u64>())??;
            dirs.insert(epoch, entry);
        }
    }
    Ok(dirs)
}

pub async fn list_all_epochs(object_store: Arc<DynObjectStore>) -> Result<Vec<u64>> {
    let remote_epoch_dirs = find_all_dirs_with_epoch_prefix(&object_store, None).await?;
    let mut out = vec![];
    let mut success_marker_found = false;
    for (epoch, path) in remote_epoch_dirs.iter().sorted() {
        let success_marker = path.child("_SUCCESS");
        let get_result = object_store.get(&success_marker).await;
        match get_result {
            Err(_) => {
                if !success_marker_found {
                    error!("No success marker found for epoch: {epoch}");
                }
            }
            Ok(_) => {
                out.push(*epoch);
                success_marker_found = true;
            }
        }
    }
    Ok(out)
}

pub async fn run_manifest_update_loop(
    store: Arc<DynObjectStore>,
    mut recv: tokio::sync::broadcast::Receiver<()>,
) -> Result<()> {
    let mut update_interval = tokio::time::interval(Duration::from_secs(300));
    loop {
        tokio::select! {
            _now = update_interval.tick() => {
                if let Ok(epochs) = list_all_epochs(store.clone()).await {
                    let manifest_path = Path::from(MANIFEST_FILENAME);
                    let manifest = Manifest::new(epochs);
                    let bytes = serde_json::to_string(&manifest)?;
                    put(&store, &manifest_path, Bytes::from(bytes)).await?;
                }
            },
             _ = recv.recv() => break,
        }
    }
    Ok(())
}

/// This function will find all child directories in the input store which are of the form "epoch_num"
/// and return a map of epoch number to the directory path
pub async fn find_all_files_with_epoch_prefix(
    store: &Arc<DynObjectStore>,
    prefix: Option<&Path>,
) -> anyhow::Result<Vec<Range<u64>>> {
    let mut ranges = Vec::new();
    let entries = store.list_with_delimiter(prefix).await?;
    for entry in entries.objects {
        let checkpoint_seq_range = entry
            .location
            .filename()
            .ok_or(anyhow!("Illegal file name"))?
            .split_once('.')
            .context("Failed to split dir name")?
            .0
            .split_once('_')
            .context("Failed to split dir name")
            .map(|(start, end)| Range {
                start: start.parse::<u64>().unwrap(),
                end: end.parse::<u64>().unwrap(),
            })?;

        ranges.push(checkpoint_seq_range);
    }
    Ok(ranges)
}

/// This function will find missing epoch directories in the input store and return a list of such
/// epoch numbers. If the highest epoch directory in the store is `epoch_N` then it is expected that the
/// store will have all epoch directories from `epoch_0` to `epoch_N`. Additionally, any epoch directory
/// should have the passed in marker file present or else that epoch number is already considered as
/// missing
pub async fn find_missing_epochs_dirs(
    store: &Arc<DynObjectStore>,
    success_marker: &str,
) -> anyhow::Result<Vec<u64>> {
    let remote_checkpoints_by_epoch = find_all_dirs_with_epoch_prefix(store, None).await?;
    let mut dirs: Vec<_> = remote_checkpoints_by_epoch.iter().collect();
    dirs.sort_by_key(|(epoch_num, _path)| *epoch_num);
    let mut candidate_epoch: u64 = 0;
    let mut missing_epochs = Vec::new();
    for (epoch_num, path) in dirs {
        while candidate_epoch < *epoch_num {
            // The whole epoch directory is missing
            missing_epochs.push(candidate_epoch);
            candidate_epoch += 1;
            continue;
        }
        let success_marker = path.child(success_marker);
        let get_result = store.get(&success_marker).await;
        match get_result {
            Err(Error::NotFound { .. }) => {
                error!("No success marker found in db checkpoint for epoch: {epoch_num}");
                missing_epochs.push(*epoch_num);
            }
            Err(_) => {
                // Probably a transient error
                warn!("Failed while trying to read success marker in db checkpoint for epoch: {epoch_num}");
            }
            Ok(_) => {
                // Nothing to do
            }
        }
        candidate_epoch += 1
    }
    missing_epochs.push(candidate_epoch);
    Ok(missing_epochs)
}

pub fn get_path(prefix: &str) -> Path {
    Path::from(prefix)
}

// Snapshot MANIFEST file is very simple. Just a newline delimited list of all paths in the snapshot directory
// this simplicty enables easy parsing for scripts to download snapshots
pub async fn write_snapshot_manifest<S: ObjectStoreListExt + ObjectStorePutExt>(
    dir: &Path,
    store: &S,
    epoch_prefix: String,
) -> Result<()> {
    let mut file_names = vec![];
    let mut paths = store.list_objects(Some(dir)).await;
    while let Some(res) = paths.next().await {
        if let Ok(object_metadata) = res {
            // trim the "epoch_XX/" dir prefix here
            let mut path_str = object_metadata.location.to_string();
            if path_str.starts_with(&epoch_prefix) {
                path_str = String::from(&path_str[epoch_prefix.len()..]);
                file_names.push(path_str);
            } else {
                warn!("{path_str}, should be coming from the files in the {epoch_prefix} dir",)
            }
        } else {
            return Err(res.err().unwrap().into());
        }
    }

    let epoch_manifest = PerEpochManifest::new(file_names);
    let bytes = Bytes::from(epoch_manifest.serialize_as_newline_delimited());
    put(
        store,
        &Path::from(format!("{}/{}", dir, MANIFEST_FILENAME)),
        bytes,
    )
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::object_store::util::{
        copy_recursively, delete_recursively, write_snapshot_manifest, MANIFEST_FILENAME,
    };
    use object_store::path::Path;
    use std::fs;
    use std::num::NonZeroUsize;
    use sui_config::object_storage_config::{ObjectStoreConfig, ObjectStoreType};
    use tempfile::TempDir;

    #[tokio::test]
    pub async fn test_copy_recursively() -> anyhow::Result<()> {
        let input = TempDir::new()?;
        let input_path = input.path();
        let child = input_path.join("child");
        fs::create_dir(&child)?;
        let file1 = child.join("file1");
        fs::write(file1, b"Lorem ipsum")?;
        let grandchild = child.join("grand_child");
        fs::create_dir(&grandchild)?;
        let file2 = grandchild.join("file2");
        fs::write(file2, b"Lorem ipsum")?;

        let output = TempDir::new()?;
        let output_path = output.path();

        let input_store = ObjectStoreConfig {
            object_store: Some(ObjectStoreType::File),
            directory: Some(input_path.to_path_buf()),
            ..Default::default()
        }
        .make()?;

        let output_store = ObjectStoreConfig {
            object_store: Some(ObjectStoreType::File),
            directory: Some(output_path.to_path_buf()),
            ..Default::default()
        }
        .make()?;

        copy_recursively(
            &Path::from("child"),
            &input_store,
            &output_store,
            NonZeroUsize::new(1).unwrap(),
        )
        .await?;

        assert!(output_path.join("child").exists());
        assert!(output_path.join("child").join("file1").exists());
        assert!(output_path.join("child").join("grand_child").exists());
        assert!(output_path
            .join("child")
            .join("grand_child")
            .join("file2")
            .exists());
        let content = fs::read_to_string(output_path.join("child").join("file1"))?;
        assert_eq!(content, "Lorem ipsum");
        let content =
            fs::read_to_string(output_path.join("child").join("grand_child").join("file2"))?;
        assert_eq!(content, "Lorem ipsum");
        Ok(())
    }

    #[tokio::test]
    pub async fn test_write_snapshot_manifest() -> anyhow::Result<()> {
        let input = TempDir::new()?;
        let input_path = input.path();
        let epoch_0 = input_path.join("epoch_0");
        fs::create_dir(&epoch_0)?;
        let file1 = epoch_0.join("file1");
        fs::write(file1, b"Lorem ipsum")?;
        let file2 = epoch_0.join("file2");
        fs::write(file2, b"Lorem ipsum")?;
        let grandchild = epoch_0.join("grand_child");
        fs::create_dir(&grandchild)?;
        let file3 = grandchild.join("file2.tar.gz");
        fs::write(file3, b"Lorem ipsum")?;

        let input_store = ObjectStoreConfig {
            object_store: Some(ObjectStoreType::File),
            directory: Some(input_path.to_path_buf()),
            ..Default::default()
        }
        .make()?;

        write_snapshot_manifest(
            &Path::from("epoch_0"),
            &input_store,
            String::from("epoch_0/"),
        )
        .await?;

        assert!(input_path.join("epoch_0").join(MANIFEST_FILENAME).exists());
        let content = fs::read_to_string(input_path.join("epoch_0").join(MANIFEST_FILENAME))?;
        assert!(content.contains("file2"));
        assert!(content.contains("file1"));
        assert!(content.contains("grand_child/file2.tar.gz"));
        Ok(())
    }

    #[tokio::test]
    pub async fn test_delete_recursively() -> anyhow::Result<()> {
        let input = TempDir::new()?;
        let input_path = input.path();
        let child = input_path.join("child");
        fs::create_dir(&child)?;
        let file1 = child.join("file1");
        fs::write(file1, b"Lorem ipsum")?;
        let grandchild = child.join("grand_child");
        fs::create_dir(&grandchild)?;
        let file2 = grandchild.join("file2");
        fs::write(file2, b"Lorem ipsum")?;

        let input_store = ObjectStoreConfig {
            object_store: Some(ObjectStoreType::File),
            directory: Some(input_path.to_path_buf()),
            ..Default::default()
        }
        .make()?;

        delete_recursively(
            &Path::from("child"),
            &input_store,
            NonZeroUsize::new(1).unwrap(),
        )
        .await?;

        assert!(!input_path.join("child").join("file1").exists());
        assert!(!input_path
            .join("child")
            .join("grand_child")
            .join("file2")
            .exists());
        Ok(())
    }
}
