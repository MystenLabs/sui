// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use bytes::Bytes;
use clap::*;
use num_enum::IntoPrimitive;
use num_enum::TryFromPrimitive;
use std::io::{BufWriter, Cursor, Read, Seek, SeekFrom, Write};
use strum_macros::EnumIter;
pub mod analytics_handler;

pub mod analytics_metrics;
pub mod csv_writer;
pub mod errors;
pub mod tables;
pub mod writer;
use anyhow::anyhow;
use anyhow::Result;
use fastcrypto::hash::{HashFunction, Sha3_256};
use object_store::DynObjectStore;
use serde::{Deserialize, Serialize};
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;
use sui_storage::blob::{Blob, BlobEncoding};
use sui_storage::object_store::util::{get, put};
use sui_storage::SHA3_BYTES;

use object_store::path::Path;
use strum::IntoEnumIterator;
use sui_storage::object_store::ObjectStoreConfig;
use sui_types::base_types::EpochId;

const MAGIC_BYTES: usize = 4;
const MANIFEST_FILE_MAGIC: u32 = 0x0050FFEE;
const MANIFEST_FILENAME: &str = "MANIFEST";
const EPOCH_DIR_PREFIX: &str = "epoch_";
const CHECKPOINT_DIR_PREFIX: &str = "checkpoints";
const OBJECT_DIR_PREFIX: &str = "objects";
const TRANSACTION_DIR_PREFIX: &str = "transactions";
const EVENT_DIR_PREFIX: &str = "events";
const TRANSACTION_OBJECT_DIR_PREFIX: &str = "transaction_objects";
const MOVE_CALL_PREFIX: &str = "move_call";

#[derive(Parser, Clone, Debug)]
#[clap(
    name = "Sui Analytics Indexer",
    about = "Indexer service to upload data for the analytics pipeline.",
    rename_all = "kebab-case"
)]
pub struct AnalyticsIndexerConfig {
    /// The url of the checkpoint client to connect to.
    #[clap(long)]
    pub rest_url: String,
    /// The url of the metrics client to connect to.
    #[clap(long, default_value = "127.0.0.1", global = true)]
    pub client_metric_host: String,
    /// The port of the metrics client to connect to.
    #[clap(long, default_value = "8081", global = true)]
    pub client_metric_port: u16,
    /// Directory to contain the temporary files for checkpoint entries.
    #[clap(long, global = true, default_value = "/tmp")]
    pub checkpoint_dir: PathBuf,
    /// Number of checkpoints to process before uploading to the datastore.
    #[clap(long, default_value = "10000", global = true)]
    pub checkpoint_interval: u64,
    /// Time to process in seconds before uploading to the datastore.
    #[clap(long, default_value = "600", global = true)]
    pub time_interval_s: u64,
    // File format to use when writing files
    #[arg(
        value_enum,
        long = "file-format",
        default_value = "csv",
        ignore_case = true,
        global = true
    )]
    pub file_format: FileFormat,
    // Remote object store where data gets written to
    #[command(flatten)]
    pub remote_store_config: ObjectStoreConfig,
}

#[derive(
    Copy,
    Clone,
    Debug,
    Eq,
    PartialEq,
    Parser,
    strum_macros::Display,
    ValueEnum,
    Serialize,
    Deserialize,
    TryFromPrimitive,
    IntoPrimitive,
    EnumIter,
)]
#[repr(u8)]
pub enum FileFormat {
    CSV = 0,
}

impl FileFormat {
    pub fn file_suffix(&self) -> &str {
        match self {
            FileFormat::CSV => "csv",
        }
    }
}

#[derive(
    Copy,
    Clone,
    Debug,
    Eq,
    PartialEq,
    Serialize,
    Deserialize,
    TryFromPrimitive,
    IntoPrimitive,
    EnumIter,
)]
#[repr(u8)]
pub enum FileType {
    Checkpoint = 0,
    Object,
    Transaction,
    TransactionObjects,
    Event,
    MoveCall,
}

impl FileType {
    pub fn dir_prefix(&self) -> Path {
        match self {
            FileType::Checkpoint => Path::from(CHECKPOINT_DIR_PREFIX),
            FileType::Transaction => Path::from(TRANSACTION_DIR_PREFIX),
            FileType::TransactionObjects => Path::from(TRANSACTION_OBJECT_DIR_PREFIX),
            FileType::Object => Path::from(OBJECT_DIR_PREFIX),
            FileType::Event => Path::from(EVENT_DIR_PREFIX),
            FileType::MoveCall => Path::from(MOVE_CALL_PREFIX),
        }
    }

    pub fn file_path(
        &self,
        file_format: FileFormat,
        epoch_num: EpochId,
        checkpoint_sequence_num: u64,
    ) -> Path {
        self.dir_prefix()
            .child(format!("{}{}", EPOCH_DIR_PREFIX, epoch_num))
            .child(format!(
                "{}.{}",
                checkpoint_sequence_num,
                file_format.file_suffix()
            ))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct FileMetadata {
    pub file_type: FileType,
    pub file_format: FileFormat,
    pub epoch_num: u64,
    pub checkpoint_seq_range: Range<u64>,
}

impl FileMetadata {
    fn new(
        file_type: FileType,
        file_format: FileFormat,
        epoch_num: u64,
        checkpoint_seq_range: Range<u64>,
    ) -> FileMetadata {
        FileMetadata {
            file_type,
            file_format,
            epoch_num,
            checkpoint_seq_range,
        }
    }

    pub fn file_path(&self) -> Path {
        self.file_type.file_path(
            self.file_format,
            self.epoch_num,
            self.checkpoint_seq_range.start,
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct CheckpointUpdates {
    files: Vec<FileMetadata>,
    pub(crate) manifest: Manifest,
}

impl CheckpointUpdates {
    pub fn new(
        epoch_num: u64,
        checkpoint_sequence_number: u64,
        files: Vec<FileMetadata>,
        manifest: &mut Manifest,
    ) -> Self {
        manifest.update(epoch_num, checkpoint_sequence_number, files.clone());
        CheckpointUpdates {
            files,
            manifest: manifest.clone(),
        }
    }

    pub fn new_for_epoch(
        file_format: FileFormat,
        epoch_num: u64,
        checkpoint_range: Range<u64>,
        manifest: &mut Manifest,
    ) -> Self {
        let files: Vec<_> = FileType::iter()
            .map(|f| FileMetadata::new(f, file_format, epoch_num, checkpoint_range.clone()))
            .collect();
        CheckpointUpdates::new(epoch_num, checkpoint_range.end, files, manifest)
    }

    pub fn files(&self) -> Vec<FileMetadata> {
        self.files.clone()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct ManifestV1 {
    pub version: u8,
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
            version: 1,
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
        files: Vec<FileMetadata>,
    ) {
        match self {
            Manifest::V1(manifest) => {
                manifest.file_metadata.extend(files);
                manifest.epoch = epoch_num;
                manifest.next_checkpoint_seq_num = checkpoint_sequence_number;
            }
        }
    }
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
