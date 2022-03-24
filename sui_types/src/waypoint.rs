// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::borrow::Borrow;
use std::fmt::Debug;
use thiserror::Error;

use std::collections::{BTreeMap, BTreeSet};

use curve25519_dalek::ristretto::RistrettoPoint;
use ed25519_dalek::Sha512;

use crate::{base_types::AuthorityName, committee::Committee};

#[cfg(test)]
#[path = "unit_tests/waypoint_tests.rs"]
mod waypoint_tests;

#[derive(Eq, PartialEq, Clone, Debug, Error)]
#[allow(clippy::large_enum_variant)]
pub enum WaypointError {
    #[error("Waypoint error: {:?}", msg)]
    Generic { msg: String },
}

impl WaypointError {
    pub fn generic(msg: String) -> WaypointError {
        WaypointError::Generic { msg }
    }
}

/*
   A MulHash accumulator: each element is mapped to a
   point on an eliptic curve on which the DL problem is
   hard. The accumulator is the sum of all points.
*/
#[derive(Default, Clone, PartialEq, Eq)]
pub struct Accumulator {
    accumulator: RistrettoPoint,
}

impl Accumulator {
    /// Insert one item in the accumulator
    pub fn insert<I>(&mut self, item: &I)
    where
        I: Borrow<[u8]>,
    {
        let point = RistrettoPoint::hash_from_bytes::<Sha512>(item.borrow());
        self.accumulator += point;
    }

    // Insert all items from an iterator into the accumulator
    pub fn insert_all<'a, I, It>(&'a mut self, items: It)
    where
        It: Iterator<Item = &'a I>,
        I: 'a + Borrow<[u8]>,
    {
        for i in items {
            self.insert(i);
        }
    }
}

/*
    A way point represents a sequential point in a stream that summarizes
    all elements so far. Waypoints with increasing sequence numbers should
    contain all element in previous waypoints and expand them.
*/
#[derive(PartialEq, Eq, Clone)]
pub struct Waypoint {
    pub sequence_number: u64,
    pub accumulator: Accumulator,
}

impl Waypoint {
    /// Make a new waypoint.
    pub fn new(sequence_number: u64) -> Waypoint {
        Waypoint {
            sequence_number,
            accumulator: Accumulator::default(),
        }
    }

    /// Inserts an element into the accumulator
    pub fn insert<I>(&mut self, item: &I)
    where
        I: Borrow<[u8]>,
    {
        self.accumulator.insert(item);
    }
}

/*
    A structure to hold a waypoint, associated items,
    and is indexed by a key provided. Such a structure
    may be used to represent checkpoints or differences
    of checkpoints.
*/

#[derive(Clone)]
pub struct WaypointWithItems<K, I>
where
    K: Clone,
    I:,
{
    pub key: K,
    pub waypoint: Waypoint,
    pub items: BTreeSet<I>,
}

impl<K, I> WaypointWithItems<K, I>
where
    K: Clone,
    I: Borrow<[u8]> + Ord,
{
    pub fn new(key: K, sequence_number: u64) -> WaypointWithItems<K, I> {
        WaypointWithItems {
            key,
            waypoint: Waypoint::new(sequence_number),
            items: BTreeSet::new(),
        }
    }

    /// Insert an element in the accumulator and list of items
    pub fn insert_full(&mut self, item: I) {
        self.waypoint.accumulator.insert(&item);
        self.items.insert(item);
    }

    /// Insert an element in the accumulator only
    pub fn insert_accumulator(&mut self, item: &I) {
        self.waypoint.accumulator.insert(item);
    }

    /// Insert an element in the items only
    pub fn insert_item(&mut self, item: I) {
        self.items.insert(item);
    }
}

/*
    Represents the difference between two waypoints
    and elements that make up this difference.
*/
#[derive(Clone)]
pub struct WaypointDiff<K, I>
where
    K: Clone,
    I: Borrow<[u8]> + Ord,
{
    pub first: WaypointWithItems<K, I>,
    pub second: WaypointWithItems<K, I>,
}

impl<K, I> WaypointDiff<K, I>
where
    K: Clone,
    I: Borrow<[u8]> + Ord,
{
    pub fn new<V>(
        first_key: K,
        first: Waypoint,
        missing_from_first: V,
        second_key: K,
        second: Waypoint,
        missing_from_second: V,
    ) -> WaypointDiff<K, I>
    where
        V: Iterator<Item = I>,
    {
        let w1 = WaypointWithItems {
            key: first_key,
            waypoint: first,
            items: missing_from_first.collect(),
        };
        let w2 = WaypointWithItems {
            key: second_key,
            waypoint: second,
            items: missing_from_second.collect(),
        };

        WaypointDiff {
            first: w1,
            second: w2,
        }
    }

    /// Swap the two waypoints.
    pub fn swap(self) -> WaypointDiff<K, I> {
        WaypointDiff {
            first: self.second,
            second: self.first,
        }
    }

    /// Check the internal invarients: ie that adding to both
    /// waypoints the missing elements makes them point to the
    /// accumulated same set.
    pub fn check(&self) -> bool {
        let mut first_plus = self.first.waypoint.accumulator.clone();
        first_plus.insert_all(self.first.items.iter());

        let mut second_plus = self.second.waypoint.accumulator.clone();
        second_plus.insert_all(self.second.items.iter());

        first_plus == second_plus
    }
}

/*
    A global checkpoint is the collection of differences
    fully connecting 2f+1 by stake authorities, with diffs
    that can be derived between all of them.
*/
#[derive(Clone)]
pub struct GlobalCheckpoint<K, I>
where
    K: Clone,
    I: Borrow<[u8]> + Ord,
{
    pub reference_waypoint: Waypoint,
    pub authority_waypoints: BTreeMap<K, WaypointWithItems<K, I>>,
}

impl<K, I> GlobalCheckpoint<K, I>
where
    K: Eq + Ord + Clone,
    I: Borrow<[u8]> + Ord + Clone,
{
    /// Initializes an empty global checkpoint at a specific
    /// sequence number.
    pub fn new(sequence_number: u64) -> GlobalCheckpoint<K, I> {
        GlobalCheckpoint {
            reference_waypoint: Waypoint::new(sequence_number),
            authority_waypoints: BTreeMap::new(),
        }
    }

    /// Inserts a waypoint diff into the checkpoint. If the checkpoint
    /// is empty both ends of the diff are inserted, and the reference
    /// waypoint set to their union. If there are already waypoints into
    /// this checkpoint the first part of the diff should be in the
    /// checkpoint and the second is added and updates all waypoints with
    /// the new union of all items.
    pub fn insert(&mut self, diff: WaypointDiff<K, I>) -> Result<(), WaypointError> {
        if !diff.check() {
            return Err(WaypointError::generic("Bad waypoint diff".to_string()));
        }

        // Check the waypoints are for the same sequence numbers.
        if diff.first.waypoint.sequence_number != diff.second.waypoint.sequence_number {
            return Err(WaypointError::generic(
                "Different sequence numbers (diff)".to_string(),
            ));
        }

        // Check the waypoints are for the same sequence numbers.
        if diff.first.waypoint.sequence_number != self.reference_waypoint.sequence_number {
            return Err(WaypointError::generic(
                "Different sequence numbers (checkpoint)".to_string(),
            ));
        }

        // The first link we add to the checkpoint does not need to be
        // connected to the graph since there is nothing to connect to.
        if self.authority_waypoints.is_empty() {
            // Add both waypoints into the checkpoint and compute root.
            let mut root = diff.first.waypoint.accumulator.clone();
            root.insert_all(diff.first.items.iter());

            self.reference_waypoint = Waypoint {
                sequence_number: diff.first.waypoint.sequence_number,
                accumulator: root,
            };

            let WaypointDiff { first, second } = diff;

            self.authority_waypoints.insert(first.key.clone(), first);
            self.authority_waypoints.insert(second.key.clone(), second);
        } else {
            // If the checkpoint is not empty, then the first element of the diff
            // must connect, and the second must not exist.

            debug_assert!(self.check());

            if !(self.authority_waypoints.contains_key(&diff.first.key)
                && self.authority_waypoints[&diff.first.key].waypoint == diff.first.waypoint)
            {
                return Err(WaypointError::generic("Diff does not connect.".to_string()));
            }

            if self.authority_waypoints.contains_key(&diff.second.key) {
                return Err(WaypointError::generic(
                    "Both parts of diff in checkpoint".to_string(),
                ));
            }

            let WaypointDiff { first, mut second } = diff;

            // Determine the items to add to all.
            let additional_first_items: Vec<_> = first
                .items
                .difference(&self.authority_waypoints[&first.key].items)
                .cloned()
                .collect();
            let save_old_first = self.authority_waypoints[&first.key].items.clone();

            // Update the root
            self.reference_waypoint
                .accumulator
                .insert_all(additional_first_items.iter());

            // Update existing keys
            for v in self.authority_waypoints.values_mut() {
                let add_items = additional_first_items.clone();
                v.items.extend(add_items);
            }

            debug_assert!(self.check());

            // Add the new key
            second.items.extend(&save_old_first - &first.items);
            self.authority_waypoints.insert(second.key.clone(), second);

            debug_assert!(self.check());
        }

        Ok(())
    }

    /// Checks the internal invariants of the checkpoint, namely that
    /// all the contained waypoints + the associated items lead to the
    /// reference waypoint.
    pub fn check(&self) -> bool {
        let root = self.reference_waypoint.accumulator.clone();
        for v in self.authority_waypoints.values() {
            let mut inner_root = v.waypoint.accumulator.clone();
            inner_root.insert_all(v.items.iter());

            if inner_root != root {
                return false;
            }
        }
        true
    }

    /// Provides the set of element that need to be added to the first party
    /// to catch up with the checkpoint (and maybe surpass it).
    pub fn catch_up_items(&self, diff: WaypointDiff<K, I>) -> Result<BTreeSet<I>, WaypointError> {
        // If the authority is one of the participants in the checkpoint
        // just read the different.
        if self.authority_waypoints.contains_key(&diff.first.key) {
            return Ok(self.authority_waypoints[&diff.first.key].items.clone());
        }

        // If not then we need to compute the difference.
        if !self.authority_waypoints.contains_key(&diff.second.key) {
            return Err(WaypointError::generic(
                "Need the second key at least to link into the checkpoint.".to_string(),
            ));
        }
        let item_sum: BTreeSet<_> = diff
            .first
            .items
            .union(&self.authority_waypoints[&diff.second.key].items)
            .cloned()
            .collect();
        let item_sum: BTreeSet<_> = item_sum.difference(&diff.second.items).cloned().collect();

        // The root after we add the extra items should be the same as if we constructed
        // a checkpoint including the first waypoint.
        debug_assert!({
            let mut first_root = diff.first.waypoint.accumulator.clone();
            first_root.insert_all(item_sum.iter());

            let mut ck2 = self.clone();
            ck2.insert(diff.swap()).is_ok() && first_root == ck2.reference_waypoint.accumulator
        });

        Ok(item_sum)
    }
}

impl<I> GlobalCheckpoint<AuthorityName, I>
where
    I: Borrow<[u8]> + Ord,
{
    /// In case keys are authority names we can check if the set of
    /// authorities represented in this checkpoint represent a quorum
    pub fn has_quorum(&self, committee: &Committee) -> bool {
        let authority_weights: usize = self
            .authority_waypoints
            .keys()
            .map(|name| committee.weight(name))
            .sum();
        authority_weights > committee.quorum_threshold()
    }
}
