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
    schema::{DefaultDependency, PackageName, ParsedManifest, ReplacementDependency},
};

use super::*;
use serde_spanned::Spanned;

// TODO: replace this with something more strongly typed
pub type Digest = String;

pub struct Manifest {
    inner: ParsedManifest,
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
    #[error(transparent)]
    IoError(#[from] std::io::Error),

    #[error("{}", .0.message())]
    ParseError(#[from] toml_edit::de::Error),

    #[error(
        "Dependency <TODO> must have a `git`, `local`, or `r` field in either the `[dependencies]` or the `[dep-replacements]` section"
    )]
    NoDepInfo,
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
            file_handle,
        };

        Ok(result)
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

    pub fn file_handle(&self) -> &FileHandle {
        &self.file_handle
    }

    pub fn into_parsed(self) -> ParsedManifest {
        self.inner
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
    fn empty_environments_allowed() {
        let manifest = load_manifest(
            r#"
            [package]
            name = "test"
            edition = "2024"
            "#,
        )
        .unwrap();

        assert!(manifest.environments().is_empty());
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
