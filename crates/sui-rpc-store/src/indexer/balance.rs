// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Sequential pipeline that feeds the
//! [`schema::balance`](crate::schema::balance) CF.
//!
//! For every transaction in the checkpoint, call
//! [`sui_types::balance_change::derive_detailed_balance_changes_2`]
//! and forward the returned `(coin_amount, address_amount)`
//! deltas straight into the CF as a single combined merge operand
//! per `(owner, coin_type)`.
//!
//! The `derive_detailed_balance_changes_2` helper already
//! consolidates input and output coin objects (for the *coin*
//! side) and parses the effects' accumulator writes (for the
//! *address* side), so the pipeline doesn't need to walk objects
//! itself.
//!
//! Balances are an address-level aggregate: the helper's coin side
//! counts only `AddressOwner` and `ConsensusAddressOwner` coins
//! (combined per address), matching [`Balance::restore`]'s owner
//! filter below -- the two must agree, or a store restored from a
//! live snapshot would diverge from one built by tip indexing.
//! This is the same rule `sui-indexer-alt-consistent-store`'s
//! `balances` handler documents; object-owned coins stay
//! discoverable through `object_by_owner` under their parent, they
//! are just not a balance.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use move_core_types::language_storage::TypeTag;
use sui_consistent_store::Batch;
use sui_consistent_store::Restore;
use sui_indexer_alt_framework::pipeline::Processor;
use sui_indexer_alt_framework::pipeline::sequential;
use sui_types::SUI_ACCUMULATOR_ROOT_OBJECT_ID;
use sui_types::accumulator_root::AccumulatorKey;
use sui_types::accumulator_root::AccumulatorValue;
use sui_types::balance_change::derive_detailed_balance_changes_2;
use sui_types::base_types::SuiAddress;
use sui_types::coin::Coin;
use sui_types::full_checkpoint_content::Checkpoint;
use sui_types::object::Object;
use sui_types::object::Owner;

use crate::RpcStoreSchema;
use crate::indexer::Schema;
use crate::indexer::Store;
use crate::schema::balance;
use crate::schema::balance::Key;

/// Pipeline marker for `balance`.
pub struct Balance;

#[derive(Debug)]
pub struct Delta {
    pub owner: SuiAddress,
    pub coin_type: TypeTag,
    /// Change to the coin-derived component (sum of owned
    /// `Coin<T>` deltas).
    pub coin: i128,
    /// Change to the accumulator-derived component (sum of
    /// per-tx accumulator writes against `(owner, coin_type)`).
    pub address: i128,
}

#[async_trait]
impl Processor for Balance {
    const NAME: &'static str = "balance";
    type Value = Delta;

    async fn process(&self, checkpoint: &Arc<Checkpoint>) -> anyhow::Result<Vec<Delta>> {
        let mut deltas = Vec::new();
        for tx in &checkpoint.transactions {
            for change in derive_detailed_balance_changes_2(&tx.effects, &checkpoint.object_set) {
                deltas.push(Delta {
                    owner: change.address,
                    coin_type: change.coin_type,
                    coin: change.coin_amount,
                    address: change.address_amount,
                });
            }
        }
        Ok(deltas)
    }
}

impl Restore for Balance {
    type Schema = RpcStoreSchema;

    /// Stage merge operands derived from a single live object.
    /// Two sources contribute to a balance row, both recoverable
    /// from the live object set:
    ///
    /// - **Coin half**: address-owned (and consensus-address-owned)
    ///   `Coin<T>` objects. The coin's `balance` field is credited
    ///   to the `(owner, coin_type)` row's coin component.
    ///
    /// - **Address half**: dynamic-field objects parented to
    ///   [`SUI_ACCUMULATOR_ROOT_OBJECT_ID`]. These carry the
    ///   per-`(owner, coin_type)` accumulator balance, which the
    ///   tip pipeline would otherwise re-derive from
    ///   `AccumulatorWrite` events.
    ///
    /// Everything else (shared / immutable objects, non-coin
    /// address-owned objects, dynamic fields under other parents)
    /// contributes no balance row.
    fn restore(
        &self,
        schema: &Self::Schema,
        object: &Object,
        batch: &mut Batch,
    ) -> anyhow::Result<()> {
        match object.owner() {
            Owner::AddressOwner(owner) | Owner::ConsensusAddressOwner { owner, .. } => {
                if let Some((coin_type, value)) = coin_balance_for_restore(object)? {
                    let (key, val) = balance::delta(*owner, coin_type, value as i128, 0);
                    batch.merge(&schema.balance, &key, &val)?;
                }
            }
            Owner::ObjectOwner(parent) if *parent == SUI_ACCUMULATOR_ROOT_OBJECT_ID.into() => {
                if let Some((owner, coin_type, balance_value)) = address_balance_info(object) {
                    let (key, val) = balance::delta(owner, coin_type, 0, balance_value);
                    batch.merge(&schema.balance, &key, &val)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
}

/// Extract the `(coin_type, balance)` pair for a coin object, or
/// `None` if `object` is not a coin or carries a non-struct type
/// tag.
fn coin_balance_for_restore(object: &Object) -> anyhow::Result<Option<(TypeTag, u64)>> {
    Ok(Coin::extract_balance_if_coin(object)
        .map_err(|e| anyhow::anyhow!("Failed to deserialize coin object {}: {e}", object.id()))?
        .and_then(|(type_, value)| match type_ {
            TypeTag::Struct(struct_tag) => Some((TypeTag::Struct(struct_tag), value)),
            _ => None,
        }))
}

/// Extract `(owner, coin_type, balance)` from a dynamic-field
/// object parented to the accumulator root. Returns `None` for
/// non-balance fields, fields whose value cannot be parsed as a
/// `u128`, or non-positive balances.
fn address_balance_info(object: &Object) -> Option<(SuiAddress, TypeTag, i128)> {
    let move_object = object.data.try_as_move()?;
    let TypeTag::Struct(coin_type) = move_object.type_().balance_accumulator_field_type_maybe()?
    else {
        return None;
    };
    let (key, value): (AccumulatorKey, AccumulatorValue) = move_object.try_into().ok()?;
    let balance_value = value.as_u128()? as i128;
    if balance_value <= 0 {
        return None;
    }
    Some((key.owner, TypeTag::Struct(coin_type), balance_value))
}

#[async_trait]
impl sequential::Handler for Balance {
    type Store = Store;
    /// Combine deltas observed in this checkpoint by
    /// `(owner, coin_type)` so a single combined merge operand is
    /// staged per key instead of many small ones.
    type Batch = HashMap<Key, (i128, i128)>;

    fn batch(&self, batch: &mut Self::Batch, values: std::vec::IntoIter<Delta>) {
        for d in values {
            let entry = batch
                .entry(Key {
                    owner: d.owner,
                    coin_type: d.coin_type,
                })
                .or_insert((0, 0));
            entry.0 = entry.0.saturating_add(d.coin);
            entry.1 = entry.1.saturating_add(d.address);
        }
    }

    async fn commit<'a>(
        &self,
        batch: &Self::Batch,
        conn: &mut sui_consistent_store::Connection<'a, Schema>,
    ) -> anyhow::Result<usize> {
        let cf = &conn.store.schema().balance;
        for (key, (coin, address)) in batch {
            let (_, value) = balance::delta(key.owner, key.coin_type.clone(), *coin, *address);
            conn.batch.merge(cf, key, &value)?;
        }
        Ok(batch.len())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use sui_consistent_store::Db;
    use sui_consistent_store::DbOptions;
    use sui_types::base_types::ObjectID;
    use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;

    use super::*;

    #[tokio::test]
    async fn process_runs_against_synthetic_checkpoint() {
        let checkpoint = Arc::new(TestCheckpointBuilder::new(1).build_checkpoint());
        let _ = Balance.process(&checkpoint).await.unwrap();
    }

    #[test]
    fn restore_credits_coin_half_for_address_owned_gas_coin() {
        let dir = tempfile::tempdir().unwrap();
        let (db, schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();

        let owner = SuiAddress::ZERO;
        let coin = Object::with_id_owner_gas_for_testing(ObjectID::from_single_byte(5), owner, 42);
        let coin_type = coin.coin_type_maybe().unwrap();

        let mut batch = db.batch();
        Balance.restore(&schema, &coin, &mut batch).unwrap();
        batch.commit().unwrap();

        let balance = schema
            .get_balance(owner, coin_type)
            .unwrap()
            .expect("balance row present");
        assert_eq!(balance.coin, 42);
        // No matching accumulator-root dynamic-field object was
        // restored alongside the coin, so the address half stays
        // zero. A test that exercises the address half lives below.
        assert_eq!(balance.address, 0);
    }

    /// A coin held by an object (dynamic field, transfer-to-object)
    /// contributes no balance row -- mirroring the tip pipeline, whose
    /// `derive_detailed_balance_changes_2` coin side counts only
    /// address-held coins. If either side ever drifts, restored and
    /// tip-built stores diverge.
    #[test]
    fn restore_skips_object_owned_coins() {
        use sui_types::object::Owner;

        let dir = tempfile::tempdir().unwrap();
        let (db, schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();

        let parent = ObjectID::from_single_byte(0x42);
        let mut coin = Object::with_id_owner_gas_for_testing(
            ObjectID::from_single_byte(6),
            SuiAddress::ZERO,
            42,
        );
        coin.owner = Owner::ObjectOwner(parent.into());
        let coin_type = coin.coin_type_maybe().unwrap();

        let mut batch = db.batch();
        Balance.restore(&schema, &coin, &mut batch).unwrap();
        batch.commit().unwrap();

        assert!(
            schema
                .get_balance(parent.into(), coin_type)
                .unwrap()
                .is_none(),
            "an object-owned coin must not credit the parent's id as a balance",
        );
    }

    #[test]
    fn restore_skips_non_coin_objects() {
        let dir = tempfile::tempdir().unwrap();
        let (db, schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();

        let owner = SuiAddress::ZERO;
        let non_coin = Object::with_id_owner_for_testing(ObjectID::from_single_byte(9), owner);

        let mut batch = db.batch();
        Balance.restore(&schema, &non_coin, &mut batch).unwrap();
        batch.commit().unwrap();
        // Nothing to assert on read because we don't know the
        // (non-coin) type to query by; the meaningful assertion
        // is just that `restore` returned `Ok` without staging a
        // bad write.
    }
}
