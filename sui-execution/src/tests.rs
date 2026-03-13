// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeSet, HashMap},
    path::PathBuf,
};

use cargo_metadata::{DependencyKind, Metadata, MetadataCommand, Package};
use petgraph::{algo::all_simple_paths, prelude::DiGraphMap};

type PackageGraph<'p> = DiGraphMap<&'p str, ()>;

struct Packages(HashMap<String, Package>);

#[test]
/// Make sure that all accesses to execution layer crates in the `sui-node` and `sui-replay` crates
/// go via the `sui-execution` crate (in other words, the `sui-execution` crate dominates execution
/// layer crates in the dependency graphs of `sui-node` and `sui-replay`).
///
/// This helps ensures that execution that may be committed on-chain respects the execution version
/// that is stated in the protocol config.
fn test_encapsulation() {
    let metadata = cargo_metadata().unwrap();
    let packages = Packages::new(&metadata);

    // Identify the crates that are part of the execution layer
    let mut exec_crates: BTreeSet<_> = packages.normal_deps("sui-execution").collect();

    // Remove the crates that the execution layer depends on but which are not directly part of the
    // execution layer -- these don't need to be accessed exclusively via `sui-execution`.
    exec_crates.remove("sui-protocol-config");
    exec_crates.remove("sui-types");
    exec_crates.remove("move-binary-format");
    exec_crates.remove("move-bytecode-utils");
    exec_crates.remove("move-core-types");
    exec_crates.remove("move-vm-config");
    // tracing is only enabled in client builds (built with `--features tracing` flag)
    // and it does not have to be accessed via `sui-execution` as it can never cause a fork
    exec_crates.remove("move-trace-format");

    // Capture problematic paths from roots to execution crates
    let mut examples = vec![];

    for root in ["sui-node", "sui-replay"] {
        let mut graph = packages.graph(root);

        // If we can still create a path from `root` to an execution crate after removing these
        // nodes then we know that we can potential bypass "sui-execution".
        graph.remove_node("sui-execution");

        for exec_crate in &exec_crates {
            let paths = all_simple_paths::<Vec<&str>, &PackageGraph>(
                &graph, root, exec_crate, /* min_intermediate_nodes */ 0,
                /* max_intermediate_nodes */ None,
            );

            examples.extend(paths.map(|p| p.join(" -> ")));
        }
    }

    if examples.is_empty() {
        return;
    }

    panic!(
        "protocol-sensitive binaries depend on execution crates outside of 'sui-execution', e.g.:\n\
         \n  {}\n\
         \n\
         This can cause execution to fork by not respecting the execution layer version set in the \
         protocol config.  Fix this by depending on these crates via 'sui-execution'.\n\
         \n\
         P.S. if you believe one of these crates should not be part of 'sui-execution' then update \
         the test to exclude this crate.",
        examples.join("\n  "),
    );
}

/// Parse `cargo metadata` for the `sui` repo.
fn cargo_metadata() -> cargo_metadata::Result<Metadata> {
    let sui_execution = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    MetadataCommand::new()
        .manifest_path(sui_execution.join("../Cargo.toml"))
        .current_dir(sui_execution.join(".."))
        .no_deps()
        .exec()
}

impl Packages {
    /// Create a mapping from package names to package `metadata` (from the output of `cargo
    /// metadata`).
    fn new(metadata: &Metadata) -> Self {
        Self(HashMap::from_iter(
            metadata
                .packages
                .iter()
                .map(|pkg| (pkg.name.clone(), pkg.clone())),
        ))
    }

    /// Extract the transitive dependency sub-graph of the package named `root`.  The graph is a
    /// directed, unweighted graph with nodes representing packages, identified by their name (a
    /// `&str`).
    fn graph<'p>(&'p self, root: &'p str) -> PackageGraph<'p> {
        let mut graph = PackageGraph::new();
        let mut stack = vec![];

        stack.extend(self.normal_edges(root));
        while let Some((from, to)) = stack.pop() {
            if !graph.contains_node(to) {
                graph.add_edge(from, to, ());
                stack.extend(self.normal_edges(to))
            }
        }

        graph
    }

    /// Returns an iterator over all the edges from `pkg` to its "normal" dependencies (represented
    /// as pairs of Node IDs).  A normal dependency is a non-target specific, non-build, non-dev
    /// dependency.
    fn normal_edges<'p, 'q>(&'q self, pkg: &'p str) -> impl Iterator<Item = (&'p str, &'q str)> {
        self.0
            .get(pkg)
            .map(|p| &p.dependencies)
            .into_iter()
            .flatten()
            .filter_map(move |dep| {
                if let (DependencyKind::Normal, None) = (dep.kind, &dep.target) {
                    Some((pkg, dep.name.as_str()))
                } else {
                    None
                }
            })
    }

    /// Returns an iterator over all of `pkg`'s "normal" dependencies. (See [normal_edges] for a
    /// definition of "normal").
    fn normal_deps<'p, 'q: 'p>(&'q self, pkg: &'p str) -> impl 'p + Iterator<Item = &'q str> {
        self.normal_edges(pkg).map(move |(_, to)| to)
    }
}

#[test]
fn temporary_make_sure_latest_v3_match_for_bella_ciao() {
    // The expected diff between `latest` and `v3`. These directories are intentionally kept in
    // sync and should only differ in their Cargo.toml crate names and dependency paths.
    // If this snapshot changes, it means someone modified one side without the other.
    let expected_diff = r#"diff -r latest/sui-adapter/Cargo.toml v3/sui-adapter/Cargo.toml
2c2
< name = "sui-adapter-latest"
---
> name = "sui-adapter-v3"
32,33c32,33
< move-bytecode-verifier = { path = "../../../external-crates/move/crates/move-bytecode-verifier" }
< move-vm-runtime = { path = "../../../external-crates/move/crates/move-vm-runtime" }
---
> move-bytecode-verifier = { path = "../../../external-crates/move/move-execution/v3/crates/move-bytecode-verifier", package = "move-bytecode-verifier-v3" }
> move-vm-runtime = { path = "../../../external-crates/move/move-execution/v3/crates/move-vm-runtime", package = "move-vm-runtime-v3" }
35,37c35,37
< sui-move-natives = { path = "../sui-move-natives", package = "sui-move-natives-latest" }
< sui-verifier = { path = "../sui-verifier", package = "sui-verifier-latest" }
< move-vm-types = { path = "../../../external-crates/move/crates/move-vm-types" }
---
> sui-move-natives = { path = "../sui-move-natives", package = "sui-move-natives-v3" }
> sui-verifier = { path = "../sui-verifier", package = "sui-verifier-v3" }
> move-vm-types = { path = "../../../external-crates/move/move-execution/v3/crates/move-vm-types", package = "move-vm-types-v3" }
diff -r latest/sui-move-natives/Cargo.toml v3/sui-move-natives/Cargo.toml
2c2
< name = "sui-move-natives-latest"
---
> name = "sui-move-natives-v3"
26,28c26,28
< move-stdlib-natives = { path = "../../../external-crates/move/crates/move-stdlib-natives" }
< move-vm-runtime = { path = "../../../external-crates/move/crates/move-vm-runtime" }
< move-vm-types = { path = "../../../external-crates/move/crates/move-vm-types" }
---
> move-stdlib-natives = { path = "../../../external-crates/move/move-execution/v3/crates/move-stdlib-natives", package = "move-stdlib-natives-v3" }
> move-vm-runtime = { path = "../../../external-crates/move/move-execution/v3/crates/move-vm-runtime", package = "move-vm-runtime-v3" }
> move-vm-types = { path = "../../../external-crates/move/move-execution/v3/crates/move-vm-types", package = "move-vm-types-v3" }
diff -r latest/sui-verifier/Cargo.toml v3/sui-verifier/Cargo.toml
2c2
< name = "sui-verifier-latest"
---
> name = "sui-verifier-v3"
18c18
< move-bytecode-verifier = { path = "../../../external-crates/move/crates/move-bytecode-verifier" }
---
> move-bytecode-verifier = { path = "../../../external-crates/move/move-execution/v3/crates/move-bytecode-verifier", package = "move-bytecode-verifier-v3" }"#;

    let sui_execution = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    let output = std::process::Command::new("diff")
        .arg("-r")
        .arg("latest")
        .arg("v3")
        .current_dir(&sui_execution)
        .output()
        .expect("failed to run diff");

    let actual_diff = String::from_utf8_lossy(&output.stdout);
    let actual_diff = actual_diff.trim();

    if actual_diff != expected_diff {
        panic!(
            "The diff between `sui-execution/latest` and `sui-execution/v3` has changed.\n\
             This is a temporary check to make sure these stay in sync during the landing process of bella ciao.\n\
             If you are making changes to the sui-execution layer and see this error, make sure you reflect all changed from `latest` into `v3`.\n\
             \n\
             Expected diff:\n{expected_diff}\n\
             \n\
             Actual diff:\n{actual_diff}"
        );
    }
}
