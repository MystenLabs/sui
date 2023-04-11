// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::language_storage::StructTag;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use sui_types::base_types::{ObjectDigest, ObjectID, ObjectRef, SequenceNumber, SuiAddress};
use sui_types::object::Owner;
use sui_types::sui_serde::SequenceNumber as AsSequenceNumber;
use sui_types::sui_serde::SuiStructTag;

/// ObjectChange are derived from the object mutations in the TransactionEffect to provide richer object information.
#[serde_as]
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum ObjectChange {
    /// Module published
    #[serde(rename_all = "camelCase")]
    Published {
        package_id: ObjectID,
        #[schemars(with = "AsSequenceNumber")]
        #[serde_as(as = "AsSequenceNumber")]
        version: SequenceNumber,
        digest: ObjectDigest,
        modules: Vec<String>,
    },
    /// Transfer objects to new address / wrap in another object
    #[serde(rename_all = "camelCase")]
    Transferred {
        sender: SuiAddress,
        recipient: Owner,
        #[schemars(with = "String")]
        #[serde_as(as = "SuiStructTag")]
        object_type: StructTag,
        object_id: ObjectID,
        #[schemars(with = "AsSequenceNumber")]
        #[serde_as(as = "AsSequenceNumber")]
        version: SequenceNumber,
        digest: ObjectDigest,
    },
    /// Object mutated.
    #[serde(rename_all = "camelCase")]
    Mutated {
        sender: SuiAddress,
        owner: Owner,
        #[schemars(with = "String")]
        #[serde_as(as = "SuiStructTag")]
        object_type: StructTag,
        object_id: ObjectID,
        #[schemars(with = "AsSequenceNumber")]
        #[serde_as(as = "AsSequenceNumber")]
        version: SequenceNumber,
        #[schemars(with = "AsSequenceNumber")]
        #[serde_as(as = "AsSequenceNumber")]
        previous_version: SequenceNumber,
        digest: ObjectDigest,
    },
    /// Delete object
    #[serde(rename_all = "camelCase")]
    Deleted {
        sender: SuiAddress,
        #[schemars(with = "String")]
        #[serde_as(as = "SuiStructTag")]
        object_type: StructTag,
        object_id: ObjectID,
        #[schemars(with = "AsSequenceNumber")]
        #[serde_as(as = "AsSequenceNumber")]
        version: SequenceNumber,
    },
    /// Wrapped object
    #[serde(rename_all = "camelCase")]
    Wrapped {
        sender: SuiAddress,
        #[schemars(with = "String")]
        #[serde_as(as = "SuiStructTag")]
        object_type: StructTag,
        object_id: ObjectID,
        #[schemars(with = "AsSequenceNumber")]
        #[serde_as(as = "AsSequenceNumber")]
        version: SequenceNumber,
    },
    /// New object creation
    #[serde(rename_all = "camelCase")]
    Created {
        sender: SuiAddress,
        owner: Owner,
        #[schemars(with = "String")]
        #[serde_as(as = "SuiStructTag")]
        object_type: StructTag,
        object_id: ObjectID,
        #[schemars(with = "AsSequenceNumber")]
        #[serde_as(as = "AsSequenceNumber")]
        version: SequenceNumber,
        digest: ObjectDigest,
    },
}

impl ObjectChange {
    pub fn object_id(&self) -> ObjectID {
        match self {
            ObjectChange::Published { package_id, .. } => *package_id,
            ObjectChange::Transferred { object_id, .. }
            | ObjectChange::Mutated { object_id, .. }
            | ObjectChange::Deleted { object_id, .. }
            | ObjectChange::Wrapped { object_id, .. }
            | ObjectChange::Created { object_id, .. } => *object_id,
        }
    }

    pub fn object_ref(&self) -> ObjectRef {
        match self {
            ObjectChange::Published {
                package_id,
                version,
                digest,
                ..
            } => (*package_id, *version, *digest),
            ObjectChange::Transferred {
                object_id,
                version,
                digest,
                ..
            }
            | ObjectChange::Mutated {
                object_id,
                version,
                digest,
                ..
            }
            | ObjectChange::Created {
                object_id,
                version,
                digest,
                ..
            } => (*object_id, *version, *digest),
            ObjectChange::Deleted {
                object_id, version, ..
            } => (*object_id, *version, ObjectDigest::OBJECT_DIGEST_DELETED),
            ObjectChange::Wrapped {
                object_id, version, ..
            } => (*object_id, *version, ObjectDigest::OBJECT_DIGEST_WRAPPED),
        }
    }

    pub fn mask_for_test(&mut self, new_version: SequenceNumber, new_digest: ObjectDigest) {
        match self {
            ObjectChange::Published {
                version, digest, ..
            }
            | ObjectChange::Transferred {
                version, digest, ..
            }
            | ObjectChange::Mutated {
                version, digest, ..
            }
            | ObjectChange::Created {
                version, digest, ..
            } => {
                *version = new_version;
                *digest = new_digest
            }
            ObjectChange::Deleted { version, .. } | ObjectChange::Wrapped { version, .. } => {
                *version = new_version
            }
        }
    }
}
