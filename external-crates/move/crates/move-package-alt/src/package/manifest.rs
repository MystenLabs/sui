// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use codespan_reporting::{
    diagnostic::{Diagnostic, Label},
    term::{
        self,
        termcolor::{ColorChoice, StandardStream},
    },
};

use serde::Deserialize;
use thiserror::Error;
use tracing::debug;

use crate::{
    dependency::{DependencySet, UnpinnedDependencyInfo},
    errors::{FileHandle, Files, Located, Location, TheFile},
    flavor::MoveFlavor,
};

use super::*;
use sha2::{Digest as ShaDigest, Sha256};

// TODO: add 2025 edition
const ALLOWED_EDITIONS: &[&str] = &["2025", "2024", "2024.beta", "legacy"];

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
        BTreeMap<EnvironmentName, BTreeMap<PackageName, Located<ManifestDependencyReplacement>>>,
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

#[derive(Error, Debug)]
#[error("{kind}")]
pub struct ManifestError {
    pub kind: Box<ManifestErrorKind>,
    location: ErrorLocation,
}

#[derive(Debug)]
enum ErrorLocation {
    WholeFile(PathBuf),
    AtLoc(Location),
}

#[derive(Error, Debug)]
pub enum ManifestErrorKind {
    #[error("package name cannot be empty")]
    EmptyPackageName,
    #[error("unsupported edition '{edition}', expected one of '{valid}'")]
    InvalidEdition { edition: String, valid: String },
    #[error("externally resolved dependencies must have exactly one resolver field")]
    BadExternalDependency,
    #[error(
        "dep-replacements.mainnet is invalid because mainnet is not in the [environments] table"
    )]
    MissingEnvironment { env: EnvironmentName },
    #[error(
        // TODO: add a suggested environment (needs to be part of the flavor)
        "you must define at least one environment in the [environments] section of `Move.toml`."
    )]
    NoEnvironments,
    #[error("{}", .0.message())]
    ParseError(#[from] toml_edit::de::Error),
    #[error(transparent)]
    IoError(#[from] std::io::Error),
}

type ManifestResult<T> = Result<T, ManifestError>;

impl<F: MoveFlavor> Manifest<F> {
    /// Read the manifest file at the given path, returning a [`Manifest`].
    // TODO: probably return a more specific error
    pub fn read_from_file(path: impl AsRef<Path>) -> ManifestResult<Self> {
        debug!("Reading manifest from {:?}", path.as_ref());

        let (manifest, file_id) = TheFile::with_file(&path, toml_edit::de::from_str::<Self>)
            .map_err(ManifestError::with_file(&path))?;

        let manifest = manifest.map_err(ManifestError::from_toml(file_id))?;

        manifest.validate_manifest(file_id)?;
        Ok(manifest)
    }

    /// Validate the manifest contents, after deserialization.
    ///
    // TODO: add more validation
    pub fn validate_manifest(&self, handle: FileHandle) -> ManifestResult<()> {
        // Validate package name
        // TODO: this should be impossible now, since [Identifier]s can't be empty
        if self.package.name.as_ref().is_empty() {
            return Err(ManifestError::with_span(self.package.name.location())(
                ManifestErrorKind::EmptyPackageName,
            ));
        }

        // Validate edition
        if !ALLOWED_EDITIONS.contains(&self.package.edition.as_ref().as_str()) {
            return Err(ManifestError::with_span(self.package.edition.location())(
                ManifestErrorKind::InvalidEdition {
                    edition: self.package.edition.as_ref().clone(),
                    valid: ALLOWED_EDITIONS.join(", ").to_string(),
                },
            ));
        }

        // Are there any environments?
        if self.environments().is_empty() {
            return Err(ManifestError::with_file(handle.path())(
                ManifestErrorKind::NoEnvironments,
            ));
        }

        // Do all dep-replacements have valid environments?
        // TODO: maybe better to do by making an `Environment` type?
        for (env, entries) in self.dep_replacements.iter() {
            if !self.environments().contains_key(env) {
                let loc = entries
                    .first_key_value()
                    .expect("dep-replacements.<env> only exists if it has a dep")
                    .1
                    .location();

                return Err(ManifestError::with_span(loc)(
                    ManifestErrorKind::MissingEnvironment { env: env.clone() },
                ));
            }
        }

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
                    if let Some(dep) = &dep.as_ref().dependency {
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

    pub fn package_name(&self) -> &PackageName {
        self.package.name()
    }
}

impl<F: MoveFlavor> PackageMetadata<F> {
    pub fn name(&self) -> &PackageName {
        self.name.as_ref()
    }

    pub fn edition(&self) -> &str {
        self.edition.as_ref()
    }
}

/// Compute a digest of this input data using SHA-256.
pub fn digest(data: &[u8]) -> Digest {
    format!("{:X}", Sha256::digest(data))
}

impl ManifestError {
    fn with_file<T: Into<ManifestErrorKind>>(path: impl AsRef<Path>) -> impl Fn(T) -> Self {
        move |e| ManifestError {
            kind: Box::new(e.into()),
            location: ErrorLocation::WholeFile(path.as_ref().to_path_buf()),
        }
    }

    fn with_span<T: Into<ManifestErrorKind>>(loc: &Location) -> impl Fn(T) -> Self {
        move |e| ManifestError {
            kind: Box::new(e.into()),
            location: ErrorLocation::AtLoc(loc.clone()),
        }
    }

    fn from_toml(file: FileHandle) -> impl Fn(toml_edit::de::Error) -> Self {
        move |e| {
            let location = e
                .span()
                .map(|span| ErrorLocation::AtLoc(Location::new(file, span)))
                .unwrap_or(ErrorLocation::WholeFile(file.path().to_path_buf()));
            ManifestError {
                kind: Box::new(e.into()),
                location,
            }
        }
    }

    /// Convert this error into a codespan Diagnostic
    pub fn to_diagnostic(&self) -> Diagnostic<FileHandle> {
        match &self.location {
            ErrorLocation::WholeFile(path) => {
                Diagnostic::error().with_message(format!("Error while loading `{path:?}`: {self}"))
            }
            ErrorLocation::AtLoc(loc) => Diagnostic::error()
                .with_message(format!("Error while loading `{:?}`", loc.file()))
                .with_labels(vec![Label::primary(loc.file(), loc.span().clone())])
                .with_notes(vec![self.to_string()]),
        }
    }
}
