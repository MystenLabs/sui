//! Conveniences for managing the entire collection of dependencies (including overrides) for a
//! package
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::package::{EnvironmentName, PackageName};

/// A set of default dependencies and dep overrides. Within each environment, package names are
/// unique
#[derive(Clone, Serialize, Deserialize)]
pub struct DependencySet<T> {
    #[serde(flatten)]
    inner: BTreeMap<Option<EnvironmentName>, BTreeMap<PackageName, T>>,
}

impl<T> DependencySet<T> {
    /// Return an empty set of dependencies
    pub fn new() -> Self {
        Self {
            inner: BTreeMap::new(),
        }
    }

    /// Combine all elements of [sets] into one. Any duplicate entries (with the same environment
    /// and package name) are silently dropped.
    pub fn merge(sets: impl IntoIterator<Item = Self>) -> Self {
        // Note: we could use std::iter::flatten here, but implementing IntoIterator::IteratorType
        // is a nightmare

        let mut result = Self::new();
        for set in sets {
            for (env, package_name, value) in set.into_iter() {
                result.insert(env, package_name, value);
            }
        }
        result
    }

    /// Return all of the dependencies for [env]: this includes the default dependencies along with
    /// any overrides (if the same package name has both, the override is returned).
    pub fn deps_for_env(&self, env: EnvironmentName) -> BTreeMap<PackageName, T>
    where
        T: Clone,
    {
        let mut result = match self.inner.get(&None) {
            Some(deps) => deps.clone(),
            None => BTreeMap::new(),
        };

        if let Some(deps) = self.inner.get(&Some(env)) {
            for (package, dep) in deps.iter() {
                result.insert(package.clone(), dep.clone());
            }
        }

        result
    }

    /// Set `self[env][package_name] = value` (dropping previous value if any)
    pub fn insert(&mut self, env: Option<EnvironmentName>, package_name: PackageName, value: T) {
        self.inner
            .entry(env)
            .or_default()
            .insert(package_name, value);
    }

    /// Return a DependencySet with the same structure as [self] but with each entry transformed by
    /// [f].
    pub fn map<R, F>(&self, f: F) -> DependencySet<R>
    where
        F: Fn(&T) -> R,
    {
        self.iter()
            .map(|(env, package, v)| (env.clone(), package.clone(), f(v)))
            .collect()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Option<EnvironmentName>, &PackageName, &T)> {
        self.inner.iter().flat_map(move |(env, deps)| {
            deps.iter()
                .map(move |(package_name, dep)| (env, package_name, dep))
        })
    }

    pub fn into_iter(self) -> impl Iterator<Item = (Option<EnvironmentName>, PackageName, T)> {
        self.inner.into_iter().flat_map(move |(env, deps)| {
            deps.into_iter()
                .map(move |(package_name, dep)| (env.clone(), package_name, dep))
        })
    }
}

impl<T> FromIterator<(Option<EnvironmentName>, PackageName, T)> for DependencySet<T> {
    /// If [iter] produces multiple items with the same environment and package,
    fn from_iter<I: IntoIterator<Item = (Option<EnvironmentName>, PackageName, T)>>(
        iter: I,
    ) -> Self {
        let mut result: BTreeMap<Option<EnvironmentName>, BTreeMap<PackageName, T>> =
            BTreeMap::new();

        for (env, package_name, value) in iter {
            result.entry(env).or_default().insert(package_name, value);
        }

        Self { inner: result }
    }
}
