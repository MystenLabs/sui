//! Conveniences for managing the entire collection of dependencies (including overrides) for a
//! package
use std::collections::{btree_map, BTreeMap};

use serde::{Deserialize, Serialize};

use crate::package::{EnvironmentName, PackageName};

/// A set of default dependencies and dep overrides. Within each environment, package names are
/// unique.
///
/// Iterating over a dependency set produces (Option<EnvironmentName>, PackageName, T) tuples; the
/// environment name is None to represent the default environment. See [DependencySet::iter] for
/// more details.
#[derive(Clone, Serialize, Deserialize)]
pub struct DependencySet<T> {
    #[serde(flatten)]
    defaults: BTreeMap<PackageName, T>,
    overrides: BTreeMap<EnvironmentName, BTreeMap<PackageName, T>>,
}

impl<T> DependencySet<T> {
    /// Return an empty set of dependencies
    pub fn new() -> Self {
        Self {
            defaults: BTreeMap::new(),
            overrides: BTreeMap::new(),
        }
    }

    /// Return true if [self] has no dependencies
    pub fn is_empty(&self) -> bool {
        self.defaults.is_empty() && self.overrides.iter().all(|(_, it)| it.is_empty())
    }

    /// Combine all elements of [sets] into one. Any duplicate entries (with the same environment
    /// and package name) are silently dropped.
    pub fn merge(sets: impl IntoIterator<Item = Self>) -> Self {
        sets.into_iter().flatten().collect()
    }

    /// Return the default dependencies (those associated with [None])
    pub fn default_deps(&self) -> &BTreeMap<PackageName, T> {
        &self.defaults
    }

    /// Return all of the dependencies for [env]: this includes the default dependencies along with
    /// any overrides (if the same package name has both, the override is returned).
    pub fn deps_for_env(&self, env: &EnvironmentName) -> BTreeMap<PackageName, &T> {
        let mut result: BTreeMap<PackageName, &T> = BTreeMap::new();
        result.extend(self.defaults.iter().map(|(k, t)| (k.clone(), t)));

        if let Some(deps) = self.overrides.get(env) {
            result.extend(deps.iter().map(|(k, t)| (k.clone(), t)));
        }

        result
    }

    /// Set `self[env][package_name] = value` (dropping previous value if any)
    pub fn insert(&mut self, env: Option<EnvironmentName>, package_name: PackageName, value: T) {
        match env {
            Some(env) => self.overrides.entry(env).or_default(),
            None => &mut self.defaults,
        }
        .insert(package_name, value);
    }

    /// Produce `(env, package, dep)` for all _declared_ dependencies. Note that this doesn't do
    /// any inheritance - default dependencies are returned with the environment `None`, and the
    /// only dependencies returned in the environment `Some(e)` are those that were added in
    /// `Some(e)`.
    ///
    /// To get the correctly merged set of dependencies for a given environment, see [default_deps]
    /// and [deps_for_env].
    pub fn iter(&self) -> Iter<T> {
        self.into_iter()
    }
}

pub struct IntoIter<T> {
    environment_name: Option<EnvironmentName>,
    inner: btree_map::IntoIter<PackageName, T>,
    outer: btree_map::IntoIter<EnvironmentName, BTreeMap<PackageName, T>>,
}

impl<T> Iterator for IntoIter<T> {
    type Item = (Option<EnvironmentName>, PackageName, T);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some((name, value)) = self.inner.next() {
                return Some((self.environment_name.clone(), name, value));
            }

            let (env, table) = self.outer.next()?;

            self.environment_name = Some(env);
            self.inner = table.into_iter();
        }
    }
}

impl<T> IntoIterator for DependencySet<T> {
    type Item = (Option<EnvironmentName>, PackageName, T);

    type IntoIter = IntoIter<T>;

    /// Returns an iterator that produces `(env, package, dep)` for all _declared_ dependencies.
    /// Note that this doesn't do any inheritance - default dependencies are returned with the
    /// environment `None`, and the only dependencies returned in the environment `Some(e)` are
    /// those that were added in `Some(e)`.
    ///
    /// To get the correctly merged set of dependencies for a given environment, see [default_deps]
    /// and [deps_for_env].
    fn into_iter(self) -> Self::IntoIter {
        IntoIter {
            environment_name: None,
            inner: self.defaults.into_iter(),
            outer: self.overrides.into_iter(),
        }
    }
}

pub struct Iter<'a, T> {
    environment_name: Option<&'a EnvironmentName>,
    inner: btree_map::Iter<'a, PackageName, T>,
    outer: btree_map::Iter<'a, EnvironmentName, BTreeMap<PackageName, T>>,
}

impl<'a, T> Iterator for Iter<'a, T> {
    type Item = (Option<&'a EnvironmentName>, &'a PackageName, &'a T);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some((name, value)) = self.inner.next() {
                return Some((self.environment_name, name, value));
            }

            let (env, table) = self.outer.next()?;
            self.environment_name = Some(env);
            self.inner = table.iter();
        }
    }
}

impl<'a, T> IntoIterator for &'a DependencySet<T> {
    type Item = (Option<&'a EnvironmentName>, &'a PackageName, &'a T);

    type IntoIter = Iter<'a, T>;

    /// Returns an iterator that produces `(env, package, dep)` for all _declared_ dependencies.
    /// Note that this doesn't do any inheritance - default dependencies are returned with the
    /// environment `None`, and the only dependencies returned in the environment `Some(e)` are
    /// those that were added in `Some(e)`.
    ///
    /// To get the correctly merged set of dependencies for a given environment, see [default_deps]
    /// and [deps_for_env].
    fn into_iter(self) -> Self::IntoIter {
        Iter {
            environment_name: None,
            inner: self.defaults.iter(),
            outer: self.overrides.iter(),
        }
    }
}

impl<T> FromIterator<(Option<EnvironmentName>, PackageName, T)> for DependencySet<T> {
    /// If [iter] produces multiple items with the same environment and package, only one of them
    /// is retained; the others are silently dropped.
    fn from_iter<I: IntoIterator<Item = (Option<EnvironmentName>, PackageName, T)>>(
        iter: I,
    ) -> Self {
        let mut result: DependencySet<T> = DependencySet::new();

        for (env, package_name, value) in iter {
            result.insert(env, package_name, value);
        }

        result
    }
}

// Note: can't be derived because that adds a spurious T: Default bound
impl<T> Default for DependencySet<T> {
    /// The empty dependency set
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Extend<(Option<EnvironmentName>, PackageName, T)> for DependencySet<T> {
    fn extend<I: IntoIterator<Item = (Option<EnvironmentName>, PackageName, T)>>(
        &mut self,
        iter: I,
    ) {
        for (env, pack, value) in iter {
            self.insert(env, pack, value);
        }
    }
}
