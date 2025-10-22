// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(dead_code)]

use crate::blob::BlobIter;
use anyhow::{anyhow, Result};
use byteorder::{BigEndian, ReadBytesExt, WriteBytesExt};
use bytes::{Buf, Bytes};
use fastcrypto::hash::{HashFunction, Sha3_256};
use futures::StreamExt;
use itertools::Itertools;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufReader, Read, Write};
use std::ops::Range;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::{fs, io};
use sui_types::committee::Committee;
use sui_types::messages_checkpoint::{
    CertifiedCheckpointSummary, CheckpointSequenceNumber, VerifiedCheckpoint,
};
use sui_types::storage::WriteStore;
use tracing::debug;

pub mod blob;
pub mod http_key_value_store;
pub mod key_value_store;
pub mod key_value_store_metrics;
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

pub fn compute_sha3_checksum_for_bytes(bytes: Bytes) -> Result<[u8; 32]> {
    let mut hasher = Sha3_256::default();
    io::copy(&mut bytes.reader(), &mut hasher)?;
    Ok(hasher.finalize().digest)
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

pub fn verify_checkpoint_with_committee(
    committee: Arc<Committee>,
    current: &VerifiedCheckpoint,
    checkpoint: CertifiedCheckpointSummary,
) -> Result<VerifiedCheckpoint, Box<CertifiedCheckpointSummary>> {
    assert_eq!(
        *checkpoint.sequence_number(),
        current.sequence_number().checked_add(1).unwrap()
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
        return Err(Box::new(checkpoint));
    }

    let current_epoch = current.epoch();
    if checkpoint.epoch() != current_epoch
        && checkpoint.epoch() != current_epoch.checked_add(1).unwrap()
    {
        debug!(
            checkpoint_seq = checkpoint.sequence_number(),
            checkpoint_epoch = checkpoint.epoch(),
            current_checkpoint_seq = current.sequence_number(),
            current_epoch = current_epoch,
            "cannot verify checkpoint with too high of an epoch",
        );
        return Err(Box::new(checkpoint));
    }

    if checkpoint.epoch() == current_epoch.checked_add(1).unwrap()
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
        return Err(Box::new(checkpoint));
    }

    checkpoint
        .verify_authority_signatures(&committee)
        .map_err(|e| {
            debug!("error verifying checkpoint: {e}");
            checkpoint.clone()
        })?;
    Ok(VerifiedCheckpoint::new_unchecked(checkpoint))
}

pub fn verify_checkpoint<S>(
    current: &VerifiedCheckpoint,
    store: S,
    checkpoint: CertifiedCheckpointSummary,
) -> Result<VerifiedCheckpoint, Box<CertifiedCheckpointSummary>>
where
    S: WriteStore,
{
    let committee = store.get_committee(checkpoint.epoch()).unwrap_or_else(|| {
        panic!(
            "BUG: should have committee for epoch {} before we try to verify checkpoint {}",
            checkpoint.epoch(),
            checkpoint.sequence_number()
        )
    });

    verify_checkpoint_with_committee(committee, current, checkpoint)
}

pub async fn verify_checkpoint_range<S>(
    checkpoint_range: Range<CheckpointSequenceNumber>,
    store: S,
    checkpoint_counter: Arc<AtomicU64>,
    max_concurrency: usize,
) where
    S: WriteStore + Clone,
{
    let range_clone = checkpoint_range.clone();
    futures::stream::iter(range_clone.into_iter().tuple_windows())
        .map(|(a, b)| {
            let current = store
                .get_checkpoint_by_sequence_number(a)
                .unwrap_or_else(|| {
                    panic!(
                        "Checkpoint {} should exist in store after summary sync but does not",
                        a
                    );
                });
            let next = store
                .get_checkpoint_by_sequence_number(b)
                .unwrap_or_else(|| {
                    panic!(
                        "Checkpoint {} should exist in store after summary sync but does not",
                        a
                    );
                });
            let committee = store.get_committee(next.epoch()).unwrap_or_else(|| {
                panic!(
                    "BUG: should have committee for epoch {} before we try to verify checkpoint {}",
                    next.epoch(),
                    next.sequence_number()
                )
            });
            tokio::spawn(async move {
                verify_checkpoint_with_committee(committee, &current, next.clone().into())
                    .expect("Checkpoint verification failed");
            })
        })
        .buffer_unordered(max_concurrency)
        .for_each(|result| {
            result.expect("Checkpoint verification task failed");
            checkpoint_counter.fetch_add(1, Ordering::Relaxed);
            futures::future::ready(())
        })
        .await;
    let last = checkpoint_range
        .last()
        .expect("Received empty checkpoint range");
    let final_checkpoint = store
        .get_checkpoint_by_sequence_number(last)
        .expect("Expected end of checkpoint range to exist in store");
    store
        .update_highest_verified_checkpoint(&final_checkpoint)
        .expect("Failed to update highest verified checkpoint");
}
