// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use move_cli::base::new;
use std::path::PathBuf;

#[derive(Parser)]
pub struct New {
    #[clap(flatten)]
    pub new: new::New,
}

impl New {
    pub fn execute(self, path: Option<PathBuf>) -> anyhow::Result<()> {
        let name = &self.new.name.to_lowercase();
        self.new.execute(path,
                        "0.0.1",
                        [("Sui",
                          "{ git = \"https://github.com/MystenLabs/sui.git\", subdir = \"crates/sui-framework\", rev = \"main\" }")],
                        [(name,
                          "0x0")])?;
        Ok(())
    }
}
