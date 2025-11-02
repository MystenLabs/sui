// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;

use move_cli::base::update_deps;

use std::path::Path;
use sui_package_alt::SuiFlavor;

#[derive(Parser)]
#[group(id = "sui-move-update-deps")]
pub struct UpdateDeps {
    #[clap(flatten)]
    pub update_deps: update_deps::UpdateDeps,
}

impl UpdateDeps {
    pub async fn execute(self, path: Option<&Path>) -> anyhow::Result<()> {
        self.update_deps.execute::<SuiFlavor>(path).await
    }
}
