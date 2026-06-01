// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `(parent, field_id)` → `DynamicFieldInfo`.
//!
//! A prefix scan on `parent` enumerates every dynamic field of one
//! object.

use bytes::Buf;
use bytes::BufMut;
use sui_consistent_store::Decode;
use sui_consistent_store::Encode;
use sui_consistent_store::Protobuf;
use sui_consistent_store::error::DecodeError;
use sui_consistent_store::error::EncodeError;
use sui_types::base_types::ObjectID;

use crate::proto::DynamicFieldInfo;

pub const NAME: &str = "dynamic_fields";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Key {
    pub parent: ObjectID,
    pub field_id: ObjectID,
}

pub type Value = Protobuf<DynamicFieldInfo>;

impl Encode for Key {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        buf.put_slice(self.parent.as_ref());
        buf.put_slice(self.field_id.as_ref());
        Ok(())
    }
}

impl Decode for Key {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
        let expected = ObjectID::LENGTH * 2;
        if buf.remaining() != expected {
            return Err(DecodeError::msg(format!(
                "expected {expected} bytes for {NAME} Key, got {}",
                buf.remaining(),
            )));
        }
        let mut parent = [0u8; ObjectID::LENGTH];
        let mut field = [0u8; ObjectID::LENGTH];
        buf.copy_to_slice(&mut parent);
        buf.copy_to_slice(&mut field);
        Ok(Key {
            parent: ObjectID::new(parent),
            field_id: ObjectID::new(field),
        })
    }
}

pub fn options(base_options: &rocksdb::Options) -> rocksdb::Options {
    base_options.clone()
}
