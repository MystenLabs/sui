// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use move_cli::base::migrate;
use move_package_alt_compilation::build_config::BuildConfig;
use std::path::Path;
use sui_package_alt::SuiFlavor;

#[derive(Parser)]
#[group(id = "sui-move-migrate")]
pub struct Migrate {
    #[clap(flatten)]
    pub migrate: migrate::Migrate,
}

impl Migrate {
    pub async fn execute(self, path: Option<&Path>, config: BuildConfig) -> anyhow::Result<()> {
        self.migrate.execute::<SuiFlavor>(path, config).await
    }
}
