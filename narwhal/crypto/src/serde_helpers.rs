// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use base64ct::Encoding as _;
use blst::min_sig as blst;
use serde::{
    de::{Deserializer, Error},
    ser::Serializer,
    Deserialize, Serialize,
};
use serde_with::{Bytes, DeserializeAs, SerializeAs};
use std::fmt::Debug;

fn to_custom_error<'de, D, E>(e: E) -> D::Error
where
    E: Debug,
    D: Deserializer<'de>,
{
    D::Error::custom(format!("byte deserialization failed, cause by: {:?}", e))
}

pub struct BlsSignature;

impl SerializeAs<blst::Signature> for BlsSignature {
    fn serialize_as<S>(source: &blst::Signature, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if serializer.is_human_readable() {
            base64ct::Base64::encode_string(source.to_bytes().as_ref()).serialize(serializer)
        } else {
            // Serialise to Bytes
            Bytes::serialize_as(&source.serialize(), serializer)
        }
    }
}

impl<'de> DeserializeAs<'de, blst::Signature> for BlsSignature {
    fn deserialize_as<D>(deserializer: D) -> Result<blst::Signature, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bytes = if deserializer.is_human_readable() {
            let s = String::deserialize(deserializer)?;
            base64ct::Base64::decode_vec(&s).map_err(to_custom_error::<'de, D, _>)?
        } else {
            Bytes::deserialize_as(deserializer)?
        };
        blst::Signature::deserialize(&bytes).map_err(to_custom_error::<'de, D, _>)
    }
}

pub struct Ed25519Signature;

impl SerializeAs<ed25519_dalek::Signature> for Ed25519Signature {
    fn serialize_as<S>(source: &ed25519_dalek::Signature, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if serializer.is_human_readable() {
            // Serialise to Base64 encoded String
            base64ct::Base64::encode_string(source.to_bytes().as_ref()).serialize(serializer)
        } else {
            // Serialise to Bytes
            Bytes::serialize_as(&source.to_bytes(), serializer)
        }
    }
}

impl<'de> DeserializeAs<'de, ed25519_dalek::Signature> for Ed25519Signature {
    fn deserialize_as<D>(deserializer: D) -> Result<ed25519_dalek::Signature, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bytes = if deserializer.is_human_readable() {
            let s = String::deserialize(deserializer)?;
            base64ct::Base64::decode_vec(&s).map_err(to_custom_error::<'de, D, _>)?
        } else {
            Bytes::deserialize_as(deserializer)?
        };
        ed25519_dalek::Signature::from_bytes(&bytes).map_err(to_custom_error::<'de, D, _>)
    }
}
