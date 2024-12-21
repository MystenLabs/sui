// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use dashmap::DashMap;
use sui_types::base_types::ObjectID;

#[derive(Default)]
pub(crate) struct PruningLookupTable {
    table: DashMap<u64, PruningInfo>,
}

pub(crate) struct PruningInfo {
    /// For each object, whether this object was mutated or deleted in this checkpoint.
    /// This will determine the prune checkpoint for this object.
    info: BTreeMap<ObjectID, UpdateKind>,
}

enum UpdateKind {
    /// This object was mutated in this checkpoint.
    /// To prune, we should prune anything prior to this checkpoint.
    Mutate,
    /// This object was deleted in this checkpoint.
    /// To prune, we should prune anything prior to this checkpoint,
    /// as well as this checkpoint.
    Delete,
}

impl PruningInfo {
    pub fn new() -> Self {
        Self {
            info: BTreeMap::new(),
        }
    }

    /// Add an object that was mutated in this checkpoint.
    pub fn add_mutated_object(&mut self, object_id: ObjectID) {
        let old = self.info.insert(object_id, UpdateKind::Mutate);
        assert!(old.is_none(), "object already exists in pruning info");
    }

    /// Add an object that was deleted in this checkpoint.
    pub fn add_deleted_object(&mut self, object_id: ObjectID) {
        let old = self.info.insert(object_id, UpdateKind::Delete);
        assert!(old.is_none(), "object already exists in pruning info");
    }
}

impl PruningLookupTable {
    pub fn insert(&self, checkpoint: u64, prune_info: PruningInfo) {
        self.table.insert(checkpoint, prune_info);
    }

    /// Given a range of checkpoints to prune (both inclusive), return the set of objects
    /// that should be pruned, as well as the checkpoint upper bound (exclusive) that
    /// the objects should be pruned at.
    pub fn take(&self, cp_from: u64, cp_to: u64) -> anyhow::Result<BTreeMap<ObjectID, u64>> {
        let mut result: BTreeMap<ObjectID, u64> = BTreeMap::new();
        for cp in cp_from..=cp_to {
            let info = self
                .table
                .remove(&cp)
                .ok_or_else(|| anyhow::anyhow!("Prune info for checkpoint {cp} not found"))?
                .1
                .info;
            for (object_id, update_kind) in info {
                let prune_checkpoint = match update_kind {
                    UpdateKind::Mutate => cp,
                    UpdateKind::Delete => cp + 1,
                };
                let entry = result.entry(object_id).or_default();
                *entry = (*entry).max(prune_checkpoint);
            }
        }
        Ok(result)
    }
}
