// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod whoami;

use crate::cli::lib::{get_api_server, get_oauth_token};
use anyhow::Result;
use clap::Parser;
use colored::Colorize;
use tracing::error;

#[derive(Parser, Debug, Clone)]
pub struct IAMArgs {
    #[command(subcommand)]
    action: IAMAction,
}

#[derive(clap::Subcommand, Debug, Clone)]
pub enum IAMAction {
    #[command(name = "whoami", aliases=["w"])]
    WhoAmI,
}

pub async fn iam_cmd(args: &IAMArgs) -> Result<()> {
    match &args.action {
        IAMAction::WhoAmI => {
            let token_resp = get_oauth_token().await;
            match token_resp {
                Ok(token) => {
                    let resp = whoami::get_identity(&get_api_server(), &token.access_token).await;
                    match resp {
                        Ok(username) => {
                            println!("You are: {}", username.bright_purple());
                            Ok(())
                        }
                        Err(e) => {
                            error!("Failed to get username: {}", e);
                            Err(e)
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to get token: {}", e);
                    Err(e)
                }
            }
        }
    }
}
