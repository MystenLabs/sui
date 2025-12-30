// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use codespan_reporting::diagnostic::Diagnostic;
use codespan_reporting::term;
use codespan_reporting::term::Config;
use codespan_reporting::term::termcolor::ColorChoice;
use codespan_reporting::term::termcolor::StandardStream;

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
use crate::graph::LockfileError;
use crate::graph::RenameError;
use crate::package::EnvironmentID;
use crate::package::EnvironmentName;
use crate::package::manifest::ManifestError;
use crate::package::paths::FileError;
use crate::package::paths::PackagePathError;

/// Result type for package operations
pub type PackageResult<T> = Result<T, PackageError>;

/// The main error type for package-related operations
#[derive(Error, Debug)]
pub enum PackageError {
    #[error(transparent)]
    Manifest(#[from] ManifestError),

    #[error(transparent)]
    Lockfile(#[from] LockfileError),

    #[error(transparent)]
    Git(#[from] GitError),

    #[error(transparent)]
    Resolver(#[from] ResolverError),

    #[error(transparent)]
    PackagePath(#[from] PackagePathError),

    #[error(transparent)]
    FileError(#[from] FileError),

    #[error(transparent)]
    Linkage(#[from] LinkageError),

    #[error(transparent)]
    RenameFrom(#[from] RenameError),

    #[error(transparent)]
    FetchError(#[from] FetchError),

    #[error("Invalid system dependency `{dep}`; the allowed system dependencies are: {valid}")]
    InvalidSystemDep { dep: String, valid: String },

    #[error("Error while loading dependency {dep}: {err}")]
    DepError { dep: String, err: Box<PackageError> },

    #[error(
        "Address `{address}` is defined more than once in package `{package}` (or its dependencies)"
    )]
    DuplicateNamedAddress {
        address: Identifier,
        package: String,
    },

    #[error(
        "Ephemeral publication file {file:?} has `build-env = \"{file_build_env}\"`; it cannot be used to publish with `--build-env {passed_build_env}`"
    )]
    EphemeralEnvMismatch {
        file: FileHandle,
        file_build_env: EnvironmentName,
        passed_build_env: EnvironmentName,
    },

    #[error(
        "Ephemeral publication file {file:?} has chain-id `{file_chain_id}`; it cannot be used to publish to chain with id `{passed_chain_id}`"
    )]
    EphemeralChainMismatch {
        file: FileHandle,
        file_chain_id: EnvironmentID,
        passed_chain_id: EnvironmentID,
    },

    #[error(
        "Ephemeral publication file does not exist, so you must pass `--build-env <env>` to indicate what environment it should be created for"
    )]
    EphemeralNoBuildEnv,

    #[error("{0}")]
    UnknownBuildEnv(String),

    #[error("Unable to create ephemeral publication file `{file}`: {err:?}")]
    InvalidEphemeralFile { file: PathBuf, err: std::io::Error },

    #[error("Multiple entries with `source = {{ {dep} }}` exist in the publication file")]
    MultipleEphemeralEntries { dep: String },

    #[error(
        "Cannot override default environments. Environment `{name}` is a system environment and cannot be overridden. System environments: {valid}"
    )]
    CannotOverrideDefaultEnvironments {
        name: EnvironmentName,
        valid: String,
    },

    #[error(
        "You cannot have a dependency with the same name as the package. Rename the dependency, which will require adding `rename-from=\"{name}\"`"
    )]
    DependencyWithSameNameAsPackage { name: String },
}

/// Truncate `s` to the first `head` characters and the last `tail` characters of `s`, separated by
/// "..."
pub fn fmt_truncated(s: impl AsRef<str>, head: usize, tail: usize) -> String {
    let len = s.as_ref().len();
    if head + tail + 3 >= len {
        s.as_ref().to_string()
    } else {
        let tail_start = s.as_ref().len() - tail;
        format!("{}...{}", &s.as_ref()[..head], &s.as_ref()[tail_start..])
    }
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
