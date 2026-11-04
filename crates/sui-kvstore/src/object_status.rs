// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! [`ObjectStatus`], stored as one byte by the `object_versions_by_checkpoint` table and by
//! tombstone rows in the `objects` table, and [`removed_objects`], which derives the removal
//! statuses from transaction effects for the pipelines that write those tables.

use anyhow::{Result, bail};
use sui_types::base_types::{ObjectID, SequenceNumber};
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};

/// Whether an object exists at a version, or how it was removed at that version.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ObjectStatus {
    /// The object exists; its contents are in the `objects` table at that version.
    Live,
    /// The object was deleted at that version, including an object unwrapped and deleted by the
    /// same transaction. The `objects` row at that version, if written, is a tombstone.
    Deleted,
    /// The object was wrapped into another object at that version. The `objects` row at that
    /// version, if written, is a tombstone.
    Wrapped,
}

impl ObjectStatus {
    pub(crate) fn to_byte(self) -> u8 {
        match self {
            ObjectStatus::Live => 0,
            ObjectStatus::Deleted => 1,
            ObjectStatus::Wrapped => 2,
        }
    }

    pub(crate) fn from_byte(byte: u8) -> Result<Self> {
        Ok(match byte {
            0 => ObjectStatus::Live,
            1 => ObjectStatus::Deleted,
            2 => ObjectStatus::Wrapped,
            _ => bail!("invalid object status byte {byte}"),
        })
    }
}

/// The objects `effects` removed from the live set, each with the version of its removal and its
/// status, `Deleted` or `Wrapped`.
pub(crate) fn removed_objects(
    effects: &TransactionEffects,
) -> impl Iterator<Item = (ObjectID, SequenceNumber, ObjectStatus)> + use<> {
    // Take the removal version from each ref rather than `effects.lamport_version()`: for V1
    // effects, that recomputes the lamport version from the modified inputs only, which can be
    // lower than the version the transaction was executed at.
    let deleted = effects
        .deleted()
        .into_iter()
        .chain(effects.unwrapped_then_deleted())
        .map(|(id, version, _)| (id, version, ObjectStatus::Deleted));
    let wrapped = effects
        .wrapped()
        .into_iter()
        .map(|(id, version, _)| (id, version, ObjectStatus::Wrapped));
    deleted.chain(wrapped)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use sui_types::base_types::SuiAddress;
    use sui_types::base_types::random_object_ref;
    use sui_types::digests::ObjectDigest;
    use sui_types::digests::TransactionDigest;
    use sui_types::execution_status::ExecutionStatus;
    use sui_types::gas::GasCostSummary;
    use sui_types::object::Owner;

    use super::*;

    /// Every kind of removal is reported once, at the version in its ref, even where that differs
    /// from the version `lamport_version()` recomputes for V1 effects.
    #[test]
    fn removed_objects_reports_each_removal_at_its_ref_version() {
        let deleted = ObjectID::random();
        let unwrapped_then_deleted = ObjectID::random();
        let wrapped = ObjectID::random();
        let removal_version = SequenceNumber::from_u64(10);
        let effects = TransactionEffects::new_from_execution_v1(
            ExecutionStatus::Success,
            0,
            GasCostSummary::default(),
            vec![(deleted, SequenceNumber::from_u64(3))],
            vec![],
            TransactionDigest::random(),
            vec![],
            vec![],
            vec![],
            vec![(
                deleted,
                removal_version,
                ObjectDigest::OBJECT_DIGEST_DELETED,
            )],
            vec![(
                unwrapped_then_deleted,
                removal_version,
                ObjectDigest::OBJECT_DIGEST_DELETED,
            )],
            vec![(
                wrapped,
                removal_version,
                ObjectDigest::OBJECT_DIGEST_WRAPPED,
            )],
            (random_object_ref(), Owner::AddressOwner(SuiAddress::ZERO)),
            None,
            vec![],
        );
        assert_ne!(effects.lamport_version(), removal_version);

        let removed: HashSet<_> = removed_objects(&effects).collect();
        assert_eq!(
            removed,
            HashSet::from([
                (deleted, removal_version, ObjectStatus::Deleted),
                (
                    unwrapped_then_deleted,
                    removal_version,
                    ObjectStatus::Deleted
                ),
                (wrapped, removal_version, ObjectStatus::Wrapped),
            ]),
        );
    }
}
