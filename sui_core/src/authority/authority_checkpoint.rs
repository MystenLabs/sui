// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::borrow::Borrow;
use std::fmt::Debug;
use thiserror::Error;

use super::*;

use curve25519_dalek::ristretto::RistrettoPoint;
use ed25519_dalek::Sha512;

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

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Item([u8; 8]);

impl Borrow<[u8]> for Item {
    fn borrow(&self) -> &[u8] {
        &self.0[..]
    }
}

impl Borrow<[u8]> for &Item {
    fn borrow(&self) -> &[u8] {
        &self.0[..]
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
    pub fn insert_all<I, It>(&mut self, items: It)
    where
        It: Iterator<Item = I>,
        I: Borrow<[u8]>,
    {
        for i in items {
            self.insert(&i);
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
    pub fn insert(&mut self, item: &Item) {
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
pub struct WaypointWithItems<K>
where
    K: Clone,
{
    pub key: K,
    pub waypoint: Waypoint,
    pub items: BTreeSet<Item>,
}

impl<K> WaypointWithItems<K>
where
    K: Clone,
{
    pub fn new(key: K, sequence_number: u64) -> WaypointWithItems<K> {
        WaypointWithItems {
            key,
            waypoint: Waypoint::new(sequence_number),
            items: BTreeSet::new(),
        }
    }

    /// Insert an element in the accumulator and list of items
    pub fn insert_full(&mut self, item: Item) {
        self.waypoint.accumulator.insert(&item);
        self.items.insert(item);
    }

    /// Insert an element in the accumulator only
    pub fn insert_accumulator(&mut self, item: &Item) {
        self.waypoint.accumulator.insert(item);
    }

    /// Insert an element in the items only
    pub fn insert_item(&mut self, item: Item) {
        self.items.insert(item);
    }
}

/*
    Represents the difference between two waypoints
    and elements that make up this difference.
*/
#[derive(Clone)]
pub struct WaypointDiff<K>
where
    K: Clone,
{
    pub first: WaypointWithItems<K>,
    pub second: WaypointWithItems<K>,
}

impl<K> WaypointDiff<K>
where
    K: Clone,
{
    pub fn new<V>(
        first_key: K,
        first: Waypoint,
        missing_from_first: V,
        second_key: K,
        second: Waypoint,
        missing_from_second: V,
    ) -> WaypointDiff<K>
    where
        V: Iterator<Item = Item>,
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
    pub fn swap(self) -> WaypointDiff<K> {
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
pub struct GlobalCheckpoint<K>
where
    K: Clone,
{
    pub reference_waypoint: Waypoint,
    pub authority_waypoints: BTreeMap<K, WaypointWithItems<K>>,
}

impl<K> GlobalCheckpoint<K>
where
    K: Eq + Ord + Clone,
{
    pub fn new(sequence_number: u64) -> GlobalCheckpoint<K> {
        GlobalCheckpoint {
            reference_waypoint: Waypoint::new(sequence_number),
            authority_waypoints: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, diff: WaypointDiff<K>) -> Result<(), WaypointError> {
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
            for (_k, v) in &mut self.authority_waypoints {
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

    pub fn check(&self) -> bool {
        let root = self.reference_waypoint.accumulator.clone();
        for (_k, v) in &self.authority_waypoints {
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
    pub fn catch_up_items(&self, diff: WaypointDiff<K>) -> Result<BTreeSet<Item>, WaypointError> {
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
            ck2.insert(diff.clone().swap()).is_ok()
                && first_root == ck2.reference_waypoint.accumulator
        });

        Ok(item_sum)
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use rand::Rng;

    fn make_item() -> Item {
        let mut rng = rand::thread_rng();
        let item: [u8; 8] = rng.gen();
        Item(item)
    }

    #[test]
    fn test_diff() {
        let mut first = Waypoint::new(10);
        let mut second = Waypoint::new(10);

        let v1 = make_item();
        let v2 = make_item();
        let v3 = make_item();
        let v4 = make_item();

        first.insert(&v1);
        first.insert(&v2);
        first.insert(&v3);

        second.insert(&v1);
        second.insert(&v2);
        second.insert(&v4);

        let diff = WaypointDiff::new(
            0,
            first,
            vec![v4].into_iter(),
            1,
            second,
            vec![v3].into_iter(),
        );
        assert!(diff.check());
    }

    #[test]
    fn test_checkpoint() {
        let mut w1 = Waypoint::new(10);
        let mut w2 = Waypoint::new(10);
        let mut w3 = Waypoint::new(10);

        let v1 = make_item();
        let v2 = make_item();
        let v3 = make_item();
        let v4 = make_item();

        w1.insert(&v1);
        w1.insert(&v2);

        w2.insert(&v1);
        w2.insert(&v3);

        w3.insert(&v2);
        w3.insert(&v3);
        w3.insert(&v4);

        let diff1 = WaypointDiff::new(
            0,
            w1.clone(),
            vec![v3.clone()].into_iter(),
            1,
            w2.clone(),
            vec![v2.clone()].into_iter(),
        );
        assert!(diff1.check());

        let diff2 = WaypointDiff::new(
            0,
            w1.clone(),
            vec![v3.clone(), v4.clone()].into_iter(),
            2,
            w3.clone(),
            vec![v1.clone()].into_iter(),
        );
        assert!(diff2.check());

        let mut ck = GlobalCheckpoint::new(10);
        assert!(ck.insert(diff1).is_ok());
        assert!(ck.check());
        assert!(ck.insert(diff2).is_ok());
        assert!(ck.check());

        // Now test catch_up_items
        let mut w4 = Waypoint::new(10);
        let v5 = make_item();

        w4.insert(&v5);
        w4.insert(&v1);

        let diff3 = WaypointDiff::new(
            3,
            w4.clone(),
            vec![v2.clone()].into_iter(),
            0,
            w1.clone(),
            vec![v5].into_iter(),
        );
        assert!(diff3.check());

        assert!(ck.catch_up_items(diff3).is_ok());
    }
}
