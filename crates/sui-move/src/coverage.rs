// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use move_cli::base::coverage;
use move_package_alt_compilation::build_config::BuildConfig;
use std::path::Path;
use sui_package_alt::SuiFlavor;

#[derive(Parser)]
#[group(id = "sui-move-coverage")]
pub struct Coverage {
    #[clap(flatten)]
    pub coverage: coverage::Coverage,
}

impl Coverage {
    pub async fn execute(
        self,
        path: Option<&Path>,
        build_config: BuildConfig,
    ) -> anyhow::Result<()> {
        self.coverage
            .execute::<SuiFlavor>(path, build_config)
            .await?;
        Ok(())
    }
}
