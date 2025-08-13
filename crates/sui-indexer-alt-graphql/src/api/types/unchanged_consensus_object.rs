// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{Context, Enum, Object, SimpleObject, Union};
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
    /// The checkpoint when the transaction was executed (not the current view checkpoint)
    execution_checkpoint: u64,
}

#[Object]
impl PerEpochConfig {
    /// The per-epoch configuration object as of the checkpoint when the transaction was executed.
    async fn object(&self, ctx: &Context<'_>) -> Result<Option<Object>, RpcError<object::Error>> {
        let cp: UInt53 = self.execution_checkpoint.into();
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
        execution_checkpoint: u64,
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
                execution_checkpoint,
            }),
        }
    }
}
