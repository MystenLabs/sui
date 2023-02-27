// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::util::compute_sha3_checksum;
use anyhow::{anyhow, Context, Result};
use byteorder::{BigEndian, ByteOrder, ReadBytesExt};
use fastcrypto::hash::{HashFunction, Sha3_256};
use integer_encoding::VarInt;
use integer_encoding::*;
use num_enum::IntoPrimitive;
use num_enum::TryFromPrimitive;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::fs::{read_dir, File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::{fs, io};
use sui_core::authority::authority_store_tables::AuthorityPerpetualTables;
use sui_types::base_types::{ObjectDigest, ObjectID, ObjectRef, SequenceNumber, TransactionDigest};
use sui_types::object::Object;
use tracing::log::error;
use tracing::{debug, info};
use typed_store::Map;

/// The following describes the format of an object file (*.obj) used for writing live sui objects.
/// The maximum size per .obj file is 128MB. Object snapshot will be taken at the end of every epoch.
/// Sui Snapshot Directory Layout
///  - snapshot/
///     - epoch_0/
///        - 1_1.obj
///        - 1_2.obj
///        - 1_3.obj
///        - 2_1.obj
///        - ...
///        - 1000_1.obj
///        - REFERENCE-1
///        - REFERENCE-2
///        - ...
///        - REFERENCE-1000
///        - MANIFEST
///     - epoch_1/
///       - 1_1.obj
///       - ...
/// Object File Disk Format
///┌──────────────────────────────┐
///│  magic(0x00B7EC75) <4 byte>  │
///├──────────────────────────────┤
///│ ┌──────────────────────────┐ │
///│ │         Object 1         │ │
///│ ├──────────────────────────┤ │
///│ │          ...             │ │
///│ ├──────────────────────────┤ │
///│ │         Object N         │ │
///│ └──────────────────────────┘ │
///└──────────────────────────────┘
/// Object
///┌───────────────┬───────────────────┬──────────────┬───────────────────┐
///│ len <uvarint> │ encoding <1 byte> │ data <bytes> │txdigest<32 bytes> │
///└───────────────┴───────────────────┴──────────────┴───────────────────┘
///
/// REFERENCE File Disk Format
///┌──────────────────────────────┐
///│  magic(0x5EFE5E11) <4 byte>  │
///├──────────────────────────────┤
///│ ┌──────────────────────────┐ │
///│ │         ObjectRef 1      │ │
///│ ├──────────────────────────┤ │
///│ │          ...             │ │
///│ ├──────────────────────────┤ │
///│ │         ObjectRef N      │ │
///│ └──────────────────────────┘ │
///└──────────────────────────────┘
/// ObjectRef (ObjectID, SequenceNumber, ObjectDigest)
///┌───────────────┬───────────────────┬──────────────┐
///│         data (<(address_len + 8 + 32) bytes>)    │
///└───────────────┴───────────────────┴──────────────┘
///
/// MANIFEST File Disk Format
///┌──────────────────────────────┐
///│  magic(0x00C0FFEE) <4 byte>  │
///├──────────────────────────────┤
///│ snapshot version(1) <1 byte> │
///├──────────────────────────────┤
///│    address_len <8 bytes>     │
///├──────────────────────────────┤
///│    epoch <8 byte>            │
///├──────────────────────────────┤
///│checkpoint seq num <8 bytes>  │
///├──────────────────────────────┤
///│     padding(0) <3 byte>      │
///├──────────────────────────────┤
///│ ┌──────────────────────────┐ │
///│ │      FileMetadata 1      │ │
///│ ├──────────────────────────┤ │
///│ │          ...             │ │
///│ ├──────────────────────────┤ │
///│ │      FileMetadata N      │ │
///│ └──────────────────────────┘ │
///├──────────────────────────────┤
///│      sha3 <32 bytes>         │
///└──────────────────────────────┘
/// FileMetadata
///┌───────────────┬───────────────────┬───────────────────┬────────────────────────────┬───────────────────┐
///│ type <1 byte> │ bucket <4 bytes>  │   part <4 byte>   │ compression_type <1 byte>  │ sha3 <32 bytes>   │
///└───────────────┴───────────────────┴───────────────────┴────────────────────────────┴───────────────────┘
const OBJECT_FILE_MAGIC: u32 = 0x00B7EC75;
const REFERENCE_FILE_MAGIC: u32 = 0xDEADBEEF;
const MANIFEST_FILE_MAGIC: u32 = 0x00C0FFEE;
const MAGIC_BYTES: usize = 4;
const SNAPSHOT_VERSION_BYTES: usize = 1;
const ADDRESS_LENGTH_BYTES: usize = 8;
const PADDING_BYTES: usize = 3;
const EPOCH_BYTES: usize = 8;
const CHECKPOINT_SEQ_NUMBER_BYTES: usize = 8;
const MANIFEST_FILE_HEADER_BYTES: usize =
    MAGIC_BYTES + SNAPSHOT_VERSION_BYTES + ADDRESS_LENGTH_BYTES + EPOCH_BYTES + CHECKPOINT_SEQ_NUMBER_BYTES + PADDING_BYTES;
const OBJECT_FILE_MAX_BYTES: usize = 128 * 1024 * 1024 * 1024;
const OBJECT_ENCODING_BYTES: usize = 1;
const TRANSACTION_DIGEST_BYTES: usize = 32;
const OBJECT_ID_BYTES: usize = ObjectID::LENGTH;
const SEQUENCE_NUM_BYTES: usize = 8;
const OBJECT_DIGEST_BYTES: usize = 32;
const OBJECT_REF_BYTES: usize = OBJECT_ID_BYTES + SEQUENCE_NUM_BYTES + OBJECT_DIGEST_BYTES;
const MAX_VARINT_LENGTH: usize = 5;
const FILE_TYPE_BYTES: usize = 1;
const BUCKET_BYTES: usize = 4;
const BUCKET_PARTITION_BYTES: usize = 4;
const COMPRESSION_TYPE_BYTES: usize = 1;
const SHA3_BYTES: usize = 32;
const FILE_METADATA_BYTES: usize =
    FILE_TYPE_BYTES + BUCKET_BYTES + BUCKET_PARTITION_BYTES + COMPRESSION_TYPE_BYTES + SHA3_BYTES;

#[derive(Copy, Clone, Debug, Eq, PartialEq, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum Encoding {
    Bcs = 1,
}

pub struct Blob {
    pub data: Vec<u8>,
    pub encoding: Encoding,
}

impl Blob {
    pub fn encode<T: Serialize>(value: &T, encoding: Encoding) -> Result<Self> {
        let value_buf = bcs::to_bytes(value)?;
        let (data, encoding) = match encoding {
            Encoding::Bcs => (value_buf, encoding),
        };
        Ok(Blob { data, encoding })
    }
    pub fn decode<T: DeserializeOwned>(self) -> Result<T> {
        let data = match &self.encoding {
            Encoding::Bcs => self.data,
        };
        let res = bcs::from_bytes(&data)?;
        Ok(res)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum FileCompression {
    None = 0,
    Zstd,
}

impl FileCompression {
    fn zstd_compress(source: &Path) -> io::Result<()> {
        let mut file = File::open(source)?;
        let tmp_file_name = source.with_extension("obj.tmp");
        let mut encoder = {
            let target = File::create(&tmp_file_name)?;
            // TODO: Add compression level as function argument
            zstd::Encoder::new(target, 1)?
        };
        io::copy(&mut file, &mut encoder)?;
        encoder.finish()?;
        fs::rename(tmp_file_name, source)?;
        Ok(())
    }
    pub fn compress(&self, source: &Path) -> io::Result<()> {
        match self {
            FileCompression::Zstd => {
                Self::zstd_compress(source)?;
            }
            FileCompression::None => {}
        }
        Ok(())
    }
    pub fn stream_decompress(&self, source: &Path) -> Result<Box<dyn Read>> {
        let file = File::open(source)?;
        let res: Box<dyn Read> = match self {
            FileCompression::Zstd => Box::new(zstd::stream::Decoder::new(file)?),
            FileCompression::None => Box::new(BufReader::new(file)),
        };
        Ok(res)
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum FileType {
    Object = 0,
    Reference,
}

#[derive(Clone)]
pub struct FileMetadata {
    pub file_type: FileType,
    pub bucket_num: u32,
    pub part_num: u32,
    pub compression_type: FileCompression,
    pub sha3_digest: [u8; 32],
}

impl FileMetadata {
    fn file_path(&self, dir_path: &Path) -> PathBuf {
        match self.file_type {
            FileType::Object => dir_path.join(format!("{}_{}.obj", self.bucket_num, self.part_num)),
            FileType::Reference => dir_path.join(format!("REFERENCE-{}", self.bucket_num)),
        }
    }
}

struct BucketWriter {
    dir_path: PathBuf,
    bucket_num: u32,
    current_part_num: u32,
    wbuf: BufWriter<File>,
    ref_wbuf: BufWriter<File>,
    n: usize,
}

impl BucketWriter {
    fn new(dir_path: PathBuf, bucket_num: u32) -> Result<Self> {
        let (n, obj_file, part_num) = Self::next_object_file(dir_path.clone(), bucket_num)?;
        let ref_file = Self::ref_file(dir_path.clone(), bucket_num)?;
        Ok(BucketWriter {
            dir_path,
            bucket_num,
            current_part_num: part_num,
            wbuf: BufWriter::new(obj_file),
            ref_wbuf: BufWriter::new(ref_file),
            n,
        })
    }
    fn finalize(&mut self) -> Result<()> {
        self.wbuf.flush()?;
        self.wbuf.get_ref().sync_data()?;
        let off = self.wbuf.get_ref().stream_position()?;
        self.wbuf.get_ref().set_len(off)?;
        Ok(())
    }
    fn finalize_ref(&mut self) -> Result<()> {
        self.ref_wbuf.flush()?;
        self.ref_wbuf.get_ref().sync_data()?;
        let off = self.ref_wbuf.get_ref().stream_position()?;
        self.ref_wbuf.get_ref().set_len(off)?;
        Ok(())
    }
    fn cut(&mut self) -> Result<()> {
        self.finalize()?;
        let (n, f, part_num) = Self::next_object_file(self.dir_path.clone(), self.bucket_num)?;
        self.n = n;
        self.current_part_num = part_num;
        self.wbuf = BufWriter::new(f);
        Ok(())
    }
    fn next_object_file(dir_path: PathBuf, bucket_num: u32) -> Result<(usize, File, u32)> {
        let part_num = Self::next_object_file_number(dir_path.clone(), bucket_num)?;
        let next_part_file_path = dir_path.join(format!("{bucket_num}_{part_num}.obj"));
        let next_part_file_tmp_path = dir_path.join(format!("{bucket_num}_{part_num}.obj.tmp"));
        let mut f = File::create(next_part_file_tmp_path.clone())?;
        let mut metab = [0u8; MAGIC_BYTES];
        BigEndian::write_u32(&mut metab, OBJECT_FILE_MAGIC);
        f.rewind()?;
        let n = f.write(&metab)?;
        drop(f);
        fs::rename(next_part_file_tmp_path, next_part_file_path.clone())?;
        let mut f = OpenOptions::new().append(true).open(next_part_file_path)?;
        f.seek(SeekFrom::Start(n as u64))?;
        Ok((n, f, part_num))
    }
    fn next_object_file_number(dir: PathBuf, bucket: u32) -> Result<u32> {
        let files = read_dir(&dir)?;
        let mut max = 0u32;
        for file_path in files {
            let entry = file_path?;
            let file_name = format!("{:?}", entry.file_name());
            if !file_name.ends_with(".obj") {
                continue;
            }
            let (bucket_num, part_num) = file_name
                .strip_suffix(".obj")
                .context(format!("Invalid object file {file_name} in snapshot dir {}, should be named <bucket_num>_<part_num>.obj", dir.display()))?
                .split_once('_')
                .map(|(b, p)| (b.parse::<u32>(), (p.parse::<u32>())))
                .ok_or(anyhow!("Failed to parse object file name: {file_name} in snapshot dir {}", dir.display()))?;
            if bucket_num? != bucket {
                continue;
            }
            let part_num = part_num?;
            if part_num > max {
                max = part_num;
            }
        }
        Ok(max + 1)
    }
    fn ref_file(dir_path: PathBuf, bucket_num: u32) -> Result<File> {
        let ref_path = dir_path.join(format!("REFERENCE-{bucket_num}"));
        let ref_tmp_path = dir_path.join(format!("REFERENCE-{bucket_num}.tmp"));
        let mut f = File::create(ref_tmp_path.clone())?;
        f.rewind()?;
        let mut metab = [0u8; MAGIC_BYTES];
        BigEndian::write_u32(&mut metab, REFERENCE_FILE_MAGIC);
        let n = f.write(&metab)?;
        drop(f);
        fs::rename(ref_tmp_path, ref_path.clone())?;
        let mut f = OpenOptions::new().append(true).open(ref_path)?;
        f.seek(SeekFrom::Start(n as u64))?;
        Ok(f)
    }
    fn write(&mut self, bytes: &[u8]) -> Result<()> {
        self.wbuf.write_all(bytes)?;
        self.n += bytes.len();
        Ok(())
    }
    fn write_object_blob(&mut self, blob: &Blob, tx_digest: &TransactionDigest) -> Result<()> {
        let mut buf = [0u8; MAX_VARINT_LENGTH];
        let n = (blob.data.len() as u64).encode_var(&mut buf);
        self.write(&buf[0..n])?;
        buf[0] = blob.encoding.into();
        self.write(&buf[0..OBJECT_ENCODING_BYTES])?;
        self.write(&blob.data)?;
        self.write(tx_digest.inner())?;
        Ok(())
    }
    fn write_object(&mut self, object: &Object, tx_digest: &TransactionDigest) -> Result<()> {
        let blob = Blob::encode(object, Encoding::Bcs)?;
        let mut blob_size = blob.data.len().required_space();
        blob_size += OBJECT_ENCODING_BYTES;
        blob_size += blob.data.len();
        blob_size += TRANSACTION_DIGEST_BYTES;
        let cut_new_part_file = (self.n + blob_size) > OBJECT_FILE_MAX_BYTES;
        if cut_new_part_file {
            self.cut()?;
        }
        self.write_object_blob(&blob, tx_digest)?;
        Ok(())
    }
    fn write_ref(&mut self, bytes: &[u8]) -> Result<()> {
        self.ref_wbuf.write_all(bytes)?;
        Ok(())
    }
    fn serialize_object_ref(object_ref: &ObjectRef) -> Result<[u8; OBJECT_REF_BYTES]> {
        let mut buf = [0u8; OBJECT_REF_BYTES];
        buf[0..ObjectID::LENGTH].copy_from_slice(object_ref.0.as_ref());
        BigEndian::write_u64(
            &mut buf[ObjectID::LENGTH..OBJECT_REF_BYTES],
            object_ref.1.value(),
        );
        buf[ObjectID::LENGTH + SEQUENCE_NUM_BYTES..OBJECT_REF_BYTES]
            .copy_from_slice(object_ref.2.as_ref());
        Ok(buf)
    }
    fn write_object_ref(&mut self, object_ref: &ObjectRef) -> Result<()> {
        let serialized_ref = Self::serialize_object_ref(object_ref)?;
        self.write_ref(&serialized_ref)?;
        Ok(())
    }
}

impl Drop for BucketWriter {
    fn drop(&mut self) {
        if self.finalize().is_err() {
            error!(
                "Failed to safely close object file: {}_{}.obj in dir: {}",
                self.bucket_num,
                self.current_part_num,
                self.dir_path.to_string_lossy()
            );
        }
        if self.finalize_ref().is_err() {
            error!(
                "Failed to safely close ref file: REFERENCE-{} in dir: {}",
                self.bucket_num,
                self.dir_path.to_string_lossy()
            );
        }
    }
}

pub struct StateSnapshotWriterV1 {
    dir_path: PathBuf,
    dir: File,
    bucket_writers: HashMap<u32, BucketWriter>,
    compression_type: FileCompression,
    snapshot_version: u8,
}

impl StateSnapshotWriterV1 {
    pub fn new(dir_path: &PathBuf) -> Result<Self> {
        if dir_path.exists() {
            return Err(anyhow!(
                "State snapshot dir already exists at {:?}",
                &dir_path
            ));
        }
        fs::create_dir_all(dir_path.clone())?;
        let dir = File::open(dir_path)?;
        Ok(StateSnapshotWriterV1 {
            dir_path: dir_path.clone(),
            dir,
            bucket_writers: HashMap::new(),
            compression_type: FileCompression::Zstd,
            snapshot_version: 1,
        })
    }
    pub fn write_objects(
        mut self,
        objects: impl Iterator<Item = ObjectRef>,
        perpetual_db: &AuthorityPerpetualTables,
        checkpoint_seq_number: u64,
    ) -> Result<()> {
        let epoch = perpetual_db.get_recovery_epoch_at_restart()?;
        let num_objects_written =
            self.write_object_with_bucket_func(objects, perpetual_db, Self::bucket_func)?;
        debug!(
            "Wrote a total of {} objects in snapshot for epoch: {}",
            num_objects_written,
            epoch
        );
        self.finalize(epoch, checkpoint_seq_number)?;
        Ok(())
    }
    fn bucket_func(_object_ref: &ObjectRef) -> u32 {
        // TODO: Get the right bucketing function
        1u32
    }
    pub(crate) fn write_object_with_bucket_func<F>(
        &mut self,
        objects: impl Iterator<Item = ObjectRef>,
        perpetual_db: &AuthorityPerpetualTables,
        bucket_func: F,
    ) -> Result<u64>
    where
        F: Fn(&ObjectRef) -> u32,
    {
        let mut counter = 0u64;
        for object_ref in objects {
            let bucket_num = bucket_func(&object_ref);
            let object = perpetual_db.get_object_by_ref(&object_ref)?.ok_or(anyhow!(
                "No matching object for ref: {:?} in db",
                &object_ref
            ))?;
            let tx_digest = perpetual_db.parent_sync.get(&object_ref)?.ok_or(anyhow!(
                "No transaction digest for ref: {:?} in db",
                &object_ref
            ))?;
            let bucket_writer = self.get_bucket_writer(bucket_num)?;
            bucket_writer.write_object_ref(&object_ref)?;
            bucket_writer.write_object(&object, &tx_digest)?;
            counter += 1;
        }
        Ok(counter)
    }
    pub(crate) fn finalize(mut self, epoch: u64, checkpoint_seq_number: u64) -> Result<()> {
        for (_i, writer) in self.bucket_writers.iter_mut() {
            writer.finalize()?;
            writer.finalize_ref()?;
            self.dir.sync_data()?;
        }
        self.write_manifest(epoch, checkpoint_seq_number)?;
        Ok(())
    }
    fn write_manifest(mut self, epoch: u64, checkpoint_seq_number: u64) -> Result<()> {
        let (f, manifest_file_path) = self.manifest_file(epoch, checkpoint_seq_number)?;
        let mut wbuf = BufWriter::new(f);
        let files = read_dir(self.dir_path.clone())?;
        for file_path in files {
            let entry = file_path?;
            let file_name = entry
                .file_name()
                .into_string()
                .map_err(|o| anyhow!("Failed while converting path to string for {:?}", o))?;
            if !file_name.ends_with(".obj") && !file_name.starts_with("REFERENCE-") {
                continue;
            }
            if file_name.ends_with(".obj") {
                self.compression_type.compress(&entry.path())?;
                let sha3_digest = compute_sha3_checksum(&entry.path())?;
                let (bucket_num, part_num) = file_name
                    .strip_suffix(".obj")
                    .context(format!("Invalid object file: {file_name} in snapshot dir"))?
                    .split_once('_')
                    .map(|(b, p)| (b.parse::<u32>(), (p.parse::<u32>())))
                    .ok_or(anyhow!("Failed to parse object file name: {file_name}"))?;
                let bucket_num = bucket_num?;
                let part_num = part_num?;
                let file_metadata = FileMetadata {
                    file_type: FileType::Object,
                    bucket_num,
                    part_num,
                    compression_type: self.compression_type,
                    sha3_digest,
                };
                self.write_file_metadata(&mut wbuf, &file_metadata)?;
            }
            if file_name.starts_with("REFERENCE-") {
                self.compression_type.compress(&entry.path())?;
                let bucket_num = file_name
                    .strip_prefix("REFERENCE-")
                    .context(format!(
                        "Invalid REFERENCE file: {file_name} in snapshot dir"
                    ))?
                    .parse::<u32>()?;
                let sha3_digest = compute_sha3_checksum(&entry.path())?;
                let file_metadata = FileMetadata {
                    file_type: FileType::Reference,
                    bucket_num,
                    part_num: 0,
                    compression_type: self.compression_type,
                    sha3_digest,
                };
                self.write_file_metadata(&mut wbuf, &file_metadata)?;
            }
        }
        wbuf.flush()?;
        wbuf.get_ref().sync_data()?;
        let sha3_digest = compute_sha3_checksum(&manifest_file_path)?;
        wbuf.write_all(&sha3_digest)?;
        wbuf.flush()?;
        wbuf.get_ref().sync_data()?;
        let off = wbuf.get_ref().stream_position()?;
        wbuf.get_ref().set_len(off)?;
        self.dir.sync_data()?;
        Ok(())
    }
    fn manifest_file(&mut self, epoch: u64, checkpoint_seq_number: u64) -> Result<(File, PathBuf)> {
        let manifest_file_path = self.dir_path.join("MANIFEST");
        let manifest_file_tmp_path = self.dir_path.join("MANIFEST.tmp");
        let mut f = File::create(manifest_file_tmp_path.clone())?;
        let mut metab = [0u8; MANIFEST_FILE_HEADER_BYTES];
        BigEndian::write_u32(&mut metab, MANIFEST_FILE_MAGIC);
        metab[MAGIC_BYTES] = self.snapshot_version;
        BigEndian::write_u64(
            &mut metab[MAGIC_BYTES + SNAPSHOT_VERSION_BYTES..MANIFEST_FILE_HEADER_BYTES],
            ObjectID::LENGTH as u64,
        );
        BigEndian::write_u64(
            &mut metab[MAGIC_BYTES + SNAPSHOT_VERSION_BYTES..MANIFEST_FILE_HEADER_BYTES],
            ObjectID::LENGTH as u64,
        );
        BigEndian::write_u64(
            &mut metab[MAGIC_BYTES + SNAPSHOT_VERSION_BYTES + ADDRESS_LENGTH_BYTES..MANIFEST_FILE_HEADER_BYTES],
            epoch,
        );
        BigEndian::write_u64(
            &mut metab[MAGIC_BYTES + SNAPSHOT_VERSION_BYTES + ADDRESS_LENGTH_BYTES + EPOCH_BYTES..MANIFEST_FILE_HEADER_BYTES],
            checkpoint_seq_number,
        );
        f.rewind()?;
        f.write_all(&metab)?;
        drop(f);
        fs::rename(manifest_file_tmp_path, manifest_file_path.clone())?;
        self.dir.sync_data()?;
        let mut f = OpenOptions::new()
            .append(true)
            .open(manifest_file_path.clone())?;
        f.seek(SeekFrom::Start(MANIFEST_FILE_HEADER_BYTES as u64))?;
        Ok((f, manifest_file_path))
    }
    fn write_file_metadata(
        &mut self,
        wbuf: &mut BufWriter<File>,
        file_metadata: &FileMetadata,
    ) -> Result<()> {
        let mut buf = [0u8; FILE_METADATA_BYTES - SHA3_BYTES];
        buf[0] = file_metadata.file_type.into();
        BigEndian::write_u32(
            &mut buf[FILE_TYPE_BYTES..FILE_METADATA_BYTES - SHA3_BYTES],
            file_metadata.bucket_num,
        );
        BigEndian::write_u32(
            &mut buf[FILE_TYPE_BYTES + BUCKET_BYTES..FILE_METADATA_BYTES - SHA3_BYTES],
            file_metadata.part_num,
        );
        buf[FILE_TYPE_BYTES + BUCKET_BYTES + BUCKET_PARTITION_BYTES] =
            file_metadata.compression_type.into();
        wbuf.write_all(&buf)?;
        wbuf.write_all(&file_metadata.sha3_digest)?;
        Ok(())
    }
    fn get_bucket_writer(&mut self, bucket_num: u32) -> Result<&mut BucketWriter> {
        if let std::collections::hash_map::Entry::Vacant(e) = self.bucket_writers.entry(bucket_num)
        {
            e.insert(BucketWriter::new(self.dir_path.clone(), bucket_num)?);
        }
        self.bucket_writers.get_mut(&bucket_num).ok_or(anyhow!(
            "Unexpected missing bucket writer for bucket: {bucket_num}"
        ))
    }
}

pub enum StateSnapshotReader {
    StateSnapshotReaderV1(StateSnapshotReaderV1),
}

impl StateSnapshotReader {
    pub fn new(dir_path: &Path) -> Result<Self> {
        let manifest_path = dir_path.join("MANIFEST");
        if !manifest_path.exists() {
            return Err(anyhow!(
                "Manifest file doesn't exist in snapshot dir: {}",
                &dir_path.display()
            ));
        }
        let manifest = File::open(manifest_path.clone())?;
        let manifest_file_size = manifest.metadata()?.len() as usize;
        let mut manifest_reader = BufReader::new(manifest);
        manifest_reader.rewind()?;
        let magic = manifest_reader.read_u32::<BigEndian>()?;
        if magic != MANIFEST_FILE_MAGIC {
            return Err(anyhow!(
                "Unexpected magic byte: {} in manifest file: {}",
                magic,
                &manifest_path.display(),
            ));
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
                "Corrupted manifest file: {} as file checksum: {:?} doesn't match with checksum in the file: {:?}",
                &manifest_path.display(),
                computed_digest,
                sha3_digest,
            ));
        }
        manifest_reader.seek(SeekFrom::Start(MAGIC_BYTES as u64))?;
        let snapshot_version = manifest_reader.read_u8()?;
        if snapshot_version == 1 {
            info!(
                "Creating snapshot reader v1 for snapshot at path: {:?}",
                dir_path.display()
            );
            Ok(StateSnapshotReader::StateSnapshotReaderV1(
                StateSnapshotReaderV1::new(dir_path)?,
            ))
        } else {
            Err(anyhow!(
                "No reader for snapshot version: {snapshot_version} is available"
            ))
        }
    }
    pub fn epoch(&self) -> u64 {
        match self {
            StateSnapshotReader::StateSnapshotReaderV1(reader) => reader.epoch,
        }
    }
    pub fn checkpoint_seq_number(&self) -> u64 {
        match self {
            StateSnapshotReader::StateSnapshotReaderV1(reader) => reader.checkpoint_seq_number,
        }
    }
    pub fn version(&self) -> u64 {
        match self {
            StateSnapshotReader::StateSnapshotReaderV1(_) => 1,
        }
    }
}

pub struct StateSnapshotReaderV1 {
    dir_path: PathBuf,
    object_files: BTreeMap<u32, BTreeMap<u32, FileMetadata>>,
    ref_files: BTreeMap<u32, FileMetadata>,
    pub(crate) epoch: u64,
    checkpoint_seq_number: u64,
}

impl StateSnapshotReaderV1 {
    fn new(dir_path: &Path) -> Result<Self> {
        let manifest_path = dir_path.join("MANIFEST");
        if !manifest_path.exists() {
            return Err(anyhow!(
                "Manifest file doesn't exist in snapshot dir: {}",
                &dir_path.display()
            ));
        }
        let manifest = File::open(manifest_path.clone())?;
        let manifest_file_size = manifest.metadata()?.len() as usize;
        let mut manifest_reader = BufReader::new(manifest);
        manifest_reader.rewind()?;
        manifest_reader.seek(SeekFrom::Start(MAGIC_BYTES as u64))?;
        let snapshot_version = manifest_reader.read_u8()?;
        if snapshot_version != 1 {
            return Err(anyhow!(
                "Unexpected snapshot version: {} in manifest file: {}",
                snapshot_version,
                &manifest_path.display(),
            ));
        }
        let address_len = manifest_reader.read_u64::<BigEndian>()? as usize;
        if address_len > ObjectID::LENGTH {
            return Err(anyhow!(
                "Object address length in snapshot is: {} but max possible address length is: {}",
                &address_len,
                ObjectID::LENGTH
            ));
        }
        let epoch = manifest_reader.read_u64::<BigEndian>()?;
        let checkpoint_seq_number = manifest_reader.read_u64::<BigEndian>()?;
        let mut object_files = BTreeMap::new();
        let mut ref_files = BTreeMap::new();
        manifest_reader.seek(SeekFrom::Start(MANIFEST_FILE_HEADER_BYTES as u64))?;
        let mut offset = manifest_reader.stream_position()? as usize;
        while offset < (manifest_file_size - SHA3_BYTES) {
            let file_metadata = Self::read_file_metadata(&mut manifest_reader)?;
            let file_path = file_metadata.file_path(dir_path);
            {
                let mut f = File::open(&file_path)?;
                let mut hasher = Sha3_256::default();
                io::copy(&mut f, &mut hasher)?;
                let computed_digest = hasher.finalize().digest;
                if computed_digest != file_metadata.sha3_digest {
                    return Err(anyhow!(
                        "Checksum mismatch for snapshot file: {}, computed digest: {:?}, stored digest: {:?}",
                        &file_path.display(),
                        computed_digest,
                        file_metadata.sha3_digest
                    ));
                }
            }
            match file_metadata.file_type {
                FileType::Object => {
                    let entry = object_files
                        .entry(file_metadata.bucket_num)
                        .or_insert_with(BTreeMap::new);
                    entry.insert(file_metadata.part_num, file_metadata);
                }
                FileType::Reference => {
                    ref_files.insert(file_metadata.bucket_num, file_metadata);
                }
            }
            offset += FILE_METADATA_BYTES;
        }
        Ok(StateSnapshotReaderV1 {
            dir_path: dir_path.to_path_buf(),
            object_files,
            ref_files,
            epoch,
            checkpoint_seq_number
        })
    }
    fn read_file_metadata(reader: &mut BufReader<File>) -> Result<FileMetadata> {
        let file_type = FileType::try_from(reader.read_u8()?)?;
        let bucket_num = reader.read_u32::<BigEndian>()?;
        let part_num = reader.read_u32::<BigEndian>()?;
        let compression_type = FileCompression::try_from(reader.read_u8()?)?;
        let mut sha3_digest = [0u8; SHA3_BYTES];
        reader.read_exact(&mut sha3_digest)?;
        Ok(FileMetadata {
            file_type,
            bucket_num,
            part_num,
            compression_type,
            sha3_digest,
        })
    }
    pub fn buckets(&self) -> Result<Vec<u32>> {
        Ok(self.ref_files.keys().copied().collect())
    }
    pub fn num_parts_for_bucket(&self, bucket_num: u32) -> Result<u32> {
        Ok(self
            .object_files
            .get(&bucket_num)
            .context(format!(
                "No object files found for bucket: {bucket_num} in snapshot dir: {}",
                self.dir_path.display()
            ))?
            .len() as u32)
    }
    pub fn ref_iter(&mut self, bucket_num: u32) -> Result<impl Iterator<Item = ObjectRef>> {
        let file_metadata = self.ref_files.get(&bucket_num).context(format!(
            "No ref files found for bucket: {bucket_num} in snapshot dir: {}",
            self.dir_path.display()
        ))?;
        ObjectRefIter::new(file_metadata, &self.dir_path)
    }
    pub fn obj_iter(
        &mut self,
        bucket_num: u32,
        part_num: u32,
    ) -> Result<impl Iterator<Item = (Object, TransactionDigest)>> {
        let file_metadata = self
            .object_files
            .get(&bucket_num)
            .context(format!("Bucket: {bucket_num} doesn't exist in snapshot"))?
            .get(&part_num)
            .context("Partition: {part_num} doesn't exist in snapshot!")?;
        ObjectIter::new(file_metadata, &self.dir_path)
    }
    pub fn safe_obj_iter(
        &mut self,
        bucket_num: u32,
    ) -> Result<impl Iterator<Item = (Object, TransactionDigest)>> {
        let ref_file = self.ref_files.get(&bucket_num).context(format!(
            "No ref files found for bucket: {bucket_num} in snapshot dir: {}",
            self.dir_path.display()
        ))?;
        let obj_files = self
            .object_files
            .get(&bucket_num)
            .context(format!("Bucket: {bucket_num} doesn't exist in snapshot"))?;
        SafeObjectIter::new(ref_file, obj_files, &self.dir_path)
    }
}

/// An iterator over all object refs in a reference file.
pub struct ObjectRefIter {
    reader: Box<dyn Read>,
}

impl ObjectRefIter {
    pub fn new(file_metadata: &FileMetadata, dir_path: &Path) -> Result<Self> {
        let file_path = file_metadata.file_path(dir_path);
        let mut reader = file_metadata
            .compression_type
            .stream_decompress(&file_path)?;
        let magic = reader.read_u32::<BigEndian>()?;
        if magic != REFERENCE_FILE_MAGIC {
            Err(anyhow!(
                "Unexpected magic string in REFERENCE file: {:?}",
                magic
            ))
        } else {
            Ok(ObjectRefIter { reader })
        }
    }
    fn next_ref(&mut self) -> Result<ObjectRef> {
        let mut object_id = [0u8; ObjectID::LENGTH];
        self.reader.read_exact(&mut object_id)?;
        let sequence_number = self.reader.read_u64::<BigEndian>()?;
        let mut sha3_digest = [0u8; 32];
        self.reader.read_exact(&mut sha3_digest)?;
        Ok((
            ObjectID::new(object_id),
            SequenceNumber::from_u64(sequence_number),
            ObjectDigest::new(sha3_digest),
        ))
    }
}

impl Iterator for ObjectRefIter {
    type Item = ObjectRef;
    fn next(&mut self) -> Option<Self::Item> {
        self.next_ref().ok()
    }
}

/// An iterator over all objects in an objects file.
pub struct ObjectIter {
    reader: Box<dyn Read>,
}

impl ObjectIter {
    pub fn new(file_metadata: &FileMetadata, dir_path: &Path) -> Result<Self> {
        let file_path = file_metadata.file_path(dir_path);
        let mut reader = file_metadata
            .compression_type
            .stream_decompress(&file_path)?;
        let magic = reader.read_u32::<BigEndian>()?;
        if magic != OBJECT_FILE_MAGIC {
            Err(anyhow!(
                "Unexpected magic string in object file: {:?}",
                magic
            ))
        } else {
            Ok(ObjectIter { reader })
        }
    }
    fn next_object(&mut self) -> Result<(Object, TransactionDigest)> {
        let len = self.reader.read_varint::<u64>()? as usize;
        if len == 0 {
            return Err(anyhow!("Invalid object length of 0 in file"));
        }
        let encoding = self.reader.read_u8()?;
        let mut data = vec![0u8; len];
        self.reader.read_exact(&mut data)?;
        let blob = Blob {
            data,
            encoding: Encoding::try_from(encoding)?,
        };
        let mut sha3_digest = [0u8; 32];
        self.reader.read_exact(&mut sha3_digest)?;
        Ok((blob.decode()?, TransactionDigest::new(sha3_digest)))
    }
}

impl Iterator for ObjectIter {
    type Item = (Object, TransactionDigest);
    fn next(&mut self) -> Option<Self::Item> {
        self.next_object().ok()
    }
}

pub struct SafeObjectIter {
    ref_iter: ObjectRefIter,
    obj_iter: ObjectIter,
    obj_files: BTreeMap<u32, FileMetadata>,
    part_num: u32,
    dir_path: PathBuf,
    encountered_error: bool,
}

impl SafeObjectIter {
    pub fn new(
        ref_file: &FileMetadata,
        obj_files: &BTreeMap<u32, FileMetadata>,
        dir_path: &Path,
    ) -> Result<Self> {
        let ref_iter = ObjectRefIter::new(ref_file, dir_path)?;
        let (part_num, obj_file) = obj_files.first_key_value().ok_or(anyhow!(
            "No object files available in snapshot dir: {}",
            dir_path.display()
        ))?;
        let obj_iter = ObjectIter::new(obj_file, dir_path)?;
        Ok(SafeObjectIter {
            ref_iter,
            obj_iter,
            obj_files: obj_files.clone(),
            part_num: *part_num,
            dir_path: dir_path.to_path_buf(),
            encountered_error: false,
        })
    }
    pub fn encountered_error(&self) -> bool {
        // Invoke after iterating to check if we encountered any errors
        self.encountered_error
    }
    fn next_ref(&mut self) -> Result<ObjectRef> {
        self.ref_iter.next_ref()
    }
    fn next_object(&mut self) -> Result<(Object, TransactionDigest)> {
        let (obj, tx_digest) = match self.obj_iter.next_object() {
            Ok(res) => res,
            Err(_) => {
                let next_part_num = self.part_num + 1;
                let obj_file = self.obj_files.get(&next_part_num).ok_or(anyhow!(
                    "No object file for partition: {next_part_num} available in snapshot dir: {}",
                    self.dir_path.display()
                ))?;
                self.part_num = next_part_num;
                self.obj_iter = ObjectIter::new(obj_file, &self.dir_path)?;
                self.next_object()?
            }
        };
        Ok((obj, tx_digest))
    }
}

impl Iterator for SafeObjectIter {
    type Item = (Object, TransactionDigest);
    fn next(&mut self) -> Option<Self::Item> {
        let next_ref = self.next_ref().ok();
        if next_ref.is_none() {
            return None;
        }
        let next_ref = next_ref.unwrap();
        let next_object = self.next_object();
        if let Err(e) = next_object {
            self.encountered_error = true;
            error!("Failed to get next object with error: {:?}", e);
            return None;
        }
        let (object, tx_digest) = next_object.unwrap();
        if next_ref != object.compute_object_reference() {
            self.encountered_error = true;
            error!(
                "Object ref in ref file: {:?} doesn't match with one in object file: {:?}",
                next_ref,
                object.compute_object_reference()
            );
            return None;
        }
        Some((object, tx_digest))
    }
}
