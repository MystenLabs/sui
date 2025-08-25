// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

mod combine;
pub use combine::CombinedDependency;

mod resolve;
pub use resolve::{ResolvedDependency, ResolverError};

mod pin;
pub use pin::PinnedDependencyInfo;

mod fetch;
pub use fetch::{FetchError, FetchedDependency};

use crate::{
    errors::FileHandle,
    schema::{EnvironmentName, PackageName, PublishAddresses},
};

// TODO(refactor): instead of `Dependency<DepInfo>`, we should just have `DependencyContext`, and
// the dependency types will hold one of those and pass it around.

/// [Dependency] wraps information about the location of a dependency (such as the `git` or `local`
/// fields) with additional metadata about how the dependency is used (such as the source file,
/// enviroment overrides, etc).
///
/// At different stages of the pipeline we have different information about the dependency location
/// (e.g. resolved dependencies have no `External` variant, pinned dependencies have a pinned git
/// dependency, etc). The `DepInfo` type encapsulates these invariants.
#[derive(Debug, Clone)]
struct Dependency<DepInfo> {
    dep_info: DepInfo,

    /// The environment in the dependency's namespace to use. For example, given
    /// ```toml
    /// dep-replacements.mainnet.foo = { ..., use-environment = "testnet" }
    /// ```
    /// `use_environment` variable would be `testnet`
    use_environment: EnvironmentName,

    /// The `rename-from` field for the dependency
    rename_from: Option<PackageName>,

    /// Was this dependency written with `override = true` in its original manifest?
    is_override: bool,

    /// Does the original manifest override the published address?
    addresses: Option<PublishAddresses>,

    /// What manifest or lockfile does this dependency come from?
    containing_file: FileHandle,
}

impl<T> Dependency<T> {
    /// Apply `f` to `self.dep_info`, keeping the remaining fields unchanged
    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> Dependency<U> {
        Dependency {
            dep_info: f(self.dep_info),
            use_environment: self.use_environment,
            is_override: self.is_override,
            addresses: self.addresses,
            containing_file: self.containing_file,
            rename_from: self.rename_from,
        }
    }

    pub fn use_environment(&self) -> &EnvironmentName {
        &self.use_environment
    }

    pub fn rename_from(&self) -> &Option<PackageName> {
        &self.rename_from
    }
}
