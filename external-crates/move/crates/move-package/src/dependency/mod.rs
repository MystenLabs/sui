// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

mod combine;
pub use combine::CombinedDependency;

mod resolve;
pub use resolve::ResolverError;

mod pin;
pub(crate) use pin::Pinned;
pub use pin::PinnedDependency;

pub mod fetch;
pub use fetch::FetchError;

use crate::{
    errors::FileHandle,
    schema::{EnvironmentName, ModeName, PackageName, PublishAddresses},
};

/// Metadata about how a dependency is used, shared across all pipeline stages.
#[derive(Debug, Clone)]
pub(super) struct DependencyContext {
    /// The name given to this dependency in the manifest. For modern manifests, this is the same
    /// as the name used for the package in the source code, while for legacy manifests this name
    /// may be different (it is still normalized to be a valid identifier but does not correspond
    /// to the named address).
    pub(super) name: PackageName,

    /// The environment in the dependency's namespace to use. For example, given
    /// ```toml
    /// dep-replacements.mainnet.foo = { ..., use-environment = "testnet" }
    /// ```
    /// `use_environment` variable would be `testnet`
    pub(super) use_environment: EnvironmentName,

    /// The `rename-from` field for the dependency
    pub(super) rename_from: Option<PackageName>,

    /// Was this dependency written with `override = true` in its original manifest?
    pub(super) is_override: bool,

    /// Does the original manifest override the published address?
    pub(super) addresses: Option<PublishAddresses>,

    /// The `modes` field for the dependency
    pub(super) modes: Option<Vec<ModeName>>,

    /// What manifest or lockfile does this dependency come from?
    pub(super) containing_file: FileHandle,
}
