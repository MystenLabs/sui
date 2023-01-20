// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, Context, Result};
use byteorder::{BigEndian, ByteOrder, ReadBytesExt};
use crc::{Crc, CRC_32_ISCSI};
use integer_encoding::VarInt;
use integer_encoding::*;
use num_enum::IntoPrimitive;
use num_enum::TryFromPrimitive;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::fs::{read_dir, File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::{fs, mem};
use tracing::info;
use tracing::log::error;

/// The following describes the format of a blob file, which is created in the `transactions/`,
/// `/effects` or `/objects` directory of an archive. The maximum size per file is 32MB. Blobs (
/// transactions, effects or objects) in the files are referenced from the index by uint64 composed
/// of in-file offset (lower 4 bytes) and file sequence number (upper 4 bytes). Blobs are written
/// out to local disk and shipped to remote storage optionally. A checkpoint tailer will subscribe
/// to and continuously archive transactions and effects as blobs. For state snapshot, objects will
/// be archived at the end of every epoch by scanning the checkpointed objects db table. One
/// checkpoint directory could have data for multiple checkpoints in case of transactions and effects.
/// For objects, a snapshot would be taken at the end of epoch and stored under the end of epoch
/// checkpoint.
/// Sui Archive Directory Layout
///  - archive/
///     - transactions/
///       - checkpoint_000001/
///         - 000001.blob
///         - 000002.blob
///         - 000003.blob
///         - ...
///         - 020000.blob
///         - index
///       - checkpoint_000100/
///         - 000001.blob
///     - effects/
///       - checkpoint_000001/
///         - 000001.blob
///         - 000002.blob
///         - ...
///         - index
///       - checkpoint_000050/
///         - 000001.blob
///         - 000002.blob
///         - ....
///         - index
///     - objects/
///       - checkpoint_100000/
///         - 000001.blob
///         - 000002.blob
///         - ...
///         - index
///       - checkpoint_200000/
///         - 000001.blob
/// Blob File Disk Format
///┌──────────────────────────────┐
///│  magic(0xCAFE500D) <4 byte>  │
///├──────────────────────────────┤
///│    version(1) <1 byte>       │
///├──────────────────────────────┤
///│    padding(0) <3 byte>       │
///├──────────────────────────────┤
///│ ┌──────────────────────────┐ │
///│ │         Blob 1           │ │
///│ ├──────────────────────────┤ │
///│ │          ...             │ │
///│ ├──────────────────────────┤ │
///│ │         Blob N           │ │
///│ └──────────────────────────┘ │
///└──────────────────────────────┘
/// Blob
///┌───────────────┬───────────────────┬──────────────┬────────────────┐
///│ len <uvarint> │ encoding <1 byte> │ data <bytes> │ CRC32 <4 byte> │
///└───────────────┴───────────────────┴──────────────┴────────────────┘
const BLOB_FILE_MAGIC: u32 = 0xCAFE500D;
const BLOB_FILE_MAGIC_SIZE: usize = mem::size_of::<u32>();
const BLOB_FILE_FORMAT_VERSION: u8 = 1;
const BLOB_FILE_FORMAT_VERSION_SIZE: usize = mem::size_of::<u8>();
const BLOB_FILE_HEADER_PADDING_SIZE: usize = 3;
const BLOB_FILE_HEADER_SIZE: usize =
    BLOB_FILE_MAGIC_SIZE + BLOB_FILE_FORMAT_VERSION_SIZE + BLOB_FILE_HEADER_PADDING_SIZE;
const MAX_BLOB_FILE_SIZE: u64 = 32 * 1024 * 1024 * 1024;

const MAX_BLOB_DATA_VARINT_LENGTH: usize = 5;
const BLOB_ENCODING_SIZE: usize = 1;
const BLOB_CHECKSUM_SIZE: usize = mem::size_of::<u32>();

#[derive(Copy, Clone, Debug, Eq, PartialEq, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum BlobEncoding {
    Binary = 1,
    Zstd,
}

pub struct Blob {
    pub data: Vec<u8>,
    pub blob_encoding: BlobEncoding,
}

impl Blob {
    pub fn encode<T: Serialize>(value: &T, encoding: BlobEncoding) -> Result<Self> {
        let value_buf = bincode::serialize(value)?;
        let (data, blob_encoding) = match encoding {
            BlobEncoding::Binary => (value_buf, encoding),
            BlobEncoding::Zstd => {
                let mut encoder = zstd::stream::write::Encoder::new(Vec::new(), 0)?;
                match encoder
                    .write_all(&value_buf)
                    .and_then(|()| encoder.finish())
                {
                    Ok(value_buf) => (value_buf, encoding),
                    Err(_) => (value_buf, BlobEncoding::Binary),
                }
            }
        };
        Ok(Blob {
            data,
            blob_encoding,
        })
    }
    pub fn decode<T: DeserializeOwned>(self) -> Result<T> {
        let data = match &self.blob_encoding {
            BlobEncoding::Binary => Ok(self.data),
            BlobEncoding::Zstd => {
                let mut data = vec![];
                zstd::stream::read::Decoder::new(self.data.as_slice())
                    .and_then(|mut reader| reader.read_to_end(&mut data))
                    .map(|_| data)
                    .map_err(anyhow::Error::from)
            }
        }?;
        let res = bincode::deserialize(&data)?;
        Ok(res)
    }
}

pub struct BlobWriter {
    dir_path: PathBuf,
    dir: File,
    blob_file_seq: u64,
    wbuf: BufWriter<File>,
    n: usize,
}

impl BlobWriter {
    pub fn new(dir_path: PathBuf) -> Result<Self> {
        fs::create_dir_all(dir_path.clone())?;
        let dir = File::open(&dir_path)?;
        let (n, f, seq) = Self::get_next_blob_file(
            &dir,
            dir_path.clone(),
            BLOB_FILE_MAGIC,
            BLOB_FILE_FORMAT_VERSION,
        )?;
        Ok(BlobWriter {
            dir_path,
            dir,
            blob_file_seq: seq,
            wbuf: BufWriter::new(f),
            n,
        })
    }
    fn finalize(&mut self) -> Result<()> {
        self.wbuf.flush()?;
        self.wbuf.get_ref().sync_data()?;
        let off = self.wbuf.get_ref().seek(SeekFrom::Current(0))?;
        self.wbuf.get_ref().set_len(off)?;
        Ok(())
    }
    fn cut(&mut self) -> Result<()> {
        self.finalize()?;
        // open new blob file
        let (n, f, seq) = Self::get_next_blob_file(
            &self.dir,
            self.dir_path.clone(),
            BLOB_FILE_MAGIC,
            BLOB_FILE_FORMAT_VERSION,
        )?;
        self.n = n;
        self.blob_file_seq = seq;
        self.wbuf = BufWriter::new(f);
        Ok(())
    }
    fn get_next_blob_file(
        dir: &File,
        dir_path: PathBuf,
        magic_number: u32,
        blob_format: u8,
    ) -> Result<(usize, File, u64)> {
        let seq = Self::next_sequence_file_number(dir_path.clone())?;
        let next_seq_path = dir_path.join(format!("{}.blob", seq));
        let next_seq_tmp_path = dir_path.join(format!("{}.blob.tmp", seq));
        let mut f = fs::File::create(next_seq_tmp_path.clone())?;
        let mut metab = [0u8; BLOB_FILE_HEADER_SIZE];
        BigEndian::write_u32(&mut metab, magic_number);
        metab[BLOB_FILE_MAGIC_SIZE] = blob_format;
        f.seek(SeekFrom::Start(0))?;
        let n = f.write(&metab)?;
        mem::drop(f);
        fs::rename(next_seq_tmp_path, next_seq_path.clone())?;
        dir.sync_data()?;
        let mut f = OpenOptions::new().append(true).open(next_seq_path)?;
        f.seek(SeekFrom::Start(n as u64))?;
        Ok((n, f, seq))
    }
    fn write(&mut self, bytes: &[u8]) -> Result<()> {
        let n = self.wbuf.write(bytes)?;
        self.n += n;
        Ok(())
    }
    pub fn write_blobs(&mut self, blobs: Vec<Blob>) -> Result<()> {
        // Write as many blobs as possible to the current blob file, cuts a new blob file when the current one
        // is full and writes rest of the blobs in the new one and so on and so forth
        let mut batch_size = 0usize;
        let mut first_batch = true;
        let mut batches = vec![];
        let mut batch_id = 0;
        batches.push(vec![]);
        for (i, blob) in blobs.into_iter().enumerate() {
            let mut blob_size = MAX_BLOB_DATA_VARINT_LENGTH;
            blob_size += BLOB_ENCODING_SIZE;
            blob_size += blob.data.len();
            blob_size += BLOB_CHECKSUM_SIZE;
            batch_size += blob_size;
            // Cut a new batch when it is not the first blob and
            // the batch is too large to fit in the current file.
            let mut cut_new_batch =
                (i != 0) && (batch_size + BLOB_FILE_HEADER_SIZE > MAX_BLOB_FILE_SIZE as usize);
            if first_batch && self.n > BLOB_FILE_HEADER_SIZE {
                cut_new_batch = batch_size + self.n > MAX_BLOB_FILE_SIZE as usize;
                if cut_new_batch {
                    first_batch = false;
                }
            }
            if cut_new_batch {
                batches.push(vec![]);
                batch_id += 1;
                batch_size = blob_size;
            }
            batches[batch_id].push(blob);
        }
        let num_batches = batches.len();
        for (i, batch) in batches.into_iter().enumerate() {
            self.do_write_blobs(batch)?;
            // Cut a new file only when there are more blobs to write.
            // Avoid creating a new empty file at the end of the write.
            if i < num_batches - 1 {
                self.cut()?;
            }
        }
        Ok(())
    }
    fn do_write_blobs(&mut self, blobs: Vec<Blob>) -> Result<()> {
        if blobs.is_empty() {
            return Ok(());
        }
        for (_i, blob) in blobs.into_iter().enumerate() {
            let mut buf = [0u8; MAX_BLOB_DATA_VARINT_LENGTH];
            let n = (blob.data.len() as u64).encode_var(&mut buf);
            self.write(&buf[0..n])?;
            buf[0] = blob.blob_encoding.into();
            self.write(&buf[0..BLOB_ENCODING_SIZE])?;
            self.write(&blob.data)?;
            let crc32 = Crc::<u32>::new(&CRC_32_ISCSI);
            let mut digest = crc32.digest();
            digest.update(&buf[0..BLOB_ENCODING_SIZE]);
            digest.update(&blob.data);
            BigEndian::write_u32(&mut buf, digest.finalize());
            self.write(&buf[0..BLOB_CHECKSUM_SIZE])?;
        }
        Ok(())
    }
    fn next_sequence_file_number(dir: PathBuf) -> Result<u64> {
        let files = read_dir(dir)?;
        let mut max = 0u64;
        for file_path in files {
            let entry = file_path?;
            let file_name = format!("{:?}", entry.file_name());
            if !file_name.ends_with(".blob") {
                continue;
            }
            let seq = file_name
                .strip_suffix(".blob")
                .context("Invalid blob file in blob dir")?
                .parse::<u64>()?;
            if seq > max {
                max = seq;
            }
        }
        Ok(max + 1)
    }
}

impl Drop for BlobWriter {
    fn drop(&mut self) {
        if self.finalize().is_err() {
            error!(
                "Failed to safely close blob file: {}.blob in dir: {}",
                self.blob_file_seq,
                self.dir_path.to_string_lossy()
            );
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd)]
pub struct BlobFileRef(u64);

#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd)]
pub struct BlobRef(BlobFileRef);

impl BlobFileRef {
    fn new(file_index: u32, file_offset: u32) -> Self {
        let num = ((file_index as u64) << 32) | (file_offset as u64);
        BlobFileRef(num)
    }
    fn parse(self) -> (u32, u32) {
        let file_index = (self.0 >> 32) as u32;
        let file_offset = (self.0 << 32) >> 32;
        (file_index, file_offset as u32)
    }
    fn next(self, size: usize) -> Self {
        let file_index = (self.0 >> 32) as u32;
        let file_offset = (self.0 << 32) >> 32;
        let next_file_offset = file_offset + size as u64;
        let num = ((file_index as u64) << 32) | next_file_offset;
        BlobFileRef(num)
    }
}

pub struct BlobReader {
    dir_path: PathBuf,
    blob_files: HashMap<u64, BufReader<File>>,
}

impl BlobReader {
    pub fn new(dir_path: PathBuf) -> Result<Self> {
        let file_paths = read_dir(&dir_path)?;
        let mut files: HashMap<u64, BufReader<File>> = HashMap::new();
        for file_path in file_paths {
            let entry = file_path?;
            let path = entry.path();
            let file_name: String = entry
                .file_name()
                .into_string()
                .map_err(|_| anyhow!("Failed to get file name"))?;
            if !file_name.ends_with(".blob") {
                info!(
                    "Ignoring file {} in blob dir: {}",
                    file_name,
                    &dir_path.display()
                );
                continue;
            }
            let seq = file_name
                .strip_suffix(".blob")
                .with_context(|| {
                    format!(
                        "Invalid file: {} in blob dir: {}",
                        file_name,
                        dir_path.display()
                    )
                })?
                .parse::<u64>()?;
            let f = File::open(path.clone())?;
            let mut reader = BufReader::new(f);
            reader.seek(SeekFrom::Start(0))?;
            let magic = reader.read_u32::<BigEndian>()?;
            if magic != BLOB_FILE_MAGIC {
                return Err(anyhow!(
                    "Unexpected blob file magic byte: {} in file: {}, dir: {}",
                    magic,
                    file_name,
                    &dir_path.display()
                ));
            }
            files.insert(seq, reader);
        }
        Ok(BlobReader {
            dir_path,
            blob_files: files,
        })
    }
    pub fn next_ref(&self, blob_ref: BlobRef, size: usize) -> Result<Option<BlobRef>> {
        let next = blob_ref.0.next(size);
        let (file_index, blob_offset) = next.parse();
        let reader = self.blob_files.get(&(file_index as u64)).with_context(|| {
            format!(
                "Blob file with index: {} doesn't exist in dir: {}",
                file_index,
                self.dir_path.display()
            )
        })?;
        let file_size = reader.get_ref().metadata()?.len();
        if (blob_offset as u64) < file_size {
            Ok(Some(BlobRef(next)))
        } else {
            let next_file_index = file_index + 1;
            if self.blob_files.contains_key(&(next_file_index as u64)) {
                Ok(Some(BlobRef(BlobFileRef::new(
                    next_file_index,
                    BLOB_FILE_HEADER_SIZE as u32,
                ))))
            } else {
                Ok(None)
            }
        }
    }
    pub fn blob(&mut self, blob_ref: BlobRef) -> Result<(Blob, usize)> {
        let (file_index, blob_offset) = blob_ref.0.parse();
        let reader = self
            .blob_files
            .get_mut(&(file_index as u64))
            .with_context(|| {
                format!(
                    "Blob file with index: {} doesn't exist in dir: {}",
                    file_index,
                    self.dir_path.display()
                )
            })?;
        reader.seek(SeekFrom::Start(blob_offset as u64))?;
        let blob_len = reader.read_varint::<u64>()? as usize;
        if blob_len == 0 {
            return Err(anyhow!(
                "Invalid blob length of 0 in file with seq: {} at offset: {}",
                file_index,
                blob_offset
            ));
        }
        let encoding = reader.read_u8()?;
        let mut blob_data = vec![0u8; blob_len];
        if reader.read(&mut blob_data)? != blob_len {
            return Err(anyhow!(
                "Not enough data in file with seq: {} at offset: {}",
                file_index,
                blob_offset
            ));
        }
        let persisted_checksum = reader.read_u32::<BigEndian>()?;
        let crc32 = Crc::<u32>::new(&CRC_32_ISCSI);
        let mut digest = crc32.digest();
        digest.update(&[encoding; BLOB_ENCODING_SIZE]);
        digest.update(&blob_data);
        if digest.finalize() != persisted_checksum {
            return Err(anyhow!(
                "Checksum mismatch in file with seq: {} at offset: {}",
                file_index,
                blob_offset
            ));
        }
        let bytes_read =
            blob_len.required_space() + BLOB_ENCODING_SIZE + blob_len + BLOB_CHECKSUM_SIZE;
        Ok((
            Blob {
                data: blob_data,
                blob_encoding: BlobEncoding::try_from(encoding)?,
            },
            bytes_read,
        ))
    }
}

/// An iterator over all blobs in a blob directory.
pub struct BlobIter {
    reader: BlobReader,
    next: Option<BlobRef>,
}

impl BlobIter {
    pub fn new(dir_path: PathBuf) -> Result<Self> {
        Ok(BlobIter {
            reader: BlobReader::new(dir_path)?,
            // start from the first file and first blob
            next: Some(BlobRef(BlobFileRef::new(1, BLOB_FILE_HEADER_SIZE as u32))),
        })
    }
    // TODO: Add seek() method to seek to a blob with user provided blob_ref
}

impl Iterator for BlobIter {
    type Item = Blob;
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(next) = self.next {
            let (blob, num_bytes) = self.reader.blob(next).expect("Failed to read blob");
            self.next = self
                .reader
                .next_ref(next, num_bytes)
                .expect("Failed to get next blob ref");
            Some(blob)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::blob::{Blob, BlobEncoding, BlobIter, BlobWriter};
    use anyhow::anyhow;
    use std::path::PathBuf;
    use sui_types::base_types::{ObjectDigest, ObjectID, ObjectRef, SequenceNumber};
    use sui_types::crypto::{get_key_pair, AccountKeyPair};
    use sui_types::messages::{
        SingleTransactionKind, TransactionData, TransactionKind, TransferSui,
    };
    use tempdir::TempDir;

    #[test]
    fn test_blob_read_write() -> Result<(), anyhow::Error> {
        let (sender, _sender_key): (_, AccountKeyPair) = get_key_pair();
        let gas_object: ObjectRef = (
            ObjectID::ZERO,
            SequenceNumber::from_u64(0),
            ObjectDigest::MIN,
        );
        let transaction = TransactionData::new_with_dummy_gas_price(
            TransactionKind::Single(SingleTransactionKind::TransferSui(TransferSui {
                recipient: Default::default(),
                amount: None,
            })),
            sender,
            gas_object,
            10000,
        );
        let archive = TempDir::new("archive")?;
        let transactions = TempDir::new_in(archive, "transactions")?;
        let mut transaction_writer = BlobWriter::new(PathBuf::from(transactions.path()))?;
        transaction_writer.write_blobs(vec![Blob::encode::<TransactionData>(
            &transaction,
            BlobEncoding::Binary,
        )?])?;
        transaction_writer.finalize()?;
        let mut transaction_iter = BlobIter::new(PathBuf::from(transactions.path()))?;
        let persisted_blob = transaction_iter
            .next()
            .ok_or_else(|| anyhow!("Failed to read the written transaction"))?;
        let persisted_transaction: TransactionData = persisted_blob.decode()?;
        assert_eq!(transaction, persisted_transaction);
        Ok(())
    }
}
