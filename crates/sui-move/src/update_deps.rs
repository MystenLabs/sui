// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;

use move_cli::base::{reroot_path, update_deps};
use move_package_alt_compilation::build_config::BuildConfig;

use std::path::Path;

use sui_move_build::find_environment;
use sui_package_alt::SuiFlavor;

#[derive(Parser)]
#[group(id = "sui-move-update-deps")]
pub struct UpdateDeps {
    #[clap(flatten)]
    pub update_deps: update_deps::UpdateDeps,
}

impl UpdateDeps {
    pub async fn execute(
        self,
        path: Option<&Path>,
        build_config: BuildConfig,
    ) -> anyhow::Result<()> {
        let path = reroot_path(path)?;
        let env = find_environment(None, None, &build_config, &path)?;
        self.update_deps
            .execute::<SuiFlavor>(Some(&path), &build_config, env)
            .await
    }
}
