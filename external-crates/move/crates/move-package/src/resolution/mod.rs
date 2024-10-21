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
    source_package::parsed_manifest::{DependencyKind, GitInfo, OnChainInfo},
    BuildConfig,
};

use self::dependency_graph::DependencyGraphBuilder;

pub mod dependency_cache;
pub mod dependency_graph;
mod digest;
pub mod resolution_graph;
pub mod resolving_table;

pub fn download_dependency_repos<Progress: Write>(
    manifest_string: String,
    lock_string: Option<String>,
    build_options: &BuildConfig,
    root_path: &Path,
    progress_output: &mut Progress,
) -> Result<()> {
    let install_dir = build_options
        .install_dir
        .as_ref()
        .unwrap_or(&root_path.to_path_buf())
        .to_owned();
    let mut dep_graph_builder = DependencyGraphBuilder::new(
        build_options.skip_fetch_latest_git_deps,
        progress_output,
        install_dir,
    );
    let (graph, _) = dep_graph_builder.get_graph(
        &DependencyKind::default(),
        root_path.to_path_buf(),
        manifest_string,
        lock_string,
    )?;

    for pkg_id in graph.topological_order() {
        if pkg_id == graph.root_package_id {
            continue;
        }

        if !(build_options.dev_mode || graph.always_deps.contains(&pkg_id)) {
            continue;
        }

        let package = graph
            .package_table
            .get(&pkg_id)
            .expect("Metadata for package");

        let DependencyGraphBuilder {
            ref mut dependency_cache,
            ref mut progress_output,
            ..
        } = dep_graph_builder;
        dependency_cache.download_and_update_if_remote(pkg_id, &package.kind, progress_output)?;
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

        // Downloaded packages are of the form <id>
        DependencyKind::OnChain(OnChainInfo { id }) => {
            [&*MOVE_HOME, &url_to_file_name(id.as_str()).to_string()]
                .iter()
                .collect()
        }
    }
}

/// The path that the dependency of kind `kind` is found at locally, after it is fetched.
fn local_path(kind: &DependencyKind) -> PathBuf {
    let mut repo_path = repository_path(kind);

    if let DependencyKind::Git(GitInfo { subdir, .. }) = kind {
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
