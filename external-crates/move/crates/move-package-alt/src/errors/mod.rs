// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

mod lockfile_error;
mod manifest_error;
use append_only_vec::AppendOnlyVec;
use codespan_reporting::files::SimpleFile;
use codespan_reporting::files::SimpleFiles;
mod git_error;
pub use git_error::GitError;
pub use git_error::GitErrorKind;
pub use lockfile_error::LockfileError;
pub use manifest_error::ManifestError;
pub use manifest_error::ManifestErrorKind;

mod located;
pub use located::{with_file, Located};

mod files;
pub use files::FileHandle;
pub use resolver_error::ResolverError;

mod resolver_error;

use std::fs;
use std::path::Path;
use std::sync::LazyLock;
use std::sync::Mutex;
use std::{ops::Range, path::PathBuf};

use codespan_reporting::diagnostic::Diagnostic;
use serde::{Deserialize, Serialize};
use thiserror::Error;

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
}

impl PackageError {
    pub fn to_diagnostic(&self) -> Diagnostic<usize> {
        match self {
            Self::Manifest(err) => err.to_diagnostic(),
            _ => Diagnostic::error()
                .with_message(self.to_string())
                .with_labels(vec![]),
        }
    }

    pub fn emit(&self) -> Result<(), codespan_reporting::files::Error> {
        match self {
            Self::Manifest(err) => err.emit(),
            _ => Ok(()),
        }
    }
}
