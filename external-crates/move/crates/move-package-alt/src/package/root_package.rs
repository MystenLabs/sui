// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, fmt, path::Path};

use super::paths::PackagePath;
use super::{EnvironmentID, manifest::Manifest};
use crate::{
    errors::{FileHandle, PackageError, PackageResult},
    flavor::MoveFlavor,
    graph::PackageGraph,
    package::{EnvironmentName, Package, PackageName},
    schema::{PackageID, ParsedLockfile, Pin},
};
use tracing::debug;

#[cfg(test)]
use crate::dependency::PinnedDependencyInfo;

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

    // TODO: probably remove this?
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
        schema::LockfileDependencyInfo,
        test_utils::{
            self, basic_manifest,
            git::{self},
        },
    };
    use move_core_types::identifier::Identifier;
    use std::{fs, path::PathBuf};

    async fn setup_test_move_project() -> PathBuf {
        let project = test_utils::project()
            .file(
                "packages/pkg_a/Move.toml",
                &basic_manifest("pkg_a", "0.0.1"),
            )
            .file(
                "packages/pkg_b/Move.toml",
                &basic_manifest("pkg_b", "0.0.1"),
            )
            .file(
                "packages/nodeps/Move.toml",
                &basic_manifest("nodeps", "0.0.1"),
            )
            .file(
                "packages/graph/Move.toml",
                &basic_manifest("graph", "0.0.1"),
            )
            .file(
                "packages/depends_a_b/Move.toml",
                &basic_manifest("depends_a_b", "0.0.1"),
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

        project.root()
    }

    #[tokio::test]
    async fn test_load_root_package() {
        let root_path = setup_test_move_project().await;
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
        let root_path = setup_test_move_project().await;

        let pkg_path = root_path.join("packages").join("graph");
        let package = Package::<Vanilla>::load_root(&pkg_path).await.unwrap();
        let deps = package.direct_deps(&"testnet".to_string()).await.unwrap();
        assert!(deps.contains_key(&Identifier::new("nodeps").unwrap()));
        assert!(!deps.contains_key(&Identifier::new("graph").unwrap()));
    }

    #[tokio::test]
    async fn test_direct_dependencies_no_transitive_deps() {
        let root_path = setup_test_move_project().await;

        let pkg_path = root_path.join("packages").join("graph");
        let package = Package::<Vanilla>::load_root(&pkg_path).await.unwrap();
        let deps = package.direct_deps(&"testnet".to_string()).await.unwrap();
        assert!(deps.contains_key(&Identifier::new("nodeps").unwrap()));
        assert!(deps.contains_key(&Identifier::new("depends_a_b").unwrap()));
        // should not contain these transitive deps
        assert!(!deps.contains_key(&Identifier::new("graph").unwrap()));
        assert!(!deps.contains_key(&Identifier::new("pkg_a").unwrap()));
        assert!(!deps.contains_key(&Identifier::new("pkg_b").unwrap()));
    }

    #[tokio::test]
    async fn test_direct_dependencies_no_env_in_manifest() {
        let root_path = setup_test_move_project().await;

        let pkg_path = root_path.join("packages").join("graph");
        let package = Package::<Vanilla>::load_root(&pkg_path).await.unwrap();
        // devnet does not exist in the manifest, should error
        let deps = package.direct_deps(&"devnet".to_string()).await;
        assert!(deps.is_err());
    }

    #[tokio::test]
    async fn test_root_package_operations() {
        let root_path = setup_test_move_project().await;

        // Test loading root package with check for environment existing in manifest
        let pkg_path = root_path.join("packages").join("graph");
        let root = RootPackage::<Vanilla>::load(&pkg_path, Some("testnet".to_string()))
            .await
            .unwrap();

        // Test environment operations
        assert!(root.environments().contains_key("testnet"));
        assert!(root.environments().contains_key("mainnet"));

        assert_eq!(root.package_name(), &Identifier::new("graph").unwrap());
    }

    #[tokio::test]
    async fn test_lockfile_deps() {
        // TODO: this should really be an insta test
        let root_path = setup_test_move_project().await;

        let pkg_path = root_path.join("packages").join("graph");
        let root = RootPackage::<Vanilla>::load(&pkg_path, None).await.unwrap();

        let lockfile_deps = root.dependencies_to_lockfile().await.unwrap();
        let expected = root.load_lockfile().unwrap();

        assert_eq!(expected.render_as_toml(), lockfile_deps.render_as_toml());
    }

    #[tokio::test]
    async fn test_load_and_check_for_env() {
        let root_path = setup_test_move_project().await;

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
        let root_path = setup_test_move_project().await;

        // Test loading non-existent package
        let non_existent_path = root_path.join("non_existent");
        assert!(
            Package::<Vanilla>::load_root(&non_existent_path)
                .await
                .is_err()
        );
    }

    /// This test creates a git repository with a Move package, and another package that depends on
    /// this package as a git dependency. It then tests the following
    /// - direct dependency resolution is correct
    /// - checkout of git dependency at the requested git sha is correct
    /// - updating the git dependency to a different sha works as expected
    /// - updating the git dependency in the manifest and re-pinning works as expected, including
    /// writing back the deps to a lockfile
    #[tokio::test]
    pub async fn test_all() {
        let (pkg_git, pkg_git_repo) = git::new_repo("pkg_git", |project| {
            project.file("Move.toml", &basic_manifest("pkg_git", "0.0.1"))
        });

        pkg_git.change_file("Move.toml", &basic_manifest("pkg_git", "0.0.2"));
        pkg_git_repo.commit();
        pkg_git.change_file("Move.toml", &basic_manifest("pkg_git", "0.0.3"));
        pkg_git_repo.commit();

        let (pkg_dep_on_git, pkg_dep_on_git_repo) = git::new_repo("pkg_dep_on_git", |project| {
            project.file(
                "Move.toml",
                r#"[package]
name = "pkg_dep_on_git"
edition = "2025"
license = "Apache-2.0"
authors = ["Move Team"]
version = "0.0.1"

[dependencies]
pkg_git = { git = "../pkg_git", rev = "main" }

[environments]
mainnet = "35834a8a"
testnet = "4c78adac"
"#,
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
