// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt;
use std::marker::PhantomData;

use ed25519_dalek::Signature;
use serde;
use serde::de::{Deserialize, Deserializer, Error, SeqAccess, Visitor};
use serde::ser::{SerializeTuple, Serializer};
use serde::Serialize;

/// Encode bytes to Hex for human readable serializer and deserializer,
/// serde to array tuple for non human readable serializer and deserializer.
pub trait BytesOrHex<'de>: Sized {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        Self: AsRef<[u8]>,
    {
        if serializer.is_human_readable() {
            hex::encode(self).serialize(serializer)
        } else {
            let arr = self.as_ref();
            let mut seq = serializer.serialize_tuple(arr.len())?;
            for elem in arr {
                seq.serialize_element(elem)?;
            }
            seq.end()
        }
    }
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>;
}

macro_rules! byte_or_hex {
    ($($len:expr,)+) => {
        $(
            impl<'de> BytesOrHex<'de> for [u8; $len]{
                fn deserialize<D>(deserializer: D) -> Result<[u8; $len], D::Error>
                    where D: Deserializer<'de>
                {
                    if deserializer.is_human_readable(){
                        let s = String::deserialize(deserializer)?;
                        let value = hex::decode(s).map_err(|_| D::Error::custom("byte deserialization failed"))?;
                        let mut array = [0u8; $len];
                        array.copy_from_slice(&value[..$len]);
                        Ok(array)
                    }else{
                        let visitor = ArrayVisitor::<u8, $len> { element: PhantomData };
                        deserializer.deserialize_tuple($len, visitor)
                    }
                }
            }
        )+
    }
}

/// Encode bytes to Base64 for human readable serializer and deserializer,
/// serde to array tuple for non human readable serializer and deserializer.
pub trait BytesOrBase64<'de>: Sized {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        Self: AsRef<[u8]>,
    {
        if serializer.is_human_readable() {
            base64::encode(self).serialize(serializer)
        } else {
            let arr = self.as_ref();
            let mut seq = serializer.serialize_tuple(arr.len())?;
            for elem in arr {
                seq.serialize_element(elem)?;
            }
            seq.end()
        }
    }
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>;
}

macro_rules! byte_or_base64 {
    ($($len:expr,)+) => {
        $(
            impl<'de> BytesOrBase64<'de> for [u8; $len]{
                fn deserialize<D>(deserializer: D) -> Result<[u8; $len], D::Error>
                    where D: Deserializer<'de>
                {
                    if deserializer.is_human_readable(){
                        let s = String::deserialize(deserializer)?;
                        let value = base64::decode(s).map_err(|_| D::Error::custom("byte deserialization failed"))?;
                        let mut array = [0u8; $len];
                        array.copy_from_slice(&value[..$len]);
                        Ok(array)
                    }else{
                        let visitor = ArrayVisitor::<u8, $len> { element: PhantomData };
                        deserializer.deserialize_tuple($len, visitor)
                    }
                }
            }
        )+
    }
}

struct ArrayVisitor<T, const N: usize> {
    element: PhantomData<T>,
}

impl<'de, const N: usize> Visitor<'de> for ArrayVisitor<u8, N> {
    type Value = [u8; N];

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&format!("an array of length {}", N))
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<[u8; N], A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut arr = [0u8; N];
        let mut i = 0usize;
        while let Ok(Some(value)) = seq.next_element() {
            arr[i] = value;
            i += 1;
        }
        Ok(arr)
    }
}

impl<'de> BytesOrBase64<'de> for ed25519_dalek::Signature {
    fn deserialize<D>(deserializer: D) -> Result<ed25519_dalek::Signature, D::Error>
    where
        D: Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            let s = String::deserialize(deserializer)?;
            let bytes =
                base64::decode(s).map_err(|_| D::Error::custom("byte deserialization failed"))?;
            let sig = ed25519_dalek::Signature::try_from(bytes.as_slice())
                .map_err(|_| D::Error::custom("byte deserialization failed"))?;
            Ok(sig)
        } else {
            let visitor = ArrayVisitor::<u8, { Signature::BYTE_SIZE }> {
                element: PhantomData,
            };
            let bytes = deserializer.deserialize_tuple(Signature::BYTE_SIZE, visitor)?;
            let sig = ed25519_dalek::Signature::try_from(bytes)
                .map_err(|_| D::Error::custom("byte deserialization failed"))?;
            Ok(sig)
        }
    }
}

// TODO: can we further simplify this?
byte_or_hex! { 20, }
byte_or_base64! { 32, 96, }
