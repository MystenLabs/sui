// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]

pub mod reader;
pub mod writer;

#[cfg(test)]
mod tests;

use anyhow::{anyhow, Result};
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use bytes::Bytes;
use fastcrypto::hash::{HashFunction, Sha3_256};
use num_enum::IntoPrimitive;
use num_enum::TryFromPrimitive;
use object_store::path::Path;
use object_store::DynObjectStore;
use serde::{Deserialize, Serialize};
use std::io::{BufWriter, Cursor, Read, Seek, SeekFrom, Write};
use std::ops::Range;
use std::sync::Arc;
use sui_storage::blob::{Blob, BlobEncoding};
use sui_storage::object_store::util::{get, put};
use sui_storage::{compute_sha3_checksum, SHA3_BYTES};

/// Checkpoints and summaries are persisted as blob files. Files are committed to local store
/// by duration or file size. Committed files are synced with the remote store continuously. Files are
/// optionally compressed with the zstd compression format. Filenames follow the format
/// <checkpoint_seq_num>.<suffix> where `checkpoint_seq_num` is the first checkpoint present in that
/// file. MANIFEST is the index and source of truth for all files present in the archive.
///
/// State Archival Directory Layout
///  - archive/
///     - MANIFEST
///     - epoch_0/
///        - 0.chk
///        - 0.sum
///        - 1000.chk
///        - 1000.sum
///        - 3000.chk
///        - 3000.sum
///        - ...
///        - 100000.chk
///        - 100000.sum
///     - epoch_1/
///        - 101000.chk
///        - ...
/// Blob File Disk Format
///┌──────────────────────────────┐
///│       magic <4 byte>         │
///├──────────────────────────────┤
///│  storage format <1 byte>     │
// ├──────────────────────────────┤
///│    file compression <1 byte> │
// ├──────────────────────────────┤
///│ ┌──────────────────────────┐ │
///│ │         Blob 1           │ │
///│ ├──────────────────────────┤ │
///│ │          ...             │ │
///│ ├──────────────────────────┤ │
///│ │        Blob N            │ │
///│ └──────────────────────────┘ │
///└──────────────────────────────┘
/// Blob
///┌───────────────┬───────────────────┬──────────────┐
///│ len <uvarint> │ encoding <1 byte> │ data <bytes> │
///└───────────────┴───────────────────┴──────────────┘
///
/// MANIFEST File Disk Format
///┌──────────────────────────────┐
///│        magic<4 byte>         │
///├──────────────────────────────┤
///│   serialized manifest        │
///├──────────────────────────────┤
///│      sha3 <32 bytes>         │
///└──────────────────────────────┘
const CHECKPOINT_FILE_MAGIC: u32 = 0x0000DEAD;
const SUMMARY_FILE_MAGIC: u32 = 0x0000CAFE;
const MANIFEST_FILE_MAGIC: u32 = 0x00C0FFEE;
const MAGIC_BYTES: usize = 4;
const CHECKPOINT_FILE_SUFFIX: &str = "chk";
const SUMMARY_FILE_SUFFIX: &str = "sum";
const EPOCH_DIR_PREFIX: &str = "epoch_";
const MANIFEST_FILENAME: &str = "MANIFEST";

#[derive(
    Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize, TryFromPrimitive, IntoPrimitive,
)]
#[repr(u8)]
pub enum FileType {
    CheckpointContent = 0,
    CheckpointSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct FileMetadata {
    pub file_type: FileType,
    pub epoch_num: u64,
    pub checkpoint_seq_range: Range<u64>,
    pub sha3_digest: [u8; 32],
}

impl FileMetadata {
    pub fn file_path(&self) -> Path {
        let dir_path = Path::from(format!("{}{}", EPOCH_DIR_PREFIX, self.epoch_num));
        match self.file_type {
            FileType::CheckpointContent => dir_path.child(&*format!(
                "{}.{CHECKPOINT_FILE_SUFFIX}",
                self.checkpoint_seq_range.start
            )),
            FileType::CheckpointSummary => dir_path.child(&*format!(
                "{}.{SUMMARY_FILE_SUFFIX}",
                self.checkpoint_seq_range.start
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct ManifestV1 {
    pub archive_version: u8,
    pub next_checkpoint_seq_num: u64,
    pub file_metadata: Vec<FileMetadata>,
    pub epoch: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub enum Manifest {
    V1(ManifestV1),
}

impl Manifest {
    pub fn new(epoch: u64, next_checkpoint_seq_num: u64) -> Self {
        Manifest::V1(ManifestV1 {
            archive_version: 1,
            next_checkpoint_seq_num,
            file_metadata: vec![],
            epoch,
        })
    }
    pub fn files(&self) -> Vec<FileMetadata> {
        match self {
            Manifest::V1(manifest) => manifest.file_metadata.clone(),
        }
    }
    pub fn epoch_num(&self) -> u64 {
        match self {
            Manifest::V1(manifest) => manifest.epoch,
        }
    }
    pub fn next_checkpoint_seq_num(&self) -> u64 {
        match self {
            Manifest::V1(manifest) => manifest.next_checkpoint_seq_num,
        }
    }
    pub fn update(
        &mut self,
        epoch_num: u64,
        checkpoint_sequence_number: u64,
        checkpoint_file_metadata: FileMetadata,
        summary_file_metadata: FileMetadata,
    ) {
        match self {
            Manifest::V1(manifest) => {
                manifest
                    .file_metadata
                    .extend(vec![checkpoint_file_metadata, summary_file_metadata]);
                manifest.epoch = epoch_num;
                manifest.next_checkpoint_seq_num = checkpoint_sequence_number;
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct CheckpointUpdates {
    checkpoint_file_metadata: FileMetadata,
    summary_file_metadata: FileMetadata,
    manifest: Manifest,
}

impl CheckpointUpdates {
    pub fn new(
        epoch_num: u64,
        checkpoint_sequence_number: u64,
        checkpoint_file_metadata: FileMetadata,
        summary_file_metadata: FileMetadata,
        manifest: &mut Manifest,
    ) -> Self {
        manifest.update(
            epoch_num,
            checkpoint_sequence_number,
            checkpoint_file_metadata.clone(),
            summary_file_metadata.clone(),
        );
        CheckpointUpdates {
            checkpoint_file_metadata,
            summary_file_metadata,
            manifest: manifest.clone(),
        }
    }
    pub fn content_file_path(&self) -> Path {
        self.checkpoint_file_metadata.file_path()
    }
    pub fn summary_file_path(&self) -> Path {
        self.summary_file_metadata.file_path()
    }
    pub fn manifest_file_path(&self) -> Path {
        Path::from(MANIFEST_FILENAME)
    }
}

pub fn create_file_metadata(
    file_path: &std::path::Path,
    file_type: FileType,
    epoch_num: u64,
    checkpoint_seq_range: Range<u64>,
) -> Result<FileMetadata> {
    let sha3_digest = compute_sha3_checksum(file_path)?;
    let file_metadata = FileMetadata {
        file_type,
        epoch_num,
        checkpoint_seq_range,
        sha3_digest,
    };
    Ok(file_metadata)
}

pub async fn read_manifest(remote_store: Arc<DynObjectStore>) -> Result<Manifest> {
    let manifest_file_path = Path::from(MANIFEST_FILENAME);
    let vec = get(&manifest_file_path, remote_store).await?.to_vec();
    let manifest_file_size = vec.len();
    let mut manifest_reader = Cursor::new(vec);
    manifest_reader.rewind()?;
    let magic = manifest_reader.read_u32::<BigEndian>()?;
    if magic != MANIFEST_FILE_MAGIC {
        return Err(anyhow!("Unexpected magic byte in manifest: {}", magic));
    }
    manifest_reader.seek(SeekFrom::End(-(SHA3_BYTES as i64)))?;
    let mut sha3_digest = [0u8; SHA3_BYTES];
    manifest_reader.read_exact(&mut sha3_digest)?;
    manifest_reader.rewind()?;
    let mut content_buf = vec![0u8; manifest_file_size - SHA3_BYTES];
    manifest_reader.read_exact(&mut content_buf)?;
    let mut hasher = Sha3_256::default();
    hasher.update(&content_buf);
    let computed_digest = hasher.finalize().digest;
    if computed_digest != sha3_digest {
        return Err(anyhow!(
            "Manifest corrupted, computed checksum: {:?}, stored checksum: {:?}",
            computed_digest,
            sha3_digest
        ));
    }
    manifest_reader.rewind()?;
    manifest_reader.seek(SeekFrom::Start(MAGIC_BYTES as u64))?;
    Blob::read(&mut manifest_reader)?.decode()
}

pub async fn write_manifest(manifest: Manifest, remote_store: Arc<DynObjectStore>) -> Result<()> {
    let path = Path::from(MANIFEST_FILENAME);
    let mut buf = BufWriter::new(vec![]);
    buf.write_u32::<BigEndian>(MANIFEST_FILE_MAGIC)?;
    let blob = Blob::encode(&manifest, BlobEncoding::Bcs)?;
    blob.write(&mut buf)?;
    buf.flush()?;
    let mut hasher = Sha3_256::default();
    hasher.update(buf.get_ref());
    let computed_digest = hasher.finalize().digest;
    buf.write_all(&computed_digest)?;
    let bytes = Bytes::from(buf.into_inner()?);
    put(&path, bytes, remote_store).await?;
    Ok(())
}
