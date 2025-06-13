// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    error::Error,
    fmt::{self, Debug, Display, Formatter},
    ops::Range,
    path::{Path, PathBuf},
};

use codespan_reporting::{
    diagnostic::{Diagnostic, Label},
    files::SimpleFiles,
    term::{
        self,
        termcolor::{ColorChoice, StandardStream},
    },
};

use derive_where::derive_where;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, info};

use crate::{
    dependency::{DependencySet, UnpinnedDependencyInfo},
    errors::{FileHandle, Files, Located, Location, PackageResult, TheFile},
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
pub enum ManifestError {
    #[error("package name cannot be empty")]
    EmptyPackageName { loc: Location },

    #[error("unsupported edition '{}', expected one of '{valid}'", edition.as_ref())]
    InvalidEdition {
        edition: Located<String>,
        valid: String,
    },

    #[error("externally resolved dependencies must have exactly one resolver field")]
    BadExternalDependency { loc: Location },

    #[error("environment {env} is not in the [environments] table")]
    MissingEnvironment { env: EnvironmentName, loc: Location },

    #[error(
        // TODO: add a suggested environment (needs to be part of the flavor)
        "you must define at least one environment in the [environments] section of `Move.toml`."
    )]
    NoEnvironments { file: FileHandle },

    #[error("{}", source.message())]
    ParseError {
        file: FileHandle,
        #[source]
        source: toml_edit::de::Error,
    },

    #[error("unable to read file: {source}")]
    IoError {
        file: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

type ManifestResult<T> = Result<T, ManifestError>;

impl<F: MoveFlavor> Manifest<F> {
    /// Read the manifest file at the given path, returning a [`Manifest`].
    // TODO: probably return a more specific error
    pub fn read_from_file(path: impl AsRef<Path>) -> ManifestResult<Self> {
        debug!("Reading manifest from {:?}", path.as_ref());
        let contents = std::fs::read_to_string(&path).map_err(ManifestError::io_error(&path))?;

        let (manifest, file_id) = TheFile::with_file(&path, toml_edit::de::from_str::<Self>)
            .map_err(ManifestError::io_error(&path))?;

        let manifest = manifest.map_err(ManifestError::toml_error(file_id))?;

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
            return Err(ManifestError::EmptyPackageName {
                loc: self.package.name.location().clone(),
            });
        }

        // Validate edition
        if !ALLOWED_EDITIONS.contains(&self.package.edition.as_ref().as_str()) {
            return Err(ManifestError::InvalidEdition {
                edition: self.package.edition.clone(),
                valid: ALLOWED_EDITIONS.join(", ").to_string(),
            });
        }

        // Are there any environments?
        if self.environments().is_empty() {
            return Err(ManifestError::NoEnvironments { file: handle });
        }

        // Do all dep-replacements have valid environments?
        // TODO: maybe better to do by making an `Environment` type?
        for (env, entries) in self.dep_replacements.iter() {
            if !self.environments().contains_key(env) {
                return Err(ManifestError::MissingEnvironment {
                    loc: entries
                        .first_key_value()
                        .expect("dep-replacements.<env> only exists if it has a dep")
                        .1
                        .location()
                        .clone(),
                    env: env.clone(),
                });
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
        self.name.get_ref()
    }

    pub fn edition(&self) -> &str {
        self.edition.get_ref()
    }
}

/// Compute a digest of this input data using SHA-256.
pub fn digest(data: &[u8]) -> Digest {
    format!("{:X}", Sha256::digest(data))
}

impl ManifestError {
    fn io_error(path: impl AsRef<Path>) -> impl Fn(std::io::Error) -> ManifestError {
        move |source| Self::IoError {
            file: path.as_ref().to_path_buf(),
            source,
        }
    }

    fn toml_error(file: FileHandle) -> impl Fn(toml_edit::de::Error) -> ManifestError {
        move |source| Self::ParseError { file, source }
    }

    fn file(&self) -> &Path {
        match self {
            ManifestError::EmptyPackageName { loc } => loc.path(),
            ManifestError::InvalidEdition { edition, .. } => edition.path(),
            ManifestError::BadExternalDependency { loc } => loc.path(),
            ManifestError::MissingEnvironment { loc, .. } => loc.path(),
            ManifestError::NoEnvironments { file } => file.path(),
            ManifestError::ParseError { file, .. } => file.path(),
            ManifestError::IoError { file, .. } => file,
        }
    }

    fn location(&self) -> Option<Location> {
        match self {
            ManifestError::EmptyPackageName { loc } => Some(loc.clone()),
            ManifestError::InvalidEdition { edition, .. } => Some(edition.location().clone()),
            ManifestError::BadExternalDependency { loc } => Some(loc.clone()),
            ManifestError::MissingEnvironment { env, loc } => Some(loc.clone()),
            ManifestError::NoEnvironments { file } => None,
            ManifestError::ParseError { file, source } => {
                source.span().map(|span| Location::new(*file, span))
            }
            ManifestError::IoError { file, source } => None,
        }
    }

    /// Convert this error into a codespan Diagnostic
    pub fn to_diagnostic(&self) -> Diagnostic<FileHandle> {
        match self.location() {
            None => Diagnostic::error()
                .with_message(format!("Error while loading `{:?}`: {self}", self.file())),
            Some(loc) => Diagnostic::error()
                .with_message(format!("Error while loading `{:?}`", self.file()))
                .with_labels(vec![Label::primary(loc.file(), loc.span().clone())])
                .with_notes(vec![self.to_string()]),
        }
    }

    /// Emit this error to stderr
    pub fn emit(&self) -> Result<(), codespan_reporting::files::Error> {
        let writer = StandardStream::stderr(ColorChoice::Always);
        let config = term::Config {
            display_style: term::DisplayStyle::Rich,
            chars: term::Chars::ascii(),
            ..Default::default()
        };

        let diagnostic = self.to_diagnostic();
        let e = term::emit(&mut writer.lock(), &config, &Files, &diagnostic);
        e
    }
}
