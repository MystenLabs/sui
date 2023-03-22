// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::language_storage::StructTag;
use serde_with::DisplayFromStr;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use sui_types::base_types::{ObjectDigest, ObjectID, SequenceNumber, SuiAddress};
use sui_types::object::Owner;

/// ObjectChange are derived from the object mutations in the TransactionEffect to provide richer object information.
#[serde_as]
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum ObjectChange {
    /// Module published
    #[serde(rename_all = "camelCase")]
    Published {
        package_id: ObjectID,
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
        #[serde_as(as = "DisplayFromStr")]
        object_type: StructTag,
        object_id: ObjectID,
        version: SequenceNumber,
        digest: ObjectDigest,
    },
    /// Object mutated.
    #[serde(rename_all = "camelCase")]
    Mutated {
        sender: SuiAddress,
        owner: Owner,
        #[schemars(with = "String")]
        #[serde_as(as = "DisplayFromStr")]
        object_type: StructTag,
        object_id: ObjectID,
        version: SequenceNumber,
        previous_version: SequenceNumber,
        digest: ObjectDigest,
    },
    /// Delete object
    #[serde(rename_all = "camelCase")]
    Deleted {
        sender: SuiAddress,
        #[schemars(with = "String")]
        #[serde_as(as = "DisplayFromStr")]
        object_type: StructTag,
        object_id: ObjectID,
        version: SequenceNumber,
    },
    /// Wrapped object
    #[serde(rename_all = "camelCase")]
    Wrapped {
        sender: SuiAddress,
        #[schemars(with = "String")]
        #[serde_as(as = "DisplayFromStr")]
        object_type: StructTag,
        object_id: ObjectID,
        version: SequenceNumber,
    },
    /// New object creation
    #[serde(rename_all = "camelCase")]
    Created {
        sender: SuiAddress,
        owner: Owner,
        #[schemars(with = "String")]
        #[serde_as(as = "DisplayFromStr")]
        object_type: StructTag,
        object_id: ObjectID,
        version: SequenceNumber,
        digest: ObjectDigest,
    },
}
