// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use codespan_reporting::diagnostic::{Diagnostic, Label};

use thiserror::Error;

use crate::{
    errors::{FileHandle, Location},
    schema::{
        DefaultDependency, PackageMetadata, PackageName, ParsedManifest, ReplacementDependency,
    },
};

use super::*;
use serde_spanned::Spanned;

const ALLOWED_EDITIONS: &[&str] = &["2025", "2024", "2024.beta", "legacy"];

// TODO: replace this with something more strongly typed
pub type Digest = String;

pub struct Manifest {
    inner: ParsedManifest,
    digest: Digest,
    file_handle: FileHandle,
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

impl Manifest {
    /// Read the manifest file from the file handle, returning a [`Manifest`].
    pub fn read_from_file(path: impl AsRef<Path>) -> ManifestResult<Self> {
        let file_handle = FileHandle::new(&path).map_err(ManifestError::with_file(&path))?;
        let parsed: ParsedManifest = toml_edit::de::from_str(file_handle.source())
            .map_err(ManifestError::from_toml(file_handle))?;

        let result = Self {
            inner: parsed,
            digest: compute_digest(file_handle.source()),
            file_handle,
        };

        result.validate_manifest(file_handle)?;

        Ok(result)
    }

    pub fn metadata(&self) -> PackageMetadata {
        self.inner.package.clone()
    }

    pub fn dep_replacements(
        &self,
    ) -> &BTreeMap<EnvironmentName, BTreeMap<PackageName, Spanned<ReplacementDependency>>> {
        &self.inner.dep_replacements
    }

    pub fn dependencies(&self) -> BTreeMap<PackageName, DefaultDependency> {
        self.inner
            .dependencies
            .clone()
            .into_iter()
            .map(|(k, v)| (k.as_ref().clone(), v.clone()))
            .collect()
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

    pub fn file_handle(&self) -> &FileHandle {
        &self.file_handle
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

    pub(crate) fn parsed(&self) -> &ParsedManifest {
        &self.inner
    }
}

impl ManifestError {
    pub(crate) fn with_file<T: Into<ManifestErrorKind>>(
        path: impl AsRef<Path>,
    ) -> impl Fn(T) -> Self {
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

impl std::fmt::Debug for Manifest {
    // TODO: not sure we want this
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.inner.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    // TODO: comprehensive testing

    use tempfile::TempDir;
    use test_log::test;

    use crate::{flavor::vanilla::default_environment, schema::PackageName};

    use super::{Manifest, ManifestResult};

    /// Create a file containing `contents` and pass it to `Manifest::read_from_file`
    fn load_manifest(contents: impl AsRef<[u8]>) -> ManifestResult<Manifest> {
        // TODO: we need a better implementation for this
        let tempdir = TempDir::new().unwrap();
        let manifest_path = tempdir.path().join("Move.toml");

        std::fs::write(&manifest_path, contents).expect("write succeeds");

        Manifest::read_from_file(manifest_path)
    }

    /// The `environments` table may be missing
    #[test]
    #[ignore] // TODO: this tests new behavior that isn't implemented yet
    fn empty_environments_allowed() {
        let manifest = load_manifest(
            r#"
            [package]
            name = "test"
            edition = "2024"
            "#,
        )
        .unwrap();

        let default_env = default_environment();
        assert_eq!(
            manifest.environments().get(default_env.name()),
            Some(default_env.id())
        );
    }

    /// Environment names in `dep-replacements` must be defined in `environments`
    #[test]
    #[ignore] // TODO: this tests new behavior that isn't implemented yet
    fn dep_replacement_envs_are_declared() {
        let manifest = load_manifest(
            r#"
            [package]
            name = "test"
            edition = "2024"

            [dep-replacements]
            mainnet.foo = { local = "../foo" }
            "#,
        )
        .unwrap();

        let name = PackageName::new("foo").unwrap();
        assert!(manifest.dependencies().contains_key(&name));
        let default_env = default_environment();
        assert!(!manifest.dep_replacements()[default_env.name()].contains_key(&name));
    }
}
