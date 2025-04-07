// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

mod manifest_error;
pub use manifest_error::ManifestError;
pub use manifest_error::ManifestErrorKind;

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
    Manifest(#[from] ManifestError),

    #[error(transparent)]
    Other(#[from] anyhow::Error),

    #[error(transparent)]
    Toml(#[from] toml_edit::de::Error),
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
