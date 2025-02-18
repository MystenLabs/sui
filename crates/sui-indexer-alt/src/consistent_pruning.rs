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

    /// Returns a list of (object_id, checkpoint_number) pairs where each pair indicates a
    /// checkpoint whose immediate predecessor should be pruned. The checkpoint_number is exclusive,
    /// meaning we'll prune the entry with the largest checkpoint number less than it.
    ///
    /// For deletions, we include two entries:
    /// 1. One to prune its immediate predecessor (like mutations)
    /// 2. Another with checkpoint+1 to prune the deletion entry itself
    ///
    /// Example:
    /// - Create at CP 0
    /// - Modify at CP 1, 2, 10
    /// - Delete at CP 15
    ///
    /// Returns pairs for:
    /// - (obj, 1)  // will prune CP 0 because 0 < 1
    /// - (obj, 2)  // will prune CP 1 because 1 < 2
    /// - (obj, 10) // will prune CP 2 because 2 < 10
    /// - (obj, 15) // will prune CP 10 because 10 < 15
    /// - (obj, 16) // will prune CP 15 because 15 < 16
    pub fn get_prune_info(
        &self,
        cp_from: u64,
        cp_to_exclusive: u64,
    ) -> anyhow::Result<Vec<(ObjectID, u64)>> {
        if cp_from >= cp_to_exclusive {
            bail!(
                "No valid range to take from the lookup table: from={}, to_exclusive={}",
                cp_from,
                cp_to_exclusive
            );
        }

        let mut result = Vec::new();
        for cp in cp_from..cp_to_exclusive {
            let info = self
                .table
                .get(&cp)
                .ok_or_else(|| anyhow::anyhow!("Prune info for checkpoint {cp} not found"))?;

            for (object_id, update_kind) in &info.value().info {
                result.push((*object_id, cp));
                if matches!(update_kind, UpdateKind::Delete) {
                    result.push((*object_id, cp + 1));
                }
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
        assert_eq!(result[0].1, 1);
        assert_eq!(result[1].1, 2);

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
        assert_eq!(result.len(), 3);
        // For deleted objects, we prune up to and including the deletion checkpoint
        assert_eq!(result[2].1, 3);
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
        assert_eq!(result.len(), 2);
        // Don't dedupe entries for the same object
        assert_eq!(result[0].1, 1);
        assert_eq!(result[1].1, 2);
    }
}
