// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `(owner, coin_type)` → `BalanceDelta`.
//!
//! Each row holds two independent `i128` accumulators: `coin` from
//! the owned-`Coin<T>`-object pipeline and `address` from the
//! accumulator-balance pipeline. Both pipelines stage merge
//! operands carrying only their own field; the merge operator
//! sums each field component-wise with saturation, and the
//! compaction filter drops rows where both components are zero so
//! a fully cancelled balance doesn't linger on disk.

use bytes::Buf;
use bytes::BufMut;
use move_core_types::language_storage::TypeTag;
use prost::Message;
use sui_consistent_store::Decode;
use sui_consistent_store::Encode;
use sui_consistent_store::Iter;
use sui_consistent_store::Protobuf;
use sui_consistent_store::error::DecodeError;
use sui_consistent_store::error::EncodeError;
use sui_consistent_store::error::Error;
use sui_consistent_store::reader::Reader;
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

/// CF options: install the field-wise i128 merge operator and the
/// drop-when-zero compaction filter.
pub fn options(base_options: &rocksdb::Options) -> rocksdb::Options {
    let mut opts = base_options.clone();
    opts.set_merge_operator_associative("balance_merge", merge);
    opts.set_compaction_filter("balance_compact_zero", compact);
    opts
}

/// Build a `(Key, Value)` pair representing a coin-side delta —
/// the change in coin balance for `(owner, coin_type)` due to a
/// `Coin<T>` create / transfer / destroy event.
pub fn coin_delta(owner: SuiAddress, coin_type: TypeTag, delta: i128) -> (Key, Value) {
    (
        Key { owner, coin_type },
        Protobuf(BalanceDelta {
            coin: delta.to_le_bytes().to_vec().into(),
            address: Default::default(),
        }),
    )
}

/// Build a `(Key, Value)` pair representing an accumulator-side
/// delta — the change in address-balance for `(owner, coin_type)`
/// observed via the accumulator-bucket pipeline.
pub fn address_delta(owner: SuiAddress, coin_type: TypeTag, delta: i128) -> (Key, Value) {
    (
        Key { owner, coin_type },
        Protobuf(BalanceDelta {
            coin: Default::default(),
            address: delta.to_le_bytes().to_vec().into(),
        }),
    )
}

/// Caller-facing view of one balance row, decomposed into its
/// two contributing sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Balance {
    /// Aggregated change from owned `Coin<T>` objects.
    pub coin: i128,
    /// Aggregated change from the accumulator-bucket pipeline.
    pub address: i128,
}

impl Balance {
    /// Total balance the caller should display: the saturating
    /// sum of the two components.
    pub fn total(&self) -> i128 {
        self.coin.saturating_add(self.address)
    }
}

/// Prefix encoder for "all balances of `owner`". The prefix is the
/// 32 raw owner bytes — the leading bytes of any `Key` whose
/// `owner` matches.
pub struct OwnerPrefix(pub SuiAddress);

impl Encode for OwnerPrefix {
    fn encode_into<B: BufMut>(&self, buf: &mut B) -> Result<(), EncodeError> {
        buf.put_slice(self.0.as_ref());
        Ok(())
    }
}

impl<R: Reader> super::RpcStoreSchema<R> {
    /// Look up the aggregated balance for `(owner, coin_type)`.
    pub fn get_balance(
        &self,
        owner: SuiAddress,
        coin_type: TypeTag,
    ) -> Result<Option<Balance>, Error> {
        let Some(stored) = self.balance.get(&Key { owner, coin_type })? else {
            return Ok(None);
        };
        let stored = stored.into_inner();
        Ok(Some(Balance {
            coin: read_i128_field(&stored.coin, "coin")?,
            address: read_i128_field(&stored.address, "address")?,
        }))
    }

    /// Iterate every coin type that `owner` has a balance in.
    pub fn iter_balances_owned_by(
        &self,
        owner: SuiAddress,
    ) -> Result<Iter<'_, Key, Value>, Error> {
        self.balance.iter_prefix(&OwnerPrefix(owner))
    }
}

/// Decode an `i128` from one of the proto's `bytes` fields, with
/// an empty payload treated as zero.
fn read_i128_field(bytes: &prost::bytes::Bytes, field: &str) -> Result<i128, Error> {
    if bytes.is_empty() {
        return Ok(0);
    }
    let array: [u8; 16] = bytes.as_ref().try_into().map_err(|_| {
        DecodeError::msg(format!(
            "expected 16 bytes for BalanceDelta.{field}, got {}",
            bytes.len(),
        ))
    })?;
    Ok(i128::from_le_bytes(array))
}

/// Read an `i128` from a raw 16-byte LE field, treating an empty
/// payload as zero. Panics if the payload is not 0 or 16 bytes —
/// only the merge operator and compaction filter call this, and
/// neither has access to the structured `Error` machinery.
fn read_i128_field_or_panic(bytes: &[u8], field: &str) -> i128 {
    if bytes.is_empty() {
        return 0;
    }
    let array: [u8; 16] = bytes.try_into().unwrap_or_else(|_| {
        panic!(
            "expected 16 bytes for BalanceDelta.{field}, got {}",
            bytes.len(),
        )
    });
    i128::from_le_bytes(array)
}

/// Associative merge: sum the two components independently with
/// saturating-on-overflow semantics.
///
/// Encode / decode failures panic — this CF is written only by
/// the crate's `coin_delta` / `address_delta` helpers, so a parse
/// failure indicates corruption rather than a recoverable
/// situation.
fn merge(
    _key: &[u8],
    existing_val: Option<&[u8]>,
    operands: &rocksdb::MergeOperands,
) -> Option<Vec<u8>> {
    let mut coin: i128 = 0;
    let mut address: i128 = 0;

    if let Some(existing) = existing_val {
        let stored = BalanceDelta::decode(existing).expect("decode existing BalanceDelta");
        coin = read_i128_field_or_panic(&stored.coin, "coin");
        address = read_i128_field_or_panic(&stored.address, "address");
    }

    for operand in operands {
        let next = BalanceDelta::decode(operand).expect("decode BalanceDelta operand");
        if !next.coin.is_empty() {
            coin = coin.saturating_add(read_i128_field_or_panic(&next.coin, "coin"));
        }
        if !next.address.is_empty() {
            address =
                address.saturating_add(read_i128_field_or_panic(&next.address, "address"));
        }
    }

    let merged = BalanceDelta {
        coin: coin.to_le_bytes().to_vec().into(),
        address: address.to_le_bytes().to_vec().into(),
    };
    Some(merged.encode_to_vec())
}

/// Compaction filter: drop rows whose two components are both
/// zero (so an account that's been emptied doesn't keep a
/// gravestone entry).
///
/// A row whose payload doesn't decode is kept rather than
/// dropped: better to surface a corruption signal at read time
/// than to silently discard it during compaction.
fn compact(_level: u32, _key: &[u8], value: &[u8]) -> rocksdb::CompactionDecision {
    let Ok(stored) = BalanceDelta::decode(value) else {
        return rocksdb::CompactionDecision::Keep;
    };
    let coin = if stored.coin.is_empty() {
        0
    } else {
        read_i128_field_or_panic(&stored.coin, "coin")
    };
    let address = if stored.address.is_empty() {
        0
    } else {
        read_i128_field_or_panic(&stored.address, "address")
    };
    if coin == 0 && address == 0 {
        rocksdb::CompactionDecision::Remove
    } else {
        rocksdb::CompactionDecision::Keep
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use move_core_types::language_storage::StructTag;
    use sui_consistent_store::Db;
    use sui_consistent_store::DbOptions;

    use super::*;
    use crate::RpcStoreSchema;

    fn fresh_db() -> (tempfile::TempDir, sui_consistent_store::Db, RpcStoreSchema) {
        let dir = tempfile::tempdir().unwrap();
        let (db, schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();
        (dir, db, schema)
    }

    fn coin_type(name: &str) -> TypeTag {
        TypeTag::Struct(Box::new(StructTag {
            address: move_core_types::account_address::AccountAddress::new([2u8; 32]),
            module: move_core_types::identifier::Identifier::new("coin").unwrap(),
            name: move_core_types::identifier::Identifier::new(name).unwrap(),
            type_params: vec![],
        }))
    }

    fn owner(b: u8) -> SuiAddress {
        SuiAddress::from_bytes([b; 32]).unwrap()
    }

    #[test]
    fn get_balance_returns_none_for_unknown_owner() {
        let (_dir, _db, schema) = fresh_db();
        assert!(
            schema
                .get_balance(owner(1), coin_type("SUI"))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn coin_and_address_deltas_accumulate_independently() {
        let (_dir, db, schema) = fresh_db();
        let (k1, v1) = coin_delta(owner(1), coin_type("SUI"), 100);
        let (k2, v2) = coin_delta(owner(1), coin_type("SUI"), 50);
        let (k3, v3) = address_delta(owner(1), coin_type("SUI"), 1_000);
        let (k4, v4) = address_delta(owner(1), coin_type("SUI"), -250);

        let mut batch = db.batch();
        batch.merge(&schema.balance, &k1, &v1).unwrap();
        batch.merge(&schema.balance, &k2, &v2).unwrap();
        batch.merge(&schema.balance, &k3, &v3).unwrap();
        batch.merge(&schema.balance, &k4, &v4).unwrap();
        batch.commit().unwrap();

        let balance = schema
            .get_balance(owner(1), coin_type("SUI"))
            .unwrap()
            .expect("balance present");
        assert_eq!(balance.coin, 150);
        assert_eq!(balance.address, 750);
        assert_eq!(balance.total(), 900);
    }

    #[test]
    fn distinct_coin_types_do_not_alias() {
        let (_dir, db, schema) = fresh_db();
        let mut batch = db.batch();
        let (k_sui, v_sui) = coin_delta(owner(1), coin_type("SUI"), 100);
        let (k_usdc, v_usdc) = coin_delta(owner(1), coin_type("USDC"), 999);
        batch.merge(&schema.balance, &k_sui, &v_sui).unwrap();
        batch.merge(&schema.balance, &k_usdc, &v_usdc).unwrap();
        batch.commit().unwrap();

        assert_eq!(
            schema
                .get_balance(owner(1), coin_type("SUI"))
                .unwrap()
                .unwrap()
                .coin,
            100,
        );
        assert_eq!(
            schema
                .get_balance(owner(1), coin_type("USDC"))
                .unwrap()
                .unwrap()
                .coin,
            999,
        );
    }

    #[test]
    fn negative_then_positive_zero_out() {
        let (_dir, db, schema) = fresh_db();
        let (k1, v1) = coin_delta(owner(1), coin_type("SUI"), 100);
        let (k2, v2) = coin_delta(owner(1), coin_type("SUI"), -100);
        let mut batch = db.batch();
        batch.merge(&schema.balance, &k1, &v1).unwrap();
        batch.merge(&schema.balance, &k2, &v2).unwrap();
        batch.commit().unwrap();

        let balance = schema
            .get_balance(owner(1), coin_type("SUI"))
            .unwrap()
            .expect("row still present pre-compaction");
        assert_eq!(balance.coin, 0);
        assert_eq!(balance.address, 0);
        assert_eq!(balance.total(), 0);
    }

    #[test]
    fn saturating_add_protects_against_i128_overflow() {
        let (_dir, db, schema) = fresh_db();
        let (k1, v1) = coin_delta(owner(1), coin_type("SUI"), i128::MAX);
        let (k2, v2) = coin_delta(owner(1), coin_type("SUI"), 1);
        let mut batch = db.batch();
        batch.merge(&schema.balance, &k1, &v1).unwrap();
        batch.merge(&schema.balance, &k2, &v2).unwrap();
        batch.commit().unwrap();

        let balance = schema
            .get_balance(owner(1), coin_type("SUI"))
            .unwrap()
            .expect("balance present");
        assert_eq!(balance.coin, i128::MAX);
    }

    #[test]
    fn iter_balances_walks_only_target_owner() {
        let (_dir, db, schema) = fresh_db();
        let target_types: BTreeSet<TypeTag> = [coin_type("SUI"), coin_type("USDC")]
            .into_iter()
            .collect();

        let mut batch = db.batch();
        for t in &target_types {
            let (k, v) = coin_delta(owner(1), t.clone(), 100);
            batch.merge(&schema.balance, &k, &v).unwrap();
        }
        // Unrelated owner — must not appear.
        let (k_other, v_other) = coin_delta(owner(2), coin_type("SUI"), 500);
        batch.merge(&schema.balance, &k_other, &v_other).unwrap();
        batch.commit().unwrap();

        let found: BTreeSet<TypeTag> = schema
            .iter_balances_owned_by(owner(1))
            .unwrap()
            .map(|res| res.unwrap().0.coin_type)
            .collect();
        assert_eq!(found, target_types);
    }
}
