// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, Context, Result};
use fastcrypto::hash::{HashFunction, Sha3_256};
use std::fs::{File, read_dir};
use std::path::{Path, PathBuf};
use std::{fs, io};
use std::collections::BTreeMap;
use tracing::{debug, info};

pub fn compute_sha3_checksum_for_file(file: &mut File) -> Result<[u8; 32]> {
    let mut hasher = Sha3_256::default();
    io::copy(file, &mut hasher)?;
    Ok(hasher.finalize().digest)
}

pub fn compute_sha3_checksum(source: &Path) -> Result<[u8; 32]> {
    let mut file = fs::File::open(source)?;
    compute_sha3_checksum_for_file(&mut file)
}

pub fn get_snapshots_by_epoch(dir: &Path) -> Result<BTreeMap<u32, PathBuf>> {
    let dirs = read_dir(dir)?;
    let mut snapshots_by_epoch = BTreeMap::new();
    for dir in dirs {
        let entry = dir?;
        let file_name = entry
            .file_name()
            .into_string()
            .map_err(|o| anyhow!("Failed while converting path to string for {:?}", o))?;
        let file_metadata = entry.metadata()?;
        if file_name.starts_with("tmp-epoch-") && file_metadata.is_dir() {
            info!("Deleting tmp snapshot dir: {file_name}");
            fs::remove_dir_all(entry.path())?;
            continue;
        }
        if !file_name.starts_with("epoch-") {
            debug!("Ignoring file in snapshot dir: {file_name}");
            continue;
        }
        if !file_metadata.is_dir() {
            info!("Deleting file as it is not a snapshot dir: {file_name}");
            fs::remove_file(entry.path())?;
            continue;
        }
        let epoch = file_name
            .split_once('-')
            .context("Failed to split dir name")
            .map(|(_, epoch)| epoch.parse::<u32>())??;
        snapshots_by_epoch.insert(epoch, entry.path());
    }
    Ok(snapshots_by_epoch)
}

pub fn get_db_checkpoints_by_epoch(dir: &Path) -> Result<BTreeMap<u32, PathBuf>> {
    let dirs = read_dir(dir)?;
    let mut checkpoints_by_epoch = BTreeMap::new();
    for dir in dirs {
        let entry = dir?;
        let file_name = entry
            .file_name()
            .into_string()
            .map_err(|o| anyhow!("Failed while converting path to string for {:?}", o))?;
        if !file_name.starts_with("epoch-") && !entry.metadata()?.is_dir() {
            debug!("Ignoring path: {file_name}");
            continue;
        }
        let epoch = file_name
            .split_once('-')
            .context("Failed to split dir name")
            .map(|(_, epoch)| epoch.parse::<u32>())??;
        checkpoints_by_epoch.insert(epoch, entry.path());
    }
    Ok(checkpoints_by_epoch)
}
