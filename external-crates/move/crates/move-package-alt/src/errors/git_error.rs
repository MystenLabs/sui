// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{ops::Range, path::PathBuf};

use codespan_reporting::{
    diagnostic::{Diagnostic, Label},
    files::SimpleFiles,
    term::{
        self,
        termcolor::{ColorChoice, StandardStream},
    },
};
use thiserror::Error;

use crate::package::PackageName;

use super::FileHandle;

#[derive(Error, Debug)]
#[error("Invalid manifest: {kind}")]
pub struct GitError {
    pub kind: GitErrorKind,
    pub span: Option<Range<usize>>,
    pub handle: Option<FileHandle>,
}

#[derive(Error, Debug)]
pub enum GitErrorKind {
    #[error("repo {repo} does not contain a Move.toml file")]
    NotMoveProject { repo: String },
    #[error(
        "{repo} repo is dirty. Please clean it up before proceeding or pass the `--allow-dirty` flag"
    )]
    Dirty { repo: String },

    #[error("git is not installed or not available on PATH")]
    GitNotFound,

    #[error("could not extract a sha for repo {repo} and rev {rev}")]
    NoSha { repo: String, rev: String },

    #[error("not a valid sha for repo {repo}: {sha}")]
    InvalidSha { repo: String, sha: String },

    #[error("could not execute git command {error}")]
    CommandError { error: String },

    #[error("{0}")]
    Generic(String),
}

impl GitError {
    pub fn dirty(repo: &str) -> Self {
        Self {
            kind: GitErrorKind::Dirty {
                repo: repo.to_string(),
            },
            span: None,
            handle: None,
        }
    }
    pub fn not_move_project(repo: &str) -> Self {
        Self {
            kind: GitErrorKind::NotMoveProject {
                repo: repo.to_string(),
            },
            span: None,
            handle: None,
        }
    }
    pub fn generic(msg: String) -> Self {
        Self {
            kind: GitErrorKind::Generic(msg),
            span: None,
            handle: None,
        }
    }
    pub fn not_found() -> Self {
        Self {
            kind: GitErrorKind::GitNotFound,
            span: None,
            handle: None,
        }
    }

    pub fn invalid_sha(repo: &str, sha: &str) -> Self {
        Self {
            kind: GitErrorKind::InvalidSha {
                repo: repo.to_string(),
                sha: sha.to_string(),
            },
            span: None,
            handle: None,
        }
    }

    pub fn command_error(error: String) -> Self {
        Self {
            kind: GitErrorKind::CommandError { error },
            span: None,
            handle: None,
        }
    }

    pub fn no_sha(repo: &str, rev: &str) -> Self {
        Self {
            kind: GitErrorKind::NoSha {
                repo: repo.to_string(),
                rev: rev.to_string(),
            },
            span: None,
            handle: None,
        }
    }
}
