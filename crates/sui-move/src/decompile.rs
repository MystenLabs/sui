// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use move_cli::base::decompile;
use move_package_alt_compilation::build_config::BuildConfig;
use std::path::Path;

#[derive(Parser)]
#[group(id = "sui-move-decompile")]
pub struct Decompile {
    #[clap(flatten)]
    pub decompile: decompile::Decompile,
}

impl Decompile {
    pub fn execute(self, path: Option<&Path>, build_config: BuildConfig) -> anyhow::Result<()> {
        self.decompile.execute(path, build_config)
    }
}
