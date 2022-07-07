// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use move_cli::base::disassemble;
use move_package::BuildConfig;
use std::path::PathBuf;

#[derive(Parser)]
pub struct Disassemble {
    #[clap(flatten)]
    pub disassemble: disassemble::Disassemble,
}

impl Disassemble {
    pub fn execute(self, path: Option<PathBuf>, build_config: BuildConfig) -> anyhow::Result<()> {
        self.disassemble.execute(path, build_config)?;
        Ok(())
    }
}
