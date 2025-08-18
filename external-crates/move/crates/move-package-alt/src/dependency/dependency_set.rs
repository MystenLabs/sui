// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Conveniences for managing the entire collection of dependencies (including replacements) for a
//! package

use std::collections::{BTreeMap, btree_map};

use derive_where::derive_where;

use crate::{package::EnvironmentName, schema::PackageName};

// TODO: the API for this type is a bit of a historical artifact - it used to also have a notion of
// default dependencies and do merging. We now do that much earlier in the process. So this type
// could potentially safely expose more of its implementation to get a cleaner API

/// A set of all dependencies for a package - keyed by the package's environments and the package
/// names.
///
/// For convenience, iteration produces (EnvironmentName, PackageName, T) triples
#[derive(Clone, Debug)]
#[derive_where(Default)]
pub struct DependencySet<T> {
    inner: BTreeMap<EnvironmentName, BTreeMap<PackageName, T>>,
}

impl<T> DependencySet<T> {
    /// Return an empty set of dependencies
    pub fn new() -> Self {
        Self::default()
    }

    /// Return true if [self] has no dependencies
    ///
    /// ```
    /// use move_package_alt::dependency::DependencySet;
    /// use move_package_alt::schema::{EnvironmentName, PackageName};
    /// let mut example = DependencySet::new();
    /// assert!(example.is_empty());
    ///
    /// example.insert(EnvironmentName::from("env"), PackageName::new("pkg").unwrap(), "dep");
    /// assert!(!example.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.inner.iter().all(|(_, it)| it.is_empty())
    }

    /// Combine all elements of [sets] into one. Any duplicate entries (with the same environment
    /// and package name) are silently dropped.
    pub fn merge(sets: impl IntoIterator<Item = Self>) -> Self {
        sets.into_iter().flatten().collect()
    }

    /// Return the dependencies for `env`
    pub fn deps_for(&self, env: &EnvironmentName) -> Option<&BTreeMap<PackageName, T>> {
        self.inner.get(env)
    }

    /// Set `self[env][package_name] = value` (returning previous value if any)
    pub fn insert(&mut self, env: EnvironmentName, pkg: PackageName, value: T) {
        self.inner.entry(env).or_default().insert(pkg, value);
    }

    /// Iterate over the declared entries of this set
    pub fn iter(&self) -> Iter<'_, T> {
        self.into_iter()
    }

    /// Check if the dependency set contains the [`package_name`] for [`env`].
    pub fn contains(&self, env: &EnvironmentName, pkg: &PackageName) -> bool {
        self.get(env, pkg).is_some()
    }

    /// Get the dependency for [`package_name`] in [`env`]. If the dependency is not found,
    /// return None.
    pub fn get(&self, env: &EnvironmentName, package_name: &PackageName) -> Option<&T> {
        self.inner.get(env).and_then(|deps| deps.get(package_name))
    }

    /// Get the dependency for [`package_name`] in [`env`]. If the dependency is not found,
    /// return None.
    pub fn get_mut(&mut self, env: &EnvironmentName, pkg: &PackageName) -> Option<&mut T> {
        self.inner.get_mut(env).and_then(|deps| deps.get_mut(pkg))
    }

    /// Remove and return the entry for `env` and `pkg`
    pub fn remove(&mut self, env: &EnvironmentName, pkg: &PackageName) -> Option<T> {
        self.inner
            .get_mut(env)
            .and_then(|packages| packages.remove(pkg))
    }
}

pub struct IntoIter<T> {
    // invariant: only None at the end of iteration
    inner: Option<(EnvironmentName, btree_map::IntoIter<PackageName, T>)>,
    outer: btree_map::IntoIter<EnvironmentName, BTreeMap<PackageName, T>>,
}

impl<T> Iterator for IntoIter<T> {
    type Item = (EnvironmentName, PackageName, T);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let Some((env, inner)) = &mut self.inner else {
                return None;
            };

            if let Some((pkg, v)) = inner.next() {
                return Some((env.clone(), pkg, v));
            }

            self.inner = self.outer.next().map(|(env, m)| (env, m.into_iter()));
        }
    }
}

impl<T> IntoIterator for DependencySet<T> {
    type Item = (EnvironmentName, PackageName, T);

    type IntoIter = IntoIter<T>;

    /// Returns an iterator that produces `(env, package, dep)`
    fn into_iter(self) -> Self::IntoIter {
        let mut outer = self.inner.into_iter();
        let inner = outer.next().map(|(env, m)| (env, m.into_iter()));

        IntoIter { outer, inner }
    }
}

pub struct Iter<'a, T> {
    // invariant: only None at the end of iteration
    inner: Option<(&'a EnvironmentName, btree_map::Iter<'a, PackageName, T>)>,
    outer: btree_map::Iter<'a, EnvironmentName, BTreeMap<PackageName, T>>,
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = (&'a EnvironmentName, &'a PackageName, &'a T);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let Some((env, inner)) = &mut self.inner else {
                return None;
            };

            if let Some((pkg, v)) = inner.next() {
                return Some((env, pkg, v));
            }

            self.inner = self.outer.next().map(|(env, m)| (env, m.iter()));
        }
    }
}

impl<'a, T> IntoIterator for &'a DependencySet<T> {
    type Item = (&'a EnvironmentName, &'a PackageName, &'a T);

    type IntoIter = Iter<'a, T>;

    /// Returns an iterator that produces `(env, package, dep)`
    fn into_iter(self) -> Self::IntoIter {
        let mut outer = self.inner.iter();
        let inner = outer.next().map(|(env, m)| (env, m.iter()));

        Iter { outer, inner }
    }
}

impl<T> FromIterator<(EnvironmentName, PackageName, T)> for DependencySet<T> {
    /// If [iter] produces multiple items with the same environment and package, only one of them
    /// is retained; the others are silently dropped.
    fn from_iter<I: IntoIterator<Item = (EnvironmentName, PackageName, T)>>(iter: I) -> Self {
        let mut result: DependencySet<T> = DependencySet::new();

        for (env, package_name, value) in iter {
            result.insert(env, package_name, value);
        }

        result
    }
}

impl<T> Extend<(EnvironmentName, PackageName, T)> for DependencySet<T> {
    fn extend<I: IntoIterator<Item = (EnvironmentName, PackageName, T)>>(&mut self, iter: I) {
        for (env, pack, value) in iter {
            self.insert(env, pack, value);
        }
    }
}
