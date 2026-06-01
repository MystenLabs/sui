// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `coin_type StructTag` → `CoinInfo`.
//!
//! Maps a coin type to the object ids of its metadata, treasury, and
//! regulated-metadata objects.

use bytes::Buf;
use bytes::BufMut;
use move_core_types::language_storage::StructTag;
use sui_consistent_store::Decode;
use sui_consistent_store::Encode;
use sui_consistent_store::Protobuf;
use sui_consistent_store::error::DecodeError;
use sui_consistent_store::error::EncodeError;

use crate::proto::CoinInfo;

pub const NAME: &str = "coin_index";

/// BCS-encoded `StructTag`. Reads are point lookups, so BCS's lack
/// of sort-preservation across distinct tags is not a concern.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Key(pub StructTag);

pub type Value = Protobuf<CoinInfo>;

impl Encode for Key {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        let bytes = bcs::to_bytes(&self.0)
            .map_err(|e| EncodeError::with_source("bcs encode StructTag", e))?;
        buf.put_slice(&bytes);
        Ok(())
    }
}

impl Decode for Key {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
        let bytes = buf.copy_to_bytes(buf.remaining());
        let tag: StructTag = bcs::from_bytes(&bytes)
            .map_err(|e| DecodeError::with_source("bcs decode StructTag", e))?;
        Ok(Key(tag))
    }
}

pub fn options(base_options: &rocksdb::Options) -> rocksdb::Options {
    base_options.clone()
}
