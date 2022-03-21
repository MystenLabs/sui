// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::borrow::Borrow;

use super::*;

use curve25519_dalek::ristretto::RistrettoPoint;
use ed25519_dalek::Sha512;

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

#[derive(Default, Clone, PartialEq, Eq)]
pub struct Accumulator {
    accumulator: RistrettoPoint,
}

impl Accumulator {
    pub fn insert<I>(&mut self, item: &I)
    where
        I: Borrow<[u8]>,
    {
        let point = RistrettoPoint::hash_from_bytes::<Sha512>(item.borrow());
        self.accumulator += point;
    }

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

#[derive(PartialEq, Eq, Clone)]
pub struct Waypoint {
    pub sequence_number: u64,
    pub accumulator: Accumulator,
}

impl Waypoint {
    pub fn new(sequence_number: u64) -> Waypoint {
        Waypoint {
            sequence_number,
            accumulator: Accumulator::default(),
        }
    }

    pub fn insert(&mut self, item: &Item) {
        self.accumulator.insert(item);
    }
}

#[derive(Clone)]
pub struct CheckpointWithItems<K>
where
    K: Clone,
{
    pub key: K,
    pub waypoint: Waypoint,
    pub items: BTreeSet<Item>,
}

impl<K> CheckpointWithItems<K>
where
    K: Clone,
{
    pub fn new(key: K, sequence_number: u64) -> CheckpointWithItems<K> {
        CheckpointWithItems {
            key,
            waypoint: Waypoint::new(sequence_number),
            items: BTreeSet::new(),
        }
    }

    pub fn insert_full(&mut self, item: Item) {
        self.waypoint.accumulator.insert(&item);
        self.items.insert(item);
    }

    pub fn insert_accumulator(&mut self, item: &Item) {
        self.waypoint.accumulator.insert(item);
    }

    pub fn insert_item(&mut self, item: Item) {
        self.items.insert(item);
    }
}

#[derive(Clone)]
pub struct WaypointDiff<K>
where
    K: Clone,
{
    pub first: CheckpointWithItems<K>,
    pub second: CheckpointWithItems<K>,
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
        let w1 = CheckpointWithItems {
            key: first_key,
            waypoint: first,
            items: missing_from_first.collect(),
        };
        let w2 = CheckpointWithItems {
            key: second_key,
            waypoint: second,
            items: missing_from_second.collect(),
        };

        WaypointDiff {
            first: w1,
            second: w2,
        }
    }

    pub fn swap(self) -> WaypointDiff<K> {
        WaypointDiff {
            first: self.second,
            second: self.first,
        }
    }

    pub fn check(&self) -> bool {
        // Check the waypoints are for the same sequence numbers.
        if self.first.waypoint.sequence_number != self.second.waypoint.sequence_number {
            return false;
        }

        let mut first_plus = self.first.waypoint.accumulator.clone();
        first_plus.insert_all(self.first.items.iter());

        let mut second_plus = self.second.waypoint.accumulator.clone();
        second_plus.insert_all(self.second.items.iter());

        first_plus == second_plus
    }
}

#[derive(Clone)]
pub struct GlobalCheckpoint<K>
where
    K: Clone,
{
    pub reference_waypoint: Waypoint,
    pub authority_waypoints: BTreeMap<K, CheckpointWithItems<K>>,
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

    pub fn insert(&mut self, diff: WaypointDiff<K>) {
        if !diff.check() {
            panic!("Bad waypoint diff");
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
                panic!("Diff does not connect.")
            }

            if self.authority_waypoints.contains_key(&diff.second.key) {
                panic!("Both parts of diff in checkpoint");
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

    pub fn catch_up_items(&self, diff: WaypointDiff<K>) -> BTreeSet<Item> {
        // If the authority is one of the participants in the checkpoint
        // just read the different.
        if self.authority_waypoints.contains_key(&diff.first.key) {
            return self.authority_waypoints[&diff.first.key].items.clone();
        }

        // If not then we need to compute the difference.
        if !self.authority_waypoints.contains_key(&diff.second.key) {
            panic!("Need the second key at least to link into the checkpoint.")
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
            ck2.insert(diff.clone().swap());

            first_root == ck2.reference_waypoint.accumulator
        });

        item_sum
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
        ck.insert(diff1);
        assert!(ck.check());
        ck.insert(diff2);
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

        ck.catch_up_items(diff3);
    }
}
