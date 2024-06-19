// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::cli::lib::{get_api_server, get_oauth_token};
use anyhow::Result;

use clap::{Parser, ValueEnum};
use colored::Colorize;
use serde::{self, Serialize};
use tracing::debug;

#[derive(Parser, Debug)]
pub struct ImageArgs {
    #[command(subcommand)]
    action: ImageAction,
}

#[derive(ValueEnum, Clone, Debug)]
#[clap(rename_all = "lowercase")]
pub enum RefType {
    Branch,
    Tag,
    Commit,
}

impl Serialize for RefType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(match self {
            RefType::Branch => "branch",
            RefType::Tag => "tag",
            RefType::Commit => "commit",
        })
    }
}

impl std::fmt::Display for RefType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RefType::Branch => write!(f, "branch"),
            RefType::Tag => write!(f, "tag"),
            RefType::Commit => write!(f, "commit"),
        }
    }
}

#[derive(clap::Subcommand, Debug)]
pub enum ImageAction {
    #[command(name = "build")]
    Build {
        #[arg(short, long)]
        repo_name: String,
        #[arg(short, long)]
        dockerfile: String,
        #[arg(long)]
        image_tag: Option<String>,
        #[arg(long)]
        image_name: Option<String>,
        #[arg(long)]
        ref_type: Option<RefType>,
        #[arg(long)]
        ref_val: Option<String>,
    },
    #[command(name = "query")]
    Query {
        #[arg(short, long)]
        repo_name: String,
        #[arg(short, long)]
        limit: Option<u32>,
    },
}

#[derive(serde::Serialize, Debug)]
struct RequestBuildRequest {
    repo_name: String,
    dockerfile: String,
    image_tag: Option<String>,
    image_name: Option<String>,
    ref_type: Option<RefType>,
    ref_val: Option<String>,
}

#[derive(serde::Serialize)]
struct QueryBuildsRequest {
    repo_name: String,
    limit: u32,
}

const ENDPOINT: &str = "/automation/image-build";

pub async fn image_cmd(args: &ImageArgs) -> Result<()> {
    let token = get_oauth_token().await?;
    debug!("token: {}", token.access_token);
    send_image_request(&token.access_token, &args.action).await?;

    Ok(())
}

#[derive(serde::Deserialize)]
struct JobStatus {
    name: String,
    status: String,
    start_time: String,
    end_time: Option<String>,
}

#[derive(serde::Deserialize)]
struct QueryBuildResponse {
    pods: Vec<JobStatus>,
}

async fn send_image_request(token: &str, action: &ImageAction) -> Result<()> {
    let req = generate_image_request(token, action);

    let resp = req.send().await?;
    debug!("resp: {:?}", resp);

    let status = resp.status();

    if status.is_success() {
        match action {
            ImageAction::Build {
                repo_name,
                dockerfile,
                image_name,
                image_tag,
                ref_type,
                ref_val,
            } => {
                let ref_type = ref_type.clone().unwrap_or(RefType::Branch);
                let ref_val = ref_val.clone().unwrap_or("main".to_string());
                let ref_name = format!("{}:{}", ref_type, ref_val);
                let image_name = image_name.clone().unwrap_or(repo_name.clone());
                let image_tag = image_tag.clone().unwrap_or("latest".to_string());
                let image_info = format!("{}:{}", image_name, image_tag);
                println!(
                    "Requested built image for repo: {}, ref: {}, dockerfile: {}, image: {}",
                    repo_name.green(),
                    ref_name.green(),
                    dockerfile.green(),
                    image_info.green()
                );
                let json_resp = resp.json::<JobStatus>().await?;
                println!("Build Job Status: {}", json_resp.status.green());
                println!("Build Job Name: {}", json_resp.name.green());
                println!("Build Job Start Time: {}", json_resp.start_time.green());
            }
            ImageAction::Query {
                repo_name,
                limit: _,
            } => {
                println!("Requested query for repo: {}", repo_name.green());
                let json_resp = resp.json::<QueryBuildResponse>().await?;
                for pod in json_resp.pods {
                    if let Some(end_time) = pod.end_time {
                        println!(
                            "Job Name: {}, Status: {}, Start Time: {}, End Time: {}",
                            pod.name.green(),
                            pod.status.green(),
                            pod.start_time.green(),
                            end_time.green()
                        );
                    } else {
                        println!(
                            "Job Name: {}, Status: {}, Start Time: {}",
                            pod.name.green(),
                            pod.status.green(),
                            pod.start_time.green()
                        );
                    }
                }
            }
        }
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "Failed to run image build request. Status: {} - {}",
            status,
            resp.text().await?
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

fn generate_image_request(token: &str, action: &ImageAction) -> reqwest::RequestBuilder {
    let client = reqwest::Client::new();
    let api_server = get_api_server();
    let full_url = format!("{}{}", api_server, ENDPOINT);
    debug!("full_url: {}", full_url);
    let req = match action {
        ImageAction::Build {
            repo_name,
            dockerfile,
            image_name,
            image_tag,
            ref_type,
            ref_val,
        } => {
            let req = client.post(full_url);
            let body = RequestBuildRequest {
                repo_name: repo_name.clone(),
                dockerfile: dockerfile.clone(),
                image_name: image_name.clone(),
                image_tag: image_tag.clone(),
                ref_type: ref_type.clone(),
                ref_val: ref_val.clone(),
            };
            debug!("req body: {:?}", body);
            req.json(&body).headers(generate_headers_with_auth(token))
        }
        ImageAction::Query { repo_name, limit } => {
            let req = client.get(full_url);
            let limit = (*limit).unwrap_or(10);
            let query = QueryBuildsRequest {
                repo_name: repo_name.clone(),
                limit,
            };
            req.query(&query).headers(generate_headers_with_auth(token))
        }
    };
    debug!("req: {:?}", req);

    req
}
