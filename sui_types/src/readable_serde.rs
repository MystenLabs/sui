// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use std::fmt::Display;

use base64ct::{Base64, Encoding as _};
use serde;
use serde::de::{Deserialize, Deserializer, Error};
use serde::ser::Serializer;
use serde::Serialize;
use serde_with::{Bytes, DeserializeAs, SerializeAs};

/// Encode bytes to hex for human-readable serializer and deserializer,
/// serde to bytes for non-human-readable serializer and deserializer.
pub trait BytesOrHex<'de>: Sized {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        Self: AsRef<[u8]>,
    {
        serialize_to_bytes_or_encode(serializer, self.as_ref(), Encoding::Hex)
    }
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>;
}

/// Encode bytes to Base64 for human-readable serializer and deserializer,
/// serde to array tuple for non-human-readable serializer and deserializer.
pub trait BytesOrBase64<'de>: Sized {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        Self: AsRef<[u8]>,
    {
        serialize_to_bytes_or_encode(serializer, self.as_ref(), Encoding::Base64)
    }
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>;
}

fn deserialize_from_bytes_or_decode<'de, D>(
    deserializer: D,
    encoding: Encoding,
) -> Result<Vec<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    if deserializer.is_human_readable() {
        let s = String::deserialize(deserializer)?;
        encoding.decode(s).map_err(to_custom_error::<'de, D, _>)
    } else {
        Bytes::deserialize_as(deserializer)
    }
}

fn serialize_to_bytes_or_encode<S>(
    serializer: S,
    data: &[u8],
    encoding: Encoding,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    if serializer.is_human_readable() {
        encoding.encode(data).serialize(serializer)
    } else {
        Bytes::serialize_as(&data, serializer)
    }
}

impl<'de> BytesOrBase64<'de> for ed25519_dalek::Signature {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        Self: AsRef<[u8]>,
    {
        if serializer.is_human_readable() {
            Encoding::Base64.encode(self.as_ref()).serialize(serializer)
        } else {
            <Self as Serialize>::serialize(self, serializer)
        }
    }
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            let s = String::deserialize(deserializer)?;
            let value = Encoding::Base64
                .decode(s)
                .map_err(to_custom_error::<'de, D, _>)?;
            Self::try_from(value.as_slice()).map_err(to_custom_error::<'de, D, _>)
        } else {
            <Self as Deserialize>::deserialize(deserializer)
        }
    }
}

fn to_custom_error<'de, D, E>(e: E) -> D::Error
where
    E: Display,
    D: Deserializer<'de>,
{
    D::Error::custom(format!("byte deserialization failed, cause by: {}", e))
}

impl<'de, const N: usize> BytesOrHex<'de> for [u8; N] {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = deserialize_from_bytes_or_decode(deserializer, Encoding::Hex)?;
        let mut array = [0u8; N];
        array.copy_from_slice(&value[..N]);
        Ok(array)
    }
}

impl<'de, const N: usize> BytesOrBase64<'de> for [u8; N] {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = deserialize_from_bytes_or_decode(deserializer, Encoding::Base64)?;
        let mut array = [0u8; N];
        array.copy_from_slice(&value[..N]);
        Ok(array)
    }
}

enum Encoding {
    Base64,
    Hex,
}

impl Encoding {
    fn decode(&self, s: String) -> Result<Vec<u8>, anyhow::Error> {
        Ok(match self {
            Encoding::Base64 => Base64::decode_vec(&s).map_err(|e| anyhow!(e))?,
            Encoding::Hex => hex::decode(s)?,
        })
    }

    fn encode<T: AsRef<[u8]>>(&self, data: T) -> String {
        match self {
            Encoding::Base64 => Base64::encode_string(data.as_ref()),
            Encoding::Hex => hex::encode(data),
        }
    }
}
