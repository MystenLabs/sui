// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt::Debug;
use std::marker::PhantomData;

use move_core_types::account_address::AccountAddress;
use serde;
use serde::de::{Deserialize, Deserializer, Error};
use serde::ser::Serializer;
use serde::Serialize;
use serde_with::{DeserializeAs, SerializeAs};

use crate::readable_serde::encoding::Encoding;

fn to_custom_error<'de, D, E>(e: E) -> D::Error
where
    E: Debug,
    D: Deserializer<'de>,
{
    D::Error::custom(format!("byte deserialization failed, cause by: {:?}", e))
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
    E: Encoding,
{
    fn serialize_as<S>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if serializer.is_human_readable() {
            E::encode(value).serialize(serializer)
        } else {
            R::serialize_as(value, serializer)
        }
    }
}
/// DeserializeAs support for Arrays
impl<'de, R, E, const N: usize> DeserializeAs<'de, [u8; N]> for Readable<E, R>
where
    R: DeserializeAs<'de, [u8; N]>,
    E: Encoding,
{
    fn deserialize_as<D>(deserializer: D) -> Result<[u8; N], D::Error>
    where
        D: Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            let s = String::deserialize(deserializer)?;
            let value = E::decode(s).map_err(to_custom_error::<'de, D, _>)?;
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
    E: Encoding,
{
    fn deserialize_as<D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            let s = String::deserialize(deserializer)?;
            E::decode(s).map_err(to_custom_error::<'de, D, _>)
        } else {
            R::deserialize_as(deserializer)
        }
    }
}
/// DeserializeAs support for Signature
impl<'de, R, E> DeserializeAs<'de, ed25519_dalek::Signature> for Readable<E, R>
where
    R: DeserializeAs<'de, ed25519_dalek::Signature>,
    E: Encoding,
{
    fn deserialize_as<D>(deserializer: D) -> Result<ed25519_dalek::Signature, D::Error>
    where
        D: Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            let s = String::deserialize(deserializer)?;
            let value = E::decode(s).map_err(to_custom_error::<'de, D, _>)?;
            ed25519_dalek::Signature::from_bytes(&value).map_err(to_custom_error::<'de, D, _>)
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

pub mod encoding {
    use anyhow::anyhow;
    use base64ct::Encoding as _;

    use crate::base_types::{decode_bytes_hex, encode_bytes_hex};

    pub trait Encoding {
        fn decode(s: String) -> Result<Vec<u8>, anyhow::Error>;
        fn encode<T: AsRef<[u8]>>(data: T) -> String;
    }
    pub struct Hex;
    pub struct Base64;

    impl Encoding for Hex {
        fn decode(s: String) -> Result<Vec<u8>, anyhow::Error> {
            decode_bytes_hex(&s)
        }

        fn encode<T: AsRef<[u8]>>(data: T) -> String {
            format!("0x{}", encode_bytes_hex(&data).to_lowercase())
        }
    }
    impl Encoding for Base64 {
        fn decode(s: String) -> Result<Vec<u8>, anyhow::Error> {
            base64ct::Base64::decode_vec(&s).map_err(|e| anyhow!(e))
        }

        fn encode<T: AsRef<[u8]>>(data: T) -> String {
            base64ct::Base64::encode_string(data.as_ref())
        }
    }
}
