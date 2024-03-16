// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use move_cli::base::migrate;
use move_package::BuildConfig as MoveBuildConfig;
use std::path::PathBuf;

#[derive(Parser)]
#[group(id = "sui-move-migrate")]
pub struct Migrate {
    #[clap(flatten)]
    pub migrate: migrate::Migrate,
}

impl Migrate {
    pub fn execute(self, path: Option<PathBuf>, config: MoveBuildConfig) -> anyhow::Result<()> {
        self.migrate.execute(path, config)
    }
}
