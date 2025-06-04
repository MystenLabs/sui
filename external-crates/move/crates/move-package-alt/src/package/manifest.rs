// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    fmt::{self, Debug, Display, Formatter},
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

    #[serde(default)]
    environments: BTreeMap<EnvironmentName, F::EnvironmentID>,

    #[serde(default)]
    dependencies: BTreeMap<PackageName, ManifestDependency<F>>,
    /// Replace dependencies for the given environment.
    #[serde(default)]
    dep_replacements:
        BTreeMap<EnvironmentName, BTreeMap<PackageName, ManifestDependencyReplacement<F>>>,
}

#[derive(Debug, Deserialize)]
#[serde(bound = "")]
struct PackageMetadata<F: MoveFlavor> {
    name: Located<PackageName>,
    edition: Located<String>,

    #[serde(flatten)]
    metadata: F::PackageMetadata,
}

#[derive(Deserialize, Debug)]
#[serde(bound = "")]
#[serde(rename_all = "kebab-case")]
pub struct ManifestDependency<F: MoveFlavor> {
    #[serde(flatten)]
    dependency_info: UnpinnedDependencyInfo<F>,

    #[serde(rename = "override", default)]
    is_override: bool,

    #[serde(default)]
    rename_from: Option<PackageName>,
}

#[derive(Debug, Deserialize)]
#[serde(bound = "")]
#[serde(rename_all = "kebab-case")]
pub struct ManifestDependencyReplacement<F: MoveFlavor> {
    #[serde(flatten, default)]
    dependency: Option<ManifestDependency<F>>,

    #[serde(flatten, default)]
    address_info: Option<F::AddressInfo>,

    #[serde(default)]
    use_environment: Option<EnvironmentName>,
}

impl<F: MoveFlavor> Manifest<F> {
    /// Read the manifest file at the given path, returning a [`Manifest`].
    pub fn read_from_file(path: impl AsRef<Path>) -> PackageResult<Self> {
        debug!("Reading manifest from {:?}", path.as_ref());
        let contents = std::fs::read_to_string(&path)?;

        let (manifest, file_id) = TheFile::with_file(&path, toml_edit::de::from_str::<Self>)?;

        match manifest {
            Ok(manifest) => {
                manifest.validate_manifest(file_id)?;
                Ok(manifest)
            }
            Err(err) => Err(err.into()),
        }
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
    pub fn dependencies(&self) -> DependencySet<UnpinnedDependencyInfo<F>> {
        let mut deps = DependencySet::new();

        for (name, dep) in &self.dependencies {
            deps.insert(None, name.clone(), dep.dependency_info.clone());
        }

        for (env, replacements) in &self.dep_replacements {
            for (name, dep) in replacements {
                if let Some(dep) = &dep.dependency {
                    deps.insert(Some(env.clone()), name.clone(), dep.dependency_info.clone());
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
