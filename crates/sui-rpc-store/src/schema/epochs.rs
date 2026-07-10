// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `EpochId` → `StoredEpoch`.
//!
//! Each row records one epoch's metadata. The CF is populated by
//! two independent indexer pipelines that emit partial records —
//! one at epoch start, one at epoch end — combined by an
//! associative merge operator that copies any field set in an
//! operand into the accumulator.

use prost::Message;
use sui_consistent_store::Protobuf;
use sui_consistent_store::error::DecodeError;
use sui_consistent_store::error::Error;
use sui_consistent_store::reader::Reader;
use sui_types::committee::Committee;
use sui_types::committee::EpochId;
use sui_types::storage::EpochInfo;
use sui_types::sui_system_state::SuiSystemState;
use sui_types::sui_system_state::SuiSystemStateTrait;

use crate::proto::StoredEpoch;
use crate::schema::primitives::U64Be;

pub const NAME: &str = "epochs";

pub type Key = U64Be;
pub type Value = Protobuf<StoredEpoch>;

/// CF options: install the field-wise merge operator.
pub fn options(resolver: &sui_consistent_store::CfOptionsResolver) -> rocksdb::Options {
    let mut opts = resolver.options(NAME);
    opts.set_merge_operator_associative("epochs_merge", merge);
    opts
}

/// Build a partial `StoredEpoch` carrying the start-of-epoch
/// fields. Indexer pipelines that observe an epoch's first
/// checkpoint stage this as a merge operand against the epoch's
/// key.
///
/// `start_checkpoint` is itself optional: the embedded fullnode
/// seeds the current epoch's row from a *mid-epoch* `SuiSystemState`
/// at restore time and cannot know the epoch's first checkpoint
/// (it precedes the restore tip). Passing `None` leaves the field
/// unset, and because the merge operator only copies fields that
/// are present, a later full start record from the upward backfill
/// fills it in without either operand clobbering the other.
pub fn start(
    protocol_version: u64,
    reference_gas_price: u64,
    start_timestamp_ms: u64,
    start_checkpoint: Option<u64>,
    system_state_bcs: Option<Vec<u8>>,
) -> Value {
    Protobuf(StoredEpoch {
        protocol_version: Some(protocol_version),
        reference_gas_price: Some(reference_gas_price),
        start_timestamp_ms: Some(start_timestamp_ms),
        start_checkpoint,
        system_state_bcs: system_state_bcs.map(Into::into),
        ..StoredEpoch::default()
    })
}

/// The end-of-epoch fields staged as a merge operand by [`end`].
///
/// The `SystemEpochInfoEvent` counters (everything from
/// [`total_stake`](Self::total_stake) onward) are `None` when the
/// epoch ended in safe mode, where no such event is emitted;
/// [`safe_mode`](Self::safe_mode) records which case applies.
/// Mirrors the columns of the postgres `kv_epoch_ends` table.
#[derive(Debug, Default, Clone)]
pub struct EpochEnd {
    pub end_timestamp_ms: u64,
    pub end_checkpoint: u64,
    /// `network_total_transactions` at the epoch's final checkpoint.
    pub tx_hi: u64,
    pub safe_mode: bool,
    /// BCS-encoded `Vec<CheckpointCommitment>` from the checkpoint's
    /// end-of-epoch data.
    pub epoch_commitments: Vec<u8>,
    pub total_stake: Option<u64>,
    pub storage_fund_balance: Option<u64>,
    pub storage_fund_reinvestment: Option<u64>,
    pub storage_charge: Option<u64>,
    pub storage_rebate: Option<u64>,
    pub stake_subsidy_amount: Option<u64>,
    pub total_gas_fees: Option<u64>,
    pub total_stake_rewards_distributed: Option<u64>,
    pub leftover_storage_fund_inflow: Option<u64>,
}

/// Build a partial `StoredEpoch` carrying the end-of-epoch fields.
/// Indexer pipelines that observe an epoch's final checkpoint
/// stage this as a merge operand against the epoch's key.
pub fn end(end: EpochEnd) -> Value {
    Protobuf(StoredEpoch {
        end_timestamp_ms: Some(end.end_timestamp_ms),
        end_checkpoint: Some(end.end_checkpoint),
        tx_hi: Some(end.tx_hi),
        safe_mode: Some(end.safe_mode),
        epoch_commitments: Some(end.epoch_commitments.into()),
        total_stake: end.total_stake,
        storage_fund_balance: end.storage_fund_balance,
        storage_fund_reinvestment: end.storage_fund_reinvestment,
        storage_charge: end.storage_charge,
        storage_rebate: end.storage_rebate,
        stake_subsidy_amount: end.stake_subsidy_amount,
        total_gas_fees: end.total_gas_fees,
        total_stake_rewards_distributed: end.total_stake_rewards_distributed,
        leftover_storage_fund_inflow: end.leftover_storage_fund_inflow,
        ..StoredEpoch::default()
    })
}

/// Associative merge operator: take any field set on an operand
/// over what was accumulated so far, processed left to right.
///
/// Encode and decode failures here signal a corrupt row or a
/// programmer error in the helpers above; this CF is written only
/// by the crate's own `start` and `end` builders, so a parse
/// failure isn't a recoverable situation — panic rather than
/// silently lose data.
fn merge(
    _key: &[u8],
    existing_val: Option<&[u8]>,
    operands: &rocksdb::MergeOperands,
) -> Option<Vec<u8>> {
    let mut merged = existing_val
        .map(|v| StoredEpoch::decode(v).expect("decode existing StoredEpoch"))
        .unwrap_or_default();
    for operand in operands {
        let next = StoredEpoch::decode(operand).expect("decode StoredEpoch operand");
        if next.protocol_version.is_some() {
            merged.protocol_version = next.protocol_version;
        }
        if next.reference_gas_price.is_some() {
            merged.reference_gas_price = next.reference_gas_price;
        }
        if next.start_timestamp_ms.is_some() {
            merged.start_timestamp_ms = next.start_timestamp_ms;
        }
        if next.end_timestamp_ms.is_some() {
            merged.end_timestamp_ms = next.end_timestamp_ms;
        }
        if next.start_checkpoint.is_some() {
            merged.start_checkpoint = next.start_checkpoint;
        }
        if next.end_checkpoint.is_some() {
            merged.end_checkpoint = next.end_checkpoint;
        }
        if next.system_state_bcs.is_some() {
            merged.system_state_bcs = next.system_state_bcs;
        }
        if next.tx_hi.is_some() {
            merged.tx_hi = next.tx_hi;
        }
        if next.safe_mode.is_some() {
            merged.safe_mode = next.safe_mode;
        }
        if next.epoch_commitments.is_some() {
            merged.epoch_commitments = next.epoch_commitments;
        }
        if next.total_stake.is_some() {
            merged.total_stake = next.total_stake;
        }
        if next.storage_fund_balance.is_some() {
            merged.storage_fund_balance = next.storage_fund_balance;
        }
        if next.storage_fund_reinvestment.is_some() {
            merged.storage_fund_reinvestment = next.storage_fund_reinvestment;
        }
        if next.storage_charge.is_some() {
            merged.storage_charge = next.storage_charge;
        }
        if next.storage_rebate.is_some() {
            merged.storage_rebate = next.storage_rebate;
        }
        if next.stake_subsidy_amount.is_some() {
            merged.stake_subsidy_amount = next.stake_subsidy_amount;
        }
        if next.total_gas_fees.is_some() {
            merged.total_gas_fees = next.total_gas_fees;
        }
        if next.total_stake_rewards_distributed.is_some() {
            merged.total_stake_rewards_distributed = next.total_stake_rewards_distributed;
        }
        if next.leftover_storage_fund_inflow.is_some() {
            merged.leftover_storage_fund_inflow = next.leftover_storage_fund_inflow;
        }
    }
    Some(merged.encode_to_vec())
}

impl<R: Reader> super::RpcStoreSchema<R> {
    /// Look up the metadata for an epoch, decoding the embedded
    /// BCS `SuiSystemState` into the canonical
    /// [`sui_types::storage::EpochInfo`].
    pub fn get_epoch(&self, epoch: EpochId) -> Result<Option<EpochInfo>, Error> {
        let Some(stored) = self.epochs.get(&U64Be(epoch))? else {
            return Ok(None);
        };
        let stored = stored.into_inner();
        let system_state = stored
            .system_state_bcs
            .as_ref()
            .map(|bytes| {
                bcs::from_bytes::<SuiSystemState>(bytes)
                    .map_err(|e| DecodeError::with_source("bcs decode SuiSystemState", e))
            })
            .transpose()?;
        Ok(Some(EpochInfo {
            epoch,
            protocol_version: stored.protocol_version,
            start_timestamp_ms: stored.start_timestamp_ms,
            end_timestamp_ms: stored.end_timestamp_ms,
            start_checkpoint: stored.start_checkpoint,
            end_checkpoint: stored.end_checkpoint,
            reference_gas_price: stored.reference_gas_price,
            system_state,
        }))
    }

    /// Look up the validator committee active during an epoch.
    ///
    /// Derived from the stored `SuiSystemState` rather than kept
    /// in its own CF — the system state already carries the
    /// validator set, so a dedicated committee row would just be
    /// duplicate bytes. Returns `Ok(None)` if no epoch row exists
    /// or if the epoch row exists but no `system_state_bcs` has
    /// been observed yet.
    pub fn get_committee(&self, epoch: EpochId) -> Result<Option<Committee>, Error> {
        let Some(stored) = self.epochs.get(&U64Be(epoch))? else {
            return Ok(None);
        };
        let stored = stored.into_inner();
        let Some(bytes) = stored.system_state_bcs else {
            return Ok(None);
        };
        let system_state: SuiSystemState = bcs::from_bytes(&bytes)
            .map_err(|e| DecodeError::with_source("bcs decode SuiSystemState", e))?;
        Ok(Some(
            system_state
                .get_current_epoch_committee()
                .committee()
                .clone(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use sui_consistent_store::Db;
    use sui_consistent_store::DbOptions;

    use super::*;
    use crate::RpcStoreSchema;

    fn fresh_db() -> (tempfile::TempDir, sui_consistent_store::Db, RpcStoreSchema) {
        let dir = tempfile::tempdir().unwrap();
        let (db, schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();
        (dir, db, schema)
    }

    #[test]
    fn get_returns_none_for_unknown_epoch() {
        let (_dir, _db, schema) = fresh_db();
        assert!(schema.get_epoch(7).unwrap().is_none());
    }

    #[test]
    fn start_then_end_merges_into_full_record() {
        let (_dir, db, schema) = fresh_db();
        let mut batch = db.batch();
        batch
            .merge(
                &schema.epochs,
                &U64Be(42),
                &start(73, 1000, 111, Some(500), None),
            )
            .unwrap();
        batch
            .merge(
                &schema.epochs,
                &U64Be(42),
                &end(EpochEnd {
                    end_timestamp_ms: 999,
                    end_checkpoint: 600,
                    ..Default::default()
                }),
            )
            .unwrap();
        batch.commit().unwrap();

        let info = schema.get_epoch(42).unwrap().expect("epoch present");
        assert_eq!(info.epoch, 42);
        assert_eq!(info.protocol_version, Some(73));
        assert_eq!(info.reference_gas_price, Some(1000));
        assert_eq!(info.start_timestamp_ms, Some(111));
        assert_eq!(info.start_checkpoint, Some(500));
        assert_eq!(info.end_timestamp_ms, Some(999));
        assert_eq!(info.end_checkpoint, Some(600));
        assert_eq!(info.system_state, None);
    }

    #[test]
    fn end_before_start_still_yields_full_record() {
        // Pipelines run independently; the synchronizer doesn't
        // guarantee start is staged before end at the CF level
        // (only that both are visible before the next snapshot).
        // A row that sees `end` first then `start` should land in
        // the same state.
        let (_dir, db, schema) = fresh_db();
        let mut batch = db.batch();
        batch
            .merge(
                &schema.epochs,
                &U64Be(42),
                &end(EpochEnd {
                    end_timestamp_ms: 999,
                    end_checkpoint: 600,
                    ..Default::default()
                }),
            )
            .unwrap();
        batch
            .merge(
                &schema.epochs,
                &U64Be(42),
                &start(73, 1000, 111, Some(500), None),
            )
            .unwrap();
        batch.commit().unwrap();

        let info = schema.get_epoch(42).unwrap().expect("epoch present");
        assert_eq!(info.protocol_version, Some(73));
        assert_eq!(info.end_checkpoint, Some(600));
    }

    #[test]
    fn later_operand_overrides_earlier_for_same_field() {
        // Re-indexing semantics: if the same field is written
        // twice the later operand wins.
        let (_dir, db, schema) = fresh_db();
        let mut batch = db.batch();
        batch
            .merge(
                &schema.epochs,
                &U64Be(42),
                &start(73, 1000, 111, Some(500), None),
            )
            .unwrap();
        batch
            .merge(
                &schema.epochs,
                &U64Be(42),
                &start(74, 1500, 222, Some(501), None),
            )
            .unwrap();
        batch.commit().unwrap();

        let info = schema.get_epoch(42).unwrap().expect("epoch present");
        assert_eq!(info.protocol_version, Some(74));
        assert_eq!(info.reference_gas_price, Some(1500));
        assert_eq!(info.start_timestamp_ms, Some(222));
        assert_eq!(info.start_checkpoint, Some(501));
    }

    #[test]
    fn get_committee_returns_none_for_unknown_epoch() {
        let (_dir, _db, schema) = fresh_db();
        assert!(schema.get_committee(7).unwrap().is_none());
    }

    #[test]
    fn get_committee_returns_none_when_system_state_absent() {
        // A row exists for the epoch but only the end-of-epoch
        // partial record has been written, so we can't derive a
        // committee from it.
        let (_dir, db, schema) = fresh_db();
        let mut batch = db.batch();
        batch
            .merge(
                &schema.epochs,
                &U64Be(42),
                &end(EpochEnd {
                    end_timestamp_ms: 999,
                    end_checkpoint: 600,
                    ..Default::default()
                }),
            )
            .unwrap();
        batch.commit().unwrap();

        assert!(schema.get_committee(42).unwrap().is_none());
    }

    #[test]
    fn only_start_leaves_end_fields_none() {
        let (_dir, db, schema) = fresh_db();
        let mut batch = db.batch();
        batch
            .merge(
                &schema.epochs,
                &U64Be(42),
                &start(73, 1000, 111, Some(500), None),
            )
            .unwrap();
        batch.commit().unwrap();

        let info = schema.get_epoch(42).unwrap().expect("epoch present");
        assert_eq!(info.end_timestamp_ms, None);
        assert_eq!(info.end_checkpoint, None);
    }

    #[test]
    fn partial_seed_then_backfill_fills_start_checkpoint() {
        // The embedded restore seeds a partial start record from the
        // mid-epoch system state with no `start_checkpoint`; a later
        // full start record from the upward backfill supplies it. The
        // merge must combine them rather than clobber.
        let (_dir, db, schema) = fresh_db();
        let mut batch = db.batch();
        batch
            .merge(
                &schema.epochs,
                &U64Be(42),
                &start(73, 1000, 111, None, None),
            )
            .unwrap();
        batch
            .merge(
                &schema.epochs,
                &U64Be(42),
                &start(73, 1000, 111, Some(500), None),
            )
            .unwrap();
        batch.commit().unwrap();

        let info = schema.get_epoch(42).unwrap().expect("epoch present");
        assert_eq!(info.start_checkpoint, Some(500));
        assert_eq!(info.protocol_version, Some(73));
    }

    #[test]
    fn partial_seed_does_not_clobber_known_start_checkpoint() {
        // Reverse order: a full start record then a partial re-seed
        // (no `start_checkpoint`) must NOT erase the known value. This
        // is the crux of the presence-tracked merge — a `None` operand
        // leaves the field untouched.
        let (_dir, db, schema) = fresh_db();
        let mut batch = db.batch();
        batch
            .merge(
                &schema.epochs,
                &U64Be(42),
                &start(73, 1000, 111, Some(500), None),
            )
            .unwrap();
        batch
            .merge(
                &schema.epochs,
                &U64Be(42),
                &start(73, 1000, 111, None, None),
            )
            .unwrap();
        batch.commit().unwrap();

        let info = schema.get_epoch(42).unwrap().expect("epoch present");
        assert_eq!(info.start_checkpoint, Some(500));
    }

    #[test]
    fn end_record_stores_system_epoch_info_fields() {
        let (_dir, db, schema) = fresh_db();
        let mut batch = db.batch();
        batch
            .merge(
                &schema.epochs,
                &U64Be(42),
                &end(EpochEnd {
                    end_timestamp_ms: 999,
                    end_checkpoint: 600,
                    tx_hi: 12_345,
                    safe_mode: false,
                    epoch_commitments: vec![1, 2, 3],
                    total_stake: Some(1_000),
                    storage_fund_balance: Some(2_000),
                    storage_fund_reinvestment: Some(3_000),
                    storage_charge: Some(4_000),
                    storage_rebate: Some(5_000),
                    stake_subsidy_amount: Some(6_000),
                    total_gas_fees: Some(7_000),
                    total_stake_rewards_distributed: Some(8_000),
                    leftover_storage_fund_inflow: Some(9_000),
                }),
            )
            .unwrap();
        batch.commit().unwrap();

        // `get_epoch` deliberately does not surface these fields yet,
        // so read the raw stored record to confirm they were written.
        let stored = schema
            .epochs
            .get(&U64Be(42))
            .unwrap()
            .expect("epoch present")
            .into_inner();
        assert_eq!(stored.tx_hi, Some(12_345));
        assert_eq!(stored.safe_mode, Some(false));
        assert_eq!(stored.epoch_commitments.as_deref(), Some(&[1, 2, 3][..]));
        assert_eq!(stored.total_stake, Some(1_000));
        assert_eq!(stored.storage_fund_balance, Some(2_000));
        assert_eq!(stored.storage_fund_reinvestment, Some(3_000));
        assert_eq!(stored.storage_charge, Some(4_000));
        assert_eq!(stored.storage_rebate, Some(5_000));
        assert_eq!(stored.stake_subsidy_amount, Some(6_000));
        assert_eq!(stored.total_gas_fees, Some(7_000));
        assert_eq!(stored.total_stake_rewards_distributed, Some(8_000));
        assert_eq!(stored.leftover_storage_fund_inflow, Some(9_000));
    }

    #[test]
    fn safe_mode_end_leaves_event_counters_unset() {
        // A safe-mode epoch emits no `SystemEpochInfoEvent`, so the
        // counters stay `None` while `safe_mode`, `tx_hi`, and the
        // commitments are still recorded.
        let (_dir, db, schema) = fresh_db();
        let mut batch = db.batch();
        batch
            .merge(
                &schema.epochs,
                &U64Be(42),
                &end(EpochEnd {
                    end_timestamp_ms: 999,
                    end_checkpoint: 600,
                    tx_hi: 7,
                    safe_mode: true,
                    epoch_commitments: vec![0],
                    ..Default::default()
                }),
            )
            .unwrap();
        batch.commit().unwrap();

        let stored = schema
            .epochs
            .get(&U64Be(42))
            .unwrap()
            .expect("epoch present")
            .into_inner();
        assert_eq!(stored.safe_mode, Some(true));
        assert_eq!(stored.tx_hi, Some(7));
        assert_eq!(stored.total_stake, None);
        assert_eq!(stored.total_gas_fees, None);
        assert_eq!(stored.storage_charge, None);
    }
}
