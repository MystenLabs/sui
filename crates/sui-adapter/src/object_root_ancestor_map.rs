// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use sui_types::base_types::ObjectID;
use sui_types::error::{ExecutionError, ExecutionErrorKind};
use sui_types::object::Owner;

/// A structure that maps from object ID to its root ancestor object.
/// For an object A that's owned by another object B, the root ancestor object of A is the root
/// ancestor object of B;
/// For an object that's not owned by another object, its root ancestor is itself.
/// Given a list map of object ID and the owner of the object, this data structure builds up
/// the ancestor map efficiently (O(N)). For each object ID, `root_ancestor_map` stores the object's
/// root ancestor object's ID and owner.
pub struct ObjectRootAncestorMap {
    root_ancestor_map: BTreeMap<ObjectID, (ObjectID, Owner)>,
}

impl ObjectRootAncestorMap {
    pub fn new(direct_owner_map: &BTreeMap<ObjectID, Owner>) -> Result<Self, ExecutionError> {
        let mut root_ancestor_map = BTreeMap::new();
        for (id, owner) in direct_owner_map {
            // All the objects we will visit while walking up the ancestor chain.
            // Remember them so that we could set each of their root ancestor to the same result.
            let mut stack = vec![];

            let mut cur_id = *id;
            let mut cur_owner = *owner;
            let ancestor_info = loop {
                if let Some(ancestor) = root_ancestor_map.get(&cur_id) {
                    break *ancestor;
                }
                stack.push(cur_id);
                match cur_owner {
                    Owner::ObjectOwner(parent_id) => {
                        cur_owner = *direct_owner_map.get(&parent_id.into()).ok_or_else(|| {
                            ExecutionErrorKind::missing_object_owner(cur_id, parent_id)
                        })?;
                        cur_id = parent_id.into();
                        if cur_id == stack[0] {
                            return Err(
                                ExecutionErrorKind::circular_object_ownership(cur_id).into()
                            );
                        }
                    }
                    Owner::AddressOwner(_) | Owner::Immutable | Owner::Shared { .. } => {
                        break (cur_id, cur_owner);
                    }
                };
            };
            while let Some(object_id) = stack.pop() {
                root_ancestor_map.insert(object_id, ancestor_info);
            }
        }
        Ok(Self { root_ancestor_map })
    }

    pub fn get_root_ancestor(&self, object_id: &ObjectID) -> Option<(ObjectID, Owner)> {
        self.root_ancestor_map.get(object_id).copied()
    }
}
