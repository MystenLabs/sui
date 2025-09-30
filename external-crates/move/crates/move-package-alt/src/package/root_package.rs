// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use std::{collections::BTreeMap, fmt, path::Path};

use tracing::debug;

use super::paths::PackagePath;
use super::{EnvironmentID, manifest::Manifest};
use crate::compatibility::legacy_lockfile::convert_legacy_lockfile;
use crate::graph::{LinkageTable, PackageInfo};
use crate::package::package_lock::PackageLock;
use crate::schema::{
    Environment, OriginalID, PackageID, PackageName, ParsedEphemeralPubs, ParsedPublishedFile,
    Publication, RenderToml,
};
use crate::{
    errors::{FileHandle, PackageError, PackageResult},
    flavor::MoveFlavor,
    graph::PackageGraph,
    package::EnvironmentName,
    schema::ParsedLockfile,
};

/// We store the publication file that we read so that we can update it later in
/// [RootPackage::write_publish_data]
#[derive(Debug)]
enum PublicationSource<F: MoveFlavor> {
    /// Addresses are stored in the `Published.toml` file and retrieved from dependencies' files
    Published(ParsedPublishedFile<F>),

    /// Addresses are retrieved from and stored to the ephemeral publication file located at `file`
    /// and with contents `pubs`
    Ephemeral {
        file: PathBuf,
        pubs: ParsedEphemeralPubs<F>,
    },
}

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
    lockfile: ParsedLockfile,

    /// The stored publications for the root package
    pubs: PublicationSource<F>,

    /// The list of published ids for every dependency in the root package
    // TODO: the comment says published ids but the type says original id; what is this for?
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

    /// Load the root package for `env` using the "normal" path - we first try to load from the
    /// lockfiles; if the digests don't match then we repin using the manifests. Note that it does
    /// not write to the lockfile; you should call [Self::write_pinned_deps] to save the results.
    pub async fn load(path: impl AsRef<Path>, env: Environment) -> PackageResult<Self> {
        debug!(
            "Loading RootPackage for {:?} (CWD: {:?}",
            path.as_ref(),
            std::env::current_dir()
        );
        let _mutx = PackageLock::lock(); // held until function returns
        let package_path = PackagePath::new(path.as_ref().to_path_buf())?;
        let graph = PackageGraph::<F>::load(&package_path, &env).await?;

        let mut root_pkg = Self::_validate_and_construct(package_path, env, graph)?;

        root_pkg.update_lockfile_digests();

        Ok(root_pkg)
    }

    /// Load the root package from `root` in environment `build_env`, but replace all the addresses
    /// with the addresses in `pubfile`. Saving publication data will also save to the output to
    /// `pubfile` rather than `Published.toml`
    ///
    /// If `pubfile` does not exist, one is created with the provided `chain_id` and `build_env`;
    /// If the file does exist but these fields differ, then an error is returned.
    pub async fn load_ephemeral(
        root: impl AsRef<Path>,
        build_env: Option<EnvironmentName>,
        chain_id: EnvironmentID,
        pubfile_path: impl AsRef<Path>,
    ) -> PackageResult<Self> {
        // Load the publication file
        let pubfile =
            Self::load_ephemeral_pubfile(build_env, chain_id.clone(), &pubfile_path).await?;

        // extract the environment
        let build_env_name = pubfile.build_env.clone();

        let build_env_id = Self::environments(&root)?
            .get(&build_env_name)
            .ok_or(PackageError::UnknownBuildEnv {
                build_env: build_env_name.clone(),
            })?
            .clone();

        let build_env = Environment {
            name: build_env_name,
            id: build_env_id,
        };

        // load the package as if in the build_env
        let mut result = Self::load(root, build_env).await?;

        // update the packages to use the ephemeral addresses
        result
            .graph
            .add_publish_overrides(localpubs_to_publications(&pubfile));
        result.pubs = PublicationSource::Ephemeral {
            file: pubfile_path.as_ref().to_path_buf(),
            pubs: pubfile,
        };

        Ok(result)
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

        convert_legacy_lockfile::<F>(&package_path)?;

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
        debug!(
            "creating RootPackage at {:?} (CWD: {:?})",
            package_path.path(),
            std::env::current_dir()
        );
        let lockfile = Self::load_lockfile(&package_path)?;
        let pubs = Self::load_pubfile(&package_path)?;

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
            pubs: PublicationSource::Published(pubs),
        })
    }

    /// Ensure that the in-memory lockfile digests are consistent with the package graph
    fn update_lockfile_digests(&mut self) {
        self.lockfile
            .pinned
            .insert(self.environment.name().clone(), BTreeMap::from(&self.graph));
    }

    /// The id of the root package (TODO: perhaps this method is poorly named; check where it's
    /// used and decide if they should be using `id` or `display_name`)
    pub fn name(&self) -> &PackageID {
        self.package_graph().root_package_info().id()
    }

    /// Returns the `display_name` for the root package.
    /// Invariant: For modern packages, this is always equal to `name().as_str()`
    pub fn display_name(&self) -> &str {
        self.package_graph().root_package().display_name()
    }

    /// The path to the root of the package
    pub fn path(&self) -> &PackagePath {
        &self.package_path
    }

    /// Return the list of all packages in the root package's package graph (including itself and all
    /// transitive dependencies). This includes the non-duplicate addresses only.
    pub fn packages(&self) -> PackageResult<Vec<PackageInfo<F>>> {
        self.graph.packages()
    }

    /// Return the linkage table for the root package. This contains an entry for each package that
    /// this package depends on (transitively). Returns an error if any of the packages that this
    /// package depends on is unpublished.
    pub fn linkage(&self) -> PackageResult<LinkageTable<F>> {
        Ok(self.graph.linkage()?)
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

    /// Record metadata for a publication for the root package in either its `Published.toml` or
    /// its ephemeral pubfile (depending on how it was loaded)
    pub fn write_publish_data(&mut self, publish_data: Publication<F>) -> PackageResult<()> {
        let package_id = self.name().to_string();

        match &mut self.pubs {
            PublicationSource::Published(pubfile) => {
                pubfile
                    .published
                    .insert(self.environment.name().clone(), publish_data);
                std::fs::write(&self.package_path, pubfile.render_as_toml())?;
            }
            PublicationSource::Ephemeral { file, pubs } => {
                pubs.published.insert(package_id, publish_data.into());
                std::fs::write(&file, pubs.render_as_toml())?;
            }
        }

        Ok(())
    }

    /// Read the lockfile from the root directory, returning an empty structure if none exists
    fn load_lockfile(package_path: &PackagePath) -> PackageResult<ParsedLockfile> {
        convert_legacy_lockfile::<F>(package_path)?;

        let path = package_path.lockfile_path();
        debug!("loading lockfile {:?}", path);

        if !path.exists() {
            return Ok(ParsedLockfile::default());
        }

        let file = FileHandle::new(path)?;
        Ok(toml_edit::de::from_str(file.source())?)
    }

    /// Read the pubfile from the root directory, returning an empty structure if none exists
    fn load_pubfile(path: &PackagePath) -> PackageResult<ParsedPublishedFile<F>> {
        let path = path.publications_path();

        if !path.exists() {
            return Ok(ParsedPublishedFile::default());
        }

        let file = FileHandle::new(path)?;
        Ok(toml_edit::de::from_str(file.source())?)
    }

    /// Load ephemeral publications from `pubfile`, checking that they have the correct `chain-id`
    /// and `build-env`. If the file does not exist, a new file is created and returned
    async fn load_ephemeral_pubfile(
        build_env: Option<EnvironmentName>,
        chain_id: EnvironmentID,
        pubfile: impl AsRef<Path>,
    ) -> PackageResult<ParsedEphemeralPubs<F>> {
        if let Ok(file) = FileHandle::new(&pubfile) {
            let parsed: ParsedEphemeralPubs<F> = toml_edit::de::from_str(file.source())?;
            if let Some(build_env) = build_env {
                if build_env != parsed.build_env {
                    return Err(PackageError::EphemeralEnvMismatch {
                        file_build_env: parsed.build_env,
                        passed_build_env: build_env,
                    });
                }
            }
            if chain_id != parsed.chain_id {
                return Err(PackageError::EphemeralChainMismatch {
                    file_chain_id: parsed.chain_id,
                    passed_chain_id: chain_id,
                });
            }

            Ok(parsed)
        } else {
            let file = pubfile.as_ref().to_path_buf();
            let Some(build_env) = build_env else {
                return Err(PackageError::EphemeralNoBuildEnv);
            };

            let pubs = ParsedEphemeralPubs {
                build_env,
                chain_id,
                published: BTreeMap::new(),
            };
            debug!("writing empty file {file:?}");
            std::fs::write(&file, pubs.render_as_toml())?;

            Ok(pubs)
        }
    }

    pub fn lockfile_for_testing(&self) -> &ParsedLockfile {
        &self.lockfile
    }

    /// Return the package graph for `env`
    // TODO: what's the right API here?
    pub fn package_graph(&self) -> &PackageGraph<F> {
        &self.graph
    }

    pub fn lockfile(&self) -> &ParsedLockfile {
        &self.lockfile
    }

    /// Return the publication information for the root package in the current environment
    pub fn publication(&self) -> Option<&Publication<F>> {
        self.graph.root_package().publication()
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

    // TODO: what is the spec of this function?
    pub fn deps_published_ids(&self) -> &Vec<OriginalID> {
        &self.deps_published_ids
    }
}

fn localpubs_to_publications<F: MoveFlavor>(
    pubfile: &ParsedEphemeralPubs<F>,
) -> BTreeMap<PackageID, Publication<F>> {
    pubfile
        .published
        .iter()
        .map(|(id, local_pub)| {
            (
                id.clone(),
                Publication::<F> {
                    chain_id: pubfile.chain_id.clone(),
                    addresses: local_pub.addresses.clone(),
                    version: local_pub.version,
                    metadata: local_pub.metadata.clone(),
                },
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use indoc::formatdoc;
    use insta::assert_snapshot;
    use std::{fs, io::Write, path::PathBuf};
    use test_log::test;

    use super::*;
    use crate::{
        flavor::{
            Vanilla,
            vanilla::{self, DEFAULT_ENV_NAME, default_environment},
        },
        graph::NamedAddress,
        schema::{LockfileDependencyInfo, PackageID, PublishAddresses, PublishedID},
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

        assert_eq!(root.name(), &PackageID::from("graph"));
    }

    #[test(tokio::test)]
    async fn test_lockfile_deps() {
        let (env, root_path) = setup_test_move_project().await;
        let pkg_path = root_path.join("packages").join("graph");

        let root = RootPackage::<Vanilla>::load(&pkg_path, env).await.unwrap();

        let new_lockfile = root.lockfile().clone();

        // TODO: put this snapshot in a more sensible place
        assert_snapshot!("test_lockfile_deps", new_lockfile.render_as_toml());
    }

    #[test(tokio::test)]
    async fn test_load_and_check_for_env() {
        let (_, root_path) = setup_test_move_project().await;

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
        let pkg_git = git::new("pkg_git", |project| {
            project.file(
                "Move.toml",
                &basic_manifest_with_env("pkg_git", "0.0.1", env.name(), env.id()),
            )
        })
        .await;

        pkg_git.as_ref().change_file(
            "Move.toml",
            &basic_manifest_with_env("pkg_git", "0.0.2", env.name(), env.id()),
        );
        pkg_git.commit().await;
        pkg_git.as_ref().change_file(
            "Move.toml",
            &basic_manifest_with_env("pkg_git", "0.0.3", env.name(), env.id()),
        );
        pkg_git.commit().await;

        let pkg_dep = git::new("pkg_dep_on_git", |project| {
            project.file(
                "Move.toml",
                &formatdoc!(
                    r#"
                    [package]
                    name = "pkg_dep_on_git"
                    edition = "2024"
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
        })
        .await;

        let root_pkg_path = pkg_dep.as_ref().root();
        let commits = pkg_git.commits().await;
        let mut root_pkg_manifest = fs::read_to_string(root_pkg_path.join("Move.toml")).unwrap();

        // we need to replace this relative path with the actual git repository path, because find_sha
        // function does not take a cwd, so this `git ls-remote` would be called from the cwd and not from the
        // repo path.
        root_pkg_manifest =
            root_pkg_manifest.replace("../pkg_git", pkg_git.as_ref().root_path_str());
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

    ////////////////////////////////////////////////////////////////////////////////////////////////
    // Ephemeral loading and storing ///////////////////////////////////////////////////////////////
    ////////////////////////////////////////////////////////////////////////////////////////////////

    /// Loading an ephemeral root package with root in the ephemeral file outputs `RootPackage` for
    /// the root address (with the ephemeral original ID)
    #[test(tokio::test)]
    async fn ephemeral_root() {
        let scenario = TestPackageGraph::new(["dummy"])
            .add_published("root", OriginalID::from(1), PublishedID::from(1))
            .build();

        let mut ephemeral = tempfile::NamedTempFile::new().unwrap();
        write!(
            ephemeral,
            r###"
            chain-id = "localnet"
            build-env = "{DEFAULT_ENV_NAME}"

            [published.root]
            original-id = "0x2"
            published-at = "0x3"
            version = 0
            "###,
        )
        .unwrap();

        // load root package with ephemeral file
        let root = RootPackage::<Vanilla>::load_ephemeral(
            scenario.path_for("root"),
            None,
            "localnet".into(),
            ephemeral.path(),
        )
        .await
        .unwrap();

        // check the root package's named address
        let root_addr = root
            .package_graph()
            .root_package_info()
            .named_addresses()
            .unwrap()
            .into_iter()
            .next()
            .unwrap()
            .1;

        assert_eq!(root_addr, NamedAddress::RootPackage(Some(2.into())));
    }

    /// Ephemerally loading a dependency that is both published and in the ephemeral file produces
    /// the ephemeral address
    #[test(tokio::test)]
    async fn ephemeral_pub_and_eph() {
        let scenario = TestPackageGraph::new(["root"])
            .add_published("dep", OriginalID::from(1), PublishedID::from(1))
            .add_deps([("root", "dep")])
            .build();

        let mut ephemeral = tempfile::NamedTempFile::new().unwrap();
        write!(
            ephemeral,
            r###"
            chain-id = "localnet"
            build-env = "{DEFAULT_ENV_NAME}"

            [published.dep]
            original-id = "0x2"
            published-at = "0x3"
            version = 0
            "###,
        )
        .unwrap();

        // load root package with ephemeral file

        let root = RootPackage::<Vanilla>::load_ephemeral(
            scenario.path_for("root"),
            None,
            "localnet".into(),
            ephemeral.path(),
        )
        .await
        .unwrap();

        // check the dependency's addresses

        let dep_addrs = root
            .package_graph()
            .package_info_by_id(&PackageID::from("dep"))
            .unwrap()
            .published()
            .unwrap()
            .clone();

        assert_eq!(dep_addrs.original_id, OriginalID::from(2));
        assert_eq!(dep_addrs.published_at, PublishedID::from(3));
    }

    /// Ephemerally loading a dep that is published but not in the ephemeral file produces the
    /// original address. Note: it should also warn but this is not tested
    #[test(tokio::test)]
    async fn ephemeral_only_pub() {
        let scenario = TestPackageGraph::new(["root"])
            .add_published("dep", OriginalID::from(1), PublishedID::from(1))
            .add_deps([("root", "dep")])
            .build();

        let mut ephemeral = tempfile::NamedTempFile::new().unwrap();
        write!(
            ephemeral,
            r###"
            chain-id = "localnet"
            build-env = "{DEFAULT_ENV_NAME}"
            "###,
        )
        .unwrap();

        // load root package with ephemeral file

        let root = RootPackage::<Vanilla>::load_ephemeral(
            scenario.path_for("root"),
            None,
            "localnet".into(),
            ephemeral.path(),
        )
        .await
        .unwrap();

        // check the dependency's addresses

        let dep_addrs = root
            .package_graph()
            .package_info_by_id(&PackageID::from("dep"))
            .unwrap()
            .published()
            .unwrap()
            .clone();

        assert_eq!(dep_addrs.original_id, OriginalID::from(1));
        assert_eq!(dep_addrs.published_at, PublishedID::from(1));
    }

    /// Ephemerally loading a dep that is not published but is in the ephemeral file produces the
    /// ephemeral address.
    #[test(tokio::test)]
    async fn ephemeral_only_eph() {
        let scenario = TestPackageGraph::new(["root", "dep"])
            .add_deps([("root", "dep")])
            .build();

        let mut ephemeral = tempfile::NamedTempFile::new().unwrap();
        write!(
            ephemeral,
            r###"
            chain-id = "localnet"
            build-env = "{DEFAULT_ENV_NAME}"

            [published.dep]
            original-id = "0x2"
            published-at = "0x3"
            version = 0
            "###,
        )
        .unwrap();

        // load root package with ephemeral file

        let root = RootPackage::<Vanilla>::load_ephemeral(
            scenario.path_for("root"),
            None,
            "localnet".into(),
            ephemeral.path(),
        )
        .await
        .unwrap();

        // check the dependency's addresses

        let dep_addrs = root
            .package_graph()
            .package_info_by_id(&PackageID::from("dep"))
            .unwrap()
            .published()
            .unwrap()
            .clone();

        assert_eq!(dep_addrs.original_id, OriginalID::from(2));
        assert_eq!(dep_addrs.published_at, PublishedID::from(3));
    }

    /// Ephemerally loading a dep that is neither published nor in the ephemeral file produces an
    /// unpublished package
    #[test(tokio::test)]
    async fn ephemeral_unpublished() {
        let scenario = TestPackageGraph::new(["root", "dep"])
            .add_deps([("root", "dep")])
            .build();

        let mut ephemeral = tempfile::NamedTempFile::new().unwrap();
        write!(
            ephemeral,
            r###"
            chain-id = "localnet"
            build-env = "{DEFAULT_ENV_NAME}"
            "###,
        )
        .unwrap();

        // load root package with ephemeral file

        let root = RootPackage::<Vanilla>::load_ephemeral(
            scenario.path_for("root"),
            None,
            "localnet".into(),
            ephemeral.path(),
        )
        .await
        .unwrap();

        // check the dependency's addresses
        assert!(
            root.package_graph()
                .package_info_by_id(&PackageID::from("dep"))
                .unwrap()
                .published()
                .is_none()
        );
    }

    /// If two dep addresses differ in the build environment but match in the ephemeral
    /// environment, loading still succeeds.
    #[test(tokio::test)]
    async fn ephemeral_adds_equality() {
        let scenario = TestPackageGraph::new(["root"])
            .add_published("dep1", OriginalID::from(1), PublishedID::from(1))
            .add_published("dep2", OriginalID::from(2), PublishedID::from(2))
            .add_deps([("root", "dep1"), ("root", "dep2")])
            .build();

        let mut ephemeral = tempfile::NamedTempFile::new().unwrap();
        write!(
            ephemeral,
            r###"
            chain-id = "localnet"
            build-env = "{DEFAULT_ENV_NAME}"

            [published.dep1]
            original-id = "0x4"
            published-at = "0x5"
            version = 0

            [published.dep2]
            original-id = "0x4"
            published-at = "0x6"
            version = 0
            "###,
        )
        .unwrap();

        RootPackage::<Vanilla>::load_ephemeral(
            scenario.path_for("root"),
            None,
            "localnet".into(),
            ephemeral.path(),
        )
        .await
        .unwrap();
    }

    /// If two dep addresses match in the build environment but differ in the ephemeral
    /// environment, there is an error.
    #[test(tokio::test)]
    async fn ephemeral_drops_equality() {
        let scenario = TestPackageGraph::new(["root"])
            .add_published("dep1", OriginalID::from(1), PublishedID::from(1))
            .add_published("dep2", OriginalID::from(1), PublishedID::from(2))
            .add_deps([("root", "dep1"), ("root", "dep2")])
            .build();

        let mut ephemeral = tempfile::NamedTempFile::new().unwrap();
        write!(
            ephemeral,
            r###"
            chain-id = "localnet"
            build-env = "{DEFAULT_ENV_NAME}"

            [published.dep1]
            original-id = "0x2"
            published-at = "0x5"
            version = 0

            [published.dep2]
            original-id = "0x3"
            published-at = "0x6"
            version = 0
            "###,
        )
        .unwrap();

        let root = RootPackage::<Vanilla>::load_ephemeral(
            scenario.path_for("root"),
            None,
            "localnet".into(),
            ephemeral.path(),
        )
        .await;

        assert_snapshot!(root.unwrap_err().to_string(), @r###"
        Package depends on multiple versions of the package with ID 0x00...0001:

          root::dep1 refers to { local = "../dep1" }
          root::dep2 refers to { local = "../dep2" }

        To resolve this, you must explicitly add an override in your Move.toml:

            [dependencies]
            _dep1 = { ..., override = true }
        "###);
    }

    /// Loading an ephemeral root package from a non-existing file succeeds and uses the published
    /// addresses for the build environment
    #[test(tokio::test)]
    async fn ephemeral_empty() {
        let scenario = TestPackageGraph::new(["root"])
            .add_published("dep", OriginalID::from(1), PublishedID::from(2))
            .add_deps([("root", "dep")])
            .build();

        let tempdir = tempfile::tempdir().unwrap();
        let ephemeral = tempdir.path().join("nonexistent.toml");

        // load root package with ephemeral file

        let root = RootPackage::<Vanilla>::load_ephemeral(
            scenario.path_for("root"),
            Some(DEFAULT_ENV_NAME.to_string()),
            "localnet".into(),
            ephemeral.as_path(),
        )
        .await
        .unwrap();

        // check the dependency's addresses
        let dep_addrs = root
            .package_graph()
            .package_info_by_id(&PackageID::from("dep"))
            .unwrap()
            .published()
            .unwrap()
            .clone();

        assert_eq!(dep_addrs.original_id, OriginalID::from(1));
        assert_eq!(dep_addrs.published_at, PublishedID::from(2));
    }

    /// Loading an ephemeral root package and then publishing correctly updates the ephemeral file
    /// (and does not update the normal pubfile)
    #[test(tokio::test)]
    async fn ephemeral_publish() {
        let scenario = TestPackageGraph::new(["root", "dep"])
            .add_deps([("root", "dep")])
            .build();

        let mut ephemeral = tempfile::NamedTempFile::new().unwrap();
        write!(
            ephemeral,
            r###"
            chain-id = "localnet"
            build-env = "{DEFAULT_ENV_NAME}"
            "###,
        )
        .unwrap();

        // load root package with ephemeral file

        let mut root = RootPackage::<Vanilla>::load_ephemeral(
            scenario.path_for("root"),
            None,
            "localnet".into(),
            ephemeral.path(),
        )
        .await
        .unwrap();

        let prepublish_pubfile = std::fs::read_to_string(root.path().publications_path()).unwrap();

        // publish
        root.write_publish_data(Publication {
            version: 0,
            chain_id: "localnet".into(),
            addresses: PublishAddresses {
                original_id: OriginalID::from(1),
                published_at: PublishedID::from(2),
            },
            metadata: vanilla::PublishedMetadata::default(),
        })
        .unwrap();

        // check
        let postpublish_pubfile = std::fs::read_to_string(root.path().publications_path()).unwrap();
        let ephemeral_data = std::fs::read_to_string(ephemeral.path()).unwrap();

        assert_eq!(prepublish_pubfile, postpublish_pubfile);
        assert_snapshot!(ephemeral_data, @r###"
        # generated by Move
        # this file contains metadata from ephemeral publications
        # this file should not be committed to source control

        build-env = "_test_env"
        chain-id = "localnet"

        [published.root]
        published-at = "0x0000000000000000000000000000000000000000000000000000000000000002"
        original-id = "0x0000000000000000000000000000000000000000000000000000000000000001"
        version = 0
        "###);
    }

    /// Loading an ephemeral package with a mismatched `chain-id` fails
    #[test(tokio::test)]
    async fn ephemeral_chain_mismatch() {
        let scenario = TestPackageGraph::new(["root"]).build();

        let mut ephemeral = tempfile::NamedTempFile::new().unwrap();
        write!(
            ephemeral,
            r###"
            chain-id = "not localnet"
            build-env = "{DEFAULT_ENV_NAME}"
            "###,
        )
        .unwrap();

        // load root package with ephemeral file

        let root = RootPackage::<Vanilla>::load_ephemeral(
            scenario.path_for("root"),
            None,
            "localnet".into(),
            ephemeral.path(),
        )
        .await;

        assert_snapshot!(root.unwrap_err().to_string(), @"Ephemeral publication file has chain-id `not localnet`; it cannot be used to publish to chain with id `localnet`");
    }

    /// Loading an ephemeral package with a mismatched `build-env` fails
    #[test(tokio::test)]
    async fn ephemeral_build_env_mismatch() {
        let scenario = TestPackageGraph::new(["root"]).build();

        let mut ephemeral = tempfile::NamedTempFile::new().unwrap();
        write!(
            ephemeral,
            r###"
            chain-id = "localnet"
            build-env = "not {DEFAULT_ENV_NAME}"
            "###,
        )
        .unwrap();

        // load root package with ephemeral file

        let root = RootPackage::<Vanilla>::load_ephemeral(
            scenario.path_for("root"),
            Some(DEFAULT_ENV_NAME.to_string()),
            "localnet".into(),
            ephemeral.path(),
        )
        .await;

        assert_snapshot!(root.unwrap_err().to_string(), @r###"Ephemeral publication file has `build-env = "not _test_env"`; it cannot be used to publish with `--build-env _test_env`"###);
    }

    /// Loading an ephemeral package with no `build-env` (either passed or in the file) fails
    #[test(tokio::test)]
    async fn ephemeral_no_build_env() {
        let scenario = TestPackageGraph::new(["root"]).build();

        let tempdir = tempfile::tempdir().unwrap();
        let ephemeral = tempdir.path().join("nonexistent.toml");

        // load root package with ephemeral file

        let root = RootPackage::<Vanilla>::load_ephemeral(
            scenario.path_for("root"),
            None,
            "localnet".into(),
            ephemeral.join("nonexistent.toml"),
        )
        .await;

        assert_snapshot!(root.unwrap_err().to_string(), @"Ephemeral publication file does not have a `build-env` so you must pass `--build-env <env>`");
    }

    /// Loading an ephemeral package with an unrecognized `build-env` fails
    #[test(tokio::test)]
    async fn ephemeral_bad_build_env() {
        let scenario = TestPackageGraph::new(["root"]).build();

        let tempdir = tempfile::tempdir().unwrap();
        let ephemeral = tempdir.path().join("nonexistent.toml");

        // load root package with ephemeral file

        let root = RootPackage::<Vanilla>::load_ephemeral(
            scenario.path_for("root"),
            Some("unknown environment".into()),
            "localnet".into(),
            ephemeral,
        )
        .await;

        assert_snapshot!(root.unwrap_err().to_string(), @"Cannot build with build-env `unknown environment`: the recognized environments are <TODO>");
    }
}
