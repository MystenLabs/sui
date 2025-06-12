// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
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
    errors::{FileHandle, Located, PackageResult, TheFile},
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

#[derive(Error, Debug)]
#[error("Invalid manifest: {kind}")]
pub struct ManifestError {
    pub kind: ManifestErrorKind,
    pub span: Option<Range<usize>>,
    pub file: PathBuf,
}

#[derive(Error, Debug)]
pub enum ManifestErrorKind {
    #[error("package name cannot be empty")]
    EmptyPackageName,
    #[error("unsupported edition '{edition}', expected one of '{valid}'")]
    InvalidEdition { edition: String, valid: String },
    #[error("externally resolved dependencies must have exactly one resolver field")]
    BadExternalDependency,
    #[error("environment {env} is not in the [environments] table")]
    MissingEnvironment { env: EnvironmentName },
    #[error(
        // TODO: add a suggested environment (needs to be part of the flavor)
        "you must define at least one environment in the [environments] section of `Move.toml`."
    )]
    NoEnvironments,
    #[error(transparent)]
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
        let contents = std::fs::read_to_string(&path).map_err(ManifestError::with_file(&path))?;

        let (manifest, file_id) = TheFile::with_file(&path, toml_edit::de::from_str::<Self>)
            .map_err(ManifestError::with_file(&path))?;

        let manifest = manifest.map_err(ManifestError::from_toml(&path))?;

        manifest.validate_manifest(file_id)?;
        Ok(manifest)
    }

    /// Validate the manifest contents, after deserialization.
    ///
    // TODO: add more validation
    pub fn validate_manifest(&self, handle: FileHandle) -> ManifestResult<()> {
        // Validate package name
        // TODO: this should be impossible now, since [Identifier]s can't be empty
        if self.package.name.get_ref().is_empty() {
            return Err(ManifestError {
                kind: ManifestErrorKind::EmptyPackageName,
                span: Some(self.package.name.span()),
                file: handle.path().to_path_buf(),
            });
        }

        // Validate edition
        if !ALLOWED_EDITIONS.contains(&self.package.edition.get_ref().as_str()) {
            return Err(ManifestError {
                kind: ManifestErrorKind::InvalidEdition {
                    edition: self.package.edition.get_ref().clone(),
                    valid: ALLOWED_EDITIONS.join(", ").to_string(),
                },
                span: Some(self.package.edition.span()),
                file: handle.path().to_path_buf(),
            });
        }

        // Are there any environments?
        if self.environments().is_empty() {
            return Err(ManifestError {
                kind: ManifestErrorKind::NoEnvironments,
                span: None,
                file: handle.path().to_path_buf(),
            });
        }

        // Do all dep-replacements have valid environments?
        // TODO: maybe better to do by making an `Environment` type?
        for (env, _) in self.dep_replacements.iter() {
            if !self.environments().contains_key(env) {
                return Err(ManifestError {
                    kind: ManifestErrorKind::MissingEnvironment { env: env.clone() },
                    span: None, // TODO
                    file: handle.path().to_path_buf(),
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

struct ErrorBuilder {
    span: Range<usize>,
    file: PathBuf,
}

impl ManifestError {
    fn with_file<T: Into<ManifestErrorKind>>(path: impl AsRef<Path>) -> impl Fn(T) -> Self {
        move |e| ManifestError {
            kind: e.into(),
            span: None,
            file: path.as_ref().to_path_buf(),
        }
    }

    fn with_span<T: Into<ManifestErrorKind>>(
        path: impl AsRef<Path>,
        span: Range<usize>,
    ) -> impl Fn(T) -> Self {
        move |e| ManifestError {
            kind: e.into(),
            span: Some(span.clone()),
            file: path.as_ref().to_path_buf(),
        }
    }

    fn from_toml(path: impl AsRef<Path>) -> impl Fn(toml_edit::de::Error) -> Self {
        move |e| {
            let span = e.span();
            ManifestError {
                kind: e.into(),
                span,
                file: path.as_ref().to_path_buf(),
            }
        }
    }

    /// Convert this error into a codespan Diagnostic
    pub fn to_diagnostic(&self) -> Diagnostic<usize> {
        let (file_id, span) = self.span_info();
        Diagnostic::error()
            .with_message(self.kind.to_string())
            .with_labels(vec![Label::primary(file_id, span.unwrap_or_default())])
    }

    /// Get the file ID and span for this error
    fn span_info(&self) -> (usize, Option<Range<usize>>) {
        let mut files = SimpleFiles::new();
        let file_id = files.add(self.file.to_string_lossy(), self.handle.source());
        (file_id, self.span.clone())
    }

    /// Emit this error to stderr
    pub fn emit(&self) -> Result<(), codespan_reporting::files::Error> {
        let mut files = SimpleFiles::new();
        let file_id = files.add(self.handle.path().to_string_lossy(), self.handle.source());

        let writer = StandardStream::stderr(ColorChoice::Always);
        let config = term::Config {
            display_style: term::DisplayStyle::Rich,
            chars: term::Chars::ascii(),
            ..Default::default()
        };

        let diagnostic = self.to_diagnostic();
        let e = term::emit(&mut writer.lock(), &config, &files, &diagnostic);
        e
    }
}
