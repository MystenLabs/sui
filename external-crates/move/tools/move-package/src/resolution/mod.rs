// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use move_command_line_common::env::MOVE_HOME;
use std::{
    io::Write,
    path::{Path, PathBuf},
};

use crate::{
    source_package::parsed_manifest::{CustomDepInfo, DependencyKind, GitInfo, SourceManifest},
    BuildConfig,
};

use self::{dependency_cache::DependencyCache, dependency_graph::DependencyGraph};

pub mod dependency_cache;
pub mod dependency_graph;
mod digest;
pub mod lock_file;
pub mod resolution_graph;
pub mod resolving_table;

pub fn download_dependency_repos<Progress: Write>(
    manifest: &SourceManifest,
    build_options: &BuildConfig,
    root_path: &Path,
    progress_output: &mut Progress,
) -> Result<()> {
    let mut dependency_cache = DependencyCache::new(build_options.skip_fetch_latest_git_deps);

    let graph = DependencyGraph::new(
        manifest,
        root_path.to_path_buf(),
        &mut dependency_cache,
        progress_output,
    )?;

    for pkg_name in graph.topological_order() {
        if pkg_name == graph.root_package {
            continue;
        }

        if !(build_options.dev_mode || graph.always_deps.contains(&pkg_name)) {
            continue;
        }

        let package = graph
            .package_table
            .get(&pkg_name)
            .expect("Metadata for package");

        dependency_cache.download_and_update_if_remote(pkg_name, &package.kind, progress_output)?;
    }

    Ok(())
}

/// The local location of the repository containing the dependency of kind `kind` (and potentially
/// other, related dependencies).
fn repository_path(kind: &DependencyKind) -> PathBuf {
    match kind {
        DependencyKind::Local(path) => path.clone(),

        // Downloaded packages are of the form <sanitized_git_url>_<rev_name>
        DependencyKind::Git(GitInfo {
            git_url,
            git_rev,
            subdir: _,
        }) => [
            &*MOVE_HOME,
            &format!(
                "{}_{}",
                url_to_file_name(git_url.as_str()),
                git_rev.replace('/', "__"),
            ),
        ]
        .iter()
        .collect(),

        // Downloaded packages are of the form <sanitized_node_url>_<address>_<package>
        DependencyKind::Custom(CustomDepInfo {
            node_url,
            package_address,
            package_name,
            subdir: _,
        }) => [
            &*MOVE_HOME,
            &format!(
                "{}_{}_{}",
                url_to_file_name(node_url.as_str()),
                package_address.as_str(),
                package_name.as_str(),
            ),
        ]
        .iter()
        .collect(),
    }
}

/// The path that the dependency of kind `kind` is found at locally, after it is fetched.
fn local_path(kind: &DependencyKind) -> PathBuf {
    let mut repo_path = repository_path(kind);

    if let DependencyKind::Git(GitInfo { subdir, .. })
    | DependencyKind::Custom(CustomDepInfo { subdir, .. }) = kind
    {
        repo_path.push(subdir);
    }

    repo_path
}

fn url_to_file_name(url: &str) -> String {
    regex::Regex::new(r"/|:|\.|@")
        .unwrap()
        .replace_all(url, "_")
        .to_string()
}
