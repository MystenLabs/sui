// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `(StructTag, ObjectID)` → `VersionDigest`.
//!
//! Type-only filtering — list every live object of a given Move
//! type regardless of owner. The `StructTag` component is
//! BCS-encoded.

use bytes::Buf;
use bytes::BufMut;
use move_core_types::language_storage::StructTag;
use sui_consistent_store::Decode;
use sui_consistent_store::Encode;
use sui_consistent_store::error::DecodeError;
use sui_consistent_store::error::EncodeError;
use sui_types::base_types::ObjectID;

use crate::schema::keys::U64Varint;

pub const NAME: &str = "type_index";

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Key {
    pub type_: StructTag,
    pub object_id: ObjectID,
}

pub type Value = U64Varint;

impl Encode for Key {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        let type_bytes = bcs::to_bytes(&self.type_)
            .map_err(|e| EncodeError::with_source("bcs encode StructTag", e))?;
        buf.put_slice(&type_bytes);
        buf.put_slice(self.object_id.as_ref());
        Ok(())
    }
}

impl Decode for Key {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
        if buf.remaining() < ObjectID::LENGTH {
            return Err(DecodeError::msg(format!(
                "{NAME} Key too short: {} bytes",
                buf.remaining(),
            )));
        }
        let prefix = buf.copy_to_bytes(buf.remaining() - ObjectID::LENGTH);
        let type_: StructTag = bcs::from_bytes(&prefix)
            .map_err(|e| DecodeError::with_source("bcs decode StructTag", e))?;
        let mut id = [0u8; ObjectID::LENGTH];
        buf.copy_to_slice(&mut id);
        Ok(Key {
            type_,
            object_id: ObjectID::new(id),
        })
    }
}

pub fn options(base_options: &rocksdb::Options) -> rocksdb::Options {
    base_options.clone()
}
