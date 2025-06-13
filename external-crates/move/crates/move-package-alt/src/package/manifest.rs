// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    fmt::{self, Debug, Display, Formatter},
    ops::Range,
    path::Path,
};

use derive_where::derive_where;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use crate::{
    dependency::{DependencySet, UnpinnedDependencyInfo},
    errors::{FileHandle, Located, ManifestError, ManifestErrorKind, PackageResult, TheFile},
    flavor::{MoveFlavor, Vanilla},
};

use super::*;
use sha2::{Digest as ShaDigest, Sha256};

// TODO: add 2025 edition
const ALLOWED_EDITIONS: &[&str] = &["2024", "2024.beta", "legacy"];

// TODO: replace this with something more strongly typed
type Digest = String;

// Note: [Manifest] objects are immutable and should not implement [serde::Serialize]; any tool
// writing these files should use [toml_edit] to set / preserve the formatting, since these are
// user-editable files
#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
#[serde(bound = "")]
pub struct Manifest<F: MoveFlavor> {
    package: PackageMetadata<F>,

    // invariant: environments is nonempty
    environments: BTreeMap<EnvironmentName, F::EnvironmentID>,

    #[serde(default)]
    dependencies: BTreeMap<PackageName, ManifestDependency>,

    /// Replace dependencies for the given environment.
    /// invariant: all keys have entries in `self.environments`
    #[serde(default)]
    dep_replacements:
        BTreeMap<EnvironmentName, BTreeMap<PackageName, ManifestDependencyReplacement>>,
}

#[derive(Debug, Deserialize)]
#[serde(bound = "")]
struct PackageMetadata<F: MoveFlavor> {
    name: Located<PackageName>,
    edition: Located<String>,

    #[serde(flatten)]
    metadata: F::PackageMetadata,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(bound = "")]
#[serde(rename_all = "kebab-case")]
pub struct ManifestDependency {
    #[serde(flatten)]
    dependency_info: UnpinnedDependencyInfo,

    #[serde(rename = "override", default)]
    is_override: bool,

    #[serde(default)]
    rename_from: Option<PackageName>,
}

#[derive(Debug, Deserialize)]
#[serde(bound = "")]
#[serde(rename_all = "kebab-case")]
pub struct ManifestDependencyReplacement {
    #[serde(flatten, default)]
    dependency: Option<ManifestDependency>,

    #[serde(flatten, default)]
    address_info: Option<AddressInfo>,

    #[serde(default)]
    use_environment: Option<EnvironmentName>,
}

impl<F: MoveFlavor> Manifest<F> {
    /// Read the manifest file at the given path, returning a [`Manifest`].
    pub fn read_from_file(path: impl AsRef<Path>) -> PackageResult<Self> {
        debug!("Reading manifest from {:?}", path.as_ref());
        let contents = std::fs::read_to_string(&path)?;

        let (manifest, file_id) = TheFile::with_file(&path, toml_edit::de::from_str::<Self>)?;
        let manifest = manifest?;

        manifest.validate_manifest(file_id)?;
        Ok(manifest)
    }

    /// Validate the manifest contents, after deserialization.
    ///
    // TODO: add more validation
    pub fn validate_manifest(&self, handle: FileHandle) -> PackageResult<()> {
        // Validate package name
        if self.package.name.get_ref().is_empty() {
            let err = ManifestError {
                kind: ManifestErrorKind::EmptyPackageName,
                span: Some(self.package.name.span()),
                handle,
            };
            err.emit()?;
            return Err(err.into());
        }

        // Validate edition
        if !ALLOWED_EDITIONS.contains(&self.package.edition.get_ref().as_str()) {
            let err = ManifestError {
                kind: ManifestErrorKind::InvalidEdition {
                    edition: self.package.edition.get_ref().clone(),
                    valid: ALLOWED_EDITIONS.join(", ").to_string(),
                },
                span: Some(self.package.edition.span()),
                handle,
            };
            err.emit()?;
            return Err(err.into());
        }

        if self.environments().is_empty() {
            let err = ManifestError {
                kind: ManifestErrorKind::NoEnvironments,
                span: None,
                handle,
            };
            err.emit()?;
            return Err(err.into());
        }

        for (env, _) in self.dep_replacements.iter() {
            if !self.environments().contains_key(env) {
                let err = ManifestError {
                    kind: ManifestErrorKind::MissingEnvironment { env: env.clone() },
                    span: None, // TODO
                    handle,
                };
                err.emit()?;
                return Err(err.into());
            }
        }

        Ok(())
    }

    fn write_template(path: impl AsRef<Path>, name: &PackageName) -> PackageResult<()> {
        std::fs::write(
            path,
            r###"
            "###,
        )?;

        Ok(())
    }

    /// Return the dependency set of this manifest, including replacements.
    pub fn dependencies(&self) -> DependencySet<UnpinnedDependencyInfo> {
        let mut deps = DependencySet::new();

        // TODO: this drops everything besides the [UnpinnedDependencyInfo] (e.g. override,
        // published-at, etc).
        for env in self.environments().keys() {
            for (pkg, dep) in self.dependencies.iter() {
                deps.insert(env.clone(), pkg.clone(), dep.dependency_info.clone());
            }

            if let Some(replacements) = self.dep_replacements.get(env) {
                for (pkg, dep) in replacements {
                    if let Some(dep) = &dep.dependency {
                        deps.insert(env.clone(), pkg.clone(), dep.dependency_info.clone());
                    }
                }
            }
        }

        deps
    }

    pub fn environments(&self) -> &BTreeMap<EnvironmentName, F::EnvironmentID> {
        &self.environments
    }
}

/// Compute a digest of this input data using SHA-256.
pub fn digest(data: &[u8]) -> Digest {
    format!("{:X}", Sha256::digest(data))
}
