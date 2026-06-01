// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `(owner, coin_type)` → `BalanceDelta`.
//!
//! Coin-derived balances, accumulated by an associative merge
//! operator. The compaction filter drops zero entries so a fully
//! cancelled balance doesn't linger on disk.

use bytes::Buf;
use bytes::BufMut;
use move_core_types::language_storage::TypeTag;
use sui_consistent_store::Decode;
use sui_consistent_store::Encode;
use sui_consistent_store::Protobuf;
use sui_consistent_store::error::DecodeError;
use sui_consistent_store::error::EncodeError;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SuiAddress;

use crate::proto::BalanceDelta;

pub const NAME: &str = "balance";

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Key {
    pub owner: SuiAddress,
    pub coin_type: TypeTag,
}

pub type Value = Protobuf<BalanceDelta>;

impl Encode for Key {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        buf.put_slice(self.owner.as_ref());
        let type_bytes = bcs::to_bytes(&self.coin_type)
            .map_err(|e| EncodeError::with_source("bcs encode TypeTag", e))?;
        buf.put_slice(&type_bytes);
        Ok(())
    }
}

impl Decode for Key {
    fn decode<B: Buf>(buf: &mut B) -> Result<Self, DecodeError> {
        if buf.remaining() < ObjectID::LENGTH {
            return Err(DecodeError::msg(format!(
                "expected at least {} bytes for {NAME} Key owner, got {}",
                ObjectID::LENGTH,
                buf.remaining(),
            )));
        }
        let mut owner_bytes = [0u8; ObjectID::LENGTH];
        buf.copy_to_slice(&mut owner_bytes);
        let owner = SuiAddress::from_bytes(owner_bytes)
            .map_err(|e| DecodeError::with_source("decode SuiAddress", e))?;
        let remaining = buf.copy_to_bytes(buf.remaining());
        let coin_type: TypeTag = bcs::from_bytes(&remaining)
            .map_err(|e| DecodeError::with_source("bcs decode TypeTag", e))?;
        Ok(Key { owner, coin_type })
    }
}

// TODO: install a field-wise i128 merge operator (summing
// `coin` and `address` independently) and a compaction filter
// that drops rows where both components are zero. Lands with the
// balance + accumulator indexers.
pub fn options(base_options: &rocksdb::Options) -> rocksdb::Options {
    base_options.clone()
}
