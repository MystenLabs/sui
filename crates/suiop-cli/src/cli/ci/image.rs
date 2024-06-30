// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::cli::lib::{get_api_server, get_oauth_token};
use anyhow::Result;

use chrono::{DateTime, Local, Utc};
use clap::{Parser, ValueEnum};
use colored::Colorize;
use serde::{self, Deserialize, Serialize};
use std::{fmt::Display, str::FromStr};
use tabled::{settings::Style, Table, Tabled};
use tracing::debug;

#[derive(Tabled)]
struct BuildInfo {
    name: String,
    status: String,
    #[tabled(rename = "Start Time (Local Time)")]
    start_time: String,
    #[tabled(rename = "End Time")]
    end_time: String,
}

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

impl Display for RefType {
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
        /// The name of the git repository within the mystenlabs org
        #[arg(short, long)]
        repo_name: String,
        /// The path to the dockerfile within the source code repository given by `--repo_name`
        #[arg(short, long)]
        dockerfile: String,
        /// Optional image tag to use, by default the image is tagged with code repo commit SHA & "latest"
        #[arg(long)]
        image_tag: Option<String>,
        /// Optional image name, default to "app", only used if multiple images are built within one repo
        #[arg(long)]
        image_name: Option<String>,
        /// Optioanl reference type, default to "branch"
        #[arg(long)]
        ref_type: Option<RefType>,
        /// Optional reference value, default to "main"
        #[arg(long)]
        ref_val: Option<String>,
        /// Optional cpu resource request, default to "2"
        #[arg(long)]
        cpu: Option<String>,
        /// Optional memory resource request, default to "4Gi"
        #[arg(long)]
        memory: Option<String>,
        /// Optional disk resource request, default to "10Gi"
        #[arg(long)]
        disk: Option<String>,
        /// Optional build args to pass to the docker build command
        #[arg(long)]
        build_args: Vec<String>,
    },
    #[command(name = "query")]
    Query {
        #[arg(short, long)]
        repo_name: String,
        #[arg(short, long)]
        limit: Option<u32>,
    },
    #[command(name = "status")]
    Status {
        #[arg(short = 'r', long)]
        repo_name: String,
        #[arg(short = 'i', long)]
        image_name: String,
        #[arg(short = 't', long)]
        ref_type: Option<RefType>,
        #[arg(short = 'v', long)]
        ref_val: Option<String>,
    },
    #[command(name = "list")]
    List {
        #[arg(short, long)]
        repo_name: String,
        #[arg(short, long)]
        image_name: Option<String>,
        #[arg(short, long)]
        limit: Option<i32>,
    },
}

#[derive(serde::Serialize, Debug)]
struct RequestBuildRequest {
    repo_name: String,
    dockerfile: String,
    image_name: Option<String>,
    image_tag: Option<String>,
    ref_type: Option<RefType>,
    ref_val: Option<String>,
    cpu: Option<String>,
    memory: Option<String>,
    disk: Option<String>,
    build_args: Vec<String>,
}

#[derive(serde::Serialize)]
struct QueryBuildsRequest {
    repo_name: String,
    limit: u32,
}

#[derive(serde::Serialize)]
struct ImageStatusRequest {
    repo_name: String,
    image_name: String,
    repo_ref_type: RefType,
    repo_ref: String,
}

const ENDPOINT: &str = "/automation/image-build";
const STATUS_ENDPOINT: &str = "/automation/image-status";
const LIST_ENDPOINT: &str = "/automation/images";

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

#[derive(ValueEnum, Clone, Debug)]
// #[clap(rename_all = "snake_case")]
enum ImageStatus {
    Found,
    Pending,
    Building,
    BuiltNotFound,
    Failed,
    Unknown,
    NotBuiltNotFound,
}

impl<'a> Deserialize<'a> for ImageStatus {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'a>,
    {
        let s = i32::deserialize(deserializer)?;
        match s {
            0 => Ok(ImageStatus::Found),
            1 => Ok(ImageStatus::Pending),
            2 => Ok(ImageStatus::Building),
            3 => Ok(ImageStatus::BuiltNotFound),
            4 => Ok(ImageStatus::Failed),
            5 => Ok(ImageStatus::Unknown),
            6 => Ok(ImageStatus::NotBuiltNotFound),
            _ => Ok(ImageStatus::Unknown),
        }
    }
}

impl Display for ImageStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImageStatus::Found => write!(f, "Found"),
            ImageStatus::Pending => write!(f, "Pending"),
            ImageStatus::Building => write!(f, "Building"),
            ImageStatus::BuiltNotFound => {
                write!(f, "Build succeed but image not found")
            }
            ImageStatus::Failed => write!(f, "Failed"),
            ImageStatus::Unknown => write!(f, "Unknown"),
            ImageStatus::NotBuiltNotFound => {
                write!(f, "Not built and image not found")
            }
        }
    }
}

#[derive(serde::Deserialize)]
struct ImageStatusResponse {
    pub status: ImageStatus,
    pub image_sha: String,
}

#[derive(serde::Serialize)]
struct ImageListRequest {
    repo_name: String,
    image_name: Option<String>,
    limit: Option<i32>,
}

#[derive(serde::Deserialize)]
struct ImageDetails {
    pub name: String,
    pub tags: Vec<String>,
}

#[derive(Tabled)]
struct ImageRow {
    name: String,
    tags: String,
}

#[derive(serde::Deserialize)]
struct ImageListResponse {
    pub images: Vec<ImageDetails>,
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
                cpu: _,
                memory: _,
                disk: _,
                build_args: _,
            } => {
                let ref_type = ref_type.clone().unwrap_or(RefType::Branch);
                let ref_val = ref_val.clone().unwrap_or("main".to_string());
                let ref_name = format!("{}:{}", ref_type, ref_val);
                let image_name = image_name.clone().unwrap_or("app".to_string());
                let image_tag = image_tag.clone().unwrap_or("".to_string());
                let mut image_info = image_name;
                if !image_tag.is_empty() {
                    image_info += &format!(":{}", image_tag);
                }
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
                println!(
                    "Build Job Start Time: {}",
                    utc_to_local_time(json_resp.start_time).green()
                );
            }
            ImageAction::Query {
                repo_name,
                limit: _,
            } => {
                println!("Requested query for repo: {}", repo_name.green());
                let json_resp = resp.json::<QueryBuildResponse>().await?;
                let job_statuses = json_resp.pods.into_iter().map(|pod| {
                    // Parse the string into a NaiveDateTime
                    let start_time = utc_to_local_time(pod.start_time);
                    let end_time = utc_to_local_time(pod.end_time.unwrap_or("".to_string()));

                    BuildInfo {
                        name: pod.name,
                        status: pod.status,
                        start_time,
                        end_time,
                    }
                });
                let mut tabled = Table::new(job_statuses);
                tabled.with(Style::rounded());

                let tabled_str = tabled.to_string();
                println!("{}", tabled_str);
            }
            ImageAction::Status {
                repo_name,
                image_name,
                ref_type,
                ref_val,
            } => {
                let mut ref_name = "".to_string();
                if let Some(ref_type) = ref_type {
                    ref_name.push_str(&ref_type.to_string())
                } else {
                    ref_name.push_str("branch");
                }
                if let Some(ref_val) = ref_val {
                    ref_name.push_str(&format!(":{}", ref_val))
                } else {
                    ref_name.push_str(":main")
                }
                println!(
                    "Requested status for repo: {}, image: {}, ref: {}",
                    repo_name.green(),
                    image_name.green(),
                    ref_name.green()
                );
                // println!("resp: {:?}", resp.text().await?);

                let json_resp = resp.json::<ImageStatusResponse>().await?;
                println!("Image Status: {}", json_resp.status.to_string().green());
                println!("Image SHA: {}", json_resp.image_sha.green());
            }
            ImageAction::List {
                repo_name,
                image_name: _,
                limit: _,
            } => {
                println!("Requested list for repo: {}", repo_name.green());
                let json_resp = resp.json::<ImageListResponse>().await?;
                let details = json_resp.images.into_iter().map(|image| {
                    let image_name = image.name;
                    let image_tags = image.tags;
                    ImageRow {
                        name: image_name,
                        // convert images tags vec to multiple strings
                        tags: image_tags.join(" | "),
                    }
                });
                let mut tabled = Table::new(details);
                tabled.with(Style::rounded());

                let tabled_str = tabled.to_string();
                println!("{}", tabled_str);
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

fn utc_to_local_time(utc_time: String) -> String {
    if utc_time.is_empty() {
        return utc_time;
    }
    let utc_time_result =
        DateTime::<Utc>::from_str(&format!("{}T{}Z", &utc_time[..10], &utc_time[11..19]));
    if let Ok(utc_time) = utc_time_result {
        let local_time = utc_time.with_timezone(&Local);
        local_time.format("%Y-%m-%d %H:%M:%S").to_string()
    } else {
        utc_time.to_string()
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
    let req = match action {
        ImageAction::Build {
            repo_name,
            dockerfile,
            image_name,
            image_tag,
            ref_type,
            ref_val,
            cpu,
            memory,
            disk,
            build_args,
        } => {
            let full_url = format!("{}{}", api_server, ENDPOINT);
            debug!("full_url: {}", full_url);
            let req = client.post(full_url);
            let body = RequestBuildRequest {
                repo_name: repo_name.clone(),
                dockerfile: dockerfile.clone(),
                image_name: image_name.clone(),
                image_tag: image_tag.clone(),
                ref_type: ref_type.clone(),
                ref_val: ref_val.clone(),
                cpu: cpu.clone(),
                memory: memory.clone(),
                disk: disk.clone(),
                build_args: build_args.clone(),
            };
            debug!("req body: {:?}", body);
            req.json(&body).headers(generate_headers_with_auth(token))
        }
        ImageAction::Query { repo_name, limit } => {
            let full_url = format!("{}{}", api_server, ENDPOINT);
            debug!("full_url: {}", full_url);
            let req = client.get(full_url);
            let limit = (*limit).unwrap_or(10);
            let query = QueryBuildsRequest {
                repo_name: repo_name.clone(),
                limit,
            };
            req.query(&query).headers(generate_headers_with_auth(token))
        }
        ImageAction::Status {
            repo_name,
            image_name,
            ref_type,
            ref_val,
        } => {
            let full_url = format!("{}{}", api_server, STATUS_ENDPOINT);
            debug!("full_url: {}", full_url);
            let req = client.get(full_url);
            let ref_type = ref_type.clone().unwrap_or(RefType::Branch);
            let ref_val = ref_val.clone().unwrap_or("main".to_string());
            let query = ImageStatusRequest {
                repo_name: repo_name.clone(),
                image_name: image_name.clone(),
                repo_ref_type: ref_type,
                repo_ref: ref_val,
            };
            req.query(&query).headers(generate_headers_with_auth(token))
        }
        ImageAction::List {
            repo_name,
            image_name,
            limit,
        } => {
            let full_url = format!("{}{}", api_server, LIST_ENDPOINT);
            debug!("full_url: {}", full_url);
            let req = client.get(full_url);
            let query = ImageListRequest {
                repo_name: repo_name.clone(),
                image_name: image_name.clone(),
                limit: *limit,
            };
            req.query(&query).headers(generate_headers_with_auth(token))
        }
    };
    debug!("req: {:?}", req);

    req
}
