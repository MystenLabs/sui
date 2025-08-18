// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, fmt, path::Path};

use tracing::debug;

use super::paths::PackagePath;
use super::{EnvironmentID, manifest::Manifest};
use crate::graph::PackageInfo;
use crate::schema::{Environment, OriginalID, PackageName, Publication};
use crate::{
    errors::{FileHandle, PackageError, PackageResult},
    flavor::MoveFlavor,
    graph::PackageGraph,
    package::EnvironmentName,
    schema::ParsedLockfile,
};

/// A package that is defined as the root of a Move project.
///
/// This is a special package that contains the project manifest and dependencies' graphs,
/// and associated functions to operate with this data.
///
/// TODO(manos): We should try to hold a lock on the manifest / lockfile when we do operations
/// to avoid race conditions.
#[derive(Debug)]
pub struct RootPackage<F: MoveFlavor + fmt::Debug> {
    /// The path to the root package
    package_path: PackagePath,
    /// The environment we're operating on for this root package.
    environment: Environment,
    /// The dependency graph for this package.
    graph: PackageGraph<F>,
    /// The lockfile we're operating on
    /// Invariant: lockfile.pinned matches graph, except that digests may differ
    lockfile: ParsedLockfile<F>,
    /// The list of published ids for every dependency in the root package
    deps_published_ids: Vec<OriginalID>,
}

/// Root package is the "public" entrypoint for operations with the package management.
/// It's like a facade for all functionality, controlled by this.
impl<F: MoveFlavor + fmt::Debug> RootPackage<F> {
    pub fn environments(
        path: impl AsRef<Path>,
    ) -> PackageResult<BTreeMap<EnvironmentName, EnvironmentID>> {
        let package_path = PackagePath::new(path.as_ref().to_path_buf())?;
        let mut environments = F::default_environments();

        if let Ok(modern_manifest) = Manifest::read_from_file(package_path.manifest_path()) {
            // TODO(manos): Decide on validation (e.g. if modern manifest declares environments differently,
            // we should error?!)
            environments.extend(modern_manifest.environments());
        }

        Ok(environments)
    }

    /// Load the root package from `env` using the "normal" path - we first try to load from the
    /// lockfiles; if the digests don't match then we repin using the manifests. Note that it does
    /// not write to the lockfile; you should call [Self::write_pinned_deps] to save the results.
    pub async fn load(path: impl AsRef<Path>, env: Environment) -> PackageResult<Self> {
        debug!("Loading RootPackage for {:?}", path.as_ref());
        let package_path = PackagePath::new(path.as_ref().to_path_buf())?;
        let graph = PackageGraph::<F>::load(&package_path, &env).await?;

        let mut root_pkg = Self::_validate_and_construct(package_path, env, graph)?;

        root_pkg.update_lockfile_digests();

        Ok(root_pkg)
    }

    /// Loads the root package from path and builds a dependency graph from the manifests.
    /// This forcefully re-pins all dependencies even if the manifest digests match. Note that it
    /// does not write to the lockfile; you should call [Self::save_to_disk] to save the results.
    ///
    /// TODO: We should load from lockfiles instead of manifests for deps.
    pub async fn load_force_repin(path: impl AsRef<Path>, env: Environment) -> PackageResult<Self> {
        let package_path = PackagePath::new(path.as_ref().to_path_buf())?;
        let graph = PackageGraph::<F>::load_from_manifests(&package_path, &env).await?;

        let mut root_pkg = Self::_validate_and_construct(package_path, env, graph)?;
        root_pkg.update_lockfile_digests();

        Ok(root_pkg)
    }

    /// Loads the root lockfile only, ignoring all manifests. Returns an error if the lockfile
    /// doesn't exist of if it doesn't contain a dependency graph for `env`.
    ///
    /// Note that this still fetches all of the dependencies, it just doesn't look at their
    /// manifests.
    pub async fn load_ignore_digests(
        path: impl AsRef<Path>,
        env: Environment,
    ) -> PackageResult<Self> {
        let package_path = PackagePath::new(path.as_ref().to_path_buf())?;

        let Some(graph) =
            PackageGraph::<F>::load_from_lockfile_ignore_digests(&package_path, &env).await?
        else {
            return Err(PackageError::Generic(format!(
                "No lockfile found for environment `{}`",
                env.name()
            )));
        };

        Self::_validate_and_construct(package_path, env, graph)
        // Note: we do not sync the lockfile here because we haven't repinned so we don't want to
        // update the digests
    }

    /// Central validation point for a RootPackage.
    ///
    /// This helps validate:
    /// 1. TODO: Fill this in! (deduplicate nodes etc)
    fn _validate_and_construct(
        package_path: PackagePath,
        env: Environment,
        graph: PackageGraph<F>,
    ) -> PackageResult<Self> {
        let mut lockfile = Self::load_lockfile(&package_path)?;

        // check that there is a consistent linkage
        let _linkage = graph.linkage()?;
        graph.check_rename_from()?;

        let deps_published_ids = _linkage.into_keys().collect();

        Ok(Self {
            package_path,
            environment: env,
            graph,
            lockfile,
            deps_published_ids,
        })
    }

    /// Ensure that the in-memory lockfile digests are consistent with the package graph
    fn update_lockfile_digests(&mut self) {
        self.lockfile
            .pinned
            .insert(self.environment.name().clone(), BTreeMap::from(&self.graph));
    }

    /// The name of the root package
    pub fn name(&self) -> &PackageName {
        self.package_graph().root_package().name()
    }

    /// The path to the root of the package
    pub fn path(&self) -> &PackagePath {
        &self.package_path
    }

    /// Return the list of all packages in the root package's package graph (including itself and all
    /// transitive dependencies). This includes the non-duplicate addresses only.
    pub fn packages(&self) -> PackageResult<Vec<PackageInfo<'_, F>>> {
        self.graph.packages()
    }

    /// Return the linkage table for the root package. This contains an entry for each package that
    /// this package depends on (transitively). Returns an error if any of the packages that this
    /// package depends on is unpublished.
    pub fn linkage(&self) -> PackageResult<BTreeMap<OriginalID, PackageInfo<'_, F>>> {
        todo!()
    }

    /// Output an updated lockfile containg the dependency graph represented by `self`. Note that
    /// if `self` was loaded with [Self::load_ignore_digests], then the digests will not be
    /// changed (since no repinning was performed).
    pub fn save_to_disk(&self) -> PackageResult<()> {
        std::fs::write(
            self.graph.root_package().path().lockfile_path(),
            self.lockfile.render_as_toml(),
        )?;
        Ok(())
    }

    /// Set the publish information, coming in from the compiler & result of `Publish` command.
    pub fn write_publish_data(&mut self, publish_data: Publication<F>) -> PackageResult<()> {
        // Write the publish data.
        self.lockfile
            .published
            .insert(self.environment.name().clone(), publish_data);

        self.save_to_disk()
    }

    /// Read the lockfile from the root directory, returning an empty structure if none exists
    /// TODO(Manos): Do we wanna try to read this when loading, to make sure we can operate on it?
    /// That will avoid doing all the work (to repin / publish etc), and then be unable to operate it.
    fn load_lockfile(package_path: &PackagePath) -> PackageResult<ParsedLockfile<F>> {
        let path = package_path.lockfile_path();
        debug!("loading lockfile {:?}", path);

        if !path.exists() {
            return Ok(ParsedLockfile::<F>::default());
        }

        let file = FileHandle::new(path)?;
        Ok(toml_edit::de::from_str(file.source())?)
    }

    pub fn lockfile_for_testing(&self) -> &ParsedLockfile<F> {
        &self.lockfile
    }

    /// Return the package graph for `env`
    // TODO: what's the right API here?
    pub fn package_graph(&self) -> &PackageGraph<F> {
        &self.graph
    }

    pub fn lockfile(&self) -> &ParsedLockfile<F> {
        &self.lockfile
    }

    /// Return the publication information for this environment.
    pub fn publication(&self, env: EnvironmentName) -> PackageResult<Publication<F>> {
        self.lockfile
            .published
            .get(&env)
            .ok_or_else(|| {
                PackageError::Generic(format!(
                    "Could not find publication info for {} environment in package {}",
                    env,
                    self.name()
                ))
            })
            .cloned()
    }

    // *** PATHS RELATED FUNCTIONS ***

    /// Return the package path wrapper
    pub fn package_path(&self) -> &PackagePath {
        &self.package_path
    }

    /// Return a list of sorted package names
    pub fn sorted_deps(&self) -> Vec<&PackageName> {
        self.package_graph().sorted_deps()
    }

    pub fn deps_published_ids(&self) -> &Vec<OriginalID> {
        &self.deps_published_ids
    }
}

// TODO(all of us!): We need to test everything.
#[cfg(test)]
mod tests {
    use insta::assert_snapshot;
    use std::{fs, path::PathBuf};
    use test_log::test;

    use super::*;
    use crate::{
        flavor::{
            Vanilla,
            vanilla::{DEFAULT_ENV_NAME, default_environment},
        },
        schema::LockfileDependencyInfo,
        test_utils::{
            self, basic_manifest_with_env,
            git::{self},
            graph_builder::TestPackageGraph,
        },
    };

    async fn setup_test_move_project() -> (Environment, PathBuf) {
        let env = crate::flavor::vanilla::default_environment();
        let project = test_utils::project()
            .file(
                "packages/pkg_a/Move.toml",
                &basic_manifest_with_env("pkg_a", "0.0.1", env.name(), env.id()),
            )
            .file(
                "packages/pkg_b/Move.toml",
                &basic_manifest_with_env("pkg_b", "0.0.1", env.name(), env.id()),
            )
            .file(
                "packages/nodeps/Move.toml",
                &basic_manifest_with_env("nodeps", "0.0.1", env.name(), env.id()),
            )
            .file(
                "packages/graph/Move.toml",
                &basic_manifest_with_env("graph", "0.0.1", env.name(), env.id()),
            )
            .file(
                "packages/depends_a_b/Move.toml",
                &basic_manifest_with_env("depends_a_b", "0.0.1", env.name(), env.id()),
            );

        let project = project.build();
        project.extend_file(
            "packages/graph/Move.toml",
            r#"
[dependencies]
nodeps = { local = "../nodeps" }
depends_a_b = { local = "../depends_a_b" }"#,
        );

        project.extend_file(
            "packages/depends_a_b/Move.toml",
            r#"
[dependencies]
pkg_a = { local = "../pkg_a" }
pkg_b = { local = "../pkg_b" }"#,
        );
        fs::copy(
            "tests/data/basic_move_project/graph/Move.lock",
            project.root().join("packages/graph/Move.lock"),
        )
        .unwrap();

        (env, project.root())
    }

    #[test(tokio::test)]
    async fn test_load_root_package() {
        let (env, root_path) = setup_test_move_project().await;
        let names = &["pkg_a", "pkg_b", "nodeps", "graph"];

        for name in names {
            let pkg_path = root_path.join("packages").join(name);
            let package = RootPackage::<Vanilla>::load(&pkg_path, env.clone())
                .await
                .unwrap();
            assert_eq!(
                &&package.name().to_string(),
                name,
                "Failed to load package: {name}"
            );
        }
    }

    #[test(tokio::test)]
    async fn test_root_package_operations() {
        let (env, root_path) = setup_test_move_project().await;

        // Test loading root package with check for environment existing in manifest
        let pkg_path = root_path.join("packages").join("graph");
        let root = RootPackage::<Vanilla>::load(&pkg_path, env).await.unwrap();

        // Test environment operations
        assert!(
            RootPackage::<Vanilla>::environments(pkg_path)
                .unwrap()
                .contains_key(DEFAULT_ENV_NAME)
        );

        assert_eq!(root.name(), &PackageName::new("graph").unwrap());
    }

    #[test(tokio::test)]
    async fn test_lockfile_deps() {
        let (env, root_path) = setup_test_move_project().await;
        let pkg_path = root_path.join("packages").join("graph");

        let mut root = RootPackage::<Vanilla>::load(&pkg_path, env).await.unwrap();

        let new_lockfile = root.lockfile().clone();

        // TODO: put this snapshot in a more sensible place
        assert_snapshot!("test_lockfile_deps", new_lockfile.render_as_toml());
    }

    #[test(tokio::test)]
    async fn test_load_and_check_for_env() {
        let (env, root_path) = setup_test_move_project().await;

        let path = root_path.join("graph");
        // should fail as devnet does not exist in the manifest
        assert!(
            RootPackage::<Vanilla>::load(
                &path,
                Environment::new("devnet".to_string(), "abcd1234".to_string())
            )
            .await
            .is_err()
        );
    }

    /// This just ensures that `RootPackage` does the `rename-from` validation; see
    /// [crate::graph::rename_from::tests] for more detailed tests that operate directly on the
    /// package graph
    #[test(tokio::test)]
    async fn test_rename_from() {
        // `a` depends on `b` which has name `b_name`, but there is no rename-from
        // building the root package should fail because of rename-from validation
        let scenario = TestPackageGraph::new(["a"])
            .add_package("b", |b| b.package_name("b_name"))
            .add_deps([("a", "b")])
            .build();

        RootPackage::<Vanilla>::load(scenario.path_for("a"), default_environment())
            .await
            .unwrap_err();
    }

    /// This test creates a git repository with a Move package, and another package that depends on
    /// this package as a git dependency. It then tests the following
    /// - checkout of git dependency at the requested git sha is correct
    /// - updating the git dependency to a different sha works as expected
    /// - updating the git dependency in the manifest and re-pinning works as expected, including
    /// writing back the deps to a lockfile
    #[test(tokio::test)]
    pub async fn test_all() {
        debug!("running test_all");
        let env = crate::flavor::vanilla::default_environment();
        let (pkg_git, pkg_git_repo) = git::new_repo("pkg_git", |project| {
            project.file(
                "Move.toml",
                (&basic_manifest_with_env("pkg_git", "0.0.1", env.name(), env.id())),
            )
        });

        pkg_git.change_file(
            "Move.toml",
            (&basic_manifest_with_env("pkg_git", "0.0.2", env.name(), env.id())),
        );
        pkg_git_repo.commit();
        pkg_git.change_file(
            "Move.toml",
            &basic_manifest_with_env("pkg_git", "0.0.3", env.name(), env.id()),
        );
        pkg_git_repo.commit();

        let (pkg_dep_on_git, pkg_dep_on_git_repo) = git::new_repo("pkg_dep_on_git", |project| {
            project.file(
                "Move.toml",
                &format!(
                    r#"[package]
name = "pkg_dep_on_git"
edition = "2025"
license = "Apache-2.0"
authors = ["Move Team"]
version = "0.0.1"

[dependencies]
pkg_git = {{ git = "../pkg_git", rev = "main" }}

[environments]
{} = "{}"
"#,
                    env.name(),
                    env.id(),
                ),
            )
        });

        let root_pkg_path = pkg_dep_on_git.root();
        let commits = pkg_git.commits();
        let mut root_pkg_manifest = fs::read_to_string(root_pkg_path.join("Move.toml")).unwrap();

        // we need to replace this relative path with the actual git repository path, because find_sha
        // function does not take a cwd, so this `git ls-remote` would be called from the cwd and not from the
        // repo path.
        root_pkg_manifest = root_pkg_manifest.replace("../pkg_git", pkg_git.root_path_str());
        fs::write(root_pkg_path.join("Move.toml"), &root_pkg_manifest).unwrap();

        let root_pkg = RootPackage::<Vanilla>::load(&root_pkg_path, env.clone())
            .await
            .unwrap();

        let pinned_deps = root_pkg.lockfile.pinned.get(env.name()).unwrap();
        debug!("pinned_deps: {pinned_deps:#?}");
        let git_dep = pinned_deps.get("pkg_git").unwrap();

        match &git_dep.source {
            LockfileDependencyInfo::Git(p) => {
                assert_eq!(&p.rev.to_string(), commits.first().unwrap())
            }
            _ => panic!("Expected a git dependency"),
        }

        // Change ts second commit
        root_pkg_manifest = root_pkg_manifest.replace(
            "rev = \"main\"",
            format!("rev = \"{}\"", commits[1]).as_str(),
        );
        fs::write(root_pkg_path.join("Move.toml"), &root_pkg_manifest).unwrap();

        let root_pkg = RootPackage::<Vanilla>::load(&root_pkg_path, env.clone())
            .await
            .unwrap();

        let pinned_deps = root_pkg.lockfile.pinned.get(env.name()).unwrap();
        let git_dep = pinned_deps.get("pkg_git").unwrap();

        match &git_dep.source {
            LockfileDependencyInfo::Git(p) => {
                assert_eq!(p.rev.to_string(), commits[1])
            }
            _ => panic!("Expected a git dependency"),
        }

        root_pkg.save_to_disk().unwrap();
        let lockfile = root_pkg.lockfile;
        // Change to first commit in the rev in the manifest
        root_pkg_manifest = root_pkg_manifest.replace(
            format!("rev = \"{}\"", commits[1]).as_str(),
            format!("rev = \"{}\"", commits[0]).as_str(),
        );

        fs::write(root_pkg_path.join("Move.toml"), &root_pkg_manifest).unwrap();

        // check if update deps works as expected
        let root_pkg = RootPackage::<Vanilla>::load_force_repin(&root_pkg_path, env)
            .await
            .unwrap();

        let updated_lockfile = root_pkg.lockfile;

        assert_ne!(updated_lockfile.render_as_toml(), lockfile.render_as_toml());

        let updated_lockfile_dep = &updated_lockfile.pinned[DEFAULT_ENV_NAME]["pkg_git"].source;
        match updated_lockfile_dep {
            LockfileDependencyInfo::Git(p) => assert_eq!(p.rev.to_string(), commits[0]),
            x => panic!("Expected a git dependency, but got {:?}", x),
        }
    }
}
