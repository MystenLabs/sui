// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use move_cli::base::{self, build};
use move_package::BuildConfig;
use std::path::{Path, PathBuf};

#[derive(Parser)]
pub struct Build {
    #[clap(flatten)]
    pub build: build::Build,
    /// Whether we are printing in base64.
    #[clap(long, global = true)]
    pub dump_bytecode_as_base64: bool,
}

impl Build {
    pub fn execute(&self, path: Option<PathBuf>, build_config: BuildConfig) -> anyhow::Result<()> {
        let rerooted_path = base::reroot_path(path)?;
        Self::execute_internal(&rerooted_path, build_config, self.dump_bytecode_as_base64)
    }
    pub fn execute_internal(
        rerooted_path: &Path,
        build_config: BuildConfig,
        dump_bytecode_as_base64: bool,
    ) -> anyhow::Result<()> {
        // find manifest file directory from a given path or (if missing) from current dir
        if dump_bytecode_as_base64 {
            let compiled_modules =
                sui_framework::build_move_package_to_base64(rerooted_path, build_config)?;
            println!("{:?}", compiled_modules);
        } else {
            sui_framework::build_and_verify_package(rerooted_path, build_config)?;
        }
        Ok(())
    }
}
