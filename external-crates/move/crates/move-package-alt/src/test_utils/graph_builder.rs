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

#![allow(unused)]

use std::{
    collections::BTreeMap,
    convert::identity,
    path::{Path, PathBuf},
};

use heck::CamelCase;
use indoc::{formatdoc, indoc};
use petgraph::{
    graph::{DiGraph, NodeIndex},
    visit::EdgeRef,
};
use tempfile::TempDir;
use tracing::debug;

use crate::{
    errors::PackageResult,
    flavor::{
        Vanilla,
        vanilla::{self, DEFAULT_ENV_ID, DEFAULT_ENV_NAME, default_environment},
    },
    package::{
        EnvironmentID, EnvironmentName, RootPackage, package_lock::PackageSystemLock,
        paths::PackagePath,
    },
    schema::{Environment, ModeName, OriginalID, PublishAddresses, PublishedID},
    test_utils::{Project, project},
};

use crate::graph::PackageGraph;

use super::git::RepoProject;

pub struct TestPackageGraph {
    // invariant: for all `node` and `id`, `inner[node].id = id` if and only if `nodes[id] = node`
    // in other words, there is exactly one entry in `nodes` for each node in `inner` and its key
    // is the same as the node's id
    inner: DiGraph<PackageSpec, DepSpec>,
    nodes: BTreeMap<String, NodeIndex>,
    root: Option<PathBuf>,
}

/// Information used to build a node in the package graph
pub struct PackageSpec {
    /// The `package.name` field.
    name: String,

    /// The identifier used to refer to the package in tests and on the filesystem
    id: String,

    /// The publications for each environment
    pubs: BTreeMap<EnvironmentName, PubSpec>,

    /// Is the package a legacy package?
    is_legacy: bool,

    /// ```toml
    /// name = "MoveStdLib" <-- `legacy_name`
    ///
    /// [addresses]
    /// std = "0x1" <-- `name`
    /// ```
    legacy_name: Option<String>,

    /// The version field in the manifest
    version: Option<String>,

    /// Any git deps
    git_deps: Vec<GitSpec>,

    /// Additional files
    files: BTreeMap<PathBuf, String>,

    /// Are implicit deps included?
    implicit_deps: bool,

    /// The environments to be applied to the package's manifest.
    /// IF empty, no environments will be written to the manifest.
    environments: BTreeMap<EnvironmentName, EnvironmentID>,

    /// Custom addresses for legacy packages (name -> Option<address>)
    /// If None is provided, the address is considered to be the legacy `_`.
    legacy_addresses: BTreeMap<String, Option<String>>,
}

struct GitSpec {
    repo: String,
    path: String,
    rev: String,
    spec: DepSpec,
}

/// Information used to build an edge in the package graph
pub struct DepSpec {
    /// The name that the containing package gives to the dependency
    name: String,

    /// whether to include `override = true`
    is_override: bool,

    /// the `rename-from` field for the dep
    rename_from: Option<String>,

    /// the `[dep-replacements]` environment to include the dep in (or `None` for the default section)
    in_env: Option<EnvironmentName>,

    /// the `use-environment` field for the dep
    use_env: Option<EnvironmentName>,

    /// the `modes` field for the dep
    modes: Option<Vec<ModeName>>,
}

/// Information about a publication
pub struct PubSpec {
    chain_id: EnvironmentID,
    addresses: PublishAddresses,
    version: u64,
}

pub struct Scenario {
    root_path: PathBuf,
    tempdir: Option<TempDir>,
}

impl TestPackageGraph {
    /// Create a package graph containing nodes named `node_names`
    pub fn new(node_names: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
        let mut inner = DiGraph::new();
        let mut nodes = BTreeMap::new();
        let result = Self {
            inner,
            nodes,
            root: None,
        };
        result.add_packages(node_names)
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

    /// Add a list of packages with no additional configuration
    pub fn add_packages(self, node_names: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
        node_names
            .into_iter()
            .fold(self, |graph, node| graph.add_package(node, |pkg| pkg))
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

    pub fn add_legacy_packages(mut self, nodes: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
        for node in nodes {
            self = self.add_package(node, |pkg| pkg.set_legacy())
        }
        self
    }

    /// `builder.add_published("a", original, published_at, None)` is shorthand for
    /// ```ignore
    /// builder.add_package("a", |a| a.publish(original, published_at, None))
    /// ```
    pub fn add_published(
        self,
        node: impl AsRef<str>,
        original_id: OriginalID,
        published_at: PublishedID,
    ) -> Self {
        self.add_package(node, |package| {
            package.publish(original_id, published_at, None)
        })
    }

    /// Add a dependency from package `source` to package `target` and customize it using `build`
    pub fn add_dep(
        mut self,
        source: impl AsRef<str>,
        target: impl AsRef<str>,
        build: impl FnOnce(DepSpec) -> DepSpec,
    ) -> Self {
        let source_idx = self.nodes[source.as_ref()];
        let target_idx = self.nodes[target.as_ref()];
        let dep_spec = build(DepSpec::new(target));

        self.inner.add_edge(source_idx, target_idx, dep_spec);
        self
    }

    /// Add a git dependency from `source` to package `target` inside the git repository `repo`
    /// with revision `rev`
    pub fn add_git_dep(
        mut self,
        source: impl AsRef<str>,
        repo: &RepoProject,
        target: impl AsRef<str>,
        rev: impl AsRef<str>,
        build: impl FnOnce(DepSpec) -> DepSpec,
    ) -> Self {
        let source_idx = self.nodes[source.as_ref()];
        let dep_spec = build(DepSpec::new(&target));
        self.inner[self.nodes[source.as_ref()]]
            .git_deps
            .push(GitSpec {
                repo: repo.repo_path_str(),
                path: target.as_ref().to_string(),
                rev: rev.as_ref().to_string(),
                spec: dep_spec,
            });
        self
    }

    pub fn at(mut self, path: impl AsRef<Path>) -> Self {
        self.root = Some(path.as_ref().to_path_buf());
        self
    }

    /// Generate a project containing subdirectories for each package; each subdirectory will have
    /// a manifest and a lockfile. The manifests will contain all of the dependency information, and
    /// the lockfiles will contain all of the publication information, but the pinned sections of
    /// the lockfiles will be empty (so that the package graph will be built from the manifest).
    /// All dependencies are local
    pub fn build(self) -> Scenario {
        let (tempdir, root_path) = match &self.root {
            Some(file) => (None, file.to_path_buf()),
            None => {
                let tmp = TempDir::new().unwrap();
                let path = tmp.path().to_path_buf();
                (Some(tmp), path)
            }
        };

        for (package_id, node) in self.nodes.iter() {
            let dir = root_path.join(package_id.as_str());
            std::fs::create_dir_all(&dir).unwrap();

            let manifest = &self.format_manifest(*node);
            debug!(
                "Generated test manifest for {package_id} ({:?}):\n\n{manifest}",
                dir.join("Move.toml")
            );
            std::fs::write(dir.join("Move.toml"), manifest).unwrap();

            let pubfile = &self.format_pubfile(*node);
            if !pubfile.is_empty() {
                debug!("Generated test pubfile for {package_id}:\n\n{pubfile}");
                std::fs::write(dir.join("Published.toml"), pubfile).unwrap();
            }

            // add extra files
            for (path, contents) in self.inner[*node].files.iter() {
                std::fs::create_dir_all(dir.join(path).parent().unwrap()).unwrap();
                std::fs::write(dir.join(path), contents).unwrap();
            }
        }

        Scenario { tempdir, root_path }
    }

    /// Return the contents of a `Move.toml` file for the package represented by `node`
    fn format_manifest(&self, node: NodeIndex) -> String {
        if self.inner[node].is_legacy {
            return self.format_legacy_manifest(node);
        }

        let version_str = match &self.inner[node].version {
            Some(v) => format!("version = \"{v}\"\n"),
            None => "".into(),
        };

        let implicits = if self.inner[node].implicit_deps {
            ""
        } else {
            "implicit-dependencies = false\n"
        };

        let environments = if self.inner[node].environments.is_empty() {
            "".to_string()
        } else {
            format!(
                r#"
                [environments]
                {}
                "#,
                self.inner[node]
                    .environments
                    .iter()
                    .map(|(name, id)| format!("{name} = \"{id}\""))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        };

        let mut move_toml = formatdoc!(
            r#"
                [package]
                name = "{}"
                edition = "2024"
                {version_str}
                {implicits}
                {environments}

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

        for git_dep in &self.inner[node].git_deps {
            let dep_str = self.format_git_dep(git_dep);
            if let Some(env) = &git_dep.spec.use_env {
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

    /// Return the contents of a legacy `Move.toml` file for the legacy package represented by
    /// `node`
    fn format_legacy_manifest(&self, node: NodeIndex) -> String {
        let package = &self.inner[node];
        assert!(package.is_legacy);

        assert!(
            package.pubs.len() <= 1,
            "legacy packages may have at most one publication"
        );
        let publication = package
            .pubs
            .first_key_value()
            .map(|(env, publication)| publication);

        let published_at = publication
            .map(|it| format!("published-at = {}", it.addresses.published_at))
            .unwrap_or_default();

        let implicits = if package.implicit_deps {
            ""
        } else {
            "implicit-dependencies = false\n"
        };

        let mut move_toml = formatdoc!(
            r#"
            [package]
            name = "{}"
            edition = "2024"
            {published_at}
            {implicits}
            "#,
            package
                .legacy_name
                .clone()
                .unwrap_or(package.id.to_camel_case())
        );

        let mut deps = String::from("\n[dependencies]\n");
        for edge in self.inner.edges(node) {
            let dep_spec = edge.weight();
            let dep_str = self.format_legacy_dep(edge.weight(), &self.inner[edge.target()]);
            deps.push_str(&dep_str);
            deps.push('\n');
        }
        move_toml.push_str(&deps);
        move_toml.push('\n');

        // Generate [addresses] section
        move_toml.push_str("[addresses]\n");

        // If custom addresses are provided, use them
        if !package.legacy_addresses.is_empty() {
            for (name, addr) in &package.legacy_addresses {
                match addr {
                    Some(addr_val) => {
                        move_toml.push_str(&format!("{} = \"{}\"\n", name, addr_val));
                    }
                    None => {
                        // Unresolved address - omit the value to make it Option<>
                        move_toml.push_str(&format!("{} = \"_\"\n", name));
                    }
                }
            }
        } else {
            // Default behavior: single address for the package name
            move_toml.push_str(&format!(
                "{} = \"{}\"\n",
                package.name,
                publication
                    .map(|it| it.addresses.original_id.to_string())
                    .unwrap_or("0x0".to_string())
            ));
        }

        move_toml
    }

    /// Return the contents of a `Move.lock` file for the package represented by
    /// `node`.
    fn format_pubfile(&self, node: NodeIndex) -> String {
        let mut pubfile = String::new();

        for (env, publication) in self.inner[node].pubs.iter() {
            let PubSpec {
                addresses:
                    PublishAddresses {
                        original_id,
                        published_at,
                    },
                version,
                ..
            } = publication;

            pubfile.push_str(&formatdoc!(
                r#"
                    [published.{env}]
                    chain-id = "{DEFAULT_ENV_ID}"
                    published-at = "{published_at}"
                    original-id = "{original_id}"
                    version = {version}
                    "#,
            ));
        }

        pubfile
    }

    /// Output the dependency in the form
    /// `<env>.<name> = { local = "...", rename_from = "...", ... }`
    /// (or just `<name> = { ... }` if the environment is `None`)
    fn format_dep(&self, dep: &DepSpec, target: &PackageSpec) -> String {
        let path = &target.id;
        Self::decorate_dep(&format!(r#"local = "../{path}""#), dep)
    }

    fn format_git_dep(&self, dep: &GitSpec) -> String {
        let GitSpec {
            repo,
            path,
            rev,
            spec: dep,
        } = dep;

        let git = format!(r#"git = "{repo}", subdir = "{path}", rev = "{rev}""#);
        Self::decorate_dep(&git, dep)
    }

    /// Returns `{{ {location} {additional_fields} }}` where `additional_fields` are generated from
    /// `spec`
    fn decorate_dep(location: &str, dep: &DepSpec) -> String {
        let env = dep
            .in_env
            .as_ref()
            .map(|env| format!("{env}."))
            .unwrap_or("".to_string());

        let name = &dep.name;

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

        let modes = dep
            .modes
            .as_ref()
            .map(|modes| format!(r#", modes = {modes:?}"#))
            .unwrap_or("".to_string());
        format!(r#"{env}{name} = {{ {location}{is_override}{rename_from}{use_env}{modes} }}"#)
    }

    /// Output the dependency in the form
    /// `<capitalized-name> = { ... }`, failing if the dependency uses non-legacy features
    fn format_legacy_dep(&self, dep: &DepSpec, target: &PackageSpec) -> String {
        // TODO: we could share more code with the non-legacy stuff I think
        let name = &dep.name;
        let path = &target.id;

        let is_override = if dep.is_override {
            ", override = true"
        } else {
            ""
        };

        assert!(
            dep.rename_from.is_none(),
            "legacy manifests don't support rename-from"
        );

        assert!(
            dep.use_env.is_none(),
            "legacy manifests don't support use-env"
        );

        format!(r#"{name} = {{ local = "../{path}"{is_override} }}"#)
    }
}

impl PackageSpec {
    /// Create a new empty package spec
    fn new(name: impl AsRef<str>) -> Self {
        Self {
            name: name.as_ref().to_string(),
            pubs: BTreeMap::new(),
            id: name.as_ref().to_string(),
            is_legacy: false,
            legacy_name: None,
            git_deps: vec![],
            version: None,
            files: BTreeMap::new(),
            implicit_deps: true,
            environments: BTreeMap::new(),
            legacy_addresses: BTreeMap::new(),
        }
    }

    pub fn publish(
        mut self,
        original_id: OriginalID,
        published_at: PublishedID,
        version: Option<u64>,
    ) -> Self {
        self.publish_in_env(
            DEFAULT_ENV_NAME,
            DEFAULT_ENV_ID,
            original_id,
            published_at,
            version,
        )
    }

    pub fn publish_in_env(
        mut self,
        env_name: impl AsRef<str>,
        env_id: impl AsRef<str>,
        original_id: OriginalID,
        published_at: PublishedID,
        version: Option<u64>,
    ) -> Self {
        self.pubs.insert(
            env_name.as_ref().to_string(),
            PubSpec {
                chain_id: env_id.as_ref().to_string(),
                addresses: PublishAddresses {
                    original_id,
                    published_at,
                },
                version: version.unwrap_or(1),
            },
        );
        self
    }

    pub fn add_env(mut self, env_name: impl AsRef<str>, env_id: impl AsRef<str>) -> Self {
        self.environments
            .insert(env_name.as_ref().to_string(), env_id.as_ref().to_string());
        self
    }

    /// Update that `name` field in the `[package]` section of the manifest
    pub fn package_name(mut self, name: impl AsRef<str>) -> Self {
        self.name = name.as_ref().to_string();
        self
    }

    pub fn add_file(mut self, path: impl AsRef<Path>, contents: impl AsRef<str>) -> Self {
        self.files
            .insert(path.as_ref().to_path_buf(), contents.as_ref().to_string());
        self
    }

    /// Change this package to a legacy package. Legacy packages will produce manfests with
    /// upper-cased names for the package and the dependency, and will contain an `[addresses]`
    /// section with a single variable given by the package name.
    ///
    /// If the package is published, the `published-at` field will be added and the named-address
    /// will be set to the original ID; otherwise there will be no published-at field and the
    /// named address will be set to 0.
    pub fn set_legacy(mut self) -> Self {
        self.is_legacy = true;
        self
    }

    /// Set the `name` field in the manifest of legacy packages.
    pub fn set_legacy_name(mut self, name: impl AsRef<str>) -> Self {
        assert!(self.is_legacy);
        self.legacy_name = Some(name.as_ref().to_string());
        self
    }

    pub fn version(mut self, version: impl AsRef<str>) -> Self {
        self.version = Some(version.as_ref().to_string());
        self
    }

    pub fn implicit_deps(mut self, implicits: bool) -> Self {
        self.implicit_deps = implicits;
        self
    }

    pub fn set_legacy_addresses(
        mut self,
        addresses: impl IntoIterator<Item = (impl AsRef<str>, Option<impl AsRef<str>>)>,
    ) -> Self {
        assert!(
            self.is_legacy,
            "Setting addresses is only supported for legacy packages"
        );
        self.legacy_addresses = addresses
            .into_iter()
            .map(|(k, v)| (k.as_ref().to_string(), v.map(|v| v.as_ref().to_string())))
            .collect();
        self
    }
}

impl DepSpec {
    fn new(name: impl AsRef<str>) -> Self {
        Self {
            name: name.as_ref().to_string(),
            is_override: false,
            rename_from: None,
            in_env: None,
            use_env: None,
            modes: None,
        }
    }

    /// Add `override = true` to the dependency
    pub fn set_override(mut self) -> Self {
        self.is_override = true;
        self
    }

    /// Set the name used for the dependency in the containing package
    pub fn name(mut self, name: impl AsRef<str>) -> Self {
        self.name = name.as_ref().to_string();
        self
    }

    /// Set the `rename-from` field of the dependency
    pub fn rename_from(mut self, original: impl AsRef<str>) -> Self {
        self.rename_from = Some(original.as_ref().to_string());
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

    /// Add a `modes` field
    pub fn modes(mut self, modes: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
        self.modes = Some(
            modes
                .into_iter()
                .map(|s| ModeName::from(s.as_ref()))
                .collect(),
        );
        self
    }
}

impl Scenario {
    pub fn path_for(&self, package: impl AsRef<str>) -> PathBuf {
        self.root_path.join(package.as_ref())
    }

    pub(crate) async fn graph_for(&self, package: impl AsRef<str>) -> PackageGraph<Vanilla> {
        self.try_graph_for(package)
            .await
            .map_err(|e| e.emit())
            .expect("could load package")
    }

    pub(crate) async fn try_graph_for(
        &self,
        package: impl AsRef<str>,
    ) -> PackageResult<PackageGraph<Vanilla>> {
        let path = PackagePath::new(self.path_for(package)).unwrap();
        let mtx = path.lock().unwrap();

        PackageGraph::<Vanilla>::load_from_manifests(&path, &vanilla::default_environment(), &mtx)
            .await
    }

    /// Loads the root package for `package` in the default environment and with no modes
    pub async fn root_package(&self, package: impl AsRef<str>) -> RootPackage<Vanilla> {
        self.try_root_package(package)
            .await
            .map_err(|e| e.emit())
            .expect("could load package")
    }

    /// Loads the root package for `package` and expects an error; returns the (redacted) contents
    /// of the error
    pub async fn root_package_err(&self, package: impl AsRef<str>) -> String {
        match self.try_root_package(package).await {
            Ok(_) => panic!("expected root package to fail to load"),
            Err(err) => err
                .to_string()
                .replace(self.root_path.to_string_lossy().as_ref(), "<ROOT>"),
        }
    }

    /// Loads the root package for `package` in the default environment and with no modes
    pub async fn try_root_package(
        &self,
        package: impl AsRef<str>,
    ) -> PackageResult<RootPackage<Vanilla>> {
        RootPackage::<Vanilla>::load(self.path_for(package), default_environment(), vec![]).await
    }

    pub fn read_file(&self, file: impl AsRef<Path>) -> String {
        let path = self.root_path.join(&file);
        debug!("reading file at {path:?}");
        std::fs::read_to_string(self.root_path.join(file.as_ref())).unwrap()
    }

    pub fn extend_file(&self, file: impl AsRef<Path>, contents: impl AsRef<str>) {
        let path = self.root_path.join(&file);
        debug!("adding to file at {path:?}");
        let mut file_contents = std::fs::read_to_string(&path).unwrap();
        file_contents.push_str(contents.as_ref());
        std::fs::write(&path, &file_contents).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;
    use test_log::test;
    use tracing::debug;

    use crate::{
        schema::{OriginalID, PublishedID},
        test_utils::git,
    };

    use super::TestPackageGraph;

    /// Ensure that using the basic features of [TestPackageGraph] gives a correct manifest and
    /// lockfile
    #[test]
    fn simple() {
        let graph = TestPackageGraph::new(["a", "b", "c"])
            .add_deps([("a", "b"), ("a", "c")])
            .build();

        assert_snapshot!(graph.read_file("a/Move.toml"), @r#"
        [package]
        name = "a"
        edition = "2024"





        [dependencies]
        c = { local = "../c" }
        b = { local = "../b" }

        [dep-replacements]
        "#);
    }

    /// Ensure that using all the features of [TestPackageGraph] gives the correct manifests and
    /// lockfiles
    #[test]
    fn complex() {
        // TODO: break this into separate tests
        let graph = TestPackageGraph::new(["a", "b"])
            .add_package("c", |c| {
                c.package_name("c_name")
                    .publish(OriginalID::from(0xcc00), PublishedID::from(0xcccc), None)
                    .version("v1.2.3")
                    .add_file("sources/extra.move", "// comment")
                    .implicit_deps(false)
            })
            .add_deps([("b", "c")])
            .add_dep("a", "b", |dep| {
                dep.set_override()
                    .name("a_name_for_b")
                    .rename_from("b")
                    .in_env("foo")
                    .use_env("bar")
                    .modes(vec!["test", "spec"])
            })
            .build();

        assert_snapshot!(graph.read_file("a/Move.toml"), @r#"
        [package]
        name = "a"
        edition = "2024"





        [dependencies]

        [dep-replacements]
        foo.a_name_for_b = { local = "../b", override = true, rename-from = "b", use-environment = "bar", modes = ["test", "spec"] }
        "#);

        assert_snapshot!(graph.read_file("b/Move.toml"), @r#"
        [package]
        name = "b"
        edition = "2024"





        [dependencies]
        c = { local = "../c" }

        [dep-replacements]
        "#);

        assert_snapshot!(graph.read_file("c/Move.toml"), @r#"
        [package]
        name = "c_name"
        edition = "2024"
        version = "v1.2.3"

        implicit-dependencies = false




        [dependencies]

        [dep-replacements]
        "#);

        assert_snapshot!(graph.read_file("c/Published.toml"), @r###"
        [published._test_env]
        chain-id = "_test_env_id"
        published-at = "0x000000000000000000000000000000000000000000000000000000000000cccc"
        original-id = "0x000000000000000000000000000000000000000000000000000000000000cc00"
        version = 1
        "###);

        assert_snapshot!(graph.read_file("c/sources/extra.move"), @r###"
        // comment
        "###);
    }

    /// Check generation of legacy manifests
    #[test]
    fn legacy() {
        let graph = TestPackageGraph::new(["a", "c"])
            .add_legacy_packages(["b"])
            .add_package("d", |d| {
                d.set_legacy()
                    .set_legacy_name("Any")
                    .publish(OriginalID::from(0x4444), PublishedID::from(0x5555), None)
                    .implicit_deps(false)
            })
            .add_deps([("a", "b"), ("b", "c"), ("c", "d")])
            .build();

        assert_snapshot!(graph.read_file("b/Move.toml"), @r###"
        [package]
        name = "B"
        edition = "2024"



        [dependencies]
        c = { local = "../c" }

        [addresses]
        b = "0x0"
        "###);

        assert_snapshot!(graph.read_file("d/Move.toml"), @r###"
        [package]
        name = "Any"
        edition = "2024"
        published-at = 0x0000000000000000000000000000000000000000000000000000000000005555
        implicit-dependencies = false


        [dependencies]

        [addresses]
        d = "0x0000000000000000000000000000000000000000000000000000000000004444"
        "###);
    }

    /// Snapshot test for a repo with a git dependency
    #[test(tokio::test)]
    async fn git() {
        let git_repo = git::new().await;
        let commit = git_repo
            .commit(|project| project.add_packages(["git_dep"]))
            .await;
        let branch = commit.branch("main").await;

        let graph = TestPackageGraph::new(["root"])
            .add_git_dep("root", &git_repo, "git_dep", "main", |dep| dep)
            .build();

        // redact tempdir
        let manifest = graph.read_file("root/Move.toml");
        let path = git_repo.repo_path().to_string_lossy().to_string();

        assert_snapshot!(manifest.replace(&path, "REPO"), @r#"
        [package]
        name = "root"
        edition = "2024"





        [dependencies]
        git_dep = { git = "REPO", subdir = "git_dep", rev = "main" }

        [dep-replacements]
        "#);
    }
}
