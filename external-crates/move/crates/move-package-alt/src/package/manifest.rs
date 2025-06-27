// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    marker::PhantomData,
    path::{Path, PathBuf},
};

use codespan_reporting::diagnostic::{Diagnostic, Label};

use thiserror::Error;
use tracing::debug;

use crate::{
    dependency::{CombinedDependency, DependencySet},
    errors::{FileHandle, Location},
    flavor::MoveFlavor,
    schema::ParsedManifest,
};

use super::*;
use sha2::{Digest as ShaDigest, Sha256};

const ALLOWED_EDITIONS: &[&str] = &["2025", "2024", "2024.beta", "legacy"];

// TODO: replace this with something more strongly typed
pub type Digest = String;

pub struct Manifest<F: MoveFlavor> {
    inner: ParsedManifest,
    digest: Digest,
    dependencies: DependencySet<CombinedDependency>,
    // TODO: remove <F>
    phantom: PhantomData<F>,
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

pub type ManifestResult<T> = Result<T, ManifestError>;

impl<F: MoveFlavor> Manifest<F> {
    /// Read the manifest file at the given path, returning a [`Manifest`].
    pub fn read_from_file(path: impl AsRef<Path>) -> ManifestResult<Self> {
        debug!("Reading manifest from {:?}", path.as_ref());

        let file_id = FileHandle::new(&path).map_err(ManifestError::with_file(&path))?;
        let parsed: ParsedManifest =
            toml_edit::de::from_str(file_id.source()).map_err(ManifestError::from_toml(file_id))?;

        let dependencies = CombinedDependency::combine_deps(file_id, &parsed)?;

        let result = Self {
            inner: parsed,
            digest: format!("{:X}", Sha256::digest(file_id.source().as_ref())),
            dependencies,
            phantom: PhantomData,
        };
        result.validate_manifest(file_id)?;
        Ok(result)
    }

    /// The combined entries of the `[dependencies]` and `[dep-replacements]` sections for this
    /// manifest
    pub fn dependencies(&self) -> DependencySet<CombinedDependency> {
        self.dependencies.clone()
    }

    /// The entries from the `[environments]` section
    pub fn environments(&self) -> BTreeMap<EnvironmentName, EnvironmentID> {
        self.inner
            .environments
            .iter()
            .map(|(name, id)| (name.as_ref().clone(), id.as_ref().clone()))
            .collect()
    }

    /// The name declared in the `[package]` section
    pub fn package_name(&self) -> &PackageName {
        self.inner.package.name.as_ref()
    }

    /// A digest of the file, suitable for detecting changes
    pub fn digest(&self) -> &Digest {
        &self.digest
    }

    /// Validate the manifest contents, after deserialization.
    ///
    // TODO: add more validation
    fn validate_manifest(&self, handle: FileHandle) -> ManifestResult<()> {
        // Are there any environments?
        if self.environments().is_empty() {
            return Err(ManifestError::with_file(handle.path())(
                ManifestErrorKind::NoEnvironments,
            ));
        }

        // Do all dep-replacements have valid environments?
        for (env, entries) in self.inner.dep_replacements.iter() {
            if !self.environments().contains_key(env) {
                let span = entries
                    .first_key_value()
                    .expect("dep-replacements.<env> only exists if it has a dep")
                    .1
                    .span();

                let loc = Location::new(handle, span);

                return Err(ManifestError::with_span(&loc)(
                    ManifestErrorKind::MissingEnvironment { env: env.clone() },
                ));
            }
        }

        Ok(())
    }
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

impl<F: MoveFlavor> std::fmt::Debug for Manifest<F> {
    // TODO: not sure we want this
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.inner.fmt(f)
    }
}
