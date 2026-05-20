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

/// The dep_info type for the combined stage. This is `ManifestDependencyInfo`, but with the
/// additional invariant that all on-chain dependencies have addresses (i.e. they use the
/// `OnChain` variant, not `OnChainPlaceholder`). This is enforced during combining: `on-chain = true`
/// without a dep-replacement is rejected.
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
                Self::from_default(*file, pkg.clone(), env.name().to_string(), default.clone())?
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
    ) -> ManifestResult<Self> {
        // on-chain = "0x..." belongs in [dep-replacements], not [dependencies]
        if let ManifestDependencyInfo::OnChain(_) = &default.dependency_info {
            return Err(ManifestError::with_file(file)(
                ManifestErrorKind::OnChainDepWithAddress { name },
            ));
        }

        // on-chain = true with no dep-replacement means no address — error
        if let ManifestDependencyInfo::OnChainPlaceholder(_) = &default.dependency_info {
            return Err(ManifestError::with_file(file)(
                ManifestErrorKind::OnChainDepMissingReplacement { name },
            ));
        }

        Ok(Self {
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

        // On-chain deps in [dep-replacements] must use `on-chain = "0x..."`, not `true`
        if let ManifestDependencyInfo::OnChainPlaceholder(_) = &dep.dependency_info {
            return Err(ManifestError::with_file(file)(
                ManifestErrorKind::OnChainReplacementWithoutAddress { name },
            ));
        }

        // On-chain deps use a fixed environment; reject explicit use-environment
        let use_environment = if matches!(&dep.dependency_info, ManifestDependencyInfo::OnChain(_))
        {
            if replacement.use_environment.is_some() {
                return Err(ManifestError::with_file(file)(
                    ManifestErrorKind::OnChainWithUseEnvironment { name },
                ));
            }
            crate::on_chain::fetch::ON_CHAIN_ENV_NAME.to_string()
        } else {
            replacement.use_environment.unwrap_or(source_env_name)
        };

        Ok(Self {
            context: DependencyContext {
                name,
                use_environment,
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
        // on-chain = "0x..." belongs in [dep-replacements], not [dependencies]
        if let ManifestDependencyInfo::OnChain(_) = &default.dependency_info {
            return Err(ManifestError::with_file(file)(
                ManifestErrorKind::OnChainDepWithAddress { name },
            ));
        }

        let dep = replacement.dependency.unwrap_or(default);

        // Enforce invariant: after combining, all on-chain deps must have addresses
        if let ManifestDependencyInfo::OnChainPlaceholder(_) = &dep.dependency_info {
            return Err(ManifestError::with_file(file)(
                ManifestErrorKind::OnChainReplacementWithoutAddress { name },
            ));
        }

        // On-chain deps use a fixed environment; reject explicit use-environment
        let use_environment = if matches!(&dep.dependency_info, ManifestDependencyInfo::OnChain(_))
        {
            if replacement.use_environment.is_some() {
                return Err(ManifestError::with_file(file)(
                    ManifestErrorKind::OnChainWithUseEnvironment { name },
                ));
            }
            crate::on_chain::fetch::ON_CHAIN_ENV_NAME.to_string()
        } else {
            replacement.use_environment.unwrap_or(source_env_name)
        };

        // TODO: possibly additional compatibility checks here?
        Ok(Self {
            context: DependencyContext {
                name,
                use_environment,
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

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;
    use test_log::test;

    use crate::{flavor::vanilla::DEFAULT_ENV_NAME, test_utils::graph_builder::TestPackageGraph};

    const ADDR: &str = "0x0000000000000000000000000000000000000000000000000000000000000001";

    /// on-chain = "0x..." in [dependencies] is rejected
    #[test(tokio::test)]
    async fn on_chain_address_in_deps_rejected() {
        let scenario = TestPackageGraph::new(["root"])
            .add_on_chain_dep("root", "dep", ADDR, |d| d)
            .build();

        assert_snapshot!(
            scenario.root_package_err("root").await,
            @r###"Error while loading dependency <ROOT>/root: On-chain dependency `dep` in `[dependencies]` must use `on-chain = true`. Specify the address in `[dep-replacements]` with `on-chain = "0x..."`.
        "###
        );
    }

    /// on-chain = true in [dependencies] with no replacement is rejected
    #[test(tokio::test)]
    async fn on_chain_flag_without_replacement_rejected() {
        let scenario = TestPackageGraph::new(["root"])
            .add_on_chain_dep("root", "dep", "true", |d| d)
            .build();

        assert_snapshot!(
            scenario.root_package_err("root").await,
            @r###"Error while loading dependency <ROOT>/root: On-chain dependency `dep` requires an address. Add a `[dep-replacements]` entry with `on-chain = "0x..."`.
        "###
        );
    }

    /// on-chain = true in [dep-replacements] is rejected
    #[test(tokio::test)]
    async fn on_chain_flag_in_replacement_rejected() {
        let scenario = TestPackageGraph::new(["root"])
            .add_on_chain_dep("root", "dep", "true", |d| d.in_env(DEFAULT_ENV_NAME))
            .build();

        assert_snapshot!(
            scenario.root_package_err("root").await,
            @r###"Error while loading dependency <ROOT>/root: On-chain dependency `dep` in `[dep-replacements]` must specify an address: `on-chain = "0x..."`.
        "###
        );
    }

    /// on-chain = "0x..." in [dependencies] is rejected even with a replacement
    #[test(tokio::test)]
    async fn on_chain_address_in_deps_with_replacement_rejected() {
        let scenario = TestPackageGraph::new(["root"])
            .add_on_chain_dep("root", "dep", ADDR, |d| d)
            .add_on_chain_dep("root", "dep", ADDR, |d| d.in_env(DEFAULT_ENV_NAME))
            .build();

        assert_snapshot!(
            scenario.root_package_err("root").await,
            @r###"Error while loading dependency <ROOT>/root: On-chain dependency `dep` in `[dependencies]` must use `on-chain = true`. Specify the address in `[dep-replacements]` with `on-chain = "0x..."`.
        "###
        );
    }

    /// on-chain = true + address replacement passes combining and fails during fetch
    /// because Vanilla has no on-chain packages registered.
    #[test(tokio::test)]
    async fn on_chain_with_address_replacement_passes_combining() {
        let scenario = TestPackageGraph::new(["root"])
            .add_on_chain_dep("root", "dep", "true", |d| d)
            .add_on_chain_dep("root", "dep", ADDR, |d| d.in_env(DEFAULT_ENV_NAME))
            .build();

        let err = scenario.root_package_err("root").await;
        // Redact the MOVE_HOME path which varies per user
        let err = err.replace(
            move_command_line_common::env::MOVE_HOME.as_str(),
            "<MOVE_HOME>",
        );
        assert_snapshot!(err, @"Error while loading dependency <MOVE_HOME>/on-chain/_test_env_id/0x0000000000000000000000000000000000000000000000000000000000000001: on-chain package not found: 0x0000000000000000000000000000000000000000000000000000000000000001");
    }
}
