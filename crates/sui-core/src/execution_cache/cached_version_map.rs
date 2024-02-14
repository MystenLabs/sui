// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::cmp::Ordering;
use std::collections::VecDeque;

use sui_types::base_types::SequenceNumber;

/// CachedVersionMap is a map from version to value, with the additional contraints:
/// - The key (SequenceNumber) must be monotonically increasing for each insert. If
///   a key is inserted that is less than the previous key, it results in an assertion
///   failure.
/// - Similarly, only the item with the least key can be removed.
/// - The intent of these constraints is to ensure that there are never gaps in the collection,
///   so that membership in the map can be tested by comparing to both the highest and lowest
///   (first and last) entries.
#[derive(Debug)]
pub struct CachedVersionMap<V> {
    values: VecDeque<(SequenceNumber, V)>,
}

impl<V> Default for CachedVersionMap<V> {
    fn default() -> Self {
        Self {
            values: VecDeque::new(),
        }
    }
}

impl<V> CachedVersionMap<V> {
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub fn insert(&mut self, version: SequenceNumber, value: V) {
        if !self.values.is_empty() {
            let back = self.values.back().unwrap().0;
            assert!(
                back < version,
                "version must be monotonically increasing ({} < {})",
                back,
                version
            );
        }
        self.values.push_back((version, value));
    }

    pub fn all_versions_lt_or_eq_descending<'a>(
        &'a self,
        version: &'a SequenceNumber,
    ) -> impl Iterator<Item = &'a (SequenceNumber, V)> {
        self.values.iter().rev().filter(move |(v, _)| v <= version)
    }

    pub fn get(&self, version: &SequenceNumber) -> Option<&V> {
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

    /// returns the newest (highest) version in the map
    pub fn get_highest(&self) -> Option<&(SequenceNumber, V)> {
        self.values.back()
    }

    /// returns the oldest (lowest) version in the map
    pub fn get_least(&self) -> Option<&(SequenceNumber, V)> {
        self.values.front()
    }

    // pop items from the front of the collection until the size is <= limit
    pub fn truncate_to(&mut self, limit: usize) {
        while self.values.len() > limit {
            self.values.pop_front();
        }
    }

    // remove the value if it is the first element in values.
    pub fn pop_oldest(&mut self, version: &SequenceNumber) -> Option<V> {
        let oldest = self.values.pop_front()?;
        // if this assert fails it indicates we are committing transaction data out
        // of causal order
        assert_eq!(oldest.0, *version, "version must be the oldest in the map");
        Some(oldest.1)
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

        let last = map.get_highest().unwrap();
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

        let removed = map.pop_oldest(&version1);
        assert_eq!(removed, Some("First"));
        assert!(!map.values.iter().any(|(v, _)| *v == version1));
    }

    #[test]
    #[should_panic(expected = "version must be the oldest in the map")]
    fn remove_second_item_panics() {
        let mut map = CachedVersionMap::default();
        let version1 = seq(1);
        let version2 = seq(2);
        map.insert(version1, "First");
        map.insert(version2, "Second");

        let removed = map.pop_oldest(&version2);
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
        assert_eq!(map.pop_oldest(&seq(1)), None);
    }

    #[test]
    #[should_panic(expected = "version must be the oldest in the map")]
    fn remove_nonexistent_item() {
        let mut map = CachedVersionMap::default();
        map.insert(seq(1), "First");
        assert_eq!(map.pop_oldest(&seq(2)), None);
    }

    #[test]
    fn all_versions_lt_or_eq_descending_with_existing_version() {
        let mut map = CachedVersionMap::default();
        map.insert(seq(1), "First");
        map.insert(seq(2), "Second");
        let two = seq(2);
        let result: Vec<_> = map.all_versions_lt_or_eq_descending(&two).collect();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].0, seq(2));
        assert_eq!(result[1].0, seq(1));

        let one = seq(1);
        let result: Vec<_> = map.all_versions_lt_or_eq_descending(&one).collect();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, seq(1));
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
        map.truncate_to(3);
        assert_eq!(map.values.len(), 3);
        assert_eq!(map.values.front().unwrap().0, seq(3));
    }

    #[test]
    fn get_last_on_empty_map() {
        let map: CachedVersionMap<&str> = CachedVersionMap::default();
        assert!(map.get_highest().is_none());
    }
}
