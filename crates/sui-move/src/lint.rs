// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use move_cli::base::lint;
use move_package_alt_compilation::build_config::BuildConfig;
use std::path::Path;
use sui_package_alt::SuiFlavor;

#[derive(Parser)]
#[group(id = "sui-move-lint")]
pub struct Lint {
    #[clap(flatten)]
    pub lint: lint::Lint,
}

impl Lint {
    pub async fn execute(
        self,
        path: Option<&Path>,
        build_config: BuildConfig,
        flavor: SuiFlavor,
    ) -> anyhow::Result<()> {
        self.lint.execute(path, build_config, flavor).await
    }
}
