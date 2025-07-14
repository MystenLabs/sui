// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

mod lockfile_error;
pub use lockfile_error::LockfileError;

mod located;
pub use located::Location;

mod files;
pub use files::FileHandle;
pub use files::Files;

use thiserror::Error;

use crate::dependency::ResolverError;
use crate::git::GitError;
use crate::package::manifest::ManifestError;
use crate::package::paths::PackagePathError;

/// Result type for package operations
pub type PackageResult<T> = Result<T, PackageError>;

/// The main error type for package-related operations
#[derive(Error, Debug)]
pub enum PackageError {
    #[error(transparent)]
    Codespan(#[from] codespan_reporting::files::Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    FromUTF8(#[from] std::string::FromUtf8Error),

    #[error(transparent)]
    Manifest(#[from] ManifestError),

    #[error(transparent)]
    Other(#[from] anyhow::Error),

    #[error("{0}")]
    Generic(String),

    #[error(transparent)]
    Git(#[from] GitError),

    #[error(transparent)]
    Toml(#[from] toml_edit::de::Error),

    #[error(transparent)]
    Resolver(#[from] ResolverError),

    #[error(transparent)]
    PackagePath(#[from] PackagePathError),
}
