// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use move_cli::base::{self, build};
use move_package::BuildConfig as MoveBuildConfig;
use serde_json::json;
use std::path::{Path, PathBuf};
use sui_framework_build::compiled_package::BuildConfig;

#[derive(Parser)]
pub struct Build {
    #[clap(flatten)]
    pub build: build::Build,
    /// Whether we are printing in base64.
    #[clap(long, global = true)]
    pub dump_bytecode_as_base64: bool,
}

impl Build {
    pub fn execute(
        &self,
        path: Option<PathBuf>,
        build_config: MoveBuildConfig,
    ) -> anyhow::Result<()> {
        let rerooted_path = base::reroot_path(path)?;
        Self::execute_internal(&rerooted_path, build_config, self.dump_bytecode_as_base64)
    }

    pub fn execute_internal(
        rerooted_path: &Path,
        config: MoveBuildConfig,
        dump_bytecode_as_base64: bool,
    ) -> anyhow::Result<()> {
        let pkg = sui_framework::build_move_package(
            rerooted_path,
            BuildConfig {
                config: MoveBuildConfig {
                    test_mode: true,
                    ..config
                },
                run_bytecode_verifier: true,
                print_diags_to_stderr: true,
            },
        )?;
        if dump_bytecode_as_base64 {
            println!("{}", json!(pkg.get_package_base64()))
        }
        Ok(())
    }
}
