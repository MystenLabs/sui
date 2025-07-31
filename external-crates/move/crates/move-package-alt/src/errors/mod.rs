// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

mod lockfile_error;
use codespan_reporting::diagnostic::Diagnostic;
use codespan_reporting::term;
use codespan_reporting::term::Config;
use codespan_reporting::term::termcolor::ColorChoice;
use codespan_reporting::term::termcolor::StandardStream;
pub use lockfile_error::LockfileError;

mod located;
pub use located::Location;

mod files;
pub use files::FileHandle;
pub use files::Files;

use move_core_types::identifier::Identifier;
use thiserror::Error;

use crate::dependency::FetchError;
use crate::dependency::ResolverError;
use crate::git::GitError;
use crate::graph::LinkageError;
use crate::graph::RenameError;
use crate::package::manifest::ManifestError;
use crate::package::paths::PackagePathError;
use crate::schema::PackageName;

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

    #[error(transparent)]
    Linkage(#[from] LinkageError),

    #[error(transparent)]
    RenameFrom(#[from] RenameError),

    #[error(transparent)]
    FetchError(#[from] FetchError),

    #[error(
        "Address `{address}` is defined more than once in package `{package}` (or its dependencies)"
    )]
    DuplicateNamedAddress {
        address: Identifier,
        package: PackageName,
    },
}

impl PackageError {
    pub fn to_diagnostic(&self) -> Diagnostic<FileHandle> {
        match self {
            Self::Manifest(e) => e.to_diagnostic(),
            _ => Diagnostic::error().with_message(format!("{self}")),
        }
    }

    pub fn emit(&self) {
        let diagnostic = self.to_diagnostic();
        let mut writer = StandardStream::stderr(ColorChoice::Auto);
        term::emit(&mut writer, &Config::default(), &Files, &diagnostic).unwrap();
    }
}
