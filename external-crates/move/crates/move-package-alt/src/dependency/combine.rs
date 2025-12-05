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

use super::Dependency;

// TODO: this should probably be its own type (not including system deps)
pub(super) type Combined = ManifestDependencyInfo;

/// [CombinedDependency]s contain the dependency-type-specific things that users write in their
/// Move.toml files. They are formed by combining the entries from the `[dependencies]` and the
/// `[dep-replacements]` section of the manifest.
#[derive(Debug, Clone)]
pub struct CombinedDependency(pub(super) Dependency<Combined>);

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
        Self(Dependency {
            name,
            dep_info: default.dependency_info,
            use_environment: source_env_name,
            is_override: default.is_override,
            addresses: None,
            containing_file: file,
            rename_from: default.rename_from,
            modes: default.modes,
        })
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

        Ok(Self(Dependency {
            name,
            dep_info: dep.dependency_info,
            use_environment: replacement.use_environment.unwrap_or(source_env_name),
            is_override: dep.is_override,
            addresses: replacement.addresses,
            containing_file: file,
            rename_from: dep.rename_from,
            modes: dep.modes,
        }))
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

        Ok(Self(Dependency {
            name,
            dep_info: dep.dependency_info,
            use_environment: replacement.use_environment.unwrap_or(source_env_name),
            is_override: dep.is_override,
            addresses: replacement.addresses,
            containing_file: file,
            rename_from: dep.rename_from,
            modes: dep.modes,
        }))
    }

    /// Return the name for this dependency
    pub fn name(&self) -> &PackageName {
        &self.0.name
    }
}

impl From<CombinedDependency> for ReplacementDependency {
    fn from(combined: CombinedDependency) -> Self {
        // note: if this changes, you may change the manifest digest format and cause repinning
        ReplacementDependency {
            dependency: Some(DefaultDependency {
                dependency_info: combined.0.dep_info,
                is_override: combined.0.is_override,
                rename_from: combined.0.rename_from,
                modes: combined.0.modes,
            }),
            addresses: combined.0.addresses,
            use_environment: Some(combined.0.use_environment),
        }
    }
}
