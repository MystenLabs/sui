// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(unused)]

use std::io::{Cursor, Read, Seek as _, SeekFrom};

use anyhow::{ensure, Context as _};
use fastcrypto::hash::{HashFunction, Sha3_256};
use integer_encoding::VarIntReader as _;
use serde::Deserialize;
use sui_indexer_alt_framework::types::{
    base_types::{ObjectID, SequenceNumber},
    object::Object,
};
use sui_storage::blob::{Blob, BlobEncoding};
use zstd::stream::read::Decoder;

const EPOCH_MANIFEST_MAGIC: u32 = 0x00C0FFEE;
const OBJECT_FILE_MAGIC: u32 = 0x00B7EC75;
const DIGEST_LEN: usize = Sha3_256::OUTPUT_SIZE;

/// The root of the formal snapshot store contains JSON-encoded manifest that mentions all the
/// epochs that have a formal snapshot available.
#[derive(Deserialize)]
pub(super) struct RootManifest {
    available_epochs: Vec<u64>,
}

/// The Epoch Manifest is stored in a custom binary format, starting with a magic number, ending
/// with a digest, and the contents in between is BCS-serialized, with the following structure.
#[derive(Deserialize, Debug)]
pub(super) enum EpochManifest {
    V1(EpochManifestV1),
}

#[derive(Deserialize, Debug)]
pub(super) struct EpochManifestV1 {
    version: u8,
    address_length: u64,
    metadata: Vec<FileMetadata>,
    epoch: u64,
}

#[derive(Deserialize, Debug, Clone)]
pub(super) struct FileMetadata {
    pub file_type: FileType,
    pub bucket: u32,
    pub partition: u32,
    pub compression: FileCompression,
    pub digest: [u8; DIGEST_LEN],
}

#[derive(Deserialize, Debug, Copy, Clone)]
#[repr(u8)]
pub(super) enum FileType {
    Object = 0,
    Reference,
}

#[derive(Deserialize, Debug, Copy, Clone)]
#[repr(u8)]
pub(super) enum FileCompression {
    None = 0,
    Zstd,
}

pub(super) struct LiveObjects {
    pub bucket: u32,
    pub partition: u32,
    pub objects: Vec<Object>,
}

#[derive(Deserialize)]
pub enum LiveObject {
    Normal(Object),
    Wrapped(ObjectKey),
}

#[derive(Deserialize)]
pub struct ObjectKey(pub ObjectID, pub SequenceNumber);

impl RootManifest {
    pub(super) fn read(data: &[u8]) -> anyhow::Result<Self> {
        serde_json::from_slice(data).context("Failed to parse root manifest")
    }

    /// Returns the latest epoch available, or `None` if there are no epochs.
    pub(super) fn latest(&self) -> Option<u64> {
        self.available_epochs.iter().copied().max()
    }

    /// Decides whether the given epoch is present in the manifest or not.
    pub(super) fn contains(&self, epoch: u64) -> bool {
        self.available_epochs.contains(&epoch)
    }
}

impl EpochManifest {
    pub(super) fn read(data: &[u8]) -> anyhow::Result<Self> {
        const MAGIC_LEN: usize = size_of_val(&EPOCH_MANIFEST_MAGIC);

        ensure!(
            data.len() >= MAGIC_LEN + DIGEST_LEN,
            "Epoch manifest too short",
        );

        let mut cursor = Cursor::new(data);

        // Check Magic Number
        let mut magic = [0u8; MAGIC_LEN];
        cursor.read_exact(&mut magic)?;

        ensure!(
            u32::from_be_bytes(magic) == EPOCH_MANIFEST_MAGIC,
            "Not an epoch manifest"
        );

        // Check Digest
        cursor.seek(SeekFrom::End(-(DIGEST_LEN as i64)))?;
        let end = cursor.position() as usize;
        let mut digest = [0u8; DIGEST_LEN];
        cursor.read_exact(&mut digest)?;

        let mut hasher = Sha3_256::new();
        hasher.update(&data[..end]);

        ensure!(
            hasher.finalize().digest == digest,
            "Epoch manifest digest mismatch",
        );

        // Deserialize
        bcs::from_bytes(&data[MAGIC_LEN..end]).context("Failed to deserialize epoch manifest")
    }

    pub(super) fn metadata(&self) -> &[FileMetadata] {
        match self {
            EpochManifest::V1(m) => &m.metadata,
        }
    }
}

impl FileCompression {
    pub(super) fn reader(self, data: &[u8]) -> anyhow::Result<Box<dyn Read + '_>> {
        Ok(match self {
            FileCompression::None => Box::new(Cursor::new(data)),
            FileCompression::Zstd => Box::new(Decoder::new(Cursor::new(data))?),
        })
    }
}

impl LiveObjects {
    pub(super) fn read(bytes: &[u8], metadata: &FileMetadata) -> anyhow::Result<Self> {
        const MAGIC_LEN: usize = size_of_val(&OBJECT_FILE_MAGIC);

        let mut read = metadata.compression.reader(bytes)?;

        let mut magic = [0u8; MAGIC_LEN];
        read.read_exact(&mut magic)?;

        ensure!(
            u32::from_be_bytes(magic) == OBJECT_FILE_MAGIC,
            "Not an object file"
        );

        let mut objects = vec![];
        while let Ok(len) = read.read_varint::<u64>() {
            if len == 0 {
                break;
            }

            let mut e = vec![0u8; 1];
            read.read_exact(&mut e)?;
            let encoding = BlobEncoding::try_from(e[0])
                .with_context(|| format!("Invalid encoding in object file: {}", e[0]))?;

            let mut data = vec![0u8; len as usize];
            read.read_exact(&mut data)?;

            let object = Blob { data, encoding }
                .decode()
                .context("Failed to decode object from blob")?;

            if let LiveObject::Normal(object) = object {
                objects.push(object);
            }
        }

        Ok(Self {
            bucket: metadata.bucket,
            partition: metadata.partition,
            objects,
        })
    }
}
