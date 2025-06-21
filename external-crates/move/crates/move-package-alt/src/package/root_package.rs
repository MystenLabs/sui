// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::BTreeMap,
    fmt::{self, Debug},
    marker::PhantomData,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use super::paths::PackagePath;
use super::{EnvironmentID, lockfile::Lockfiles, manifest::Manifest};
use crate::{
    dependency::{DependencySet, PinnedDependencyInfo, pin},
    errors::{FileHandle, PackageError, PackageResult},
    flavor::MoveFlavor,
    graph::PackageGraph,
    package::{EnvironmentName, Package, PackageName},
    schema::{PackageID, ParsedLockfile, Pin},
};
use move_core_types::identifier::Identifier;
use tracing::{debug, info};

/// A package that is defined as the root of a Move project.
///
/// This is a special package that contains the project manifest and dependencies' graphs,
/// and associated functions to operate with this data.
pub struct RootPackage<F: MoveFlavor + fmt::Debug> {
    /// The root package itself as a Package
    root: Package<F>,
    /// A map from an environment in the manifest to its dependency graph.
    dependencies: BTreeMap<EnvironmentName, PackageGraph<F>>,
}

// TODO: this interface needs to be designed more carefully. In particular, it focuses on a single
// lockfile instead of a bunch. Also, it's not clear whether it represents all the environments,
// one environment, or some set of environments
impl<F: MoveFlavor + fmt::Debug> RootPackage<F> {
    /// Loads the root package from path and builds a dependency graph from the manifest. If `env`
    /// is passed, it will check that this environment exists in the manifest, and will only load
    /// the dependencies for that environment.
    // TODO: maybe we want to check multiple envs
    // TODO: load should probably use PackageGraph::load and have the same behavior?
    pub async fn load(path: impl AsRef<Path>, env: Option<EnvironmentName>) -> PackageResult<Self> {
        let package_path = PackagePath::new(path.as_ref().to_path_buf())?;
        let root = Package::<F>::load_root(package_path.path()).await?;
        let dependencies = if let Some(env) = env {
            if root.manifest().environments().get(&env).is_none() {
                return Err(PackageError::Generic(format!(
                    "Package {} does not have `{env}` defined as an environment in its manifest",
                    root.name(),
                )));
            }
            BTreeMap::from([(
                env.clone(),
                PackageGraph::<F>::load_from_manifest_by_env(&package_path, &env).await?,
            )])
        } else {
            PackageGraph::load_from_manifests(&package_path).await?
        };

        Ok(Self { root, dependencies })
    }

    /// Only load the root manifest and ignore any dependencies. The `dependencies` field will be
    /// empty.
    pub async fn load_manifest(
        path: impl AsRef<Path>,
        env: Option<EnvironmentName>,
    ) -> PackageResult<Self> {
        let package_path = PackagePath::new(path.as_ref().to_path_buf())?;
        let root = Package::<F>::load_root(package_path.path()).await?;

        if let Some(env) = env {
            if root.manifest().environments().get(&env).is_none() {
                return Err(PackageError::Generic(format!(
                    "Package {} does not have `{env}` defined as an environment in its manifest",
                    root.name(),
                )));
            }
        }

        Ok(Self {
            root,
            dependencies: BTreeMap::new(),
        })
    }

    /// Load the root package and check if the lockfile is up-to-date. If it is not, then
    /// all dependencies will be re-pinned.
    pub async fn load_and_repin(path: impl AsRef<Path>) -> PackageResult<Self> {
        let root = Package::<F>::load_root(path).await?;
        let dependencies = PackageGraph::<F>::load(root.path()).await?;

        Ok(Self { root, dependencies })
    }

    /// Read the lockfile from the root directory, returning an empty structure if none exists
    pub fn load_lockfile(&self) -> PackageResult<ParsedLockfile<F>> {
        let path = self.package_path().lockfile_path();
        debug!("loading lockfile {:?}", path);

        if !path.exists() {
            return Ok(ParsedLockfile::<F>::default());
        }

        let file = FileHandle::new(self.package_path().lockfile_path())?;
        Ok(toml_edit::de::from_str(file.source())?)
    }

    /// The package's manifest
    pub fn manifest(&self) -> &Manifest<F> {
        self.root.manifest()
    }

    /// The package's defined environments
    pub fn environments(&self) -> BTreeMap<EnvironmentName, EnvironmentID> {
        self.manifest().environments()
    }

    /// Return the defined package name in the manifest
    pub fn package_name(&self) -> &PackageName {
        self.manifest().package_name()
    }

    // *** DEPENDENCIES RELATED FUNCTIONS ***

    pub fn dependencies(&self) -> &BTreeMap<EnvironmentName, PackageGraph<F>> {
        &self.dependencies
    }

    /// Create a [`Lockfile`] with the current package's dependencies. The lockfile will have no
    /// published information.
    pub async fn dependencies_to_lockfile(&self) -> PackageResult<ParsedLockfile<F>> {
        let pinned: BTreeMap<EnvironmentName, BTreeMap<PackageID, Pin>> = self
            .dependencies()
            .iter()
            .map(|(env, graph)| (env.clone(), graph.into()))
            .collect();

        Ok(ParsedLockfile {
            pinned,
            published: BTreeMap::new(),
        })
    }

    /// Repin dependencies for the given environments and write back to lockfile.
    ///
    /// Note that this will not update the [`dependencies`] field itself.
    pub async fn update_deps_and_write_to_lockfile(
        &self,
        envs: &BTreeMap<EnvironmentName, F::EnvironmentID>,
    ) -> PackageResult<()> {
        let mut lockfile = self.load_lockfile()?;

        for env in envs.keys() {
            let graph =
                PackageGraph::<F>::load_from_manifest_by_env(self.package_path(), env).await?;
            let pinned_deps: BTreeMap<PackageID, Pin> = (&graph).into();
            lockfile.pinned.insert(env.clone(), pinned_deps);
        }

        debug!("writing lockfile {:?}", self.package_path().lockfile_path());
        std::fs::write(
            self.package_path().lockfile_path(),
            lockfile.render_as_toml(),
        );

        Ok(())
    }

    #[cfg(test)]
    pub async fn direct_dependencies(
        &self,
    ) -> PackageResult<BTreeMap<PackageName, PinnedDependencyInfo>> {
        let mut output = BTreeMap::new();
        for env in self.environments().keys() {
            output.extend(self.root.direct_deps(env).await?);
        }

        Ok(output)
    }

    // *** PATHS RELATED FUNCTIONS ***

    /// Return the package path wrapper
    pub fn package_path(&self) -> &PackagePath {
        self.root.path()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        flavor::Vanilla,
        git::{GitCache, GitResult, GitTree, run_git_cmd_with_args},
        schema::LockfileDependencyInfo,
    };
    use std::{fs, process::Output};
    use tempfile::{TempDir, tempdir};
    use test_log::test;
    use tokio::process::Command;

    async fn setup_test_move_project() -> (TempDir, PathBuf) {
        // Create a temporary directory
        let temp_dir = tempfile::tempdir().unwrap();
        let root_path = temp_dir.path().to_path_buf();

        // Create the root directory for the Move project
        fs::create_dir_all(&root_path).unwrap();

        let packages = ["pkg_a", "pkg_b", "nodeps", "graph", "depends_a_b"];

        let pkgs_paths = packages
            .iter()
            .map(|p| root_path.join("packages").join(p))
            .collect::<Vec<_>>();

        for idx in 0..packages.len() {
            let name = packages[idx];
            let path = pkgs_paths[idx].clone();
            fs::create_dir_all(&path).unwrap();
            fs::copy(
                format!("tests/data/basic_move_project/{name}/Move.toml"),
                path.join("Move.toml"),
            )
            .unwrap();

            if name == "graph" {
                fs::copy(
                    format!("tests/data/basic_move_project/{name}/Move.lock"),
                    path.join("Move.lock"),
                )
                .unwrap();
            }
        }

        (temp_dir, root_path)
    }

    #[tokio::test]
    async fn test_load_root_package() {
        let (temp_dir, root_path) = setup_test_move_project().await;
        let names = &["pkg_a", "pkg_b", "nodeps", "graph"];

        for name in names {
            let pkg_path = root_path.join("packages").join(name);
            let package = Package::<Vanilla>::load_root(&pkg_path).await.unwrap();
            assert_eq!(
                &&package.name().to_string(),
                name,
                "Failed to load package: {name}"
            );
        }
    }

    #[tokio::test]
    async fn test_direct_dependencies() {
        let (temp_dir, root_path) = setup_test_move_project().await;

        let pkg_path = root_path.join("packages").join("graph");
        let package = Package::<Vanilla>::load_root(&pkg_path).await.unwrap();
        let deps = package.direct_deps(&"testnet".to_string()).await.unwrap();
        assert!(deps.contains_key(&Identifier::new("nodeps").unwrap()));
        assert!(!deps.contains_key(&Identifier::new("graph").unwrap()));
    }

    #[tokio::test]
    async fn test_direct_dependencies_no_transitive_deps() {
        let (temp_dir, root_path) = setup_test_move_project().await;

        let pkg_path = root_path.join("packages").join("graph");
        let package = Package::<Vanilla>::load_root(&pkg_path).await.unwrap();
        let deps = package.direct_deps(&"testnet".to_string()).await.unwrap();
        assert!(deps.contains_key(&Identifier::new("nodeps").unwrap()));
        assert!(deps.contains_key(&Identifier::new("depends_a_b").unwrap()));
        assert!(!deps.contains_key(&Identifier::new("graph").unwrap()));
        assert!(!deps.contains_key(&Identifier::new("pkg_a").unwrap()));
        assert!(!deps.contains_key(&Identifier::new("pkg_b").unwrap()));
    }

    #[tokio::test]
    async fn test_direct_dependencies_no_env_in_manifest() {
        let (temp_dir, root_path) = setup_test_move_project().await;

        let pkg_path = root_path.join("packages").join("graph");
        let package = Package::<Vanilla>::load_root(&pkg_path).await.unwrap();
        // devnet does not exist in the manifest, should error
        let deps = package.direct_deps(&"devnet".to_string()).await;
        assert!(deps.is_err());
    }

    #[tokio::test]
    async fn test_root_package_operations() {
        let (temp_dir, root_path) = setup_test_move_project().await;

        // Test loading root package with check for environment existing in manifest
        let pkg_path = root_path.join("packages").join("graph");
        let root = RootPackage::<Vanilla>::load(&pkg_path, Some("testnet".to_string()))
            .await
            .unwrap();

        // Test environment operations
        assert!(root.environments().contains_key("testnet"));
        assert!(root.environments().contains_key("mainnet"));

        // Test dependencies operations
        let deps = root.direct_dependencies().await.unwrap();
        assert!(!deps.is_empty());

        assert_eq!(root.package_name(), &Identifier::new("graph").unwrap());
    }

    #[tokio::test]
    async fn test_lockfile_deps() {
        // TODO: this should really be an insta test
        let (temp_dir, root_path) = setup_test_move_project().await;

        let pkg_path = root_path.join("packages").join("graph");
        let root = RootPackage::<Vanilla>::load(&pkg_path, None).await.unwrap();

        let lockfile_deps = root.dependencies_to_lockfile().await.unwrap();
        let expected = root.load_lockfile().unwrap();

        assert_eq!(expected.render_as_toml(), lockfile_deps.render_as_toml());
    }

    #[tokio::test]
    async fn test_load_and_check_for_env() {
        let (temp_dir, root_path) = setup_test_move_project().await;

        let path = root_path.join("graph");
        // should fail as devnet does not exist in the manifest
        assert!(
            RootPackage::<Vanilla>::load(&path, Some("devnet".to_string()))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn test_load_non_existent_package() {
        let (temp_dir, root_path) = setup_test_move_project().await;

        // Test loading non-existent package
        let non_existent_path = root_path.join("non_existent");
        assert!(
            Package::<Vanilla>::load_root(&non_existent_path)
                .await
                .is_err()
        );
    }

    /// Sets up a test Move project with git repository
    /// It returns the temporary directory, the root path of the project, and the commits' sha
    pub async fn run_git_cmd(args: &[&str], repo_path: &PathBuf) -> GitResult<String> {
        run_git_cmd_with_args(args, Some(repo_path)).await
    }

    pub async fn setup_test_move_git_repo() -> (TempDir, PathBuf, Vec<String>) {
        // Create a temporary directory
        let temp_dir = tempdir().unwrap();
        let root_path = temp_dir.path().to_path_buf();

        debug!("=== setting up test repo ===");

        // Create the root directory for the Move project
        fs::create_dir_all(&root_path).unwrap();

        let pkg_path = root_path.join("packages").join("pkg_dep_on_git");
        fs::create_dir_all(&pkg_path).unwrap();
        fs::copy(
            "tests/data/basic_move_project/pkg_dep_on_git/Move.toml",
            pkg_path.join("Move.toml"),
        )
        .unwrap();

        // Create directory structure
        let pkg_path = root_path.join("packages").join("pkg_git");
        fs::create_dir_all(&pkg_path).unwrap();

        // Initialize git repository with main as default branch
        run_git_cmd(&["init", "--initial-branch=main"], &pkg_path).await;

        fs::copy(
            "tests/data/basic_move_project/config",
            pkg_path.join(".git").join("config"),
        )
        .unwrap();

        fs::copy(
            "tests/data/basic_move_project/pkg_git/Move.toml",
            pkg_path.join("Move.toml"),
        )
        .unwrap();

        // Initial commit
        run_git_cmd(&["add", "."], &pkg_path).await;
        run_git_cmd(&["commit", "-m", "Initial commit"], &pkg_path).await;
        run_git_cmd(&["tag", "-a", "v0.0.1", "-m", "Initial version"], &pkg_path).await;

        // Modify pkg_git and commit
        fs::copy(
            "tests/data/basic_move_project/pkg_git/Move.toml.new",
            pkg_path.join("Move.toml"),
        )
        .unwrap();

        let cmd = Command::new("cat")
            .arg(pkg_path.join("Move.toml"))
            .output()
            .await
            .unwrap();

        // Commit updates
        run_git_cmd(&["add", "."], &pkg_path).await;
        run_git_cmd(&["commit", "-m", "Second commit"], &pkg_path).await;
        run_git_cmd(&["tag", "-a", "v0.0.2", "-m", "Second version"], &pkg_path).await;

        // Modify pkg_git and commit
        fs::copy(
            "tests/data/basic_move_project/pkg_git/Move.toml.new2",
            pkg_path.join("Move.toml"),
        )
        .unwrap();

        // Commit updates
        run_git_cmd(&["add", "."], &pkg_path).await;
        run_git_cmd(&["commit", "-m", "Third commit"], &pkg_path).await;
        run_git_cmd(&["tag", "-a", "v0.0.3", "-m", "Third version"], &pkg_path).await;

        // Get commits SHA
        let commits = run_git_cmd(&["log", "--pretty=format:%H"], &pkg_path)
            .await
            .unwrap();
        let commits: Vec<_> = commits.lines().map(|x| x.to_string()).collect();

        debug!("=== test repo setup complete ===");

        (temp_dir, root_path, commits)
    }

    #[test(tokio::test)]
    async fn test_all() {
        let (temp_dir, root_path, commits) = setup_test_move_git_repo().await;
        let move_dir = temp_dir.path().join(".move");
        // TODO: we need to figure a way to allow fetch to work in non ~/.move folder which would
        // end being the ~/.move folder on the machine, rather than some temp dir.

        let git_repo = root_path.join("packages").join("pkg_git");

        let root_pkg_path = root_path.join("packages").join("pkg_dep_on_git");
        let mut root_pkg_manifest = fs::read_to_string(root_pkg_path.join("Move.toml")).unwrap();

        // we need to replace this relative path with the actual git repository path, because find_sha
        // function does not take a cwd, so this `git ls-remote` would be called from the cwd and not from the
        // repo path.
        root_pkg_manifest =
            root_pkg_manifest.replace("../pkg_git", git_repo.to_path_buf().to_str().unwrap());
        fs::write(root_pkg_path.join("Move.toml"), &root_pkg_manifest).unwrap();

        let root_pkg = RootPackage::<Vanilla>::load(&root_pkg_path, None)
            .await
            .unwrap();

        let direct_deps = root_pkg.direct_dependencies().await.unwrap();
        assert!(direct_deps.contains_key(&Identifier::new("pkg_git").unwrap()));
        let git_dep = direct_deps
            .get(&Identifier::new("pkg_git").unwrap())
            .unwrap();

        match git_dep.clone().into() {
            LockfileDependencyInfo::Git(p) => {
                assert_eq!(&p.rev.to_string(), commits.first().unwrap())
            }
            _ => panic!("Expected a git dependency"),
        }

        // Change to second commit
        root_pkg_manifest = root_pkg_manifest.replace(
            "rev = \"main\"",
            format!("rev = \"{}\"", commits[1]).as_str(),
        );
        fs::write(root_pkg_path.join("Move.toml"), &root_pkg_manifest).unwrap();

        let root_pkg = RootPackage::<Vanilla>::load(&root_pkg_path, None)
            .await
            .unwrap();

        let direct_deps = root_pkg.direct_dependencies().await.unwrap();
        let git_dep = direct_deps
            .get(&Identifier::new("pkg_git").unwrap())
            .unwrap();

        match git_dep.clone().into() {
            LockfileDependencyInfo::Git(p) => assert_eq!(p.rev.to_string(), commits[1]),
            _ => panic!("Expected a git dependency"),
        }

        let lockfile = root_pkg.dependencies_to_lockfile().await.unwrap();
        // Change to first commit in the rev in the manifest
        root_pkg_manifest = root_pkg_manifest.replace(
            format!("rev = \"{}\"", commits[1]).as_str(),
            format!("rev = \"{}\"", commits[0]).as_str(),
        );

        fs::write(root_pkg_path.join("Move.toml"), &root_pkg_manifest).unwrap();

        // check if update deps works as expected
        root_pkg
            .update_deps_and_write_to_lockfile(&root_pkg.environments())
            .await
            .unwrap();

        let updated_lockfile = root_pkg.load_lockfile().unwrap();

        assert_ne!(updated_lockfile.render_as_toml(), lockfile.render_as_toml());

        let updated_lockfile_dep = &updated_lockfile.pinned["mainnet"]["pkg_git"].source;
        match updated_lockfile_dep {
            LockfileDependencyInfo::Git(p) => assert_eq!(p.rev.to_string(), commits[0]),
            x => panic!("Expected a git dependency, but got {:?}", x),
        }
    }
}
