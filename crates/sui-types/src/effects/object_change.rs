// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    base_types::{SequenceNumber, VersionDigest},
    digests::ObjectDigest,
    object::{Object, Owner},
};
use serde::{Deserialize, Serialize};

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
                    ObjectOut::ObjectWrite((o.digest(), o.owner))
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

    pub fn is_created(&self) -> bool {
        !self.input_state.exists()
            && self.output_state.exists()
            // Check on id operation to distinguish from unwrapped.
            && self.id_operation == IDOperation::Created
    }

    pub fn is_unwrapped(&self) -> bool {
        !self.input_state.exists()
            && self.output_state.exists()
            && self.id_operation == IDOperation::None
    }

    pub fn is_mutated(&self) -> bool {
        self.input_state.exists()
            && self.output_state.exists()
            && self.id_operation == IDOperation::None
    }

    pub fn is_deleted(&self) -> bool {
        self.input_state.exists()
            && !self.output_state.exists()
            // Check on id operation to distinguish from wrapped.
            && self.id_operation == IDOperation::Deleted
    }

    pub fn is_wrapped(&self) -> bool {
        self.input_state.exists()
            && !self.output_state.exists()
            && self.id_operation == IDOperation::None
    }

    pub fn is_unwrapped_then_deleted(&self) -> bool {
        !self.input_state.exists()
            && !self.output_state.exists()
            && self.id_operation == IDOperation::Deleted
    }

    pub fn is_created_then_wrapped(&self) -> bool {
        !self.input_state.exists()
            && !self.output_state.exists()
            && self.id_operation == IDOperation::Created
    }

    pub fn get_modified_at(&self) -> Option<(VersionDigest, Owner)> {
        match &self.input_state {
            ObjectIn::NotExist => None,
            ObjectIn::Exist(vd) => Some(*vd),
        }
    }

    pub fn get_written_object(
        &self,
        lamport_version: SequenceNumber,
    ) -> Option<(VersionDigest, Owner)> {
        match &self.output_state {
            ObjectOut::ObjectWrite((digest, owner)) => Some(((lamport_version, *digest), *owner)),
            ObjectOut::PackageWrite(vd) => Some((*vd, Owner::Immutable)),
            ObjectOut::NotExist => None,
        }
    }
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub(crate) enum IDOperation {
    None,
    Created,
    Deleted,
}

/// If an object exists (at root-level) in the store prior to this transaction,
/// it should be Exist, otherwise it's NonExist, e.g. wrapped objects should be
/// NonExist.
#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub(crate) enum ObjectIn {
    NotExist,
    /// The old version, digest and owner.
    Exist((VersionDigest, Owner)),
}

impl ObjectIn {
    pub fn exists(&self) -> bool {
        match self {
            ObjectIn::NotExist => false,
            ObjectIn::Exist(_) => true,
        }
    }
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub(crate) enum ObjectOut {
    /// Same definition as in ObjectIn.
    NotExist,
    /// Any written object, including all of mutated, created, unwrapped today.
    ObjectWrite((ObjectDigest, Owner)),
    /// Packages writes need to be tracked separately with version because
    /// we don't use lamport version for package publish and upgrades.
    PackageWrite(VersionDigest),
}

impl ObjectOut {
    pub fn exists(&self) -> bool {
        match self {
            ObjectOut::NotExist => false,
            ObjectOut::ObjectWrite(_) | ObjectOut::PackageWrite(_) => true,
        }
    }
}
