// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use std::path::Path;

#[cfg(feature = "prove")]
pub mod prove;

#[derive(Parser)]
pub enum Command {
    #[cfg(feature = "prove")]
    #[command(about = "Verify specs")]
    Prove(prove::Prove),
}

pub fn execute_command(
    package_path: Option<&Path>,
    command: Command,
) -> anyhow::Result<()> {
    match command {
        #[cfg(feature = "prove")]
        Command::Prove(c) => c.execute(package_path),
    }
}
