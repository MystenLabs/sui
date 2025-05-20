// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    base_types::{SuiAddress, VersionDigest},
    digests::ObjectDigest,
    object::{Object, Owner},
};
use move_core_types::language_storage::StructTag;
use serde::{Deserialize, Serialize};

use super::IDOperation;

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct EffectsObjectChange {
    // input_state and output_state are the core fields that's required by
    // the protocol as it tells how an object changes on-chain.
    /// State of the object in the store prior to this transaction.
    pub(crate) input_state: ObjectIn,
    /// State of the object in the store after this transaction.
    pub(crate) output_state: ObjectOut,

    /// Whether this object ID is created or deleted in this transaction.
    /// This information isn't required by the protocol but is useful for providing more detailed
    /// semantics on object changes.
    pub(crate) id_operation: IDOperation,
}

impl EffectsObjectChange {
    pub fn new(
        modified_at: Option<(VersionDigest, Owner)>,
        written: Option<&Object>,
        id_created: bool,
        id_deleted: bool,
    ) -> Self {
        debug_assert!(
            !id_created || !id_deleted,
            "Object ID can't be created and deleted at the same time."
        );
        Self {
            input_state: modified_at.map_or(ObjectIn::NotExist, ObjectIn::Exist),
            output_state: written.map_or(ObjectOut::NotExist, |o| {
                if o.is_package() {
                    ObjectOut::PackageWrite((o.version(), o.digest()))
                } else {
                    ObjectOut::ObjectWrite((o.digest(), o.owner.clone()))
                }
            }),
            id_operation: if id_created {
                IDOperation::Created
            } else if id_deleted {
                IDOperation::Deleted
            } else {
                IDOperation::None
            },
        }
    }
}

/// If an object exists (at root-level) in the store prior to this transaction,
/// it should be Exist, otherwise it's NonExist, e.g. wrapped objects should be
/// NotExist.
#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub enum ObjectIn {
    NotExist,
    /// The old version, digest and owner.
    Exist((VersionDigest, Owner)),
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub enum AccumulatorOperation {
    /// Merge the value into the accumulator.
    Merge,
    /// Split the value from the accumulator.
    Split,
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub enum AccumulatorValue {
    // u64 should be sufficient for coin balance.
    U64(u64),
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct AccumulatorWriteV1 {
    /// The recipient of the accumulator.
    pub recipient: SuiAddress,
    /// The type of the accumulator. It is used together with the recipient to
    /// derive the dynamic field ID of the accumulator.
    pub accumulator_type: StructTag,
    /// The operation to be applied to the accumulator.
    pub operation: AccumulatorOperation,
    /// The value to be applied to the accumulator.
    pub value: AccumulatorValue,
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub enum ObjectOut {
    /// Same definition as in ObjectIn.
    NotExist,
    /// Any written object, including all of mutated, created, unwrapped today.
    ObjectWrite((ObjectDigest, Owner)),
    /// Packages writes need to be tracked separately with version because
    /// we don't use lamport version for package publish and upgrades.
    PackageWrite(VersionDigest),
    /// This isn't an object write, but a special write to an accumulator.
    AccumulatorWriteV1(AccumulatorWriteV1),
}
