// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    errors::{FileHandle, PackageResult},
    flavor::MoveFlavor,
    package::manifest::ManifestResult,
    schema::{
        DefaultDependency, EnvironmentName, ManifestDependencyInfo, ParsedManifest,
        ReplacementDependency,
    },
};

use super::{Dependency, DependencySet, PinnedDependencyInfo};

pub(super) type Combined = ManifestDependencyInfo;

/// [CombinedDependency]s contain the dependency-type-specific things that users write in their
/// Move.toml files. They are formed by combining the entries from the `[dependencies]` and the
/// `[dep-replacements]` section of the manifest.
#[derive(Debug, Clone)]
pub struct CombinedDependency(pub(super) Dependency<Combined>);

impl CombinedDependency {
    /// Combine the `[dependencies]` and `[dep-replacements]` sections of `manifest` (which was read
    /// from `file`).
    // TODO: add implicit dependencies here too
    pub fn combine_deps(
        file: FileHandle,
        manifest: &ParsedManifest,
    ) -> ManifestResult<DependencySet<Self>> {
        let mut result = DependencySet::new();

        for env in manifest.environments.keys() {
            let mut replacements = manifest
                .dep_replacements
                .get(env.as_ref())
                .cloned()
                .unwrap_or_default();

            for (pkg, default) in manifest.dependencies.iter() {
                let combined = if let Some(replacement) = replacements.remove(pkg.as_ref()) {
                    Self::from_default_with_replacement(
                        file,
                        env.as_ref().clone(),
                        default.clone(),
                        replacement.into_inner(),
                    )?
                } else {
                    Self::from_default(file, env.as_ref().clone(), default.clone())
                };
                result.insert(env.as_ref().clone(), pkg.as_ref().clone(), combined);
            }

            for (pkg, dep) in replacements {
                result.insert(
                    env.as_ref().clone(),
                    pkg.clone(),
                    Self::from_replacement(file, env.as_ref().clone(), dep.as_ref().clone())?,
                );
            }
        }

        Ok(result)
    }

    /// Specialize an entry in the `[dependencies]` section, for the environment named
    /// `source_env_name`
    fn from_default(
        file: FileHandle,
        source_env_name: EnvironmentName,
        default: DefaultDependency,
    ) -> Self {
        Self(Dependency {
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
    fn from_replacement(
        file: FileHandle,
        source_env_name: EnvironmentName,
        replacement: ReplacementDependency,
    ) -> ManifestResult<Self> {
        let Some(dep) = replacement.dependency else {
            return Err(todo!());
        };

        Ok(Self(Dependency {
            dep_info: dep.dependency_info,
            use_environment: replacement.use_environment.unwrap_or(source_env_name),
            is_override: dep.is_override,
            published_at: replacement.published_at,
            containing_file: file,
        }))
    }

    fn from_default_with_replacement(
        file: FileHandle,
        source_env_name: EnvironmentName,
        default: DefaultDependency,
        replacement: ReplacementDependency,
    ) -> ManifestResult<Self> {
        let dep = replacement.dependency.unwrap_or(default);

        // TODO: possibly additional compatibility checks here?

        Ok(Self(Dependency {
            dep_info: dep.dependency_info,
            use_environment: replacement.use_environment.unwrap_or(source_env_name),
            is_override: dep.is_override,
            published_at: replacement.published_at,
            containing_file: file,
        }))
    }
}
