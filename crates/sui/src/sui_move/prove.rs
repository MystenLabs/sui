// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use move_cli::base::prove;
use move_package::BuildConfig;
use std::path::PathBuf;

#[derive(Parser)]
pub struct Prove {
    #[clap(flatten)]
    pub prove: prove::Prove,
}

impl Prove {
    pub fn execute(self, path: Option<PathBuf>, build_config: BuildConfig) -> anyhow::Result<()> {
        self.prove.execute(path, build_config)?;
        Ok(())
    }
}
