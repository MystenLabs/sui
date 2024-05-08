// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod key;

use anyhow::Result;
use key::key_cmd;

use clap::Parser;

use self::key::KeyArgs;

#[derive(Parser, Debug)]
pub struct CIArgs {
    #[command(subcommand)]
    action: CIAction,
}

#[derive(clap::Subcommand, Debug)]
pub(crate) enum CIAction {
    #[clap(aliases = ["k", "key"])]
    Keys(KeyArgs),
}

pub async fn ci_cmd(args: &CIArgs) -> Result<()> {
    match &args.action {
        CIAction::Keys(keys) => key_cmd(keys).await?,
    }

    Ok(())
}
