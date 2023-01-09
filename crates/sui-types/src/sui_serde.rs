// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt::Debug;
use std::marker::PhantomData;

use anyhow::anyhow;
use fastcrypto::encoding::{Base64, Encoding};
use fastcrypto::traits::ToFromBytes;
use move_core_types::account_address::AccountAddress;
use serde;
use serde::de::{Deserializer, Error};
use serde::ser::{Error as SerError, Serializer};
use serde::Deserialize;
use serde::Serialize;
use serde_with::{Bytes, DeserializeAs, SerializeAs};

use crate::crypto::{AggregateAuthoritySignature, AuthoritySignature, KeypairTraits};

#[inline]
fn to_custom_error<'de, D, E>(e: E) -> D::Error
where
    E: Debug,
    D: Deserializer<'de>,
{
    Error::custom(format!("byte deserialization failed, cause by: {:?}", e))
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
/// ```text
/// #[serde_as]
/// #[derive(Deserialize, Serialize)]
/// struct Example(#[serde_as(as = "Readable(Hex, _)")] [u8; 20]);
/// ```
///
/// The above example will encode the byte array to Hex string for human-readable serializer
/// and array tuple (default) for non-human-readable serializer.
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
                return Err(Error::custom(anyhow!(
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

/// Serializes a bitmap according to the roaring bitmap on-disk standard.
/// <https://github.com/RoaringBitmap/RoaringFormatSpec>
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
        Base64::encode(value.as_ref()).serialize(serializer)
    }
}

impl<'de> DeserializeAs<'de, AuthoritySignature> for AuthSignature {
    fn deserialize_as<D>(deserializer: D) -> Result<AuthoritySignature, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let sig_bytes = Base64::decode(&s).map_err(to_custom_error::<'de, D, _>)?;
        AuthoritySignature::from_bytes(&sig_bytes[..]).map_err(to_custom_error::<'de, D, _>)
    }
}

pub struct AggrAuthSignature {}

impl SerializeAs<AggregateAuthoritySignature> for AggrAuthSignature {
    fn serialize_as<S>(
        value: &AggregateAuthoritySignature,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        Base64::encode(value.as_ref()).serialize(serializer)
    }
}

impl<'de> DeserializeAs<'de, AggregateAuthoritySignature> for AggrAuthSignature {
    fn deserialize_as<D>(deserializer: D) -> Result<AggregateAuthoritySignature, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let sig_bytes = Base64::decode(&s).map_err(to_custom_error::<'de, D, _>)?;
        AggregateAuthoritySignature::from_bytes(&sig_bytes[..])
            .map_err(to_custom_error::<'de, D, _>)
    }
}

#[macro_export]
macro_rules! serde_to_from_bytes {
    ($type:ty) => {
        impl ::serde::Serialize for $type {
            fn serialize<S: ::serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                match serializer.is_human_readable() {
                    true => serializer.serialize_str(&self.encode_base64()),
                    false => self.as_ref().serialize(serializer),
                }
            }
        }

        impl<'de> ::serde::Deserialize<'de> for $type {
            fn deserialize<D: ::serde::Deserializer<'de>>(
                deserializer: D,
            ) -> Result<Self, D::Error> {
                use serde::de::Error;
                match deserializer.is_human_readable() {
                    true => {
                        let s = <String as ::serde::Deserialize>::deserialize(deserializer)?;
                        Self::decode_base64(&s).map_err(::serde::de::Error::custom)
                    }
                    false => {
                        let data: Vec<u8> = Vec::deserialize(deserializer)?;
                        Self::from_bytes(&data).map_err(|e| Error::custom(e.to_string()))
                    }
                }
            }
        }
    };
}
