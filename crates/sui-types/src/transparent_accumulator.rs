// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::hash::{Digest, MultisetHash};
use itertools::Itertools;

use crate::accumulator::Accumulator;
use anyhow::{anyhow, Result};

use core::fmt::Debug;
use std::cmp::Eq;
use std::collections::HashMap;
use std::hash::Hash;

#[derive(Default, Clone, Eq, PartialEq, Debug)]
pub struct HashCounter<Data: AsRef<[u8]> + Hash + Eq + Clone + Debug + Ord> {
    map: HashMap<Data, i64>,
}

impl<Data> HashCounter<Data>
where
    Data: AsRef<[u8]> + Hash + Eq + Clone + Debug + Ord,
{
    pub fn new() -> Self {
        Self {
            map: HashMap::new(),
        }
    }

    pub fn insert(&mut self, item: Data) -> &Self {
        self.insert_n(item, 1)
    }

    pub fn extend<It>(&mut self, items: It) -> &Self
    where
        It: IntoIterator<Item = Data>,
    {
        for item in items {
            self.insert(item);
        }
        self
    }

    pub fn remove(&mut self, item: Data) -> &Self {
        self.remove_n(item, 1)
    }

    pub fn join(&mut self, other: Self) -> &Self {
        for (key, val) in other.map {
            self.insert_n(key, val);
        }
        self
    }

    pub fn difference(&self, other: &Self) -> Option<Self> {
        let mut diff = self.clone();
        for (key, val) in other.map.clone() {
            diff.remove_n(key, val);
        }
        if diff.is_empty() {
            None
        } else {
            Some(diff)
        }
    }

    // Return the backing data that went into the generated multiset (with duplicates).
    // You generally don't want to call this function at an intermediate state of accumulation,
    // i.e. a state where there may be elements removed before having been inserted, as this
    // will cause a panic (which we want to raise, if unexpected, for debugging purposes)
    pub fn data(&self) -> Vec<Data> {
        let mut ret = vec![];
        for (item, count) in self.map.clone() {
            match count.cmp(&0) {
                std::cmp::Ordering::Greater => {
                    // Manifest duplicate insertions
                    let entries = vec![item; count.try_into().unwrap()];
                    ret.extend(entries);
                }
                std::cmp::Ordering::Equal => {
                    // No longer exists in the accumulator multiset.
                    // Do not include
                    continue;
                }
                std::cmp::Ordering::Less => {
                    // This is for debugging purposes to make cases of removing a
                    // non-existent item (which is perfectly legal in an elliptic
                    // curve multiset) more visible.
                    panic!(
                        "Item {:?} removed {} more times than inserted",
                        item, -count
                    );
                }
            }
        }
        ret
    }

    pub fn get(&self, item: Data) -> Option<&i64> {
        self.map.get(&item)
    }

    fn insert_n(&mut self, item: Data, n: i64) -> &Self {
        *self.map.entry(item).or_insert(0) += n;
        self
    }

    fn remove_n(&mut self, item: Data, n: i64) -> &Self {
        *self.map.entry(item.clone()).or_insert(0) -= n;
        if self.map[&item] == 0 {
            self.map.remove(&item);
        }
        self
    }

    fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

#[derive(Default, Clone, Debug)]
pub struct TransparentAccumulator<Data: AsRef<[u8]> + Hash + Eq + Clone + Debug + Ord> {
    accumulator: Accumulator,
    elements: Option<HashCounter<Data>>,
    pub debug: bool,
}

impl<Data> TransparentAccumulator<Data>
where
    Data: AsRef<[u8]> + Hash + Eq + Clone + Debug + Ord,
{
    pub fn new(debug: bool) -> Self {
        Self {
            accumulator: Accumulator::default(),
            elements: if debug {
                Some(HashCounter::new())
            } else {
                None
            },
            debug,
        }
    }

    pub fn accumulator(&self) -> &Accumulator {
        &self.accumulator
    }

    pub fn data(&self) -> Result<Vec<Data>> {
        Ok(self
            .elements()?
            .data()
            .into_iter()
            .sorted()
            .collect::<Vec<Data>>())
    }

    pub fn elements(&self) -> Result<&HashCounter<Data>> {
        self.elements.as_ref().ok_or(anyhow!(
            "Cannot materialize elements from a non-debug accumulator"
        ))
    }

    pub fn diff(&self, other: &Self) -> Result<Option<HashCounter<Data>>> {
        let elements = self.elements()?;
        let other_elements = other.elements()?;
        Ok(elements.difference(other_elements))
    }

    pub fn insert(&mut self, item: Data) -> &Self {
        self.accumulator.insert(item.clone());
        if let Some(elements) = self.elements.as_mut() {
            elements.insert(item);
        }
        self
    }

    pub fn insert_all<It>(&mut self, items: It) -> &Self
    where
        It: IntoIterator<Item = Data> + Clone,
    {
        self.accumulator.insert_all(items.clone());
        if let Some(elements) = self.elements.as_mut() {
            elements.extend(items);
        }
        self
    }

    pub fn union(&mut self, other: &Self) -> &Self {
        self.accumulator.union(&other.accumulator);
        if let Some(other_elements) = other.elements.clone() {
            if let Some(elements) = self.elements.as_mut() {
                elements.join(other_elements);
            }
        }
        self
    }

    pub fn remove(&mut self, item: Data) -> &Self {
        self.accumulator.remove(item.clone());
        if let Some(elements) = self.elements.as_mut() {
            elements.remove(item);
        }
        self
    }

    pub fn remove_all<It>(&mut self, items: It) -> &Self
    where
        It: IntoIterator<Item = Data> + Clone,
    {
        self.accumulator.remove_all(items.clone());
        if let Some(elements) = self.elements.as_mut() {
            for i in items {
                elements.remove(i);
            }
        }
        self
    }

    pub fn digest(&self) -> Digest<32> {
        self.accumulator.digest()
    }

    pub fn is_empty(&self) -> bool {
        self.accumulator == Accumulator::default()
    }
}

impl<Data> PartialEq for TransparentAccumulator<Data>
where
    Data: AsRef<[u8]> + Hash + Eq + Clone + Debug + Ord,
{
    fn eq(&self, other: &Self) -> bool {
        self.accumulator == other.accumulator
    }
}

impl<Data> Eq for TransparentAccumulator<Data> where
    Data: AsRef<[u8]> + Hash + Eq + Clone + Debug + Ord
{
}

#[cfg(test)]
mod tests {
    use crate::base_types::ObjectDigest;
    use crate::transparent_accumulator::{HashCounter, TransparentAccumulator};
    use std::vec;

    #[test]
    fn test_transparent_accumulator() {
        let ref1 = ObjectDigest::random();
        let ref2 = ObjectDigest::random();
        let ref3 = ObjectDigest::random();
        let ref4 = ObjectDigest::random();

        // HashCounter test
        // Test we can remove before insert, and materialize after set is non-negative.
        let mut diff_counter: HashCounter<ObjectDigest> = HashCounter::new();
        diff_counter.remove_n(ref1, 2);
        diff_counter.insert_n(ref1, 3);
        assert_eq!(diff_counter.data(), vec![ref1]);

        // Check we garbage collect elements with zero count
        diff_counter.remove_n(ref1, 1);
        assert!(diff_counter.is_empty());

        // TransparentAccumulator tests
        let mut a0 = TransparentAccumulator::new(false);
        a0.insert(ref1);
        a0.insert(ref2);
        a0.insert(ref3);

        // Cannot materialize elements from a non-debug accumulator
        assert!(a0.data().is_err());

        let mut a1 = TransparentAccumulator::new(true);
        a1.insert(ref1);
        a1.insert(ref2);
        a1.insert(ref3);
        assert_eq!(
            a1.clone().data().unwrap(),
            TransparentAccumulator::new(true)
                .insert_all(vec![ref1, ref2, ref3])
                .data()
                .unwrap()
        );

        // Multiple inserts are counted correctly
        a1.insert(ref1);
        assert_eq!(a1.elements().unwrap().get(ref1), Some(&2));
        a1.remove(ref1);

        // Remove before insert (without call to data()) yield negative count
        let ref5 = ObjectDigest::random();
        a1.remove(ref5);
        assert_eq!(a1.elements().unwrap().get(ref5), Some(&-1));
        a1.insert(ref5);
        assert_eq!(a1.elements().unwrap().get(ref5), Some(&0));

        // Insertion out of order should arrive at the same result.
        let mut a2 = TransparentAccumulator::new(true);
        a2.insert(ref3);
        assert_ne!(a1, a2);
        a2.insert(ref2);
        assert_ne!(a1, a2);
        a2.insert(ref1);
        assert_eq!(a1, a2);
        assert_eq!(a1.data().unwrap(), a2.data().unwrap());

        // Accumulator is not a set, and inserting the same element twice will change the result.
        a2.insert(ref3);
        assert_ne!(a1, a2);
        a2.remove(ref3);

        a2.insert(ref4);
        assert_ne!(a1, a2);
        a2.remove(ref4);

        // Removing elements out of order should arrive at the same result.
        a2.remove(ref3);
        a2.remove(ref1);

        a1.remove(ref1);
        a1.remove(ref3);

        assert_eq!(a1, a2);

        // After removing all elements, it should be the same as an empty one.
        a1.remove(ref2);
        assert_eq!(a1, TransparentAccumulator::new(true));
    }

    #[test]
    #[should_panic]
    fn test_transparent_accumulator_negative_set_size_panic_in_debug_mode() {
        let mut a = TransparentAccumulator::new(true);
        a.remove(ObjectDigest::random());
        let _ = a.data();
    }

    #[test]
    fn test_transparent_accumulator_set_difference() {
        let ref1 = ObjectDigest::random();
        let ref2 = ObjectDigest::random();

        let mut a1 = TransparentAccumulator::new(true);
        let mut a2 = TransparentAccumulator::new(true);
        a1.insert(ref1);
        a1.insert(ref2);
        a2.insert(ref1);

        assert_eq!(
            a1.elements()
                .unwrap()
                .difference(a2.elements().unwrap())
                .unwrap()
                .data(),
            vec![ref2],
        );
    }
}
