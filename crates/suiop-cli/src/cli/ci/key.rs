// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::cli::lib::{get_api_server, get_oauth_token};
use anyhow::Result;

use clap::Parser;
use colored::Colorize;
use tracing::debug;

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
    pub_key: Option<String>,
    message: String,
}

async fn send_key_request(token: &str, action: &KeyAction) -> Result<()> {
    let req = generate_key_request(token, action);

    println!(
        "Processing request... Please wait patiently. It may take about 20 seconds to complete."
    );
    let resp = req.send().await?;
    debug!("resp: {:?}", resp);

    let status = resp.status();
    let json_resp = resp.json::<KeyResponse>().await?;

    if status.is_success() {
        match action {
            KeyAction::Create { repo_name } | KeyAction::ReCreate { repo_name } => {
                if let Some(pubkey) = json_resp.pub_key {
                    let add_key_link = format!(
                        "https://github.com/MystenLabs/{}/settings/keys/new",
                        repo_name
                    );
                    println!(
                        r#"Public Key:
-------------------------
{}
-------------------------
Please add the public key above to your repository {} via the link below:
{}"#,
                        pubkey.yellow(),
                        repo_name.bright_purple(),
                        add_key_link.yellow()
                    )
                } else {
                    return Err(anyhow::anyhow!(
                        "Failed to get public key for repo {}.",
                        repo_name.bright_purple()
                    ));
                }
            }
            KeyAction::Delete { repo_name } => {
                println!("Key for repo {} deleted", repo_name.bright_purple());
            }
        }
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "Failed to manage keys: {} - {}",
            status,
            json_resp.message.yellow()
        ))
    }
}

fn generate_headers_with_auth(token: &str) -> reqwest::header::HeaderMap {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::AUTHORIZATION,
        reqwest::header::HeaderValue::from_str(&format!("Bearer {}", token)).unwrap(),
    );
    headers
}

fn generate_key_request(token: &str, action: &KeyAction) -> reqwest::RequestBuilder {
    let client = reqwest::Client::new();
    let api_server = get_api_server();
    let full_url = format!("{}{}", api_server, ENDPOINT);
    debug!("full_url: {}", full_url);
    let req = match action {
        KeyAction::Create { repo_name } => client
            .post(full_url)
            .headers(generate_headers_with_auth(token))
            .json(&KeyRequest {
                repo_name: repo_name.to_string(),
            }),
        KeyAction::ReCreate { repo_name } => client
            .put(full_url)
            .headers(generate_headers_with_auth(token))
            .json(&KeyRequest {
                repo_name: repo_name.to_string(),
            }),
        KeyAction::Delete { repo_name } => client
            .delete(full_url)
            .headers(generate_headers_with_auth(token))
            .json(&KeyRequest {
                repo_name: repo_name.to_string(),
            }),
    };
    debug!("req: {:?}", req);

    req
}
