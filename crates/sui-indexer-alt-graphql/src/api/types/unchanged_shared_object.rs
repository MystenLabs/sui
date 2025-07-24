// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{Object, SimpleObject, Union};
use sui_types::{
    base_types::{ObjectID, SequenceNumber},
    digests::ObjectDigest,
    effects::UnchangedSharedKind as NativeUnchangedSharedKind,
};

use crate::{
    api::scalars::{sui_address::SuiAddress, uint53::UInt53},
    scope::Scope,
};

use super::{address::Address, object::Object};

/// Details pertaining to shared objects that are referenced by but not changed by a transaction.
#[derive(Union)]
pub(crate) enum UnchangedSharedObject {
    Read(SharedObjectRead),
    MutateStreamEnded(SharedObjectMutateStreamEnded),
    ReadStreamEnded(SharedObjectReadStreamEnded),
    Cancelled(SharedObjectCancelled),
    PerEpochConfig(SharedObjectPerEpochConfig),
}

pub(crate) struct SharedObjectRead {
    scope: Scope,
    object_id: ObjectID,
    version: SequenceNumber,
    digest: ObjectDigest,
}

#[Object]
impl SharedObjectRead {
    /// The shared object that was accessed read-only.
    async fn object(&self) -> Option<Object> {
        let address = Address::with_address(self.scope.clone(), self.object_id.into());
        Some(Object::with_ref(address, self.version, self.digest))
    }
}

/// Access to a shared object whose mutable consensus stream has ended.
#[derive(SimpleObject)]
pub(crate) struct SharedObjectMutateStreamEnded {
    /// The ID of the shared object.
    address: Option<SuiAddress>,

    /// The sequence number associated with the consensus stream ending.
    sequence_number: Option<UInt53>,
}

/// Access to a shared object whose read-only consensus stream has ended.
#[derive(SimpleObject)]
pub(crate) struct SharedObjectReadStreamEnded {
    /// The ID of the shared object.
    address: Option<SuiAddress>,

    /// The sequence number associated with the consensus stream ending.
    sequence_number: Option<UInt53>,
}

/// Access to a cancelled shared object.
#[derive(SimpleObject)]
pub(crate) struct SharedObjectCancelled {
    /// The ID of the shared object.
    address: Option<SuiAddress>,

    /// The sequence number associated with the cancellation.
    sequence_number: Option<UInt53>,
}

/// Access to a per-epoch configuration object.
#[derive(SimpleObject)]
pub(crate) struct SharedObjectPerEpochConfig {
    /// The ID of the per-epoch configuration object.
    address: Option<SuiAddress>,
}

impl UnchangedSharedObject {
    pub(crate) fn from_native(scope: Scope, native: (ObjectID, NativeUnchangedSharedKind)) -> Self {
        let (object_id, kind) = native;

        match kind {
            NativeUnchangedSharedKind::ReadOnlyRoot((version, digest)) => {
                Self::Read(SharedObjectRead {
                    scope,
                    object_id,
                    version,
                    digest,
                })
            }
            NativeUnchangedSharedKind::MutateConsensusStreamEnded(sequence_number) => {
                Self::MutateStreamEnded(SharedObjectMutateStreamEnded {
                    address: Some(object_id.into()),
                    sequence_number: Some(sequence_number.into()),
                })
            }
            NativeUnchangedSharedKind::ReadConsensusStreamEnded(sequence_number) => {
                Self::ReadStreamEnded(SharedObjectReadStreamEnded {
                    address: Some(object_id.into()),
                    sequence_number: Some(sequence_number.into()),
                })
            }
            NativeUnchangedSharedKind::Cancelled(sequence_number) => {
                Self::Cancelled(SharedObjectCancelled {
                    address: Some(object_id.into()),
                    sequence_number: Some(sequence_number.into()),
                })
            }
            NativeUnchangedSharedKind::PerEpochConfig => {
                Self::PerEpochConfig(SharedObjectPerEpochConfig {
                    address: Some(object_id.into()),
                })
            }
        }
    }
}
