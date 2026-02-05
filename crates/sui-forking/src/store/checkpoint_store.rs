// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, HashMap};
use sui_types::{
    base_types::{ObjectID, SequenceNumber, SuiAddress},
    committee::{Committee, EpochId},
    digests::{ObjectDigest, TransactionDigest},
    effects::{TransactionEffects, TransactionEffectsAPI, TransactionEvents},
    error::SuiErrorKind,
    messages_checkpoint::{
        CheckpointContents, CheckpointContentsDigest, CheckpointDigest, CheckpointSequenceNumber,
        VerifiedCheckpoint,
    },
    object::{Object, Owner},
    storage::{
        BackingPackageStore, ChildObjectResolver, ObjectStore, PackageObject, ParentSync,
        get_module, load_package_object_from_object_store,
    },
    transaction::VerifiedTransaction,
};

struct CheckpointStore {
    // Checkpoint data
    checkpoints: BTreeMap<CheckpointSequenceNumber, VerifiedCheckpoint>,
    checkpoint_digest_to_sequence_number: HashMap<CheckpointDigest, CheckpointSequenceNumber>,
    checkpoint_contents: HashMap<CheckpointContentsDigest, CheckpointContents>,
}

trait CheckpointStoreApi {
    pub fn get_checkpoint_by_sequence_number(
        &self,
        sequence_number: CheckpointSequenceNumber,
    ) -> Option<&VerifiedCheckpoint>;
    pub fn get_checkpoint_by_digest(
        &self,
        digest: &CheckpointDigest,
    ) -> Option<&VerifiedCheckpoint>;
    pub fn get_checkpoint_contents(
        &self,
        contents_digest: &CheckpointContentsDigest,
    ) -> Option<&CheckpointContents>;
}
