// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use crate::{
    compilation::{build_config::BuildConfig, compiled_package::compile},
    errors::PackageResult,
    flavor::Vanilla,
    package::RootPackage,
};
use clap::Parser;

/// Build the package
#[derive(Debug, Clone, Parser)]
pub struct Build {
    /// Path to the project
    #[arg(name = "path", short = 'p', long = "path", default_value = ".")]
    path: Option<PathBuf>,

    #[arg(
        name = "env",
        short = 'e',
        long = "environment",
        default_value = "testnet"
    )]
    env: String,

    #[command(flatten)]
    build_config: BuildConfig,
}

impl Build {
    pub async fn execute(&self) -> PackageResult<()> {
        let path = self.path.clone().unwrap_or_else(|| PathBuf::from("."));

        let root_pkg = RootPackage::<Vanilla>::load(path, None).await?;
        compile::<Vanilla>(&root_pkg, &self.build_config, &self.env)
            .await
            .unwrap();

        Ok(())
    }
}
