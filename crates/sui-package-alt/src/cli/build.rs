// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use crate::SuiFlavor;
use clap::{Command, Parser, Subcommand};
use move_package_alt::{
    errors::PackageResult,
    package::{Package, RootPackage},
};
use move_package_alt_compilation::{
    build_config::BuildConfig, compile_package, compiled_package::compile, lint_flag::LintFlag,
};

/// Build the package
#[derive(Debug, Clone, Parser)]
pub struct Build {
    /// Path to the project
    #[arg(name = "path", short = 'p', long = "path", default_value = ".")]
    path: Option<PathBuf>,

    #[arg(name = "path", short = 'p', long = "path", default_value = "testnet")]
    env: String,

    #[command(flatten)]
    build_config: BuildConfig,
}

impl Build {
    pub async fn execute(&self) -> PackageResult<()> {
        let path = self.path.clone().unwrap_or_else(|| PathBuf::from("."));

        compile_package::<std::io::Write, SuiFlavor>(
            &path.as_path(),
            self.build_config,
            std::io::stdout(),
        )
        .await?;
        // compile::<SuiFlavor>(None, &path.as_path(), &self.build_config)
        //     .await
        //     .unwrap();

        Ok(())
    }
}
