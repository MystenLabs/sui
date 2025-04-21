// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Types and methods for external dependencies (of the form `{ git = "<repo>" }`)
//!
//! Git dependencies are cached in `~/.move`, which has the following structure:
//!
//! ```ignore
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

use derive_where::derive_where;
use serde::{Deserialize, Serialize};

use crate::errors::PackageResult;

use super::{DependencySet, Pinned, Unpinned};

// TODO: custom deserialization to verify pinnedness for pinned deps?
#[derive(Debug, Serialize, Deserialize)]
#[derive_where(Clone)]
pub struct GitDependency<P = Unpinned> {
    /// The repository containing the dependency
    #[serde(rename = "git")]
    repo: String,

    /// The git commit-ish for the dep; guaranteed to be a commit if [P] is [Pinned].
    #[serde(default)]
    rev: Option<String>,

    /// The path within the repository
    #[serde(default)]
    path: Option<PathBuf>,

    #[serde(skip)]
    phantom: PhantomData<P>,
}

impl GitDependency<Unpinned> {
    /// Replace all commit-ishes in [deps] with commits (i.e. SHAs). Requires fetching the git
    /// repositories
    pub fn pin(deps: DependencySet<Self>) -> PackageResult<DependencySet<GitDependency<Pinned>>> {
        Ok(deps
            .into_iter()
            .map(|(env, package, dep)| (env, package, dep.pin_one().unwrap())) // TODO: errors!
            .collect())
    }

    /// Replace the commit-ish [self.rev] with a commit (i.e. a SHA). Requires fetching the git
    /// repository
    fn pin_one(&self) -> PackageResult<GitDependency<Pinned>> {
        todo!()
    }
}

impl GitDependency<Pinned> {
    /// Ensures that the given sha is downloaded
    pub fn fetch(&self) -> PackageResult<PathBuf> {
        todo!()
    }
}
