// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod image;
mod key;

use anyhow::Result;
use image::{image_cmd, ImageArgs};
use key::{key_cmd, KeyArgs};

use clap::Parser;

#[derive(Parser, Debug)]
pub struct CIArgs {
    #[command(subcommand)]
    action: CIAction,
}

#[derive(clap::Subcommand, Debug)]
pub(crate) enum CIAction {
    #[clap(aliases = ["k", "key"])]
    Keys(KeyArgs),
    #[clap(aliases = ["i", "image"])]
    Image(ImageArgs),
}

pub async fn ci_cmd(args: &CIArgs) -> Result<()> {
    match &args.action {
        CIAction::Keys(keys) => key_cmd(keys).await?,
        CIAction::Image(image) => image_cmd(image).await?,
    }

    Ok(())
}
