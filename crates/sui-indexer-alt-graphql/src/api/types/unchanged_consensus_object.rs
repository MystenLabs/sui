// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context as _;
use async_graphql::{dataloader::DataLoader, Context, Enum, Object, SimpleObject, Union};
use std::sync::Arc;
use sui_indexer_alt_reader::{epochs::EpochStartKey, pg_reader::PgReader};
use sui_types::{
    base_types::{ObjectID, SequenceNumber},
    digests::ObjectDigest,
    effects::UnchangedConsensusKind as NativeUnchangedConsensusKind,
};

use crate::{
    api::scalars::{sui_address::SuiAddress, uint53::UInt53},
    error::RpcError,
    scope::Scope,
};

use super::{
    address::Address,
    object::{self, Object},
};

/// Context for transaction execution - either a specific checkpoint or an epoch
#[derive(Copy, Clone)]
pub(crate) enum ExecutionContext {
    /// Transaction was executed at a specific checkpoint
    Checkpoint(u64),
    /// Transaction was executed in an epoch (for ExecutedTransaction)
    Epoch(u64),
}

/// Reason why a transaction that attempted to access a consensus-managed object was cancelled.
#[derive(Enum, Copy, Clone, Eq, PartialEq)]
pub(crate) enum ConsensusObjectCancellationReason {
    /// Read operation was cancelled.
    CancelledRead,
    /// Object congestion prevented execution.
    Congested,
    /// Randomness service was unavailable.
    RandomnessUnavailable,
    /// Internal use only.
    Unknown,
}

/// Details pertaining to consensus-managed objects that are referenced by but not changed by a transaction.
#[derive(Union)]
pub(crate) enum UnchangedConsensusObject {
    Read(ConsensusObjectRead),
    MutateConsensusStreamEnded(MutateConsensusStreamEnded),
    ReadConsensusStreamEnded(ReadConsensusStreamEnded),
    Cancelled(ConsensusObjectCancelled),
    PerEpochConfig(PerEpochConfig),
}

/// A consensus-managed object that was read by this transaction but not modified.
pub(crate) struct ConsensusObjectRead {
    scope: Scope,
    object_id: ObjectID,
    version: SequenceNumber,
    digest: ObjectDigest,
}

/// A transaction that wanted to mutate a consensus-managed object but couldn't because it became not-consensus-managed before the transaction executed (for example, it was deleted, turned into an owned object, or wrapped).
#[derive(SimpleObject)]
pub(crate) struct MutateConsensusStreamEnded {
    /// The ID of the consensus-managed object.
    address: Option<SuiAddress>,

    /// The sequence number associated with the consensus stream ending.
    sequence_number: Option<UInt53>,
}

/// A transaction that wanted to read a consensus-managed object but couldn't because it became not-consensus-managed before the transaction executed (for example, it was deleted, turned into an owned object, or wrapped).
#[derive(SimpleObject)]
pub(crate) struct ReadConsensusStreamEnded {
    /// The ID of the consensus-managed object.
    address: Option<SuiAddress>,

    /// The sequence number associated with the consensus stream ending.
    sequence_number: Option<UInt53>,
}

/// A transaction that was cancelled before it could access the consensus-managed object, so the object was an input but remained unchanged.
#[derive(SimpleObject)]
pub(crate) struct ConsensusObjectCancelled {
    /// The ID of the consensus-managed object that the transaction intended to access.
    address: Option<SuiAddress>,
    /// Reason why the transaction was cancelled.
    cancellation_reason: Option<ConsensusObjectCancellationReason>,
}

/// A per-epoch configuration object that was accessed by this transaction and remains constant during the epoch.
pub(crate) struct PerEpochConfig {
    scope: Scope,
    object_id: ObjectID,
    /// The execution context (checkpoint or epoch) when the transaction was executed
    execution_context: ExecutionContext,
}

#[Object]
impl PerEpochConfig {
    /// The per-epoch configuration object as of when the transaction was executed.
    async fn object(&self, ctx: &Context<'_>) -> Result<Option<Object>, RpcError<object::Error>> {
        let cp_num = match self.execution_context {
            ExecutionContext::Checkpoint(cp_num) => cp_num,
            ExecutionContext::Epoch(epoch) => {
                let pg_loader: &Arc<DataLoader<PgReader>> = ctx.data()?;
                let Some(epoch_start) = pg_loader
                    .load_one(EpochStartKey(epoch))
                    .await
                    .context("Failed to fetch epoch start information")?
                else {
                    return Ok(None);
                };
                epoch_start.cp_lo as u64
            }
        };

        let cp: UInt53 = cp_num.into();
        Object::checkpoint_bounded(ctx, self.scope.clone(), self.object_id.into(), cp).await
    }
}

#[Object]
impl ConsensusObjectRead {
    /// The version of the consensus-managed object that was read by this transaction.
    async fn object(&self) -> Option<Object> {
        let address = Address::with_address(self.scope.clone(), self.object_id.into());
        Some(Object::with_ref(address, self.version, self.digest))
    }
}

impl UnchangedConsensusObject {
    pub(crate) fn from_native(
        scope: Scope,
        native: (ObjectID, NativeUnchangedConsensusKind),
        execution_context: ExecutionContext,
    ) -> Self {
        let (object_id, kind) = native;

        match kind {
            NativeUnchangedConsensusKind::ReadOnlyRoot((version, digest)) => {
                Self::Read(ConsensusObjectRead {
                    scope,
                    object_id,
                    version,
                    digest,
                })
            }
            NativeUnchangedConsensusKind::MutateConsensusStreamEnded(sequence_number) => {
                Self::MutateConsensusStreamEnded(MutateConsensusStreamEnded {
                    address: Some(object_id.into()),
                    sequence_number: Some(sequence_number.into()),
                })
            }
            NativeUnchangedConsensusKind::ReadConsensusStreamEnded(sequence_number) => {
                Self::ReadConsensusStreamEnded(ReadConsensusStreamEnded {
                    address: Some(object_id.into()),
                    sequence_number: Some(sequence_number.into()),
                })
            }
            NativeUnchangedConsensusKind::Cancelled(sequence_number) => {
                let cancellation_reason = match sequence_number {
                    SequenceNumber::CANCELLED_READ => {
                        ConsensusObjectCancellationReason::CancelledRead
                    }
                    SequenceNumber::CONGESTED => ConsensusObjectCancellationReason::Congested,
                    SequenceNumber::RANDOMNESS_UNAVAILABLE => {
                        ConsensusObjectCancellationReason::RandomnessUnavailable
                    }
                    _ => ConsensusObjectCancellationReason::Unknown,
                };
                Self::Cancelled(ConsensusObjectCancelled {
                    address: Some(object_id.into()),
                    cancellation_reason: Some(cancellation_reason),
                })
            }
            NativeUnchangedConsensusKind::PerEpochConfig => Self::PerEpochConfig(PerEpochConfig {
                scope,
                object_id,
                execution_context,
            }),
        }
    }
}
