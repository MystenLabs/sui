// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::cli::lib::{get_oauth_token, API_SERVER};
use anyhow::Result;

use clap::Parser;
use colored::Colorize;
use tracing::{debug, info};

#[derive(Parser, Debug)]
pub struct KeyArgs {
    #[command(subcommand)]
    action: KeyAction,
}

#[derive(clap::Subcommand, Debug)]
pub enum KeyAction {
    #[command(name = "create")]
    Create {
        #[arg(short, long)]
        repo_name: String,
    },
    #[command(name = "recreate")]
    ReCreate {
        #[arg(short, long)]
        repo_name: String,
    },
    #[command(name = "delete")]
    Delete {
        #[arg(short, long)]
        repo_name: String,
    },
}

#[derive(serde::Serialize)]
struct KeyRequest {
    repo_name: String,
}

const ENDPOINT: &str = "/automation/deploy-key";

pub async fn key_cmd(args: &KeyArgs) -> Result<()> {
    let token = get_oauth_token().await?;
    debug!("token: {}", token.access_token);
    send_key_request(&token.access_token, &args.action).await?;

    Ok(())
}

#[derive(serde::Deserialize)]
struct KeyResponse {
    pub_key: String,
}

async fn send_key_request(token: &str, action: &KeyAction) -> Result<KeyResponse> {
    let full_url = format!("{}{}", API_SERVER, ENDPOINT);
    debug!("full_url: {}", full_url);
    let client = reqwest::Client::new();

    let req = match action {
        KeyAction::Create { repo_name } => client
            .post(full_url)
            .header("Authorization", format!("Bearer {}", token))
            .json::<KeyRequest>(&KeyRequest {
                repo_name: repo_name.clone(),
            }),
        KeyAction::ReCreate { repo_name } => client
            .put(full_url)
            .header("Authorization", format!("Bearer {}", token))
            .json::<KeyRequest>(&KeyRequest {
                repo_name: repo_name.clone(),
            }),
        KeyAction::Delete { repo_name } => client
            .delete(full_url)
            .header("Authorization", format!("Bearer {}", token))
            .json::<KeyRequest>(&KeyRequest {
                repo_name: repo_name.clone(),
            }),
    };

    debug!("req: {:?}", req);

    let resp = req.send().await?;
    debug!("resp: {:?}", resp);

    let status = resp.status();

    if status.is_success() {
        let json_resp = resp.json::<KeyResponse>().await?;
        match action {
            KeyAction::Create { repo_name } | KeyAction::ReCreate { repo_name } => {
                let add_key_link = format!(
                    "https://github.com/MystenLabs/{}/settings/keys/new",
                    repo_name
                );
                println!(
                    "Public Key:
-------------------------\n
{}\n
-------------------------\n
Please add the public key above to your repository {} via the link below:
{}",
                    json_resp.pub_key.yellow(),
                    repo_name.bright_purple(),
                    add_key_link.yellow()
                )
            }
            KeyAction::Delete { repo_name } => {
                info!("Key for repo {} deleted", repo_name);
            }
        }
        Ok(json_resp)
    } else {
        Err(anyhow::anyhow!(
            "Failed to manage keys, status code: {}",
            status.to_string()
        ))
    }
}
