// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Payloads of the natively executed accumulator settlement transactions.
//!
//! These replace the `ProgrammableSystemTransaction`s that call
//! `0x2::accumulator_settlement` and `0x2::accumulator_metadata`. Execution must produce the same
//! effects that the Move implementation produced for the same updates.

use serde::{Deserialize, Serialize};

use crate::SUI_ACCUMULATOR_ROOT_OBJECT_ID;
use crate::TypeTag;
use crate::base_types::{SequenceNumber, SuiAddress};
use crate::digests::Digest;
use crate::effects::AccumulatorOperation;
use crate::transaction::{SharedInputObject, SharedObjectMutability};

/// Payload of `TransactionKind::AccumulatorSettlement`.
#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub enum AccumulatorSettlementTransaction {
    SettlementV1(AccumulatorSettlementV1),
    BarrierV1(AccumulatorBarrierV1),
}

/// Applies one chunk of the accumulator updates for a checkpoint. Takes the accumulator root as
/// `NonExclusiveWrite`, so the settlements of a checkpoint run concurrently and leave the root
/// unchanged.
///
/// Execution aborts only if a split exceeds the current balance. Any abort is fatal for the
/// validator, because settlement must never mint or burn funds.
#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct AccumulatorSettlementV1 {
    pub epoch: u64,
    pub checkpoint_height: u64,
    /// Index of this settlement among the settlements of the checkpoint. Together with `epoch`
    /// and `checkpoint_height` it makes the digest unique.
    pub idx: u64,
    /// Sequence number of the checkpoint that contains the settled transactions. Written into
    /// event stream heads.
    pub checkpoint_seq: u64,
    /// Sum of the SUI merges (deposits) in the settled transactions. Recorded for the SUI
    /// conservation check.
    pub input_sui: u64,
    /// Sum of the SUI splits (withdrawals) in the settled transactions.
    pub output_sui: u64,
    pub accumulator_root_initial_shared_version: SequenceNumber,
    pub updates: Vec<AccumulatorUpdateGroup>,
}

/// The updates for one accumulator type. A type appears in at most one group per settlement.
#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub enum AccumulatorUpdateGroup {
    /// Updates to `Field<Key<Balance<T>>, U128>` accumulators.
    Balance {
        /// The `T` of `Balance<T>`.
        coin_type: TypeTag,
        updates: Vec<BalanceUpdate>,
    },
    /// Updates to `Field<Key<EventStreamHead>, EventStreamHead>` accumulators.
    EventStream(Vec<EventStreamUpdate>),
}

/// Net change to one address balance. An owner appears at most once per group, and `amount` is
/// non-zero.
///
/// A merge into a missing accumulator creates it. A split that brings the value to zero deletes
/// it. A split larger than the current value (zero if missing) aborts.
#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct BalanceUpdate {
    pub owner: SuiAddress,
    pub operation: AccumulatorOperation,
    pub amount: u128,
}

/// Appends one checkpoint's events to an event stream. A stream appears at most once per group.
#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct EventStreamUpdate {
    pub stream_id: SuiAddress,
    /// Merkle root of the stream's events in this checkpoint, from `build_event_merkle_root`.
    /// Interpreted as a little-endian u256 when added to the MMR.
    pub merkle_root: Digest,
    pub event_count: u64,
}

/// Runs after every `AccumulatorSettlementV1` of a checkpoint. Takes the accumulator root as
/// `Mutable`, which advances its version, and records the net change in the number of
/// accumulator objects.
#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct AccumulatorBarrierV1 {
    pub epoch: u64,
    pub checkpoint_height: u64,
    pub num_settlements: u64,
    /// Accumulator objects created by the settlements of this checkpoint.
    pub objects_created: u64,
    /// Accumulator objects deleted by the settlements of this checkpoint.
    pub objects_destroyed: u64,
    pub accumulator_root_initial_shared_version: SequenceNumber,
}

impl AccumulatorSettlementTransaction {
    pub fn epoch(&self) -> u64 {
        match self {
            Self::SettlementV1(s) => s.epoch,
            Self::BarrierV1(b) => b.epoch,
        }
    }

    pub fn checkpoint_height(&self) -> u64 {
        match self {
            Self::SettlementV1(s) => s.checkpoint_height,
            Self::BarrierV1(b) => b.checkpoint_height,
        }
    }

    pub fn is_barrier(&self) -> bool {
        matches!(self, Self::BarrierV1(_))
    }

    /// The accumulator root, which is the only input of every settlement transaction.
    pub fn accumulator_root_input(&self) -> SharedInputObject {
        let (initial_shared_version, mutability) = match self {
            Self::SettlementV1(s) => (
                s.accumulator_root_initial_shared_version,
                SharedObjectMutability::NonExclusiveWrite,
            ),
            Self::BarrierV1(b) => (
                b.accumulator_root_initial_shared_version,
                SharedObjectMutability::Mutable,
            ),
        };
        SharedInputObject {
            id: SUI_ACCUMULATOR_ROOT_OBJECT_ID,
            initial_shared_version,
            mutability,
        }
    }
}
