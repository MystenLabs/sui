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
        for (env, _) in self.environments() {
            let defaults: BTreeMap<PackageName, UnpinnedDependencyInfo> = self
                .dependencies
                .iter()
                .map(|(pkg, dep)| (pkg.clone(), dep.dependency_info.clone()))
                .collect();

            let replacements: BTreeMap<PackageName, UnpinnedDependencyInfo> = self
                .dep_replacements
                .get(env)
                .unwrap_or(&BTreeMap::new())
                .iter()
                .filter_map(|(pkg, dep)| {
                    dep.dependency
                        .clone()
                        .map(|d| (pkg.clone(), d.dependency_info))
                })
                .collect();

            let combined = map_zip(defaults, replacements, |_, def, rep| {
                rep.or(def).expect("map_zip doesn't pass (None,None)")
            });

            for (pkg, dep) in combined.into_iter() {
                deps.insert(env.clone(), pkg, dep);
            }
        }

        deps
    }

    pub fn environments(&self) -> &BTreeMap<EnvironmentName, F::EnvironmentID> {
        &self.environments
    }
}

/// Produce a new map `m` containing the union of the keys of `m1` and `m2`, with `m[k]` given by
/// `f(m1.get(k), m2.get(k))`
///
/// `f(_, None, None)` is never called
///
/// Example:
/// ```
/// fn main() {
///     let m1 = BTreeMap::from([("a", 1), ("b", 2)]);
///     let m2 = BTreeMap::from([("b", 2), ("c", 3)]);
///
///     let zipped = map_zip(m1, m2, |_k, v1, v2| v1.unwrap_or_default() + v2.unwrap_or_default());
///
///     let expected = BTreeMap::from([("a", 1), ("b", 4), ("c", 3)]);
///
///     assert_eq!(zipped, expected);
/// }
/// ```
// TODO: maybe this already exists somewhere, or could be moved into a utility module
fn map_zip<K: Ord, V1, V2, V, F: Fn(&K, Option<V1>, Option<V2>) -> V>(
    mut m1: BTreeMap<K, V1>,
    mut m2: BTreeMap<K, V2>,
    f: F,
) -> BTreeMap<K, V> {
    let mut result: BTreeMap<K, V> = BTreeMap::new();

    for (k, v1) in m1.into_iter() {
        let v = f(&k, Some(v1), m2.remove(&k));
        result.insert(k, v);
    }

    for (k, v2) in m2.into_iter() {
        let v = f(&k, None, Some(v2));
        result.insert(k, v);
    }

    result
}

/// Compute a digest of this input data using SHA-256.
pub fn digest(data: &[u8]) -> Digest {
    format!("{:X}", Sha256::digest(data))
}
