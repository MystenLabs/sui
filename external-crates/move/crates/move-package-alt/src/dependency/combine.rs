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
/// `OnChainAt` variant, not `OnChain`). This is enforced during combining: `on-chain = true`
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
        if let ManifestDependencyInfo::OnChainAt(_) = &default.dependency_info {
            return Err(ManifestError::with_file(file)(
                ManifestErrorKind::OnChainDepWithAddress { name },
            ));
        }

        // on-chain = true with no dep-replacement means no address — error
        if let ManifestDependencyInfo::OnChain(_) = &default.dependency_info {
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
        if let ManifestDependencyInfo::OnChain(_) = &dep.dependency_info {
            return Err(ManifestError::with_file(file)(
                ManifestErrorKind::OnChainReplacementWithoutAddress { name },
            ));
        }

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
        // on-chain = "0x..." belongs in [dep-replacements], not [dependencies]
        if let ManifestDependencyInfo::OnChainAt(_) = &default.dependency_info {
            return Err(ManifestError::with_file(file)(
                ManifestErrorKind::OnChainDepWithAddress { name },
            ));
        }

        let dep = replacement.dependency.unwrap_or(default);

        // Enforce invariant: after combining, all on-chain deps must have addresses
        if let ManifestDependencyInfo::OnChain(_) = &dep.dependency_info {
            return Err(ManifestError::with_file(file)(
                ManifestErrorKind::OnChainReplacementWithoutAddress { name },
            ));
        }

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

#[cfg(test)]
mod tests {
    use test_log::test;

    use crate::{
        PackageLoader,
        flavor::Vanilla,
        schema::Environment,
        test_utils::{self, basic_manifest},
    };

    fn test_env() -> Environment {
        Environment::new("mainnet".to_string(), "35834a8a".to_string())
    }

    /// on-chain = "0x..." in [dependencies] is rejected
    #[test(tokio::test)]
    async fn on_chain_address_in_deps_rejected() {
        let project = test_utils::project()
            .file(
                "Move.toml",
                &format!(
                    "{}\n[dependencies]\nfoo = {{ on-chain = \"0x01\" }}\n",
                    basic_manifest("test", "0.0.1")
                ),
            )
            .build();

        let err = PackageLoader::new(&project.root(), test_env(), Vanilla)
            .load()
            .await
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("must use `on-chain = true`"), "got: {msg}");
    }

    /// on-chain = true in [dependencies] with no replacement is rejected
    #[test(tokio::test)]
    async fn on_chain_flag_without_replacement_rejected() {
        let project = test_utils::project()
            .file(
                "Move.toml",
                &format!(
                    "{}\n[dependencies]\nfoo = {{ on-chain = true }}\n",
                    basic_manifest("test", "0.0.1")
                ),
            )
            .build();

        let err = PackageLoader::new(&project.root(), test_env(), Vanilla)
            .load()
            .await
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("requires an address"), "got: {msg}");
    }

    /// on-chain = true in [dep-replacements] is rejected
    #[test(tokio::test)]
    async fn on_chain_flag_in_replacement_rejected() {
        let project = test_utils::project()
            .file(
                "Move.toml",
                &format!(
                    "{}\n[dep-replacements]\nmainnet.foo = {{ on-chain = true }}\n",
                    basic_manifest("test", "0.0.1")
                ),
            )
            .build();

        let err = PackageLoader::new(&project.root(), test_env(), Vanilla)
            .load()
            .await
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("must specify an address"), "got: {msg}");
    }

    /// on-chain = "0x..." in [dependencies] is rejected even with a replacement
    #[test(tokio::test)]
    async fn on_chain_address_in_deps_with_replacement_rejected() {
        let project = test_utils::project()
            .file(
                "Move.toml",
                &format!(
                    "{}\n\
                     [dependencies]\n\
                     foo = {{ on-chain = \"0x01\" }}\n\
                     \n\
                     [dep-replacements]\n\
                     mainnet.foo = {{ on-chain = \"0x02\" }}\n",
                    basic_manifest("test", "0.0.1")
                ),
            )
            .build();

        let err = PackageLoader::new(&project.root(), test_env(), Vanilla)
            .load()
            .await
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("must use `on-chain = true`"), "got: {msg}");
    }

    /// on-chain = true in [dependencies] with on-chain = "0x..." replacement succeeds
    #[test(tokio::test)]
    async fn on_chain_flag_with_address_replacement_ok() {
        let project = test_utils::project()
            .file(
                "Move.toml",
                &format!(
                    "{}\n\
                     [dependencies]\n\
                     foo = {{ on-chain = true }}\n\
                     \n\
                     [dep-replacements]\n\
                     mainnet.foo = {{ on-chain = \"0x0000000000000000000000000000000000000000000000000000000000000001\" }}\n",
                    basic_manifest("test", "0.0.1")
                ),
            )
            .build();

        // This should fail later (during fetch, not during combining), since we can't
        // actually fetch from chain in tests. But it should NOT fail with a combine error.
        let result = PackageLoader::new(&project.root(), test_env(), Vanilla)
            .load()
            .await;
        match result {
            Ok(_) => panic!("expected fetch error, got success"),
            Err(e) => {
                let msg = e.to_string();
                // Should NOT be a combine-time error
                assert!(!msg.contains("must use `on-chain = true`"), "got: {msg}");
                assert!(!msg.contains("must specify an address"), "got: {msg}");
                assert!(!msg.contains("requires an address"), "got: {msg}");
            }
        }
    }
}
