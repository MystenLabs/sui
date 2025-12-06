// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use std::{collections::BTreeMap, fmt, path::Path};

use indexmap::IndexMap;
use tracing::debug;

use super::paths::{EphemeralPubfilePath, OutputPath, PackagePath};
use super::{EnvironmentID, manifest::Manifest};
use crate::graph::PackageInfo;
use crate::package::block_on;
use crate::package::package_lock::PackageSystemLock;
use crate::schema::{
    Environment, LocalPub, LockfileDependencyInfo, ModeName, PackageID, ParsedEphemeralPubs,
    ParsedPublishedFile, Publication, RenderToml,
};
use crate::{
    errors::{PackageError, PackageResult},
    flavor::MoveFlavor,
    graph::PackageGraph,
    package::EnvironmentName,
    schema::ParsedLockfile,
};

#[derive(Clone, Debug)]
pub struct PackageConfig {
    /// The path to read all input files from (e.g. lockfiles, pubfiles, etc). If this path is
    /// different from `output_path`, the package system won't touch any files here Note that in
    /// the case of ephemeral loads, `self.load_type.ephemeral_file` may also be read
    input_path: PathBuf,

    /// The chain ID to build for
    chain_id: EnvironmentID,

    /// The ephemeral or persistent environment to load for
    load_type: LoadType,

    /// The directory to write all output files into (e.g. updated lockfiles, etc)
    /// Note that in the case of ephemeral loads, `self.load_type.ephemeral_file` may also be
    /// written
    output_path: PathBuf,

    /// The modes to load for
    modes: Vec<ModeName>,

    /// Repin the dependencies even if the lockfile is up-to-date
    pub(crate) force_repin: bool,

    /// Use the lockfile even if the manifest digests are out of date
    pub(crate) ignore_digests: bool,
    // TODO: The directory to use for the git cache (defaults to `~/.move`)
    // cache_dir: Option<PathBuf>,
    // TODO: `--allow-dirty`
}

#[derive(Clone, Debug)]
pub enum LoadType {
    Persistent {
        env: EnvironmentName,
    },
    Ephemeral {
        /// The environment to build for. If it is `None`, the value in `ephemeral_file` will be
        /// used; if that file also doesn't exist, then the load will fail
        build_env: Option<EnvironmentName>,

        /// The ephemeral file to use for addresses, relative to the current working directory (not
        /// to `input_path`). This file will be written if the package is published (i.e. if
        /// [RootPackage::write_publish_data] is called). It does not have to exist a priori, but
        /// if it does, the addresses will be used.
        ephemeral_file: EphemeralPubfilePath,
    },
}

/// A package that is defined as the root of a Move project.
///
/// This is a special package that contains the project manifest and dependencies' graphs,
/// and associated functions to operate with this data.
#[derive(Debug)]
pub struct RootPackage<F: MoveFlavor + fmt::Debug> {
    /// The path to the files containing the root package
    input_path: PackagePath,

    /// The path to the output directory for the root package
    output_path: OutputPath,

    /// The ephemeral file
    ephemeral_file: Option<EphemeralPubfilePath>,

    /// The environment we're operating on for this root package.
    environment: Environment,

    /// The full dependency graph, which may include multiple packages with the same original ID
    /// and edges that don't match a mode filter
    unfiltered_graph: PackageGraph<F>,

    /// The reduced dependency graph, which has had mode filters applied, but hasn't yet had
    /// overrides applied.
    /// TODO: we should apply overrides here as well
    filtered_graph: PackageGraph<F>,

    /// An exclusive lock on the package's files
    mutex: PackageSystemLock,
}

impl PackageConfig {
    fn persistent(path: impl AsRef<Path>, env: Environment, modes: Vec<ModeName>) -> Self {
        Self {
            input_path: path.as_ref().to_path_buf(),
            chain_id: env.id,
            load_type: LoadType::Persistent { env: env.name },
            output_path: path.as_ref().to_path_buf(),
            modes,
            force_repin: false,
            ignore_digests: false,
        }
    }
}

impl LoadType {
    /// return `Some(path)` if `self` is a valid ephemeral load, or None if it is a persistent load
    fn ephemeral_file(&self) -> Option<&EphemeralPubfilePath> {
        match self {
            LoadType::Persistent { .. } => None,
            LoadType::Ephemeral { ephemeral_file, .. } => Some(ephemeral_file),
        }
    }
}

/// Root package is the "public" entrypoint for operations with the package management.
/// It's like a facade for all functionality, controlled by this.
impl<F: MoveFlavor + fmt::Debug> RootPackage<F> {
    pub fn environments(
        path: impl AsRef<Path>,
    ) -> PackageResult<IndexMap<EnvironmentName, EnvironmentID>> {
        let package_path = PackagePath::new(path.as_ref().to_path_buf())?;
        let mtx = package_path.lock()?;
        let mut environments = F::default_environments();

        if let Ok(modern_manifest) = Manifest::read_from_file(&package_path, &mtx) {
            environments.extend(modern_manifest.environments());
        }

        Ok(environments)
    }

    /// Load the root package for `env` using the "normal" path - we first try to load from the
    /// lockfiles; if the digests don't match then we repin using the manifests. Note that it does
    /// not write to the lockfile; you should call [Self::write_pinned_deps] to save the results.
    ///
    /// dependencies with modes will be filtered out if those modes don't intersect with `modes`
    pub async fn load(
        path: impl AsRef<Path>,
        env: Environment,
        modes: Vec<ModeName>,
    ) -> PackageResult<Self> {
        let config = PackageConfig::persistent(path, env, modes);

        Self::validate_and_construct(config).await
    }

    /// A synchronous version of `load` that can be used to load a package while blocking in place.
    pub fn load_sync(path: PathBuf, env: Environment, modes: Vec<ModeName>) -> PackageResult<Self> {
        block_on!(Self::load(path.as_path(), env, modes))
    }

    /// Load the root package from `root` in environment `build_env`, but replace all the addresses
    /// with the addresses in `pubfile`. Saving publication data will also save to the output to
    /// `pubfile` rather than `Published.toml`
    ///
    /// If `pubfile` does not exist, one is created with the provided `chain_id` and `build_env`;
    /// If the file does exist but these fields differ, then an error is returned.
    ///
    /// dependencies with modes will be filtered out if those modes don't intersect with `modes`
    pub async fn load_ephemeral(
        root: impl AsRef<Path>,
        build_env: Option<EnvironmentName>,
        chain_id: EnvironmentID,
        pubfile_path: impl AsRef<Path>,
        modes: Vec<ModeName>,
    ) -> PackageResult<Self> {
        let ephemeral_file = EphemeralPubfilePath::new(pubfile_path)?;
        let config = PackageConfig {
            input_path: root.as_ref().to_path_buf(),
            chain_id,
            load_type: LoadType::Ephemeral {
                build_env,
                ephemeral_file,
            },
            output_path: root.as_ref().to_path_buf(),
            modes,
            force_repin: false,
            ignore_digests: false,
        };

        Self::validate_and_construct(config).await
    }

    /// Loads the root package from path and builds a dependency graph from the manifests.
    /// This forcefully re-pins all dependencies even if the manifest digests match. Note that it
    /// does not write to the lockfile; you should call [Self::save_to_disk] to save the results.
    ///
    /// TODO: We should load from lockfiles instead of manifests for deps.
    /// dependencies with modes will be filtered out if those modes don't intersect with `modes`
    pub async fn load_force_repin(
        path: impl AsRef<Path>,
        env: Environment,
        modes: Vec<ModeName>,
    ) -> PackageResult<Self> {
        let mut config = PackageConfig::persistent(path, env, modes);
        config.force_repin = true;
        /*
        let graph = PackageGraph::<F>::load_from_manifests(&package_path, &env).await?;
        */

        Self::validate_and_construct(config).await
    }

    /// Loads the root lockfile only, ignoring all manifests. Returns an error if the lockfile
    /// doesn't exist of if it doesn't contain a dependency graph for `env`.
    ///
    /// Note that this still fetches all of the dependencies, it just doesn't look at their
    /// manifests.
    ///
    /// dependencies with modes will be filtered out if those modes don't intersect with `modes`
    pub async fn load_ignore_digests(
        path: impl AsRef<Path>,
        env: Environment,
        modes: Vec<ModeName>,
    ) -> PackageResult<Self> {
        let mut config = PackageConfig::persistent(path, env, modes);
        config.ignore_digests = true;
        Self::validate_and_construct(config).await
    }

    /// The metadata for the root package in [PackageInfo] form
    pub fn package_info(&self) -> PackageInfo<F> {
        self.filtered_graph.root_package_info()
    }

    /// Central validation point for a RootPackage.
    ///
    /// 1. check that the path has a manifest
    /// 2. get an environment from the ephemeral file
    ///
    /// This helps validate:
    /// 1. TODO: Fill this in! (deduplicate nodes etc)
    async fn validate_and_construct(mut config: PackageConfig) -> PackageResult<Self> {
        let input_path = PackagePath::new(config.input_path.clone())?;
        let mutex = input_path.lock()?;

        let ephemeral_file = config.load_type.ephemeral_file().cloned();
        let output_path = OutputPath::new(config.output_path.clone())?;

        debug!(
            "creating RootPackage (CWD: {:?})\n{config:#?}",
            std::env::current_dir()
        );

        debug!("getting ephemeral files");
        let (env, ephemeral_pubs) = Self::get_env_and_ephemeral_file(&mut config).await?;

        debug!("loading unfiltered graph");
        let unfiltered_graph = if config.force_repin {
            PackageGraph::<F>::load_from_manifests(&input_path, &env, &mutex).await?
        } else if config.ignore_digests {
            PackageGraph::<F>::load_from_lockfile_ignore_digests(&input_path, &env, &mutex)
                .await?
                .unwrap()
        } else {
            PackageGraph::<F>::load(&input_path, &env, &mutex).await?
        };

        debug!("filtering graph");
        let mut filtered_graph = unfiltered_graph.filter_for_mode(&config.modes).linkage()?;
        if let Some(ephemeral_pubs) = ephemeral_pubs {
            debug!("adding overrides");
            filtered_graph.add_publish_overrides(localpubs_to_publications(&ephemeral_pubs)?);
        }

        debug!("checking rename-from");
        unfiltered_graph.check_rename_from()?;

        debug!(
            "packages (unfiltered): {:?}",
            unfiltered_graph
                .packages()
                .iter()
                .map(|pkg| pkg.display_name())
                .collect::<Vec<_>>()
        );

        Ok(Self {
            environment: env,
            unfiltered_graph,
            filtered_graph,
            output_path,
            ephemeral_file,
            input_path,
            mutex,
        })
    }

    /// Returns the build environment to use for this package. For ephemeral loads, this requires
    /// reading the ephemeral pubfile as well, so this function also returns a parsed pubfile if
    /// the load is ephemeral.
    async fn get_env_and_ephemeral_file(
        config: &mut PackageConfig,
    ) -> PackageResult<(Environment, Option<ParsedEphemeralPubs<F>>)> {
        let result = match &mut config.load_type {
            LoadType::Persistent { env } => {
                (Environment::new(env.clone(), config.chain_id.clone()), None)
            }
            LoadType::Ephemeral {
                build_env,
                ephemeral_file,
            } => {
                let ephemeral =
                    Self::load_ephemeral_pubfile(build_env, &config.chain_id, ephemeral_file)?;
                (
                    Environment::new(ephemeral.build_env.clone(), config.chain_id.clone()),
                    Some(ephemeral),
                )
            }
        };

        Ok(result)
    }

    /// The id of the root package (TODO: perhaps this method is poorly named; check where it's
    /// used and decide if they should be using `id` or `display_name`)
    pub fn name(&self) -> &PackageID {
        self.package_info().id()
    }

    /// Returns the `display_name` for the root package.
    /// Invariant: For modern packages, this is always equal to `name().as_str()`
    pub fn display_name(&self) -> &str {
        self.filtered_graph.root_package().display_name()
    }

    /// Return the path to the directory containing the root package
    pub fn package_path(&self) -> &Path {
        self.input_path.path()
    }

    /// Return the list of all packages in the root package's package graph (including itself and all
    /// transitive dependencies). This includes the non-duplicate addresses only.
    pub fn packages(&self) -> Vec<PackageInfo<F>> {
        self.filtered_graph.packages()
    }

    /// Update the dependencies in the lockfile for this environment to match the dependency graph
    /// represented by `self`.
    ///
    /// Before overwriting the lockfile, this function also extracts any publication information
    /// from the legacy lockfile and writes it into the pubfile
    pub fn save_lockfile_to_disk(&mut self) -> PackageResult<()> {
        // migrate any pubs from the legacy lockfile to the modern pubfile before we clobber the
        // legacy lockfile.
        let legacy_pubs = self.input_path.read_legacy_lockfile(&self.mutex)?;
        if let Some(pubs) = &legacy_pubs
            && !pubs.is_empty()
        {
            let old_pubfile = self
                .input_path
                .read_pubfile(&self.mutex)?
                .map(|(_, p)| p)
                .unwrap_or_default();
            let mut legacy_pubs: ParsedPublishedFile<F> = pubs.clone().into();
            // if the same publication exists in both, we keep the modern one
            legacy_pubs.published.extend(old_pubfile.published);
            self.output_path.write_pubfile(&legacy_pubs, &self.mutex)?;
        }

        let mut lockfile: ParsedLockfile = if legacy_pubs.is_some() {
            ParsedLockfile::default()
        } else {
            self.input_path
                .read_lockfile(&self.mutex)?
                .map(|(_, l)| l)
                .unwrap_or_default()
        };

        // merge our graph and write to disk
        lockfile.pinned.insert(
            self.environment.name.clone(),
            self.unfiltered_graph.to_pins()?,
        );
        self.output_path.write_lockfile(&lockfile, &self.mutex)?;

        Ok(())
    }

    /// Record metadata for a publication for the root package in either its `Published.toml` or
    /// its ephemeral pubfile (depending on how it was loaded)
    pub fn write_publish_data(&mut self, publish_data: Publication<F>) -> PackageResult<()> {
        let root_dep = self.package_info().package().dep_for_self().clone().into();
        if let Some(ephemeral_file) = &mut self.ephemeral_file {
            let mut pubs = ephemeral_file
                .read_pubfile::<F>()?
                .map(|(_, pubs)| pubs)
                .unwrap_or_default();

            pubs.published
                .retain(|localpub| localpub.source != root_dep);

            let new_pub = LocalPub {
                source: root_dep,
                addresses: publish_data.addresses,
                version: publish_data.version,
                metadata: publish_data.metadata,
            };

            // TODO: should we check build-env and chain-id again?
            pubs.published.push(new_pub);

            ephemeral_file.write_pubfile(&pubs)?;
        } else {
            let mut pubfile = self
                .input_path
                .read_pubfile(&self.mutex)?
                .map(|(_, p)| p)
                .unwrap_or_default();
            pubfile
                .published
                .insert(self.environment.name.clone(), publish_data);
            self.output_path.write_pubfile(&pubfile, &self.mutex)?;
        }

        Ok(())
    }

    /// Load ephemeral publications from `pubfile`, checking that they have the correct `chain-id`
    /// and `build-env`. If the file does not exist, a new file is created and returned
    fn load_ephemeral_pubfile(
        build_env: &Option<EnvironmentName>,
        chain_id: &EnvironmentID,
        pubfile: &mut EphemeralPubfilePath,
    ) -> PackageResult<ParsedEphemeralPubs<F>> {
        if let Some((file, parsed)) = pubfile.read_pubfile()? {
            if let Some(build_env) = build_env
                && *build_env != parsed.build_env
            {
                return Err(PackageError::EphemeralEnvMismatch {
                    file,
                    file_build_env: parsed.build_env,
                    passed_build_env: build_env.clone(),
                });
            }
            if *chain_id != parsed.chain_id {
                return Err(PackageError::EphemeralChainMismatch {
                    file,
                    file_chain_id: parsed.chain_id,
                    passed_chain_id: chain_id.clone(),
                });
            }

            Ok(parsed)
        } else {
            let Some(build_env) = build_env else {
                return Err(PackageError::EphemeralNoBuildEnv);
            };

            let pubs = ParsedEphemeralPubs {
                build_env: build_env.clone(),
                chain_id: chain_id.clone(),
                published: Vec::new(),
            };

            pubfile.write_pubfile(&pubs)?;

            Ok(pubs)
        }
    }

    /// Return the publication information for the root package in the current environment
    pub fn publication(&self) -> Option<&Publication<F>> {
        self.filtered_graph.root_package().publication()
    }

    /// Sorts topologically the dependency graph and returns the package IDs for each package. Note
    /// that this will include the root package as well.
    pub fn sorted_deps_ids(&self) -> Vec<&PackageID> {
        self.filtered_graph
            .sorted_packages()
            .into_iter()
            .map(|p| p.id())
            .collect()
    }
}

fn localpubs_to_publications<F: MoveFlavor>(
    pubfile: &ParsedEphemeralPubs<F>,
) -> PackageResult<BTreeMap<LockfileDependencyInfo, Publication<F>>> {
    let mut result = BTreeMap::new();
    for local_pub in &pubfile.published {
        let new = Publication::<F> {
            chain_id: pubfile.chain_id.clone(),
            addresses: local_pub.addresses.clone(),
            version: local_pub.version,
            metadata: local_pub.metadata.clone(),
        };

        let old = result.insert(local_pub.source.clone(), new);
        if old.is_some() {
            let mut dep = local_pub.source.render_as_toml();
            // take off trailing newline
            dep.pop();
            return Err(PackageError::MultipleEphemeralEntries { dep });
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;
    use std::{fs, io::Write, path::PathBuf};
    use test_log::test;

    use super::*;
    use crate::{
        flavor::{
            Vanilla,
            vanilla::{self, DEFAULT_ENV_ID, DEFAULT_ENV_NAME, default_environment},
        },
        graph::NamedAddress,
        schema::{
            LockfileDependencyInfo, OriginalID, PackageID, PublishAddresses, PublishedID,
            RenderToml,
        },
        test_utils::{
            self, basic_manifest_with_default_envs, basic_manifest_with_env,
            git::{self},
            graph_builder::TestPackageGraph,
        },
    };

    /// Create the following directory structure:
    /// ```ignore
    /// packages/
    ///   pkg_a/Move.toml
    ///   pkg_b/Move.toml
    ///   nodeps/Move.toml
    ///   graph/Move.toml        # depends on nodeps and depends_a_b
    ///   graph/Move.lock        # copied from `tests/data/basic_move_project/graph/Move.lock
    ///   depends_a_b/Move.toml  # depends on pkg_a and pkg_b
    /// ```
    async fn setup_test_move_project() -> (Environment, PathBuf) {
        let env = crate::flavor::vanilla::default_environment();
        let project = test_utils::project()
            .file(
                "packages/pkg_a/Move.toml",
                &basic_manifest_with_default_envs("pkg_a", "0.0.1"),
            )
            .file(
                "packages/pkg_b/Move.toml",
                &basic_manifest_with_default_envs("pkg_b", "0.0.1"),
            )
            .file(
                "packages/nodeps/Move.toml",
                &basic_manifest_with_default_envs("nodeps", "0.0.1"),
            )
            .file(
                "packages/graph/Move.toml",
                &basic_manifest_with_default_envs("graph", "0.0.1"),
            )
            .file(
                "packages/depends_a_b/Move.toml",
                &basic_manifest_with_default_envs("depends_a_b", "0.0.1"),
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
            let package = RootPackage::<Vanilla>::load(&pkg_path, env.clone(), vec![])
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
        let pkg_path = root_path.join("packages").join("graph");

        // Test environment operations
        assert!(
            RootPackage::<Vanilla>::environments(&pkg_path)
                .unwrap()
                .contains_key(DEFAULT_ENV_NAME)
        );

        // Test loading root package with check for environment existing in manifest
        let root = RootPackage::<Vanilla>::load(&pkg_path, env, vec![])
            .await
            .unwrap();

        assert_eq!(root.name(), &PackageID::from("graph"));
    }

    #[test(tokio::test)]
    async fn test_cannot_override_default_environments() {
        let project = test_utils::project()
            .file(
                "Move.toml",
                &basic_manifest_with_env(
                    "graph",
                    "0.0.1",
                    DEFAULT_ENV_NAME,
                    "DIFFERENT_FROM_DEFAULT",
                ),
            )
            .build();

        let environment =
            Environment::new(DEFAULT_ENV_NAME.to_string(), DEFAULT_ENV_ID.to_string());

        let load_err = RootPackage::<Vanilla>::load(&project.root(), environment, vec![])
            .await
            .unwrap_err();

        let message = load_err
            .to_string()
            .replace(project.root_path_str(), "<DIR>");

        assert_snapshot!(
            message,
            @"Error while loading dependency <DIR>: Cannot override default environments. Environment `_test_env` is a system environment and cannot be overridden. System environments: _test_env"
        );
    }

    #[test(tokio::test)]
    async fn test_load_and_check_for_env() {
        let (_, root_path) = setup_test_move_project().await;

        let path = root_path.join("graph");
        // should fail as devnet does not exist in the manifest
        assert!(
            RootPackage::<Vanilla>::load(
                &path,
                Environment::new("devnet".to_string(), "abcd1234".to_string()),
                vec![]
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

        RootPackage::<Vanilla>::load(scenario.path_for("a"), default_environment(), vec![])
            .await
            .unwrap_err();
    }

    /// This gives a snapshot of a generated lockfile
    #[test(tokio::test)]
    async fn graph_to_lockfile() {
        let scenario = TestPackageGraph::new(["example", "baz", "bar"])
            .add_deps([("example", "baz"), ("baz", "bar")])
            .build();

        let env = default_environment();
        let mut root = RootPackage::<Vanilla>::load(scenario.path_for("example"), env, vec![])
            .await
            .unwrap();

        root.save_lockfile_to_disk().unwrap();
        let lockfile = root
            .output_path
            .dump_lockfile(&root.mutex)
            .await
            .render_as_toml();

        // WARNING! If you change these digests, make sure you know what you are doing! If the
        // computed digests change, it means that all packages will be repinned! (made this not a
        // snapshot test to avoid accidental updating)
        assert_eq!(
            lockfile,
            indoc::indoc!(
                r#"
                # Generated by move; do not edit
                # This file should be checked in.

                [move]
                version = 4

                [pinned._test_env.bar]
                source = { local = "../bar" }
                use_environment = "_test_env"
                manifest_digest = "C4FE4C91DE74CBF223B2E380AE40F592177D21870DC2D7EB6227D2D694E05363"
                deps = {}

                [pinned._test_env.baz]
                source = { local = "../baz" }
                use_environment = "_test_env"
                manifest_digest = "3EB64C41D6605EA93535C1CF3993AEC4AB3988AB10F64134EDDC56EA90859DEF"
                deps = { bar = "bar" }

                [pinned._test_env.example]
                source = { root = true }
                use_environment = "_test_env"
                manifest_digest = "8CC8B4A8252DD091E636E52E34F3AE9A876B4A873F4A40EE2CA191459BEA2B3B"
                deps = { baz = "baz" }
                "#
            )
        );
    }

    ////////////////////////////////////////////////////////////////////////////////////////////////
    // Git pinning / repinning /////////////////////////////////////////////////////////////////////
    ////////////////////////////////////////////////////////////////////////////////////////////////

    /// A git dependency on a branch gets pinned to the sha
    #[test(tokio::test)]
    pub async fn git_branch_dep_pinned() {
        let env = default_environment();
        let repo = git::new().await;
        let commit = repo.commit(|project| project.add_packages(["a"])).await;
        commit.branch("branch-name").await;

        let project = TestPackageGraph::new(["root"])
            .add_git_dep("root", &repo, "a", "branch-name", |dep| dep)
            .build();

        let mut root_pkg =
            RootPackage::<Vanilla>::load(project.path_for("root"), env.clone(), vec![])
                .await
                .unwrap();

        root_pkg.save_lockfile_to_disk().unwrap();

        let sha = dep_sha(&root_pkg, &env.name, "a").await;
        assert_eq!(sha, commit.sha());
    }

    /// A git dependency on a short sha gets pinned to the sha
    #[test(tokio::test)]
    pub async fn git_short_sha_dep_pinned() {
        let env = default_environment();
        let repo = git::new().await;
        let commit = repo.commit(|project| project.add_packages(["a"])).await;

        let project = TestPackageGraph::new(["root"])
            .add_git_dep("root", &repo, "a", commit.short_sha(), |dep| dep)
            .build();

        let mut root_pkg =
            RootPackage::<Vanilla>::load(project.path_for("root"), env.clone(), vec![])
                .await
                .unwrap();

        root_pkg.save_lockfile_to_disk().unwrap();

        let sha = dep_sha(&root_pkg, &env.name, "a").await;
        assert_eq!(sha, commit.sha());
    }

    /// If we have a git dep, and we pin using a branch, then change the branch, then load again,
    /// we get the sha of the first commit. See also [git_force_repin]
    #[test(tokio::test)]
    pub async fn git_no_repin() {
        let env = default_environment();
        let repo = git::new().await;
        let commit1 = repo.commit(|project| project.add_packages(["a"])).await;
        commit1.branch("branch-name").await;

        let project = TestPackageGraph::new(["root"])
            .add_git_dep("root", &repo, "a", "branch-name", |dep| dep)
            .build();

        // load the root package and save the lockfile
        let mut root_pkg =
            RootPackage::<Vanilla>::load(project.path_for("root"), env.clone(), vec![])
                .await
                .unwrap();
        root_pkg.save_lockfile_to_disk().unwrap();
        drop(root_pkg); // release the fs lock

        // change the branch
        let commit2 = repo
            .commit(|project| project.add_package("a", |a| a.version("0.0.2")))
            .await;
        commit2.branch("branch-name").await;

        // reload the root package and save the lockfile again
        let mut root_pkg =
            RootPackage::<Vanilla>::load(project.path_for("root"), env.clone(), vec![])
                .await
                .unwrap();
        root_pkg.save_lockfile_to_disk().unwrap();

        // sha should still be for commit 1
        let sha = dep_sha(&root_pkg, &env.name, "a").await;
        assert_eq!(sha, commit1.sha());
    }

    /// If we have a git dep, and we pin using a branch, then change the branch, then load again
    /// with forced repinning, we get the sha of the second commit. See also [git_no_repin]
    #[test(tokio::test)]
    pub async fn git_force_repin() {
        let env = default_environment();
        let repo = git::new().await;
        let commit1 = repo.commit(|project| project.add_packages(["a"])).await;
        commit1.branch("branch-name").await;

        let project = TestPackageGraph::new(["root"])
            .add_git_dep("root", &repo, "a", "branch-name", |dep| dep)
            .build();

        // load the root package and save the lockfile
        let mut root_pkg =
            RootPackage::<Vanilla>::load(project.path_for("root"), env.clone(), vec![])
                .await
                .unwrap();
        root_pkg.save_lockfile_to_disk().unwrap();
        drop(root_pkg); // release FS lock

        // change the branch
        let commit2 = repo
            .commit(|project| project.add_package("a", |a| a.version("0.0.2")))
            .await;
        commit2.branch("branch-name").await;

        // reload the root package with force repinning and save the lockfile again
        let mut root_pkg =
            RootPackage::<Vanilla>::load_force_repin(project.path_for("root"), env.clone(), vec![])
                .await
                .unwrap();
        root_pkg.save_lockfile_to_disk().unwrap();

        // since we repinned, sha should be for commit 2
        let sha = dep_sha(&root_pkg, &env.name, "a").await;
        assert_eq!(sha, commit2.sha());
    }

    /// If we have a git dep, and we pin using a branch, then change the branch, then update the
    /// root manifest, and then load again, we get the sha of the second commit since the manifest
    /// change should trigger a repin
    #[test(tokio::test)]
    pub async fn git_change_manifest() {
        let env = default_environment();
        let repo = git::new().await;
        let commit1 = repo.commit(|project| project.add_packages(["a"])).await;
        commit1.branch("branch-name").await;

        let project = TestPackageGraph::new(["root"])
            .add_git_dep("root", &repo, "a", "branch-name", |dep| dep)
            .build();

        // load the root package and save the lockfile
        let mut root_pkg =
            RootPackage::<Vanilla>::load(project.path_for("root"), env.clone(), vec![])
                .await
                .unwrap();
        root_pkg.save_lockfile_to_disk().unwrap();
        drop(root_pkg); // release FS lock

        // change the branch so we will notice a repin
        let commit2 = repo
            .commit(|project| project.add_package("a", |a| a.version("0.0.2")))
            .await;
        commit2.branch("branch-name").await;

        // modify the manifest and then reload
        project.extend_file("root/Move.toml", "\n# extra stuff\n");
        let mut root_pkg =
            RootPackage::<Vanilla>::load_force_repin(project.path_for("root"), env.clone(), vec![])
                .await
                .unwrap();
        root_pkg.save_lockfile_to_disk().unwrap();

        // since the manifest changed, we should have repinned, so the sha should be for commit 2
        let sha = dep_sha(&root_pkg, &env.name, "a").await;
        assert_eq!(sha, commit2.sha());
    }

    /// If we have a dep whose manifest is out of date, we repin everything
    #[test(tokio::test)]
    pub async fn git_change_dep_manifest() {
        // there are 3 packages: `root`, `dirty`, and `git_dep`.
        // root depends on a branch of git_dep and also depends on dirty
        // we first pin root, then we update the branch and dirty `dirty`
        // when we reload `root`, we should notice that `dirty` is dirty and repin, which should
        // cause `git_dep` to be bumped to the latest version
        let env = default_environment();
        let repo = git::new().await;
        let commit1 = repo
            .commit(|project| project.add_packages(["git_dep"]))
            .await;
        commit1.branch("branch-name").await;

        let project = TestPackageGraph::new(["root", "dirty"])
            .add_git_dep("root", &repo, "git_dep", "branch-name", |dep| dep)
            .add_deps([("root", "dirty")])
            .build();

        // load the root package and save the lockfile
        let mut root_pkg =
            RootPackage::<Vanilla>::load(project.path_for("root"), env.clone(), vec![])
                .await
                .unwrap();
        root_pkg.save_lockfile_to_disk().unwrap();
        drop(root_pkg); // release package lock

        // change the branch so that we will notice a repin
        let commit2 = repo
            .commit(|project| project.add_package("git_dep", |pkg| pkg.version("0.0.2")))
            .await;
        commit2.branch("branch-name").await;

        // modify the manifest for `dirty` and then reload
        project.extend_file("dirty/Move.toml", "\n# extra stuff\n");
        let mut root_pkg =
            RootPackage::<Vanilla>::load_force_repin(project.path_for("root"), env.clone(), vec![])
                .await
                .unwrap();
        root_pkg.save_lockfile_to_disk().unwrap();

        // since the dependency's manifest changed, we should have repinned, so the sha should be
        // for commit 2
        let sha = dep_sha(&root_pkg, &env.name, "git_dep").await;
        assert_eq!(sha, commit2.sha());
    }

    /// Reads the root package's output lockfile and extracts the dep with `id` in `env`. Asserts
    /// that it is a git dep, and returns its sha
    async fn dep_sha(
        root: &RootPackage<Vanilla>,
        env: &EnvironmentName,
        id: impl AsRef<str>,
    ) -> String {
        let pin = &root.output_path.dump_lockfile(&root.mutex).await.pinned[env][id.as_ref()];
        let LockfileDependencyInfo::Git(git) = &pin.source else {
            panic!("expected git dep");
        };
        git.rev.to_string()
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

            [[published]]
            source = {{ root = true }}
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
            vec![],
        )
        .await
        .unwrap();

        // check the root package's named address
        let root_addr = root
            .package_info()
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

            [[published]]
            source = {{ local = "../dep" }}
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
            vec![],
        )
        .await
        .unwrap();

        // check the dependency's addresses

        let dep_addrs = root
            .filtered_graph
            .package_info_by_id(&PackageID::from("dep"))
            .unwrap()
            .published()
            .unwrap()
            .clone();

        assert_eq!(dep_addrs.original_id, OriginalID::from(2));
        assert_eq!(dep_addrs.published_at, PublishedID::from(3));
    }

    /// The ephemeral file contains two entries with the same `source`; this should not be allowed
    #[test(tokio::test)]
    async fn ephemeral_duplicates() {
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

            [[published]]
            source = {{ root = true }}
            version = 1
            published-at = "0x1"
            original-id = "0x2"

            [[published]]
            source = {{ root = true }}
            version = 2
            published-at = "0x1"
            original-id = "0x2"
            "###,
        )
        .unwrap();

        // load root package with ephemeral file

        let err = RootPackage::<Vanilla>::load_ephemeral(
            scenario.path_for("root"),
            None,
            "localnet".into(),
            ephemeral.path(),
            vec![],
        )
        .await
        .unwrap_err();

        assert_snapshot!(err.to_string(), @"Multiple entries with `source = { root = true }` exist in the publication file");
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
            vec![],
        )
        .await
        .unwrap();

        // check the dependency's addresses

        let dep_addrs = root
            .filtered_graph
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

            [[published]]
            source = {{ local = "../dep" }}
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
            vec![],
        )
        .await
        .unwrap();

        // check the dependency's addresses

        let dep_addrs = root
            .filtered_graph
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
            vec![],
        )
        .await
        .unwrap();

        // check the dependency's addresses
        assert!(
            root.filtered_graph
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

            [[published]]
            source = {{ local = "../dep1" }}
            original-id = "0x4"
            published-at = "0x5"
            version = 0

            [[published]]
            source = {{ local = "../dep2" }}
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
            vec![],
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

            [[published]]
            source = {{ local = "../dep1" }}
            original-id = "0x2"
            published-at = "0x5"
            version = 0

            [[published]]
            source = {{ local = "../dep2" }}
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
            vec![],
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
            vec![],
        )
        .await
        .unwrap();

        // check the dependency's addresses
        let dep_addrs = root
            .filtered_graph
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
        let scenario = TestPackageGraph::new(Vec::<String>::new())
            .add_published("root", OriginalID::from(1), PublishedID::from(1))
            .add_published("dep", OriginalID::from(2), PublishedID::from(2))
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
            vec![],
        )
        .await
        .unwrap();

        let prepublish_pubfile =
            std::fs::read_to_string(scenario.path_for("root/Published.toml")).unwrap();

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
        let postpublish_pubfile =
            std::fs::read_to_string(scenario.path_for("root/Published.toml")).unwrap();
        let ephemeral_data = std::fs::read_to_string(ephemeral.path()).unwrap();

        assert_eq!(prepublish_pubfile, postpublish_pubfile);
        assert_snapshot!(ephemeral_data, @r###"
        # generated by Move
        # this file contains metadata from ephemeral publications
        # this file should not be committed to source control

        build-env = "_test_env"
        chain-id = "localnet"

        [[published]]
        source = { root = true }
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
            vec![],
        )
        .await;

        let message = root
            .unwrap_err()
            .to_string()
            .replace(ephemeral.path().to_string_lossy().as_ref(), "<FILE>");

        assert_snapshot!(message, @r###"Ephemeral publication file "<FILE>" has chain-id `not localnet`; it cannot be used to publish to chain with id `localnet`"###);
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
            vec![],
        )
        .await;

        let message = root
            .unwrap_err()
            .to_string()
            .replace(ephemeral.path().to_string_lossy().as_ref(), "<FILE>");

        assert_snapshot!(message, @r###"Ephemeral publication file "<FILE>" has `build-env = "not _test_env"`; it cannot be used to publish with `--build-env _test_env`"###);
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
            vec![],
        )
        .await;

        let message = root
            .unwrap_err()
            .to_string()
            .replace(ephemeral.to_string_lossy().as_ref(), "<FILE>");

        assert_snapshot!(message, @"Ephemeral publication file does not exist, so you must pass `--build-env <env>` to indicate what environment it should be created for");
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
            ephemeral.clone(),
            vec![],
        )
        .await;

        let message = root.unwrap_err().to_string().replace(
            scenario.path_for("root").to_string_lossy().as_ref(),
            "<DIR>",
        );

        assert_snapshot!(message, @r#"Error while loading dependency <DIR>: Package `root` does not declare a `unknown environment` environment. The available environments are ["_test_env"]. Consider running with `--build-env _test_env`"#);
    }

    /// ```mermaid
    /// graph LR
    ///     root --> a --> b --> c1
    ///              a -->|test, override| c2
    /// ```
    ///
    /// In this scenario, the test-only override dependency on c2 should be ignored when computing
    /// the non-test linkage, so `c1` should be in the computed graph, and not c2
    ///
    /// See also [mode_overrides_affected]
    #[test(tokio::test)]
    async fn mode_overrides_unaffected() {
        let scenario = TestPackageGraph::new(["root", "a", "b"])
            .add_published("c1", OriginalID::from(1), PublishedID::from(1))
            .add_published("c2", OriginalID::from(1), PublishedID::from(2))
            .add_deps([("root", "a"), ("a", "b"), ("b", "c1")])
            .add_dep("a", "c2", |dep| dep.set_override().modes(["test"]))
            .build();

        let root =
            RootPackage::<Vanilla>::load(scenario.path_for("root"), default_environment(), vec![])
                .await
                .unwrap();

        let mut package_names: Vec<_> = root
            .packages()
            .into_iter()
            .map(|pkg| pkg.display_name().to_string())
            .collect();
        package_names.sort();

        assert_eq!(package_names, ["a", "b", "c1", "root"]);
    }
    /// ```mermaid
    /// graph LR
    ///     root --> a --> b --> c1
    ///              a -->|test, override| c2
    /// ```
    ///
    /// In this scenario, the test-only override dependency on c2 should NOT be ignored when computing
    /// the test linkage, so `c2` should be in the computed graph, and not c1
    ///
    /// See also [mode_overrides_unaffected]
    #[test(tokio::test)]
    async fn mode_overrides_affected() {
        let scenario = TestPackageGraph::new(["root", "a", "b"])
            .add_published("c1", OriginalID::from(1), PublishedID::from(1))
            .add_published("c2", OriginalID::from(1), PublishedID::from(2))
            .add_deps([("root", "a"), ("a", "b"), ("b", "c1")])
            .add_dep("a", "c2", |dep| dep.set_override().modes(["test"]))
            .build();

        let root = RootPackage::<Vanilla>::load(
            scenario.path_for("root"),
            default_environment(),
            vec!["test".to_string()],
        )
        .await
        .unwrap();

        let mut package_names: Vec<_> = root
            .packages()
            .into_iter()
            .map(|pkg| pkg.display_name().to_string())
            .collect();
        package_names.sort();

        assert_eq!(package_names, ["a", "b", "c2", "root"]);
    }
}
