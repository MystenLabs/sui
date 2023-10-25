// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use move_cli::base::new;
use std::path::PathBuf;

const SUI_PKG_NAME: &str = "Sui";

// Use testnet by default. Probably want to add options to make this configurable later
const SUI_PKG_PATH: &str = "{ git = \"https://github.com/MystenLabs/sui.git\", subdir = \"crates/sui-framework/packages/sui-framework\", rev = \"framework/testnet\" }";

/// Create a new Move package with name `name` at `path`. If `path` is not provided the package
/// will be created in the directory `name`.
///
/// By default, this command allows a strict naming scheme based on this regex: [A-Za-z][A-Za-z0-9-_]*, and it will replace hyphens (-) with underscore (_) in the address field in the Move.toml file.
#[derive(Parser)]
#[group(id = "sui-move-new")]
pub struct New {
    #[clap(flatten)]
    pub new: new::New,
}

impl New {
    pub fn execute(self, path: Option<PathBuf>) -> anyhow::Result<()> {
        let name = &self.new.name.to_lowercase();
        self.new
            .execute(path, [(SUI_PKG_NAME, SUI_PKG_PATH)], [(name, "0x0")], "")?;
        Ok(())
    }
}
