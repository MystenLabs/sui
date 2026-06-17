// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! On-disk binary format readers for Sui formal snapshots.
//!
//! Ported essentially verbatim from
//! `sui-indexer-alt-consistent-store::restore::format`. The
//! formats themselves are produced by the validator's snapshot
//! tool and are documented in `sui-storage`. This module only
//! exposes what the formal-snapshot
//! [`RestoreSource`](super::RestoreSource) needs to enumerate
//! files and decode each one's live objects.

use std::io::Cursor;
use std::io::Read;
use std::io::Seek as _;
use std::io::SeekFrom;

use anyhow::Context as _;
use anyhow::ensure;
use fastcrypto::hash::HashFunction;
use fastcrypto::hash::Sha3_256;
use integer_encoding::VarIntReader as _;
use serde::Deserialize;
use sui_storage::blob::Blob;
use sui_storage::blob::BlobEncoding;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::object::Object;
use zstd::stream::read::Decoder;

const EPOCH_MANIFEST_MAGIC: u32 = 0x00C0FFEE;
const OBJECT_FILE_MAGIC: u32 = 0x00B7EC75;
const DIGEST_LEN: usize = Sha3_256::OUTPUT_SIZE;

/// JSON-encoded manifest at the root of a formal snapshot store
/// listing every epoch for which a snapshot is available.
#[derive(Deserialize, Debug)]
pub(super) struct RootManifest {
    available_epochs: Vec<u64>,
}

/// Versioned binary manifest enumerating every file in a single
/// epoch's snapshot.
#[derive(Deserialize, Debug)]
pub(super) enum EpochManifest {
    V1(EpochManifestV1),
}

#[derive(Deserialize, Debug)]
pub(super) struct EpochManifestV1 {
    #[allow(dead_code)]
    version: u8,
    #[allow(dead_code)]
    address_length: u64,
    metadata: Vec<FileMetadata>,
    #[allow(dead_code)]
    epoch: u64,
}

/// One file's identifying metadata as recorded in the
/// [`EpochManifest`].
#[derive(Deserialize, Debug, Clone)]
pub(super) struct FileMetadata {
    pub file_type: FileType,
    pub bucket: u32,
    pub partition: u32,
    pub compression: FileCompression,
    #[allow(dead_code)]
    pub digest: [u8; DIGEST_LEN],
}

#[derive(Deserialize, Debug, Copy, Clone, PartialEq, Eq)]
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

/// Decoded contents of a single `.obj` file. The driver folds
/// these into a [`RestoreChunk`](super::RestoreChunk).
pub(super) struct LiveObjects {
    pub objects: Vec<Object>,
}

#[derive(Deserialize)]
enum LiveObject {
    Normal(Object),
    Wrapped(#[allow(dead_code)] ObjectKey),
}

#[derive(Deserialize)]
struct ObjectKey(
    #[allow(dead_code)] ObjectID,
    #[allow(dead_code)] SequenceNumber,
);

impl RootManifest {
    pub(super) fn read(data: &[u8]) -> anyhow::Result<Self> {
        serde_json::from_slice(data).context("Failed to parse root manifest")
    }

    /// Highest available epoch, or `None` if the manifest is empty.
    pub(super) fn latest(&self) -> Option<u64> {
        self.available_epochs.iter().copied().max()
    }

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

        // Magic.
        let mut magic = [0u8; MAGIC_LEN];
        cursor.read_exact(&mut magic)?;
        ensure!(
            u32::from_be_bytes(magic) == EPOCH_MANIFEST_MAGIC,
            "Not an epoch manifest",
        );

        // Trailing digest.
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

        bcs::from_bytes(&data[MAGIC_LEN..end]).context("Failed to deserialize epoch manifest")
    }

    pub(super) fn metadata(&self) -> &[FileMetadata] {
        match self {
            EpochManifest::V1(m) => &m.metadata,
        }
    }
}

impl FileCompression {
    fn reader(self, data: &[u8]) -> anyhow::Result<Box<dyn Read + '_>> {
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
            "Not an object file",
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

        Ok(Self { objects })
    }
}
