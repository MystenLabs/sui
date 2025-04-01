// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Types and methods for external dependencies (of the form `{ git = "<repo>" }`)
//!
//! Git dependencies are cached in `~/.move`, which has the following structure:
//!
//! ```
//! .move/
//!   git/
//!     <remote 1>/ # a headless, sparse, and shallow git repository
//!       <sha 1>/ # a worktree checked out to the given sha
//!       <sha 2>/
//!       ...
//!     <remote 2>/
//!       ...
//!     ...
//! ```

use std::{marker::PhantomData, path::PathBuf};

use serde::Serialize;

use crate::errors::PackageResult;

pub struct Pinned;
pub struct Unpinned;

#[derive(Serialize)]
pub struct GitDependency<P = Unpinned> {
    /// The repository holding the dep
    repo: String,

    /// The git commit-ish for the dep; guaranteed to be a commit if [P] is [Pinned].
    rev: String,

    /// The path within the repository
    path: PathBuf,

    phantom: PhantomData<P>,
}

impl GitDependency<Unpinned> {
    /// Replace the commit-ish [self.rev] with a commit (i.e. a SHA). Requires fetching the git
    /// repository
    pub fn pin(&self) -> PackageResult<GitDependency<Pinned>> {
        todo!()
    }
}

impl GitDependency<Pinned> {
    /// Ensures that the given sha is downloaded
    pub fn fetch(&self) -> PackageResult<PathBuf> {
        todo!()
    }
}
