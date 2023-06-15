// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod indexes;

use crate::blob::BlobIter;
use anyhow::{anyhow, Result};
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use bytes::{Buf, Bytes};
pub use indexes::{IndexStore, IndexStoreTables};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufReader, Read, Write};
use std::path::PathBuf;
use std::{fs, io};
use sui_simulator::fastcrypto::hash::{HashFunction, Sha3_256};
use sui_types::messages_checkpoint::{CertifiedCheckpointSummary, VerifiedCheckpoint};
use sui_types::storage::{ReadStore, WriteStore};
use tracing::debug;

pub mod blob;
pub mod mutex_table;
pub mod object_store;
pub mod package_object_cache;
pub mod sharded_lru;
pub mod write_path_pending_tx_log;

pub const SHA3_BYTES: usize = 32;

#[derive(
    Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize, TryFromPrimitive, IntoPrimitive,
)]
#[repr(u8)]
pub enum StorageFormat {
    Blob = 0,
}

#[derive(
    Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize, TryFromPrimitive, IntoPrimitive,
)]
#[repr(u8)]
pub enum FileCompression {
    None = 0,
    Zstd,
}

impl FileCompression {
    pub fn zstd_compress<R: Read, W: Write>(reader: &mut R, writer: &mut W) -> io::Result<()> {
        // TODO: Add zstd compression level as function argument
        let mut encoder = zstd::Encoder::new(writer, 1)?;
        io::copy(reader, &mut encoder)?;
        encoder.finish()?;
        Ok(())
    }
    pub fn compress(&self, source: &std::path::Path) -> io::Result<()> {
        match self {
            FileCompression::Zstd => {
                let mut input = File::open(source)?;
                let tmp_file_name = source.with_extension("tmp");
                let mut output = File::create(&tmp_file_name)?;
                Self::zstd_compress(&mut input, &mut output)?;
                fs::rename(tmp_file_name, source)?;
            }
            FileCompression::None => {}
        }
        Ok(())
    }
    pub fn decompress(&self, source: &PathBuf) -> Result<Box<dyn Read>> {
        let file = File::open(source)?;
        let res: Box<dyn Read> = match self {
            FileCompression::Zstd => Box::new(zstd::stream::Decoder::new(file)?),
            FileCompression::None => Box::new(BufReader::new(file)),
        };
        Ok(res)
    }
    pub fn bytes_decompress(&self, bytes: Bytes) -> Result<Box<dyn Read>> {
        let res: Box<dyn Read> = match self {
            FileCompression::Zstd => Box::new(zstd::stream::Decoder::new(bytes.reader())?),
            FileCompression::None => Box::new(BufReader::new(bytes.reader())),
        };
        Ok(res)
    }
}

pub fn compute_sha3_checksum_for_file(file: &mut File) -> Result<[u8; 32]> {
    let mut hasher = Sha3_256::default();
    io::copy(file, &mut hasher)?;
    Ok(hasher.finalize().digest)
}

pub fn compute_sha3_checksum(source: &std::path::Path) -> Result<[u8; 32]> {
    let mut file = fs::File::open(source)?;
    compute_sha3_checksum_for_file(&mut file)
}

pub fn compress<R: Read, W: Write>(reader: &mut R, writer: &mut W) -> Result<()> {
    let magic = reader.read_u32::<BigEndian>()?;
    writer.write_u32::<BigEndian>(magic)?;
    let storage_format = reader.read_u8()?;
    writer.write_u8(storage_format)?;
    let file_compression = FileCompression::try_from(reader.read_u8()?)?;
    writer.write_u8(file_compression.into())?;
    match file_compression {
        FileCompression::Zstd => {
            FileCompression::zstd_compress(reader, writer)?;
        }
        FileCompression::None => {}
    }
    Ok(())
}

pub fn read<R: Read + 'static>(
    expected_magic: u32,
    mut reader: R,
) -> Result<(Box<dyn Read>, StorageFormat)> {
    let magic = reader.read_u32::<BigEndian>()?;
    if magic != expected_magic {
        Err(anyhow!(
            "Unexpected magic string in file: {:?}, expected: {:?}",
            magic,
            expected_magic
        ))
    } else {
        let storage_format = StorageFormat::try_from(reader.read_u8()?)?;
        let file_compression = FileCompression::try_from(reader.read_u8()?)?;
        let reader: Box<dyn Read> = match file_compression {
            FileCompression::Zstd => Box::new(zstd::stream::Decoder::new(reader)?),
            FileCompression::None => Box::new(BufReader::new(reader)),
        };
        Ok((reader, storage_format))
    }
}

pub fn make_iterator<T: DeserializeOwned, R: Read + 'static>(
    expected_magic: u32,
    reader: R,
) -> Result<impl Iterator<Item = T>> {
    let (reader, storage_format) = read(expected_magic, reader)?;
    match storage_format {
        StorageFormat::Blob => Ok(BlobIter::new(reader)),
    }
}

pub fn verify_checkpoint<S>(
    current: &VerifiedCheckpoint,
    store: S,
    checkpoint: CertifiedCheckpointSummary,
) -> Result<VerifiedCheckpoint, CertifiedCheckpointSummary>
where
    S: WriteStore,
    <S as ReadStore>::Error: std::error::Error,
{
    assert_eq!(
        *checkpoint.sequence_number(),
        current.sequence_number().saturating_add(1)
    );

    if Some(*current.digest()) != checkpoint.previous_digest {
        debug!(
            current_checkpoint_seq = current.sequence_number(),
            current_digest =% current.digest(),
            checkpoint_seq = checkpoint.sequence_number(),
            checkpoint_digest =% checkpoint.digest(),
            checkpoint_previous_digest =? checkpoint.previous_digest,
            "checkpoint not on same chain"
        );
        return Err(checkpoint);
    }

    let current_epoch = current.epoch();
    if checkpoint.epoch() != current_epoch && checkpoint.epoch() != current_epoch.saturating_add(1)
    {
        debug!(
            checkpoint_seq = checkpoint.sequence_number(),
            checkpoint_epoch = checkpoint.epoch(),
            current_checkpoint_seq = current.sequence_number(),
            current_epoch = current_epoch,
            "cannot verify checkpoint with too high of an epoch",
        );
        return Err(checkpoint);
    }

    if checkpoint.epoch() == current_epoch.saturating_add(1)
        && current.next_epoch_committee().is_none()
    {
        debug!(
            checkpoint_seq = checkpoint.sequence_number(),
            checkpoint_epoch = checkpoint.epoch(),
            current_checkpoint_seq = current.sequence_number(),
            current_epoch = current_epoch,
            "next checkpoint claims to be from the next epoch but the latest verified \
            checkpoint does not indicate that it is the last checkpoint of an epoch"
        );
        return Err(checkpoint);
    }

    let committee = store
        .get_committee(checkpoint.epoch())
        .expect("store operation should not fail")
        .unwrap_or_else(|| {
            panic!(
                "BUG: should have committee for epoch {} before we try to verify checkpoint {}",
                checkpoint.epoch(),
                checkpoint.sequence_number()
            )
        });

    checkpoint.verify_signature(&committee).map_err(|e| {
        debug!("error verifying checkpoint: {e}");
        checkpoint.clone()
    })?;
    Ok(VerifiedCheckpoint::new_unchecked(checkpoint))
}
