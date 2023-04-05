// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use colored::Colorize;
use std::{
    collections::BTreeSet,
    ffi::OsStr,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
};

use crate::{
    package_hooks,
    source_package::parsed_manifest::{DependencyKind, GitInfo, PackageName},
};

use super::repository_path;

/// Fetches remote dependencies and caches information about those already fetched when building a
/// given package.
#[derive(Debug, Clone)]
pub struct DependencyCache {
    /// A set of paths for remote dependencies that have already been fetched
    fetched_deps: BTreeSet<PathBuf>,

    /// Should a dependency fetched when building a different package be refreshed to the newest
    /// version when building a new package
    skip_fetch_latest_git_deps: bool,
}

impl DependencyCache {
    pub fn new(skip_fetch_latest_git_deps: bool) -> DependencyCache {
        let fetched_deps = BTreeSet::new();
        DependencyCache {
            fetched_deps,
            skip_fetch_latest_git_deps,
        }
    }

    pub fn download_and_update_if_remote<Progress: Write>(
        &mut self,
        dep_name: PackageName,
        kind: &DependencyKind,
        progress_output: &mut Progress,
    ) -> Result<()> {
        match kind {
            DependencyKind::Local(_) => Ok(()),

            DependencyKind::Custom(node_info) => {
                // check if a give dependency type has already been fetched
                if !self.fetched_deps.insert(repository_path(kind)) {
                    return Ok(());
                }
                package_hooks::resolve_custom_dependency(dep_name, node_info)
            }

            DependencyKind::Git(GitInfo {
                git_url,
                git_rev,
                subdir: _,
            }) => {
                let repository_path = repository_path(kind);
                // check if a give dependency type has already been fetched
                if !self.fetched_deps.insert(repository_path.clone()) {
                    return Ok(());
                }
                let git_path = repository_path;
                let os_git_url = OsStr::new(git_url.as_str());
                let os_git_rev = OsStr::new(git_rev.as_str());

                if !git_path.exists() {
                    writeln!(
                        progress_output,
                        "{} {}",
                        "FETCHING GIT DEPENDENCY".bold().green(),
                        git_url,
                    )?;

                    // If the cached folder does not exist, download and clone accordingly
                    Command::new("git")
                        .args([OsStr::new("clone"), os_git_url, git_path.as_os_str()])
                        .output()
                        .map_err(|_| {
                            anyhow::anyhow!(
                                "Failed to clone Git repository for package '{}'",
                                dep_name
                            )
                        })?;

                    Command::new("git")
                        .args([
                            OsStr::new("-C"),
                            git_path.as_os_str(),
                            OsStr::new("checkout"),
                            os_git_rev,
                        ])
                        .output()
                        .map_err(|_| {
                            anyhow::anyhow!(
                                "Failed to checkout Git reference '{}' for package '{}'",
                                git_rev,
                                dep_name
                            )
                        })?;
                } else if !self.skip_fetch_latest_git_deps {
                    // Update the git dependency
                    // Check first that it isn't a git rev (if it doesn't work, just continue with the
                    // fetch)
                    if let Ok(rev) = Command::new("git")
                        .args([
                            OsStr::new("-C"),
                            git_path.as_os_str(),
                            OsStr::new("rev-parse"),
                            OsStr::new("--verify"),
                            os_git_rev,
                        ])
                        .output()
                    {
                        if let Ok(parsable_version) = String::from_utf8(rev.stdout) {
                            // If it's exactly the same, then it's a git rev
                            if parsable_version.trim().starts_with(git_rev.as_str()) {
                                return Ok(());
                            }
                        }
                    }

                    let tag = Command::new("git")
                        .args([
                            OsStr::new("-C"),
                            git_path.as_os_str(),
                            OsStr::new("tag"),
                            OsStr::new("--list"),
                            os_git_rev,
                        ])
                        .output();

                    if let Ok(tag) = tag {
                        if let Ok(parsable_version) = String::from_utf8(tag.stdout) {
                            // If it's exactly the same, then it's a git tag, for now tags won't be updated
                            // Tags don't easily update locally and you can't use reset --hard to cleanup
                            // any extra files
                            if parsable_version.trim().starts_with(git_rev.as_str()) {
                                return Ok(());
                            }
                        }
                    }

                    writeln!(
                        progress_output,
                        "{} {}",
                        "UPDATING GIT DEPENDENCY".bold().green(),
                        git_url,
                    )?;

                    // If the current folder exists, do a fetch and reset to ensure that the branch
                    // is up to date.
                    //
                    // NOTE: this means that you must run the package system with a working network
                    // connection.
                    let status = Command::new("git")
                        .args([
                            OsStr::new("-C"),
                            git_path.as_os_str(),
                            OsStr::new("fetch"),
                            OsStr::new("origin"),
                        ])
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .status()
                        .map_err(|_| {
                            anyhow::anyhow!(
                                "Failed to fetch latest Git state for package '{}', to skip set \
                             --skip-fetch-latest-git-deps",
                                dep_name
                            )
                        })?;

                    if !status.success() {
                        return Err(anyhow::anyhow!(
                            "Failed to fetch to latest Git state for package '{}', to skip set \
                         --skip-fetch-latest-git-deps | Exit status: {}",
                            dep_name,
                            status
                        ));
                    }

                    let status = Command::new("git")
                        .args([
                            OsStr::new("-C"),
                            git_path.as_os_str(),
                            OsStr::new("reset"),
                            OsStr::new("--hard"),
                            OsStr::new(&format!("origin/{}", git_rev)),
                        ])
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .status()
                        .map_err(|_| {
                            anyhow::anyhow!(
                            "Failed to reset to latest Git state '{}' for package '{}', to skip \
                             set --skip-fetch-latest-git-deps",
                            git_rev,
                            dep_name
                        )
                        })?;

                    if !status.success() {
                        return Err(anyhow::anyhow!(
                        "Failed to reset to latest Git state '{}' for package '{}', to skip set \
                         --skip-fetch-latest-git-deps | Exit status: {}",
                        git_rev,
                        dep_name,
                        status
                    ));
                    }
                }

                Ok(())
            }
        }
    }
}
