use std::path::{Path, PathBuf};

use crate::{
    errors::PackageResult,
    flavor::MoveFlavor,
    package::{EnvironmentID, EnvironmentName, RootPackage, block_on},
    schema::{Environment, ModeName},
};

/// A Builder for the [RootPackage] type
pub struct PackageLoader {
    config: PackageConfig,
}

#[derive(Clone, Debug)]
pub struct PackageConfig {
    /// The path to read all input files from (e.g. lockfiles, pubfiles, etc). If this path is
    /// different from `output_path`, the package system won't touch any files here Note that in
    /// the case of ephemeral loads, `self.load_type.ephemeral_file` may also be read
    pub input_path: PathBuf,

    /// The chain ID to build for
    pub chain_id: EnvironmentID,

    /// The ephemeral or persistent environment to load for
    pub load_type: LoadType,

    /// The directory to write all output files into (e.g. updated lockfiles, etc)
    /// Note that in the case of ephemeral loads, `self.load_type.ephemeral_file` may also be
    /// written
    pub output_path: PathBuf,

    /// The modes to load for
    pub modes: Vec<ModeName>,

    /// Repin the dependencies even if the lockfile is up-to-date
    pub force_repin: bool,

    /// Use the lockfile even if the manifest digests are out of date
    pub ignore_digests: bool,

    /// Don't fail if git cache is dirty
    pub allow_dirty: bool,
}

impl PackageLoader {
    /// A loader that loads the root package from `root_dir` for `env`
    pub fn new(root_dir: impl AsRef<Path>, env: Environment) -> Self {
        Self {
            config: PackageConfig::persistent(root_dir, env, vec![]),
        }
    }

    /// Loads the root package from `root` in environment `build_env`, but replaces all the
    /// addresses with the addresses in `pubfile`. Saving publication data will also save to the
    /// output to `pubfile` rather than `Published.toml`
    ///
    /// If `pubfile` does not exist, one is created with the provided `chain_id` and `build_env`;
    /// If the file does exist but these fields differ, then an error is returned.
    pub fn new_ephemeral(
        root_dir: impl AsRef<Path>,
        build_env: Option<EnvironmentName>,
        chain_id: EnvironmentID,
        pubfile_path: impl AsRef<Path>,
    ) -> Self {
        let config = PackageConfig {
            input_path: root_dir.as_ref().to_path_buf(),
            chain_id,
            load_type: LoadType::Ephemeral {
                build_env,
                ephemeral_file: pubfile_path.as_ref().to_path_buf(),
            },
            output_path: root_dir.as_ref().to_path_buf(),
            modes: vec![],
            force_repin: false,
            ignore_digests: false,
            allow_dirty: false,
        };
        Self { config }
    }

    /// dependencies with modes will be filtered out if those modes don't intersect with `modes`
    pub fn modes(mut self, modes: Vec<ModeName>) -> Self {
        self.config.modes = modes;
        self
    }

    /// Ignore the lockfile and automatically repin all dependencies for the current environment
    pub fn force_repin(mut self, force_repin: bool) -> Self {
        self.config.force_repin = force_repin;
        self
    }

    /// Do not repin dependencies even if their manifests have changed. This will fail if the
    /// lockfile does not have the correct dependencies stored (e.g. if a new dependency was added
    /// to the manifest)
    pub fn ignore_digests(mut self, ignore_digests: bool) -> Self {
        self.config.ignore_digests = ignore_digests;
        self
    }

    /// Do not fail if the git cache is dirty
    pub fn allow_dirty(mut self, allow_dirty: bool) -> Self {
        self.config.allow_dirty = allow_dirty;
        self
    }

    /// Write the output (generated pubfiles, lockfiles, etc) to `dir` instead of the input path
    pub fn output_path(mut self, output_dir: Option<impl AsRef<Path>>) -> Self {
        if let Some(output_dir) = output_dir {
            self.config.output_path = output_dir.as_ref().to_path_buf();
        }
        self
    }

    /// Load the root package. Note that loading does not write to the lockfile; you should call
    /// [RootPackage::write_pinned_deps] to save the results.
    ///
    /// By default `load` attempts to load the package from the lockfile, and repins if it is
    /// missing or out-of-date. However, this behavior can be changed using [Self::ignore_digests] and
    /// [Self::force_repin]
    pub async fn load<F: MoveFlavor>(self) -> PackageResult<RootPackage<F>> {
        RootPackage::validate_and_construct(self.config).await
    }

    /// Block the current thread and call [Self::load]
    pub fn load_sync<F: MoveFlavor>(self) -> PackageResult<RootPackage<F>> {
        block_on!(RootPackage::validate_and_construct(self.config))
    }

    pub(crate) fn config(&self) -> &PackageConfig {
        &self.config
    }
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
        ephemeral_file: PathBuf,
    },
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
            allow_dirty: false,
        }
    }
}

impl LoadType {
    /// return `Some(path)` if `self` is a valid ephemeral load, or None if it is a persistent load
    pub fn ephemeral_file(&self) -> Option<&Path> {
        match self {
            LoadType::Persistent { .. } => None,
            LoadType::Ephemeral { ephemeral_file, .. } => Some(ephemeral_file),
        }
    }
}
