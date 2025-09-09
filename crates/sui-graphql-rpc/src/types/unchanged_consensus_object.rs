// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::*;
use sui_types::effects::InputConsensusObject as NativeInputConsensusObject;

use super::{object_read::ObjectRead, sui_address::SuiAddress, uint53::UInt53};

/// Details pertaining to consensus objects that are referenced by but not changed by a transaction.
/// This information is considered part of the effects, because although the transaction specifies
/// the consensus object as input, consensus must schedule it and pick the version that is actually
/// used.
#[derive(Union)]
pub(crate) enum UnchangedConsensusObject {
    Read(ConsensusObjectRead),
    ConsensusStreamEnded(ConsensusObjectStreamEnded),
    Cancelled(ConsensusObjectCancelled),
}

/// The transaction accepted a consensus object as input, but only to read it.
#[derive(SimpleObject)]
pub(crate) struct ConsensusObjectRead {
    #[graphql(flatten)]
    read: ObjectRead,
}

/// The transaction accepted a consensus object as input, but its consensus stream ended before the
/// transaction executed. This can happen for ConsensusAddressOwner objects where the stream ends
/// but the object itself is not deleted.
#[derive(SimpleObject)]
pub(crate) struct ConsensusObjectStreamEnded {
    /// ID of the consensus object.
    address: SuiAddress,

    /// The version of the consensus object that was assigned to this transaction during by consensus,
    /// during sequencing.
    version: UInt53,

    /// Whether this transaction intended to use this consensus object mutably or not. See
    /// `SharedInput.mutable` for further details.
    mutable: bool,
}

/// The transaction accpeted a consensus object as input, but its execution was cancelled.
#[derive(SimpleObject)]
pub(crate) struct ConsensusObjectCancelled {
    /// ID of the consensus object.
    address: SuiAddress,

    /// The assigned consensus object version. It is a special version indicating transaction cancellation reason.
    version: UInt53,
}

/// Error for converting from an `InputConsensusObject`.
pub(crate) struct ConsensusObjectChanged;

impl UnchangedConsensusObject {
    pub fn try_from(
        input: NativeInputConsensusObject,
        checkpoint_viewed_at: u64,
    ) -> Result<Self, ConsensusObjectChanged> {
        use NativeInputConsensusObject as I;
        use UnchangedConsensusObject as U;

        match input {
            I::Mutate(_) => Err(ConsensusObjectChanged),

            I::ReadOnly(oref) => Ok(U::Read(ConsensusObjectRead {
                read: ObjectRead {
                    native: oref,
                    checkpoint_viewed_at,
                },
            })),

            I::ReadConsensusStreamEnded(id, v) => {
                Ok(U::ConsensusStreamEnded(ConsensusObjectStreamEnded {
                    address: id.into(),
                    version: v.value().into(),
                    mutable: false,
                }))
            }

            I::MutateConsensusStreamEnded(id, v) => {
                Ok(U::ConsensusStreamEnded(ConsensusObjectStreamEnded {
                    address: id.into(),
                    version: v.value().into(),
                    mutable: true,
                }))
            }

            I::Cancelled(id, v) => Ok(U::Cancelled(ConsensusObjectCancelled {
                address: id.into(),
                version: v.value().into(),
            })),
        }
    }
}
