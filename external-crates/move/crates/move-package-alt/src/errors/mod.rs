// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

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
use crate::graph::RenameError;
use crate::package::EnvironmentID;
use crate::package::EnvironmentName;
use crate::package::manifest::ManifestError;
use crate::package::paths::PackagePathError;

/// Result type for package operations
pub type PackageResult<T> = Result<T, PackageError>;

/// The main error type for package-related operations
#[derive(Error, Debug)]
pub enum PackageError {
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
        package: String,
    },

    #[error(
        // TODO: add file path?
        "Ephemeral publication file has `build-env = \"{file_build_env}\"`; it cannot be used to publish with `--build-env {passed_build_env}`"
    )]
    EphemeralEnvMismatch {
        file_build_env: EnvironmentName,
        passed_build_env: EnvironmentName,
    },

    #[error(
        // TODO: add file path?
        "Ephemeral publication file has chain-id `{file_chain_id}`; it cannot be used to publish to chain with id `{passed_chain_id}`"
    )]
    EphemeralChainMismatch {
        file_chain_id: EnvironmentID,
        passed_chain_id: EnvironmentID,
    },

    #[error(
        // TODO: add file path?
        // TODO: maybe not mention `--build-env` since that's CLI specific? Then we'll need to add
        //       it elsewhere
        "Ephemeral publication file does not have a `build-env` so you must pass `--build-env <env>`"
    )]
    EphemeralNoBuildEnv,

    #[error("Cannot build with build-env `{build_env}`: the recognized environments are <TODO>")]
    UnknownBuildEnv { build_env: EnvironmentName },
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
