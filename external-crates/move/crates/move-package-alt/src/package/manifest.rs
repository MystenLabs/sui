// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use codespan_reporting::diagnostic::Diagnostic;

use thiserror::Error;

use crate::{
    errors::FileHandle,
    schema::{DefaultDependency, PackageName, ParsedManifest, ReplacementDependency},
};

use super::{
    package_lock::PackageSystemLock,
    paths::{FileResult, PackagePath},
    *,
};
use indexmap::IndexMap;
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
}

#[derive(Error, Debug)]
pub enum ManifestErrorKind {
    #[error(transparent)]
    IoError(#[from] std::io::Error),

    #[error("{}", .0.message())]
    ParseError(#[from] toml_edit::de::Error),

    #[error(
        "Dependency must have a `git`, `local`, or `r` field in the `[dependencies]` or the `[dep-replacements]` section"
    )]
    NoDepInfo,

    #[error("{0}")]
    RenameFromError(String),

    #[error(
        "The `{name}` dependency is implicitly provided and should not be defined in your manifest."
    )]
    ExplicitImplicit { name: PackageName },

    #[error("{0}")]
    FlavorRejectedManifest(String),
}

pub type ManifestResult<T> = Result<T, ManifestError>;

impl Manifest {
    /// Read the manifest file from the file handle, returning a [`Manifest`].
    pub(crate) fn read_from_file(path: &PackagePath, mtx: &PackageSystemLock) -> FileResult<Self> {
        let (file_handle, inner) = path.read_manifest(mtx)?;

        Ok(Self { inner, file_handle })
    }

    pub fn package_name(&self) -> String {
        self.inner.package.name.get_ref().to_string()
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
    pub fn environments(&self) -> IndexMap<EnvironmentName, EnvironmentID> {
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

    /// Convert this error into a codespan Diagnostic
    pub fn to_diagnostic(&self) -> Diagnostic<FileHandle> {
        match &self.location {
            ErrorLocation::WholeFile(path) => {
                Diagnostic::error().with_message(format!("Error while loading `{path:?}`: {self}"))
            }
        }
    }

    /// Create an error object representing a flavor rejection of the manifest
    pub(crate) fn flavor_rejected_manifest(manifest_handle: FileHandle, message: String) -> Self {
        Self {
            location: ErrorLocation::WholeFile(manifest_handle.path().to_path_buf()),
            kind: Box::new(ManifestErrorKind::FlavorRejectedManifest(message)),
        }
    }
}

#[cfg(test)]
mod tests {
    // TODO: comprehensive testing

    use tempfile::TempDir;
    use test_log::test;

    use crate::{
        flavor::vanilla::default_environment, package::paths::PackagePath, schema::PackageName,
    };

    use super::Manifest;

    /// Create a file containing `contents` and pass it to `Manifest::read_from_file`
    async fn load_manifest(contents: impl AsRef<[u8]>) -> anyhow::Result<Manifest> {
        // TODO: we need a better implementation for this
        let tempdir = TempDir::new().unwrap();

        let manifest_path = tempdir.path().join("Move.toml");
        std::fs::write(&manifest_path, contents).expect("write succeeds");
        let package_path = PackagePath::new(tempdir.path().to_path_buf()).unwrap();

        Ok(Manifest::read_from_file(
            &package_path,
            &package_path.lock()?,
        )?)
    }

    /// The `environments` table may be missing
    #[test(tokio::test)]
    async fn empty_environments_allowed() {
        let manifest = load_manifest(
            r#"
            [package]
            name = "test"
            edition = "2024"
            "#,
        )
        .await
        .unwrap();

        assert!(manifest.environments().is_empty());
    }

    /// Environment names in `dep-replacements` must be defined in `environments`
    #[test(tokio::test)]
    #[ignore] // TODO: this tests new behavior that isn't implemented yet
    async fn dep_replacement_envs_are_declared() {
        let manifest = load_manifest(
            r#"
            [package]
            name = "test"
            edition = "2024"

            [dep-replacements]
            mainnet.foo = { local = "../foo" }
            "#,
        )
        .await
        .unwrap();

        let name = PackageName::new("foo").unwrap();
        assert!(manifest.dependencies().contains_key(&name));
        let default_env = default_environment();
        assert!(!manifest.dep_replacements()[default_env.name()].contains_key(&name));
    }
}
