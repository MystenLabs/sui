// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Convenience methods for building test scenarios with complicated package graphs.
//!
//! To create a simple graph, you can use [TestPackageGraph::new] (which creates the packages) and
//! `add_deps` (which adds dependencies between them):
//!
//! ```
//! use move_package_alt::test_utils::graph_builder::TestPackageGraph;
//!
//! let graph = TestPackageGraph::new(["a", "b", "c"])
//!     .add_deps([("a", "b"), ("b", "c")])
//!     .build();
//!
//! assert_eq!(graph.read_file("a/Move.toml"), r#"
//! [package]
//! name = "a"
//! edition = "2024"
//!
//! [environments]
//! _test_env = "_test_env_id"
//!
//! [dependencies]
//! b = { local = "../b" }
//!
//! [dep-replacements]
//! "#);
//! ```
//!
//! To customize the generated packages and dependencies, you can use
//! [TestPackageGraph::add_package] and [TestPackageGraph::add_dep]. These take closures that
//! customize the generated packages and deps respectively. See [tests::complex] for a complete example.

use std::{
    collections::BTreeMap,
    convert::identity,
    path::{Path, PathBuf},
};

use indoc::formatdoc;
use petgraph::{
    graph::{DiGraph, NodeIndex},
    visit::EdgeRef,
};
use tracing::debug;

use crate::{
    flavor::{
        Vanilla,
        vanilla::{self, DEFAULT_ENV_ID, DEFAULT_ENV_NAME},
    },
    package::{EnvironmentID, EnvironmentName, paths::PackagePath},
    schema::{OriginalID, PackageName, PublishAddresses, PublishedID},
    test_utils::{Project, project},
};

use crate::graph::PackageGraph;

pub struct TestPackageGraph {
    // invariant: for all `node` and `id`, `inner[node].id = id` if and only if `nodes[id] = node`
    // in other words, there is exactly one entry in `nodes` for each node in `inner` and its key
    // is the same as the node's id
    inner: DiGraph<PackageSpec, DepSpec>,
    nodes: BTreeMap<String, NodeIndex>,
}

/// Information used to build a node in the package graph
pub struct PackageSpec {
    /// The `package.name` field.
    name: PackageName,

    /// The identifier used to refer to the package in tests and on the filesystem
    id: String,

    /// The publications for each environment
    pubs: BTreeMap<EnvironmentName, PubSpec>,
}

/// Information used to build an edge in the package graph
pub struct DepSpec {
    /// The name that the containing package gives to the dependency
    name: PackageName,

    /// whether to include `override = true`
    is_override: bool,

    /// the `rename-from` field for the dep
    rename_from: Option<PackageName>,

    /// the `[dep-replacements]` environment to include the dep in (or `None` for the default section)
    in_env: Option<EnvironmentName>,

    /// the `use-environment` field for the dep
    use_env: Option<EnvironmentName>,
}

/// Information about a publication
pub struct PubSpec {
    chain_id: EnvironmentID,
    addresses: PublishAddresses,
}

pub struct Scenario {
    project: Project,
}

impl TestPackageGraph {
    /// Create a package graph containing nodes named `node_names`
    pub fn new(node_names: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
        let mut inner = DiGraph::new();
        let mut nodes = BTreeMap::new();
        for node in node_names {
            let index = inner.add_node(PackageSpec::new(node.as_ref()));
            let old = nodes.insert(node.as_ref().to_string(), index);
            assert!(old.is_none());
        }
        Self { inner, nodes }
    }

    /// Add a dependency to the graph from `a` to `b` for each pair `("a", "b")` in `edges`. The
    /// dependencies will be local dependencies in the `[dependencies]` sections.
    pub fn add_deps(
        mut self,
        edges: impl IntoIterator<Item = (impl AsRef<str>, impl AsRef<str>)>,
    ) -> Self {
        edges.into_iter().fold(self, |graph, (source, target)| {
            graph.add_dep(source, target, identity)
        })
    }

    /// Add and configure a package named
    pub fn add_package(
        mut self,
        node: impl AsRef<str>,
        build: impl FnOnce(PackageSpec) -> PackageSpec,
    ) -> Self {
        let builder = PackageSpec::new(&node);

        let index = self.inner.add_node(build(builder));
        let old = self.nodes.insert(node.as_ref().to_string(), index);
        assert!(old.is_none());

        self
    }

    pub fn add_published(
        mut self,
        node: impl AsRef<str>,
        original_id: OriginalID,
        published_at: PublishedID,
    ) -> Self {
        self.add_package(node, |package| package.publish(original_id, published_at))
    }

    pub fn add_dep(
        mut self,
        source: impl AsRef<str>,
        target: impl AsRef<str>,
        build: impl FnOnce(DepSpec) -> DepSpec,
    ) -> Self {
        let source_idx = self.nodes[source.as_ref()];
        let target_idx = self.nodes[target.as_ref()];
        self.inner
            .add_edge(source_idx, target_idx, build(DepSpec::new(target)));
        self
    }

    /// Generate a project containing subdirectories for each package; each subdirectory will have
    /// a manifest and a lockfile. The manifests will contain all of the dependency information, and
    /// the lockfiles will contain all of the publication information, but the pinned sections of
    /// the lockfiles will be empty (so that the package graph will be built from the manifest).
    /// All dependencies are local
    ///
    /// TODO: we should perhaps add more flexible ways to generate the lockfiles/manifests so that
    /// we can more easily test different aspects of repinning
    pub fn build(self) -> Scenario {
        let mut project = project();
        let mut addr = 0x1000;
        for (package_id, node) in self.nodes.iter() {
            let dir = PathBuf::from(package_id.as_str());
            addr += 1;

            project = project
                .file(dir.join("Move.toml"), &self.format_manifest(*node))
                .file(dir.join("Move.lock"), &self.format_lockfile(*node, addr));
        }

        Scenario {
            project: project.build(),
        }
    }

    /// Return the contents of a `Move.toml` file for the package represented by `node`
    fn format_manifest(&self, node: NodeIndex) -> String {
        let mut move_toml = formatdoc!(
            r#"
                [package]
                name = "{}"
                edition = "2024"

                [environments]
                {DEFAULT_ENV_NAME} = "{DEFAULT_ENV_ID}"

                "#,
            self.inner[node].name
        );
        let mut deps = String::from("\n[dependencies]\n");
        let mut dep_replacements = String::from("\n[dep-replacements]\n");

        for edge in self.inner.edges(node) {
            let dep_spec = edge.weight();
            let dep_str = self.format_dep(edge.weight(), &self.inner[edge.target()]);
            if let Some(env) = &dep_spec.use_env {
                dep_replacements.push_str(&dep_str);
                dep_replacements.push('\n');
            } else {
                deps.push_str(&dep_str);
                deps.push('\n');
            }
        }

        move_toml.push_str(&deps);
        move_toml.push_str(&dep_replacements);
        move_toml
    }

    /// Return the contents of a `Move.lock` file for the package represented by
    /// `node`. The lock file will not contain a `pinned` section, only the `published` section
    ///
    /// For publications with no published-at and original-id fields, we generate them sequentially
    /// starting from 1000 (and set them to the same value)
    fn format_lockfile(&self, node: NodeIndex, default_addr: u16) -> String {
        let mut move_lock = String::new();

        for (env, publication) in self.inner[node].pubs.iter() {
            let PubSpec {
                chain_id,
                addresses:
                    PublishAddresses {
                        original_id,
                        published_at,
                    },
            } = publication;

            move_lock.push_str(&formatdoc!(
                r#"
                    [published.{env}]
                    published-at = "{published_at}"
                    original-id = "{original_id}"
                    chain-id = "{DEFAULT_ENV_ID}"
                    toolchain-version = "test-0.0.0"
                    build-config = {{}}

                    "#,
            ));
        }

        debug!("{move_lock}");
        move_lock
    }

    /// Output the dependency in the form
    /// `<env>.<name> = { local = "...", rename_from = "...", ... }`
    /// (or just `<name> = { ... }` if the environment is `None`)
    fn format_dep(&self, dep: &DepSpec, target: &PackageSpec) -> String {
        let env = dep
            .in_env
            .as_ref()
            .map(|env| format!("{env}."))
            .unwrap_or("".to_string());

        let name = dep.name.as_ref().as_str();
        let path = &target.id;

        let is_override = if dep.is_override {
            ", override = true"
        } else {
            ""
        };

        let rename_from = dep
            .rename_from
            .as_ref()
            .map(|name| format!(r#", rename-from = "{name}""#))
            .unwrap_or("".to_string());

        let use_env = dep
            .use_env
            .as_ref()
            .map(|env| format!(r#", use-environment = "{env}""#))
            .unwrap_or("".to_string());

        format!(r#"{env}{name} = {{ local = "../{path}"{is_override}{rename_from}{use_env} }}"#)
    }
}

impl PackageSpec {
    /// Create a new empty package spec
    fn new(name: impl AsRef<str>) -> Self {
        Self {
            name: PackageName::new(name.as_ref()).expect("valid package name"),
            pubs: BTreeMap::new(),
            id: name.as_ref().to_string(),
        }
    }

    pub fn publish(mut self, original_id: OriginalID, published_at: PublishedID) -> Self {
        self.pubs.insert(
            DEFAULT_ENV_NAME.to_string(),
            PubSpec {
                chain_id: DEFAULT_ENV_ID.to_string(),
                addresses: PublishAddresses {
                    original_id,
                    published_at,
                },
            },
        );
        self
    }

    /// Update that `name` field in the `[package]` section of the manifest
    pub fn package_name(mut self, name: impl AsRef<str>) -> Self {
        self.name = PackageName::new(name.as_ref()).expect("valid package name");
        self
    }
}

impl DepSpec {
    fn new(name: impl AsRef<str>) -> Self {
        Self {
            name: PackageName::new(name.as_ref()).expect("valid package name"),
            is_override: false,
            rename_from: None,
            in_env: None,
            use_env: None,
        }
    }

    /// Add `override = true` to the dependency
    pub fn set_override(mut self) -> Self {
        self.is_override = true;
        self
    }

    /// Set the name used for the dependency in the containing package
    pub fn name(mut self, name: impl AsRef<str>) -> Self {
        self.name = PackageName::new(name.as_ref()).expect("valid package name");
        self
    }

    /// Set the `rename-from` field of the dependency
    pub fn rename_from(mut self, original: impl AsRef<str>) -> Self {
        self.rename_from = Some(PackageName::new(original.as_ref()).expect("valid package name"));
        self
    }

    /// Only include the dependency in `env` (in the `dep-replacements` section)
    pub fn in_env(mut self, env: impl AsRef<str>) -> Self {
        self.in_env = Some(env.as_ref().to_string());
        self
    }

    /// Set the `use-environment` field of the dependency
    pub fn use_env(mut self, env: impl AsRef<str>) -> Self {
        self.use_env = Some(env.as_ref().to_string());
        self
    }
}

impl Scenario {
    pub async fn graph_for(&self, package: impl AsRef<str>) -> PackageGraph<Vanilla> {
        let path = PackagePath::new(self.project.root().join(package.as_ref())).unwrap();

        PackageGraph::<Vanilla>::load_from_manifests(&path, &vanilla::default_environment())
            .await
            .map_err(|e| e.emit())
            .expect("could load package")
    }

    pub fn read_file(&self, file: impl AsRef<Path>) -> String {
        self.project.read_file(file)
    }
}

mod tests {
    use insta::assert_snapshot;

    use crate::schema::{OriginalID, PublishedID};

    use super::TestPackageGraph;

    /// Ensure that using the basic features of [TestPackageGraph] gives a correct manifest and
    /// lockfile
    #[test]
    fn simple() {
        let graph = TestPackageGraph::new(["a", "b", "c"])
            .add_deps([("a", "b"), ("a", "c")])
            .build();

        assert_snapshot!(graph.read_file("a/Move.toml"), @r###"
        [package]
        name = "a"
        edition = "2024"

        [environments]
        _test_env = "_test_env_id"


        [dependencies]
        c = { local = "../c" }
        b = { local = "../b" }

        [dep-replacements]
        "###);

        assert_snapshot!(graph.read_file("a/Move.lock"), @"");

        assert_snapshot!(graph.read_file("b/Move.lock"), @"");
    }

    /// Ensure that using all the features of [TestPackageGraph] gives the correct manifests and
    /// lockfiles
    #[test]
    fn complex() {
        let graph = TestPackageGraph::new(["a", "b"])
            .add_package("c", |c| {
                c.package_name("c_name")
                    .publish(OriginalID::from(0xcc00), PublishedID::from(0xcccc))
            })
            .add_deps([("b", "c")])
            .add_dep("a", "b", |dep| {
                dep.set_override()
                    .name("a_name_for_b")
                    .rename_from("b")
                    .in_env("foo")
                    .use_env("bar")
            })
            .build();

        assert_snapshot!(graph.read_file("a/Move.toml"), @r###"
        [package]
        name = "a"
        edition = "2024"

        [environments]
        _test_env = "_test_env_id"


        [dependencies]

        [dep-replacements]
        foo.a_name_for_b = { local = "../b", override = true, rename-from = "b", use-environment = "bar" }
        "###);

        assert_snapshot!(graph.read_file("b/Move.toml"), @r###"
        [package]
        name = "b"
        edition = "2024"

        [environments]
        _test_env = "_test_env_id"


        [dependencies]
        c = { local = "../c" }

        [dep-replacements]
        "###);

        assert_snapshot!(graph.read_file("c/Move.toml"), @r###"
        [package]
        name = "c_name"
        edition = "2024"

        [environments]
        _test_env = "_test_env_id"


        [dependencies]

        [dep-replacements]
        "###);

        assert_snapshot!(graph.read_file("c/Move.lock"), @r###"
        [published._test_env]
        published-at = "0x000000000000000000000000000000000000000000000000000000000000cccc"
        original-id = "0x000000000000000000000000000000000000000000000000000000000000cc00"
        chain-id = "_test_env_id"
        toolchain-version = "test-0.0.0"
        build-config = {}
        "###);
    }
}
