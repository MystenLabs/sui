// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::cmp::Ordering;
use std::collections::VecDeque;

use sui_types::base_types::SequenceNumber;

/// CachedVersionMap is a map from version to value, with the additional contraints:
/// - The key (SequenceNumber) must be monotonically increasing for each insert. If
///   a key is inserted that is less than the previous key, it results in an assertion
///   failure.
/// - Similarly, only the item with the least key can be removed. If an item is removed
///   from the middle of the map, it is marked for removal by setting its corresponding
///   `should_remove` flag to true. If the item with the least key is removed, it is removed
///   immediately, and any consecutive entries that are marked as `should_remove` are also
///   removed.
/// - The intent of these constraints is to ensure that there are never gaps in the collection,
///   so that membership in the map can be tested by comparing to both the highest and lowest
///   (first and last) entries.
#[derive(Debug)]
pub struct CachedVersionMap<V> {
    values: VecDeque<(SequenceNumber, V)>,
    should_remove: VecDeque<bool>,
}

impl<V> Default for CachedVersionMap<V> {
    fn default() -> Self {
        Self {
            values: VecDeque::new(),
            should_remove: VecDeque::new(),
        }
    }
}

impl<V> CachedVersionMap<V> {
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub fn insert(&mut self, version: SequenceNumber, value: V) {
        assert!(
            self.values.is_empty() || self.values.back().unwrap().0 < version,
            "version must be monotonically increasing"
        );
        self.values.push_back((version, value));
        self.should_remove.push_back(false);
    }

    pub fn all_lt_or_eq_rev<'a>(
        &'a self,
        version: &'a SequenceNumber,
    ) -> impl Iterator<Item = &'a (SequenceNumber, V)> {
        self.values
            .iter()
            .rev()
            .take_while(move |(v, _)| v <= version)
    }

    pub fn get(&self, version: &SequenceNumber) -> Option<&V> {
        if self.values.is_empty() {
            return None;
        }

        for (v, value) in self.values.iter().rev() {
            match v.cmp(version) {
                Ordering::Less => return None,
                Ordering::Equal => return Some(value),
                Ordering::Greater => (),
            }
        }

        None
    }

    pub fn get_prior_to(&self, version: &SequenceNumber) -> Option<(SequenceNumber, &V)> {
        for (v, value) in self.values.iter().rev() {
            if v < version {
                return Some((*v, value));
            }
        }

        None
    }

    pub fn get_last(&self) -> Option<&(SequenceNumber, V)> {
        self.values.back()
    }

    // pop items from the front of the map until the first item is >= version
    pub fn truncate(&mut self, limit: usize) {
        while self.values.len() > limit {
            self.should_remove.pop_front();
            self.values.pop_front();
        }
    }
}

impl<V> CachedVersionMap<V>
where
    V: Clone,
{
    // remove the value if it is the first element in values. otherwise mark it
    // for removal.
    pub fn remove(&mut self, version: &SequenceNumber) -> Option<V> {
        if self.values.is_empty() {
            return None;
        }

        if self.values.front().unwrap().0 == *version {
            self.should_remove.pop_front();
            let ret = self.values.pop_front().unwrap().1;

            // process any deferred removals
            while *self.should_remove.front().unwrap_or(&false) {
                self.should_remove.pop_front();
                self.values.pop_front();
            }

            Some(ret)
        } else {
            // Removals from the interior are deferred.
            // Removals will generally be from the front, and the collection will usually
            // be short, so linear search is preferred.
            if let Some(index) = self.values.iter().position(|(v, _)| v == version) {
                self.should_remove[index] = true;
                Some(self.values[index].1.clone())
            } else {
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sui_types::base_types::SequenceNumber;

    // Helper function to create a SequenceNumber for simplicity
    fn seq(num: u64) -> SequenceNumber {
        SequenceNumber::from(num)
    }

    #[test]
    fn insert_and_get_last() {
        let mut map = CachedVersionMap::default();
        let version1 = seq(1);
        let version2 = seq(2);
        map.insert(version1, "First");
        map.insert(version2, "Second");

        let last = map.get_last().unwrap();
        assert_eq!(last, &(version2, "Second"));
    }

    #[test]
    #[should_panic(expected = "version must be monotonically increasing")]
    fn insert_with_non_monotonic_version() {
        let mut map = CachedVersionMap::default();
        let version1 = seq(2);
        let version2 = seq(1);
        map.insert(version1, "First");
        map.insert(version2, "Second"); // This should panic
    }

    #[test]
    fn remove_first_item() {
        let mut map = CachedVersionMap::default();
        let version1 = seq(1);
        let version2 = seq(2);
        map.insert(version1, "First");
        map.insert(version2, "Second");

        let removed = map.remove(&version1);
        assert_eq!(removed, Some("First"));
        assert!(!map.values.iter().any(|(v, _)| *v == version1));
    }

    #[test]
    fn remove_second_item_deferred() {
        let mut map = CachedVersionMap::default();
        let version1 = seq(1);
        let version2 = seq(2);
        map.insert(version1, "First");
        map.insert(version2, "Second");

        let removed = map.remove(&version2);
        assert_eq!(removed, Some("Second"));
        assert!(!map.values.iter().any(|(v, _)| *v == version2));
    }

    #[test]
    fn insert_into_empty_map() {
        let mut map = CachedVersionMap::default();
        map.insert(seq(1), "First");
        assert_eq!(map.values.len(), 1);
    }

    #[test]
    fn remove_from_empty_map_returns_none() {
        let mut map: CachedVersionMap<&str> = CachedVersionMap::default();
        assert_eq!(map.remove(&seq(1)), None);
    }

    #[test]
    fn remove_nonexistent_item() {
        let mut map = CachedVersionMap::default();
        map.insert(seq(1), "First");
        assert_eq!(map.remove(&seq(2)), None);
    }

    #[test]
    fn all_lt_or_eq_rev_with_existing_version() {
        let mut map = CachedVersionMap::default();
        map.insert(seq(1), "First");
        map.insert(seq(2), "Second");
        let two = seq(2);
        let result: Vec<_> = map.all_lt_or_eq_rev(&two).collect();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].0, seq(2));
        assert_eq!(result[1].0, seq(1));
    }

    #[test]
    fn get_existing_item() {
        let mut map = CachedVersionMap::default();
        map.insert(seq(1), "First");
        let item = map.get(&seq(1));
        assert_eq!(item, Some(&"First"));
    }

    #[test]
    fn get_item_not_in_map_returns_none() {
        let mut map = CachedVersionMap::default();
        map.insert(seq(1), "First");
        assert_eq!(map.get(&seq(2)), None);
    }

    #[test]
    fn get_prior_to_with_valid_version() {
        let mut map = CachedVersionMap::default();
        map.insert(seq(1), "First");
        map.insert(seq(2), "Second");
        let prior = map.get_prior_to(&seq(2));
        assert_eq!(prior, Some((seq(1), &"First")));
    }

    #[test]
    fn get_prior_to_when_version_is_lowest() {
        let mut map = CachedVersionMap::default();
        map.insert(seq(1), "First");
        assert_eq!(map.get_prior_to(&seq(1)), None);
    }

    #[test]
    fn truncate_map_to_smaller_size() {
        let mut map = CachedVersionMap::default();
        for i in 1..=5 {
            map.insert(seq(i), format!("Item {}", i));
        }
        map.truncate(3);
        assert_eq!(map.values.len(), 3);
        assert_eq!(map.values.front().unwrap().0, seq(3));
    }

    #[test]
    fn get_last_on_empty_map() {
        let map: CachedVersionMap<&str> = CachedVersionMap::default();
        assert!(map.get_last().is_none());
    }
}
