// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt::Debug;
use std::marker::PhantomData;

use anyhow::anyhow;
use base64ct::Encoding as _;
use fastcrypto::traits::ToFromBytes;
use move_core_types::account_address::AccountAddress;
use schemars::JsonSchema;
use serde;
use serde::de::{Deserializer, Error};
use serde::ser::{Error as SerError, Serializer};
use serde::Deserialize;
use serde::Serialize;
use serde_with::{Bytes, DeserializeAs, SerializeAs};

use crate::base_types::{decode_bytes_hex, encode_bytes_hex};
use crate::crypto::{AuthoritySignature, KeypairTraits};

#[inline]
fn to_custom_error<'de, D, E>(e: E) -> D::Error
where
    E: Debug,
    D: Deserializer<'de>,
{
    D::Error::custom(format!("byte deserialization failed, cause by: {:?}", e))
}

#[inline]
fn to_custom_ser_error<S, E>(e: E) -> S::Error
where
    E: Debug,
    S: Serializer,
{
    S::Error::custom(format!("byte serialization failed, cause by: {:?}", e))
}

/// Use with serde_as to encode/decode bytes to/from Base64/Hex for human-readable serializer and deserializer
/// E : Encoding of the human readable output
/// R : serde_as SerializeAs/DeserializeAs delegation
///
/// # Example:
///
/// #[serde_as]
/// #[derive(Deserialize, Serialize)]
/// struct Example(#[serde_as(as = "Readable(Hex, _)")] [u8; 20]);
///
/// The above example will encode the byte array to Hex string for human-readable serializer
/// and array tuple (default) for non-human-readable serializer.
///
pub struct Readable<E, R> {
    element: PhantomData<R>,
    encoding: PhantomData<E>,
}

impl<T, R, E> SerializeAs<T> for Readable<E, R>
where
    T: AsRef<[u8]>,
    R: SerializeAs<T>,
    E: SerializeAs<T>,
{
    fn serialize_as<S>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if serializer.is_human_readable() {
            E::serialize_as(value, serializer)
        } else {
            R::serialize_as(value, serializer)
        }
    }
}
/// DeserializeAs support for Arrays
impl<'de, R, E, const N: usize> DeserializeAs<'de, [u8; N]> for Readable<E, R>
where
    R: DeserializeAs<'de, [u8; N]>,
    E: DeserializeAs<'de, Vec<u8>>,
{
    fn deserialize_as<D>(deserializer: D) -> Result<[u8; N], D::Error>
    where
        D: Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            let value = E::deserialize_as(deserializer)?;
            if value.len() != N {
                return Err(D::Error::custom(anyhow!(
                    "invalid array length {}, expecting {}",
                    value.len(),
                    N
                )));
            }
            let mut array = [0u8; N];
            array.copy_from_slice(&value[..N]);
            Ok(array)
        } else {
            R::deserialize_as(deserializer)
        }
    }
}
/// DeserializeAs support for Vec
impl<'de, R, E> DeserializeAs<'de, Vec<u8>> for Readable<E, R>
where
    R: DeserializeAs<'de, Vec<u8>>,
    E: DeserializeAs<'de, Vec<u8>>,
{
    fn deserialize_as<D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            E::deserialize_as(deserializer)
        } else {
            R::deserialize_as(deserializer)
        }
    }
}

/// DeserializeAs support for AccountAddress
impl<'de, R, E> DeserializeAs<'de, AccountAddress> for Readable<E, R>
where
    R: DeserializeAs<'de, AccountAddress>,
    E: Encoding,
{
    fn deserialize_as<D>(deserializer: D) -> Result<AccountAddress, D::Error>
    where
        D: Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            let s = String::deserialize(deserializer)?;
            if s.starts_with("0x") {
                AccountAddress::from_hex_literal(&s)
            } else {
                AccountAddress::from_hex(&s)
            }
            .map_err(to_custom_error::<'de, D, _>)
        } else {
            R::deserialize_as(deserializer)
        }
    }
}

pub trait Encoding {
    fn decode(s: &str) -> Result<Vec<u8>, anyhow::Error>;
    fn encode<T: AsRef<[u8]>>(data: T) -> String;
}

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
pub struct Hex(String);
#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq, JsonSchema)]
#[serde(try_from = "String")]
pub struct Base64(String);

impl TryFrom<String> for Base64 {
    type Error = anyhow::Error;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        // Make sure the value is valid base64 string.
        Base64::decode(&value)?;
        Ok(Self(value))
    }
}

impl Base64 {
    pub fn to_vec(&self) -> Result<Vec<u8>, anyhow::Error> {
        Self::decode(&self.0)
    }

    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self(Self::encode(bytes))
    }

    pub fn encoded(&self) -> String {
        self.0.clone()
    }
}

/// Serializes a bitmap according to the roaring bitmap on-disk standard.
/// https://github.com/RoaringBitmap/RoaringFormatSpec
pub struct SuiBitmap;

impl SerializeAs<roaring::RoaringBitmap> for SuiBitmap {
    fn serialize_as<S>(source: &roaring::RoaringBitmap, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut bytes = vec![];

        source
            .serialize_into(&mut bytes)
            .map_err(to_custom_ser_error::<S, _>)?;
        Bytes::serialize_as(&bytes, serializer)
    }
}

impl<'de> DeserializeAs<'de, roaring::RoaringBitmap> for SuiBitmap {
    fn deserialize_as<D>(deserializer: D) -> Result<roaring::RoaringBitmap, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bytes: Vec<u8> = Bytes::deserialize_as(deserializer)?;
        roaring::RoaringBitmap::deserialize_from(&bytes[..]).map_err(to_custom_error::<'de, D, _>)
    }
}

impl Encoding for Hex {
    fn decode(s: &str) -> Result<Vec<u8>, anyhow::Error> {
        decode_bytes_hex(s)
    }

    fn encode<T: AsRef<[u8]>>(data: T) -> String {
        format!("0x{}", encode_bytes_hex(&data).to_lowercase())
    }
}
impl Encoding for Base64 {
    fn decode(s: &str) -> Result<Vec<u8>, anyhow::Error> {
        base64ct::Base64::decode_vec(s).map_err(|e| anyhow!(e))
    }

    fn encode<T: AsRef<[u8]>>(data: T) -> String {
        base64ct::Base64::encode_string(data.as_ref())
    }
}

impl<'de> DeserializeAs<'de, Vec<u8>> for Base64 {
    fn deserialize_as<D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::decode(&s).map_err(to_custom_error::<'de, D, _>)
    }
}

impl<T> SerializeAs<T> for Base64
where
    T: AsRef<[u8]>,
{
    fn serialize_as<S>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        Self::encode(value).serialize(serializer)
    }
}

impl<'de> DeserializeAs<'de, Vec<u8>> for Hex {
    fn deserialize_as<D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::decode(&s).map_err(to_custom_error::<'de, D, _>)
    }
}

impl<T> SerializeAs<T> for Hex
where
    T: AsRef<[u8]>,
{
    fn serialize_as<S>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        Self::encode(value).serialize(serializer)
    }
}

pub struct KeyPairBase64 {}

impl<T> SerializeAs<T> for KeyPairBase64
where
    T: KeypairTraits,
{
    fn serialize_as<S>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        value.encode_base64().serialize(serializer)
    }
}

impl<'de, T> DeserializeAs<'de, T> for KeyPairBase64
where
    T: KeypairTraits,
{
    fn deserialize_as<D>(deserializer: D) -> Result<T, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        T::decode_base64(&s).map_err(to_custom_error::<'de, D, _>)
    }
}

pub struct AuthSignature {}

impl SerializeAs<AuthoritySignature> for AuthSignature {
    fn serialize_as<S>(value: &AuthoritySignature, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        base64ct::Base64::encode_string(value.as_ref()).serialize(serializer)
    }
}

impl<'de> DeserializeAs<'de, AuthoritySignature> for AuthSignature {
    fn deserialize_as<D>(deserializer: D) -> Result<AuthoritySignature, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let sig_bytes = base64ct::Base64::decode_vec(&s).map_err(to_custom_error::<'de, D, _>)?;
        AuthoritySignature::from_bytes(&sig_bytes[..]).map_err(to_custom_error::<'de, D, _>)
    }
}
