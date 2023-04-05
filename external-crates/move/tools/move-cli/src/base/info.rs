// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use super::reroot_path;
use clap::*;
use move_package::BuildConfig;
use std::path::PathBuf;

/// Print address information.
#[derive(Parser)]
#[clap(name = "info")]
pub struct Info;

impl Info {
    pub fn execute(self, path: Option<PathBuf>, config: BuildConfig) -> anyhow::Result<()> {
        let rerooted_path = reroot_path(path)?;
        config
            .resolution_graph_for_package(&rerooted_path, &mut std::io::stdout())?
            .print_info()
    }
}
