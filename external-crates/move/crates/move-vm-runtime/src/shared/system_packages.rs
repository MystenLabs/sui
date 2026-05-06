// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Pinned system packages handed to a `MoveRuntime` at construction time.
//!
//! `SystemPackages` is the public input — built by the host (e.g. `sui-adapter`) and passed in
//! exactly once. The runtime drives it through the standard load/verify/JIT pipeline and pins
//! each successful install in the cache for the lifetime of that runtime. The JIT translator
//! consults the pinned set to rewrite cross-package calls into them as direct function pointers;
//! soundness rests on the `Arc<Package>`s living at least as long as every user package
//! compiled against them, which is true so long as those Arcs sit in the same cache.

use move_core_types::{
    account_address::AccountAddress,
    resolver::{ModuleResolver, SerializedPackage},
};
use tracing::error;

use std::{collections::BTreeMap, convert::Infallible};

/// Public input type. Mirrors `NativeFunctions`: built by the host, handed to
/// `MoveRuntime::new_with_system_packages`. Pinned for the lifetime of the runtime.
#[derive(Debug, Default)]
pub struct SystemPackages {
    packages: Vec<SerializedPackage>,
}

impl SystemPackages {
    pub fn empty() -> Self {
        Self { packages: vec![] }
    }

    /// Build a `SystemPackages` from a vector of serialized packages. Inputs sharing a
    /// `version_id` are deduplicated (first occurrence wins) with a logged error per duplicate;
    /// a duplicate `version_id` in the host's input list is a host bug, not a runtime error,
    /// so we surface it loudly rather than aborting construction.
    pub fn new(packages: Vec<SerializedPackage>) -> Self {
        let mut seen = std::collections::BTreeSet::new();
        let mut out = Vec::with_capacity(packages.len());
        for pkg in packages {
            if !seen.insert(pkg.version_id) {
                error!(
                    version_id = %pkg.version_id,
                    "Duplicate system package version_id in input; keeping first occurrence",
                );
                continue;
            }
            out.push(pkg);
        }
        Self { packages: out }
    }

    pub fn is_empty(&self) -> bool {
        self.packages.is_empty()
    }

    pub fn len(&self) -> usize {
        self.packages.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &SerializedPackage> {
        self.packages.iter()
    }

    /// Convert into the internal resolver used by the install pipeline.
    pub(crate) fn into_resolver(self) -> SystemPackageResolver {
        SystemPackageResolver {
            by_version_id: self
                .packages
                .into_iter()
                .map(|p| (p.version_id, p))
                .collect(),
        }
    }
}

/// Internal resolver wrapping the deduplicated input set, keyed by `version_id` for the
/// `ModuleResolver` callbacks the load/verify/JIT pipeline expects.
#[derive(Debug)]
pub(crate) struct SystemPackageResolver {
    by_version_id: BTreeMap<AccountAddress, SerializedPackage>,
}

impl SystemPackageResolver {
    pub(crate) fn iter(&self) -> impl Iterator<Item = &SerializedPackage> {
        self.by_version_id.values()
    }
}

impl ModuleResolver for SystemPackageResolver {
    type Error = Infallible;

    fn get_packages_static<const N: usize>(
        &self,
        ids: [AccountAddress; N],
    ) -> Result<[Option<SerializedPackage>; N], Self::Error> {
        Ok(ids.map(|id| self.by_version_id.get(&id).cloned()))
    }

    fn get_packages<'a>(
        &self,
        ids: impl ExactSizeIterator<Item = &'a AccountAddress>,
    ) -> Result<Vec<Option<SerializedPackage>>, Self::Error> {
        Ok(ids.map(|id| self.by_version_id.get(id).cloned()).collect())
    }
}
