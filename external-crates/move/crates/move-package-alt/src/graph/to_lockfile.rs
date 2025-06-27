// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    dependency::PinnedDependencyInfo,
    errors::{PackageError, PackageResult},
    flavor::MoveFlavor,
    package::{
        EnvironmentName, Package, PackageName, lockfile::Lockfiles, manifest::Manifest,
        paths::PackagePath,
    },
    schema::{LockfileDependencyInfo, PackageID, Pin},
};
use derive_where::derive_where;
use move_core_types::identifier::Identifier;
use path_clean::PathClean;
use petgraph::{
    graph::{DiGraph, NodeIndex},
    visit::EdgeRef,
};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, btree_map::Entry},
    fs::read_to_string,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tokio::sync::OnceCell;
use tracing::{debug, info};

use super::PackageGraph;

impl<F: MoveFlavor> From<&PackageGraph<F>> for BTreeMap<PackageID, Pin> {
    /// Convert a PackageGraph into an entry in the lockfile's `[pinned]` section.
    fn from(value: &PackageGraph<F>) -> Self {
        let graph = &value.inner;

        let mut name_to_suffix: BTreeMap<PackageName, u8> = BTreeMap::new();
        let mut node_to_id: BTreeMap<NodeIndex, PackageID> = BTreeMap::new();

        // build index to id map
        for node in graph.node_indices() {
            let pkg_node = graph.node_weight(node).expect("node exists");
            let suffix = name_to_suffix.entry(pkg_node.name().clone()).or_default();
            let id = if *suffix == 0 {
                pkg_node.name().clone().to_string()
            } else {
                format!("{}_{suffix}", pkg_node.name())
            };
            node_to_id.insert(node, id);
            *suffix += 1;
        }

        // encode graph
        let mut result = BTreeMap::new();
        for node in graph.node_indices() {
            let pkg_node = graph.node_weight(node).expect("node exists");

            let deps: BTreeMap<PackageName, PackageID> = value
                .inner
                .edges_directed(node, petgraph::Direction::Outgoing)
                .map(|e| (e.weight().clone(), node_to_id[&e.target()].clone()))
                .collect();

            result.insert(
                node_to_id[&node].to_string(),
                Pin {
                    source: pkg_node.package.dep_for_self().clone(),
                    use_environment: Some(pkg_node.use_env.clone()),
                    manifest_digest: graph[node].package.manifest().digest().to_string(),
                    deps,
                },
            );
        }
        result
    }
}
