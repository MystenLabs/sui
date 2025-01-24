// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use anyhow::bail;
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

    /// Given a range of checkpoints to prune (from inclusive, to_exclusive exclusive), return the set of objects
    /// that should be pruned, and for each object the checkpoint upper bound (exclusive) that
    /// the objects should be pruned at.
    pub fn get_prune_info(
        &self,
        cp_from: u64,
        cp_to_exclusive: u64,
    ) -> anyhow::Result<BTreeMap<ObjectID, u64>> {
        if cp_from >= cp_to_exclusive {
            bail!(
                "No valid range to take from the lookup table: from={}, to_exclusive={}",
                cp_from,
                cp_to_exclusive
            );
        }

        let mut result: BTreeMap<ObjectID, u64> = BTreeMap::new();
        for cp in cp_from..cp_to_exclusive {
            let info = self
                .table
                .get(&cp)
                .ok_or_else(|| anyhow::anyhow!("Prune info for checkpoint {cp} not found"))?;
            for (object_id, update_kind) in &info.value().info {
                let prune_checkpoint = match update_kind {
                    UpdateKind::Mutate => cp,
                    UpdateKind::Delete => cp + 1,
                };
                let entry = result.entry(*object_id).or_default();
                *entry = (*entry).max(prune_checkpoint);
            }
        }
        Ok(result)
    }

    // Remove prune info for checkpoints that we no longer need.
    // NOTE: Only call this when we have successfully pruned all the checkpoints in the range.
    pub fn gc_prune_info(&self, cp_from: u64, cp_to_exclusive: u64) {
        for cp in cp_from..cp_to_exclusive {
            self.table.remove(&cp);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pruning_lookup_table_mutations() {
        let table = PruningLookupTable::default();
        let obj1 = ObjectID::random();
        let obj2 = ObjectID::random();

        // Checkpoint 1: obj1 mutated
        let mut info1 = PruningInfo::new();
        info1.add_mutated_object(obj1);
        table.insert(1, info1);

        // Checkpoint 2: obj2 mutated
        let mut info2 = PruningInfo::new();
        info2.add_mutated_object(obj2);
        table.insert(2, info2);

        // Prune checkpoints 1-2
        let result = table.get_prune_info(1, 3).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[&obj1], 1);
        assert_eq!(result[&obj2], 2);

        // Remove prune info for checkpoints 1-2
        table.gc_prune_info(1, 3);
        assert!(table.table.is_empty());
    }

    #[test]
    fn test_pruning_lookup_table_deletions() {
        let table = PruningLookupTable::default();
        let obj = ObjectID::random();

        // Checkpoint 1: obj mutated
        let mut info1 = PruningInfo::new();
        info1.add_mutated_object(obj);
        table.insert(1, info1);

        // Checkpoint 2: obj deleted
        let mut info2 = PruningInfo::new();
        info2.add_deleted_object(obj);
        table.insert(2, info2);

        // Prune checkpoints 1-2
        let result = table.get_prune_info(1, 3).unwrap();
        assert_eq!(result.len(), 1);
        // For deleted objects, we prune up to and including the deletion checkpoint
        assert_eq!(result[&obj], 3);
    }

    #[test]
    fn test_missing_checkpoint() {
        let table = PruningLookupTable::default();
        let obj = ObjectID::random();

        let mut info = PruningInfo::new();
        info.add_mutated_object(obj);
        table.insert(1, info);

        // Try to prune checkpoint that doesn't exist in the lookup table.
        assert!(table.get_prune_info(2, 3).is_err());
    }

    #[test]
    fn test_multiple_updates() {
        let table = PruningLookupTable::default();
        let obj = ObjectID::random();

        // Checkpoint 1: obj mutated
        let mut info1 = PruningInfo::new();
        info1.add_mutated_object(obj);
        table.insert(1, info1);

        // Checkpoint 2: obj mutated again
        let mut info2 = PruningInfo::new();
        info2.add_mutated_object(obj);
        table.insert(2, info2);

        // Prune checkpoints 1-2
        let result = table.get_prune_info(1, 3).unwrap();
        assert_eq!(result.len(), 1);
        // Should use the latest mutation checkpoint
        assert_eq!(result[&obj], 2);
    }
}
