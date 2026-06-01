// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `ObjectID` → latest live `version`.
//!
//! Resolves the latest live version of an object. Callers then read
//! the corresponding row from [`super::objects`](super::objects) to
//! fetch the full object.

use bytes::Buf;
use bytes::BufMut;
use sui_consistent_store::Decode;
use sui_consistent_store::Encode;
use sui_consistent_store::error::DecodeError;
use sui_consistent_store::error::EncodeError;
use sui_types::base_types::ObjectID;

use crate::schema::keys::U64Varint;

pub const NAME: &str = "live_objects";

/// Wrapper around `ObjectID` whose encoding is the raw 32 bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Key(pub ObjectID);

pub type Value = U64Varint;

impl Encode for Key {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        buf.put_slice(self.0.as_ref());
        Ok(())
    }
}

impl Decode for Key {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
        if buf.remaining() != ObjectID::LENGTH {
            return Err(DecodeError::msg(format!(
                "expected {} bytes for {NAME} Key, got {}",
                ObjectID::LENGTH,
                buf.remaining(),
            )));
        }
        let mut bytes = [0u8; ObjectID::LENGTH];
        buf.copy_to_slice(&mut bytes);
        Ok(Key(ObjectID::new(bytes)))
    }
}

pub fn options(base_options: &rocksdb::Options) -> rocksdb::Options {
    base_options.clone()
}
