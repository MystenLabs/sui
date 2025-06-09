// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod cache;
pub mod errors;
pub mod sha;

use std::{
    io::BufRead,
    path::{Path, PathBuf},
    process::{Output, Stdio},
};

use errors::{GitError, GitResult};
use sha::GitSha;
use tokio::process::Command;
use tracing::{debug, info};

/// Helper struct that represents a Git repository, with extra information about which folder to
/// checkout.
#[derive(Clone, Debug)]
pub struct GitRepo {
    /// Repository URL
    pub repo_url: String,
    /// Commit-ish (branch, tag, or SHA)
    pub rev: Option<String>,
    /// Folder to checkout during sparse checkout
    pub path: PathBuf,
}

impl GitRepo {}
