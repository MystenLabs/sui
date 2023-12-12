// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, Result};
use byteorder::ReadBytesExt;
use integer_encoding::{VarInt, VarIntReader};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::io::{Read, Write};
use std::marker::PhantomData;

pub const MAX_VARINT_LENGTH: usize = 10;
pub const BLOB_ENCODING_BYTES: usize = 1;

#[derive(Copy, Clone, Debug, Eq, PartialEq, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum BlobEncoding {
    Bcs = 1,
}

pub struct Blob {
    pub data: Vec<u8>,
    pub encoding: BlobEncoding,
}

impl Blob {
    pub fn encode<T: Serialize>(value: &T, encoding: BlobEncoding) -> Result<Self> {
        let value_buf = bcs::to_bytes(value)?;
        let (data, encoding) = match encoding {
            BlobEncoding::Bcs => (value_buf, encoding),
        };
        Ok(Blob { data, encoding })
    }
    pub fn decode<T: DeserializeOwned>(self) -> Result<T> {
        let data = match &self.encoding {
            BlobEncoding::Bcs => self.data,
        };
        let res = bcs::from_bytes(&data)?;
        Ok(res)
    }
    pub fn read<R: Read>(rbuf: &mut R) -> Result<Blob> {
        let len = rbuf.read_varint::<u64>()? as usize;
        if len == 0 {
            return Err(anyhow!("Invalid object length of 0 in file"));
        }
        let encoding = rbuf.read_u8()?;
        let mut data = vec![0u8; len];
        rbuf.read_exact(&mut data)?;
        let blob = Blob {
            data,
            encoding: BlobEncoding::try_from(encoding)?,
        };
        Ok(blob)
    }
    pub fn write<W: Write>(&self, wbuf: &mut W) -> Result<usize> {
        let mut buf = [0u8; MAX_VARINT_LENGTH];
        let mut counter = 0;
        let n = (self.data.len() as u64).encode_var(&mut buf);
        wbuf.write_all(&buf[0..n])?;
        counter += n;
        buf[0] = self.encoding.into();
        wbuf.write_all(&buf[0..BLOB_ENCODING_BYTES])?;
        counter += 1;
        wbuf.write_all(&self.data)?;
        counter += self.data.len();
        Ok(counter)
    }
    pub fn size(&self) -> usize {
        let mut blob_size = self.data.len().required_space();
        blob_size += BLOB_ENCODING_BYTES;
        blob_size += self.data.len();
        blob_size
    }
    pub fn to_bytes(&self) -> Vec<u8> {
        [vec![self.encoding.into()], self.data.clone()].concat()
    }
    pub fn from_bytes<T: DeserializeOwned>(bytes: &[u8]) -> Result<T> {
        let (encoding, data) = bytes.split_first().ok_or(anyhow!("empty bytes"))?;
        Blob {
            data: data.to_vec(),
            encoding: BlobEncoding::try_from(*encoding)?,
        }
        .decode()
    }
}

/// An iterator over blobs in a blob file.
pub struct BlobIter<T> {
    reader: Box<dyn Read>,
    _phantom: PhantomData<T>,
}

impl<T: DeserializeOwned> BlobIter<T> {
    pub fn new(reader: Box<dyn Read>) -> Self {
        Self {
            reader,
            _phantom: PhantomData,
        }
    }
    fn next_blob(&mut self) -> Result<T> {
        let blob = Blob::read(&mut self.reader)?;
        blob.decode()
    }
}

impl<T: DeserializeOwned> Iterator for BlobIter<T> {
    type Item = T;
    fn next(&mut self) -> Option<Self::Item> {
        self.next_blob().ok()
    }
}
