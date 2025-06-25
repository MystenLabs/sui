// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    errors::{FileHandle, PackageResult},
    flavor::MoveFlavor,
    package::manifest::ManifestResult,
    schema::{DefaultDependency, EnvironmentName, ManifestDependencyInfo, ReplacementDependency},
};

use super::{Dependency, DependencySet, Pinned};

pub type Combined = ManifestDependencyInfo;

/// [CombinedDependency]s contain the dependency-type-specific things that users write in their
/// Move.toml files. They are formed by combining the entries from the `[dependencies]` and the
/// `[dep-replacements]` section of the manifest.
#[derive(Debug, Clone)]
pub struct CombinedDependency(pub(super) Dependency<Combined>);

impl CombinedDependency {
    /// Specialize an entry in the `[dependencies]` section, for the environment named
    /// `source_env_name`
    pub fn from_default(
        file: FileHandle,
        source_env_name: EnvironmentName,
        default: DefaultDependency,
    ) -> Self {
        CombinedDependency(Dependency {
            dep_info: default.dependency_info,
            use_environment: source_env_name,
            is_override: default.is_override,
            published_at: None,
            containing_file: file,
        })
    }

    /// Load from an entry in the `[dep-replacements]` section that has no corresponding entry in
    /// the `[dependencies]` section of the manifest. `source_env_name` refers
    /// to the environment name and ID in the original manifest; it is used as the default
    /// environment for the dependency, but will be overridden if `replacement` specifies
    /// `use-environment` field.
    // TODO: replace ManifestResult here
    pub fn from_replacement(
        file: FileHandle,
        source_env_name: EnvironmentName,
        replacement: ReplacementDependency,
    ) -> ManifestResult<Self> {
        let Some(dep) = replacement.dependency else {
            return Err(todo!());
        };

        Ok(CombinedDependency(Dependency {
            dep_info: dep.dependency_info,
            use_environment: replacement.use_environment.unwrap_or(source_env_name),
            is_override: dep.is_override,
            published_at: replacement.published_at,
            containing_file: file,
        }))
    }

    pub fn from_default_with_replacement(
        file: FileHandle,
        source_env_name: EnvironmentName,
        default: DefaultDependency,
        replacement: ReplacementDependency,
    ) -> ManifestResult<Self> {
        let dep = replacement.dependency.unwrap_or(default);

        // TODO: possibly additional compatibility checks here?

        Ok(CombinedDependency(Dependency {
            dep_info: dep.dependency_info,
            use_environment: replacement.use_environment.unwrap_or(source_env_name),
            is_override: dep.is_override,
            published_at: replacement.published_at,
            containing_file: file,
        }))
    }
}

/// For each environment, if none of the implicit dependencies are present in [deps] (or the
/// default environment), then they are all added.
fn add_implicit_deps<F: MoveFlavor>(
    flavor: &F,
    deps: &mut DependencySet<Dependency<Pinned>>,
) -> PackageResult<()> {
    todo!()
}
