// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Abstract domains for dataflow analysis.
//!
//! Provides the `AbstractDomain` trait and two common domain types:
//! - `SetDomain<E>`: a set-based lattice with union as join
//! - `MapDomain<K, V>`: a map-based lattice with pointwise join

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Debug,
    ops::{Deref, DerefMut},
};

// =============================================================================
// JoinResult

/// The outcome of joining two abstract states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinResult {
    /// The left operand already subsumes the right: L join R == L.
    Unchanged,
    /// The left operand was changed by the join.
    Changed,
}

impl JoinResult {
    pub fn combine(self, other: JoinResult) -> JoinResult {
        match (self, other) {
            (JoinResult::Unchanged, JoinResult::Unchanged) => JoinResult::Unchanged,
            _ => JoinResult::Changed,
        }
    }
}

// =============================================================================
// AbstractDomain

/// A trait for lattice elements that support a join (least upper bound) operation.
///
/// `join` mutates `self` to be the join of `self` and `other`, and returns
/// whether `self` changed as a result.
pub trait AbstractDomain {
    fn join(&mut self, other: &Self) -> JoinResult;
}

// =============================================================================
// SetDomain

/// A set-based abstract domain where join is set union.
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct SetDomain<E: Ord>(BTreeSet<E>);

impl<E: Ord> Default for SetDomain<E> {
    fn default() -> Self {
        Self(BTreeSet::new())
    }
}

impl<E: Ord + Debug> Debug for SetDomain<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

impl<E: Ord> From<BTreeSet<E>> for SetDomain<E> {
    fn from(s: BTreeSet<E>) -> Self {
        Self(s)
    }
}

impl<E: Ord> Deref for SetDomain<E> {
    type Target = BTreeSet<E>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<E: Ord> DerefMut for SetDomain<E> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<E: Ord> FromIterator<E> for SetDomain<E> {
    fn from_iter<I: IntoIterator<Item = E>>(iter: I) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl<E: Ord> IntoIterator for SetDomain<E> {
    type Item = E;
    type IntoIter = std::collections::btree_set::IntoIter<E>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<E: Ord + Clone> AbstractDomain for SetDomain<E> {
    fn join(&mut self, other: &Self) -> JoinResult {
        let mut changed = JoinResult::Unchanged;
        for e in other.iter() {
            if self.insert(e.clone()) {
                changed = JoinResult::Changed;
            }
        }
        changed
    }
}

impl<E: Ord> SetDomain<E> {
    pub fn singleton(e: E) -> Self {
        let mut s = BTreeSet::new();
        s.insert(e);
        Self(s)
    }
}

// =============================================================================
// MapDomain

/// A map-based abstract domain where join is pointwise join of values.
#[derive(Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct MapDomain<K: Ord, V: AbstractDomain>(BTreeMap<K, V>);

impl<K: Ord, V: AbstractDomain> Default for MapDomain<K, V> {
    fn default() -> Self {
        Self(BTreeMap::new())
    }
}

impl<K: Ord + Debug, V: AbstractDomain + Debug> Debug for MapDomain<K, V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

impl<K: Ord, V: AbstractDomain> From<BTreeMap<K, V>> for MapDomain<K, V> {
    fn from(m: BTreeMap<K, V>) -> Self {
        Self(m)
    }
}

impl<K: Ord, V: AbstractDomain> Deref for MapDomain<K, V> {
    type Target = BTreeMap<K, V>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<K: Ord, V: AbstractDomain> DerefMut for MapDomain<K, V> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<K: Ord, V: AbstractDomain> FromIterator<(K, V)> for MapDomain<K, V> {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl<K: Ord, V: AbstractDomain> IntoIterator for MapDomain<K, V> {
    type Item = (K, V);
    type IntoIter = std::collections::btree_map::IntoIter<K, V>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<K: Ord + Clone, V: AbstractDomain + Clone> AbstractDomain for MapDomain<K, V> {
    fn join(&mut self, other: &Self) -> JoinResult {
        let mut changed = JoinResult::Unchanged;
        for (k, v) in other.iter() {
            changed = changed.combine(self.insert_join(k.clone(), v.clone()));
        }
        changed
    }
}

impl<K: Ord, V: AbstractDomain> MapDomain<K, V> {
    /// Join `v` with `self[k]` if the key exists, otherwise insert `v`.
    pub fn insert_join(&mut self, k: K, v: V) -> JoinResult {
        match self.0.get_mut(&k) {
            Some(existing) => existing.join(&v),
            None => {
                self.0.insert(k, v);
                JoinResult::Changed
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_domain_join() {
        let mut s1: SetDomain<u32> = [1, 2, 3].into_iter().collect();
        let s2: SetDomain<u32> = [2, 3, 4].into_iter().collect();
        assert_eq!(s1.join(&s2), JoinResult::Changed);
        assert_eq!(s1.len(), 4);
        assert_eq!(s1.join(&s2), JoinResult::Unchanged);
    }

    #[test]
    fn test_map_domain_join() {
        let mut m1: MapDomain<u32, SetDomain<u32>> = MapDomain::default();
        m1.insert(0, SetDomain::singleton(10));

        let mut m2: MapDomain<u32, SetDomain<u32>> = MapDomain::default();
        m2.insert(0, SetDomain::singleton(20));
        m2.insert(1, SetDomain::singleton(30));

        assert_eq!(m1.join(&m2), JoinResult::Changed);
        assert_eq!(m1.len(), 2);
        assert!(m1.get(&0).unwrap().contains(&10));
        assert!(m1.get(&0).unwrap().contains(&20));
        assert_eq!(m1.join(&m2), JoinResult::Unchanged);
    }
}
