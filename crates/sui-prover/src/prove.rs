// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::{Args, Parser};
use move_cli::base;
use move_package::{source_package::layout::SourcePackageLayout, BuildConfig, ModelConfig};
use move_prover::run_boogie_gen;
use tracing::log::LevelFilter;
use std::path::Path;

/// General prove options
#[derive(Args)]
#[clap(next_help_heading = "General Options")]
pub struct GeneralOptions {
    /// Split verification into separate proof goals for each execution path
    #[clap(name = "split-paths", long, short = 's', global = true)]
    pub split_paths: Option<usize>,

    /// Set verification timeout in seconds (default: 3000)
    #[clap(name = "timeout", long, short = 't', global = true)]
    pub timeout: Option<usize>,

    /// Don't delete temporary files after verification
    #[clap(name = "keep-temp", long, short = 'k', global = true)]
    pub keep_temp: bool,

    /// Display detailed verification progress
    #[clap(name = "verbose", long, short = 'v', global = true)]
    pub verbose: bool,
}

/// Boogie options
#[derive(Args)]
#[clap(next_help_heading = "Boggie Options")]
pub struct BoogieConfig {
    /// Display detailed verification progress
    #[clap(name = "use_array_theory", long = "use_array_theory", global = true)]
    pub use_array_theory: bool,
}

#[derive(Parser)]
#[group(id = "sui-prover-prove")]
pub struct Prove {
    /// General options
    #[clap(flatten)]
    pub config: GeneralOptions,

    /// Boggie options
    #[clap(flatten)]
    pub boogie_config: BoogieConfig,

    /// Package build options
    #[clap(flatten)]
    #[clap(next_help_heading = "Build Options")]
    pub build_config: BuildConfig,
}

impl Prove {
    pub fn execute(
        &self,
        path: Option<&Path>,
    ) -> anyhow::Result<()> {
        let rerooted_path = base::reroot_path(path)?;
        let mut config = resolve_lock_file_path(self.build_config.clone(), Some(&rerooted_path))?;

        config.verify_mode = true;
        config.dev_mode = true;

        let model = config.move_model_for_package(
            &rerooted_path,
            ModelConfig {
                all_files_as_targets: false,
                target_filter: None,
            },
        )?;
        let mut options = move_prover::cli::Options::default();
        // don't spawn async tasks when running Boogie--causes a crash if we do
        options.backend.sequential_task = true;
        options.backend.use_array_theory = self.boogie_config.use_array_theory;
        options.backend.keep_artifacts = self.config.keep_temp;
        options.backend.vc_timeout = self.config.timeout.unwrap_or(3000);
        options.backend.path_split = self.config.split_paths;
        options.verbosity_level = if self.config.verbose { LevelFilter::Trace } else { LevelFilter::Info };
        
        run_boogie_gen(&model, options)?;

        Ok(())
    }
}

fn resolve_lock_file_path(
    mut build_config: BuildConfig,
    package_path: Option<&Path>,
) -> Result<BuildConfig, anyhow::Error> {
    if build_config.lock_file.is_none() {
        let package_root = base::reroot_path(package_path)?;
        let lock_file_path = package_root.join(SourcePackageLayout::Lock.path());
        build_config.lock_file = Some(lock_file_path);
    }
    Ok(build_config)
}
