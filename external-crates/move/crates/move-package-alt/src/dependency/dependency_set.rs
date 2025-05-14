// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Conveniences for managing the entire collection of dependencies (including replacements) for a
//! package

use std::{
    collections::{BTreeMap, btree_map},
    fmt::{self, Display},
};

use serde::{Deserialize, Serialize};

use crate::package::{EnvironmentName, PackageName};

/// A set of default dependencies and dep replacements. Within each environment, package names are
/// unique.
///
/// Iterating over a dependency set produces Option<[EnvironmentName]>, [PackageName], T) tuples for
/// all declared dependencies (an environment name of `None` represents the default environment).
///
/// Note that most operations do not do any merging or inheritance - default dependencies are
/// returned with the environment `None`, and the only dependencies returned with `Some(e)` are
/// those that were explicitly added with `Some(e)`.
///
/// To get the correctly merged set of dependencies for a given environment, see [Self::default_deps],
/// [Self::deps_for_env], and [Self::deps_for].
#[derive(Clone, Serialize, Deserialize)]
pub struct DependencySet<T> {
    /// The declared default dependencies
    #[serde(flatten)]
    defaults: BTreeMap<PackageName, T>,

    /// The declared dependency replacements
    // Invariant: if e is in replacements, then replacements[e] is nonempty
    replacements: BTreeMap<EnvironmentName, BTreeMap<PackageName, T>>,
}

impl<T> DependencySet<T> {
    /// Return an empty set of dependencies
    pub fn new() -> Self {
        Self {
            defaults: BTreeMap::new(),
            replacements: BTreeMap::new(),
        }
    }

    /// Return true if [self] has no dependencies
    pub fn is_empty(&self) -> bool {
        self.defaults.is_empty() && self.replacements.iter().all(|(_, it)| it.is_empty())
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
    /// any replacements (if the same package name has both, the replacement is returned).
    pub fn deps_for_env(&self, env: &EnvironmentName) -> BTreeMap<PackageName, &T> {
        let mut result: BTreeMap<PackageName, &T> = BTreeMap::new();
        result.extend(self.defaults.iter().map(|(k, t)| (k.clone(), t)));

        if let Some(deps) = self.replacements.get(env) {
            result.extend(deps.iter().map(|(k, t)| (k.clone(), t)));
        }

        result
    }

    /// Convenience method to return either [default_deps] or [deps_for_env] depending on [env]; an
    /// [env] of [None] indicates a request for the default dependencies.
    pub fn deps_for(&self, env: Option<&EnvironmentName>) -> BTreeMap<PackageName, &T> {
        match env {
            Some(env) => self.deps_for_env(env),
            None => self
                .default_deps()
                .iter()
                .map(|(pkg, dep)| (pkg.clone(), dep))
                .collect(),
        }
    }

    /// Set `self[env][package_name] = value` (dropping previous value if any)
    pub fn insert(&mut self, env: Option<EnvironmentName>, package_name: PackageName, value: T) {
        match env {
            Some(env) => self.replacements.entry(env).or_default(),
            None => &mut self.defaults,
        }
        .insert(package_name, value);
    }

    /// Iterate over the declared entries of this set
    pub fn iter(&self) -> Iter<T> {
        self.into_iter()
    }

    /// Check if the dependency set contains the [`package_name`] for [`env`].
    pub fn contains(&self, env: &Option<EnvironmentName>, package_name: &PackageName) -> bool {
        match env {
            Some(env) => self
                .replacements
                .get(env)
                .is_some_and(|deps| deps.contains_key(package_name)),
            None => self.defaults.contains_key(package_name),
        }
    }

    /// Get the dependency for [`package_name`] in [`env`]. If the dependency is not found,
    /// return None.
    pub fn get(&self, env: &Option<EnvironmentName>, package_name: &PackageName) -> Option<&T> {
        match env {
            Some(env) => self
                .replacements
                .get(env)
                .and_then(|deps| deps.get(package_name)),
            None => self.defaults.get(package_name),
        }
    }

    /// A copy of [self] expanded with an entry (package name, env, dep) for all
    /// packages in [self] and environments in [envs].
    pub fn explode(&mut self, envs: impl IntoIterator<Item = EnvironmentName>)
    where
        T: Clone,
    {
        for env in envs {
            let deps: Vec<(PackageName, T)> = self
                .deps_for_env(&env)
                .into_iter()
                .map(|(pkg, dep)| (pkg, dep.clone()))
                .collect();

            for (pkg, dep) in deps {
                self.insert(Some(env.clone()), pkg, dep)
            }
        }
    }

    /// Remove any replacement entries from [self] that are the same as the default entries.
    ///
    /// Calling [collapse] changes the results of iteration but leaves the `deps_for...` methods
    /// unchanged
    pub fn collapse(&mut self)
    where
        T: Eq,
    {
        for (env, values) in self.replacements.iter_mut() {
            values.retain(|name, value| self.defaults.get(name) != Some(value));
        }
        self.replacements
            .retain(|env, packages| !packages.is_empty());
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
            outer: self.replacements.into_iter(),
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
            outer: self.replacements.iter(),
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

impl<T: Serialize> fmt::Debug for DependencySet<T> {
    /// Format [self] as toml for easy reading and diffing
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let json = serde_json::to_string_pretty(self).expect("dependency set should serialize");
        write!(f, "{json}")
    }
}

#[cfg(test)]
mod tests {
    // TODO
}
