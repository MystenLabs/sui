// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use serde_spanned::Spanned;

use crate::{
    errors::FileHandle,
    package::manifest::{ManifestError, ManifestErrorKind, ManifestResult},
    schema::{
        DefaultDependency, Environment, EnvironmentName, ManifestDependencyInfo, PackageName,
        ReplacementDependency,
    },
};

use super::DependencyContext;

/// The dep_info type for the combined stage.
pub(super) type Combined = ManifestDependencyInfo;

/// A [CombinedDependency] contains dependency information from the `[dependencies]` and
/// `[dep-replacements]` sections of a Move.toml file. System dependencies may be present
/// temporarily but are filtered out during pinning (see [PinnedDependency::replace_system_deps]).
#[derive(Debug, Clone)]
pub struct CombinedDependency {
    pub(super) context: DependencyContext,
    pub(super) dep_info: Combined,
}

impl CombinedDependency {
    /// Combine the `[dependencies]` and `[dep-replacements]` sections of `manifest` (which was read
    /// from `file`).
    pub fn combine_deps(
        file: &FileHandle,
        env: &Environment,
        dep_replacements: &BTreeMap<PackageName, Spanned<ReplacementDependency>>,
        dependencies: &BTreeMap<PackageName, DefaultDependency>,
        system_dependencies: &BTreeMap<PackageName, ReplacementDependency>,
    ) -> ManifestResult<Vec<Self>> {
        let mut result = Vec::new();

        let mut replacements = dep_replacements.clone();

        for (pkg, default) in dependencies.iter() {
            if system_dependencies.contains_key(pkg) {
                return Err(ManifestError::with_file(file)(
                    ManifestErrorKind::ExplicitImplicit { name: pkg.clone() },
                ));
            }
            let combined = if let Some(replacement) = replacements.remove(pkg) {
                Self::from_default_with_replacement(
                    *file,
                    pkg.clone(),
                    env.name().to_string(),
                    default.clone(),
                    replacement.into_inner(),
                )?
            } else {
                Self::from_default(*file, pkg.clone(), env.name().to_string(), default.clone())
            };
            result.push(combined);
        }

        for (pkg, dep) in replacements {
            result.push(Self::from_replacement(
                *file,
                pkg.clone(),
                env.name().to_string(),
                dep.into_inner(),
            )?);
        }

        for (pkg, dep) in system_dependencies {
            result.push(Self::from_replacement(
                *file,
                pkg.clone(),
                env.name().to_string(),
                dep.clone(),
            )?);
        }

        Ok(result)
    }

    /// Specialize an entry in the `[dependencies]` section, for the environment named
    /// `source_env_name`
    pub(crate) fn from_default(
        file: FileHandle,
        name: PackageName,
        source_env_name: EnvironmentName,
        default: DefaultDependency,
    ) -> Self {
        Self {
            context: DependencyContext {
                name,
                use_environment: source_env_name,
                is_override: default.is_override,
                addresses: None,
                containing_file: file,
                rename_from: default.rename_from,
                modes: default.modes,
            },
            dep_info: default.dependency_info,
        }
    }

    /// Load from an entry in the `[dep-replacements]` section that has no corresponding entry in
    /// the `[dependencies]` section of the manifest.
    ///
    /// `source_env_name` refers to the environment name and ID in the original manifest; it is
    /// used as the default environment for the dependency, but will be overridden if `replacement`
    /// specifies `use-environment` field.
    // TODO: replace ManifestResult here
    pub fn from_replacement(
        file: FileHandle,
        name: PackageName,
        source_env_name: EnvironmentName,
        replacement: ReplacementDependency,
    ) -> ManifestResult<Self> {
        let Some(dep) = replacement.dependency else {
            return Err(ManifestError::with_file(file)(ManifestErrorKind::NoDepInfo));
        };

        Ok(Self {
            context: DependencyContext {
                name,
                use_environment: replacement.use_environment.unwrap_or(source_env_name),
                is_override: dep.is_override,
                addresses: replacement.addresses,
                containing_file: file,
                rename_from: dep.rename_from,
                modes: dep.modes,
            },
            dep_info: dep.dependency_info,
        })
    }

    fn from_default_with_replacement(
        file: FileHandle,
        name: PackageName,
        source_env_name: EnvironmentName,
        default: DefaultDependency,
        replacement: ReplacementDependency,
    ) -> ManifestResult<Self> {
        let dep = replacement.dependency.unwrap_or(default);

        // TODO: possibly additional compatibility checks here?
        Ok(Self {
            context: DependencyContext {
                name,
                use_environment: replacement.use_environment.unwrap_or(source_env_name),
                is_override: dep.is_override,
                addresses: replacement.addresses,
                containing_file: file,
                rename_from: dep.rename_from,
                modes: dep.modes,
            },
            dep_info: dep.dependency_info,
        })
    }

    /// Return the name for this dependency
    pub fn name(&self) -> &PackageName {
        &self.context.name
    }
}

impl From<CombinedDependency> for ReplacementDependency {
    fn from(combined: CombinedDependency) -> Self {
        // note: if this changes, you may change the manifest digest format and cause repinning
        ReplacementDependency {
            dependency: Some(DefaultDependency {
                dependency_info: combined.dep_info,
                is_override: combined.context.is_override,
                rename_from: combined.context.rename_from,
                modes: combined.context.modes,
            }),
            addresses: combined.context.addresses,
            use_environment: Some(combined.context.use_environment),
        }
    }
}
