// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::cli::lib::{get_api_server, get_oauth_token};
use anyhow::Result;

use chrono::{DateTime, Local, Utc};
use clap::{Parser, ValueEnum};
use colored::Colorize;
use crossterm::{
    cursor::MoveTo,
    event::{Event, EventStream, KeyCode},
    terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType},
    ExecutableCommand,
};
use futures::StreamExt;
use futures::{select, FutureExt};
use serde::{self, Deserialize, Serialize};
use std::{fmt::Display, str::FromStr, time::Duration};
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
    pub action: ImageAction,
}

#[derive(ValueEnum, Clone, Debug)]
#[clap(rename_all = "lowercase")]
pub enum RefType {
    Branch,
    Tag,
    Commit,
}

#[derive(ValueEnum, Clone, Debug)]
#[clap(rename_all = "lowercase")]
pub enum BuildMode {
    Light,
    Moderate,
    Beast,
}

#[derive(ValueEnum, Clone, Debug)]
pub enum RepoRegion {
    #[clap(name = "us-central1")]
    UsCentral1,
    #[clap(name = "us-west1")]
    UsWest1,
    #[clap(name = "us-east1")]
    UsEast1,
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

#[derive(Parser, Debug)]
pub struct ImageBuildArgs {
    /// The name of the git repository within the mystenlabs org
    #[arg(short, long)]
    repo_name: String,
    /// The path to the dockerfile within the source code repository given by `--repo_name`
    #[arg(short, long)]
    dockerfile: String,
    /// Optional repo region, default to "us-central1"
    #[arg(long)]
    repo_region: Option<RepoRegion>,
    /// Optional image tags, default to ""
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
    /// Optional mode of the build, default to "light"
    #[arg(long)]
    build_mode: Option<BuildMode>,
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
    /// Optional flag to force build even if build pod already exists
    #[arg(short = 'f', long)]
    force: bool,
    /// Optional flag to target the image, used for multi-stage builds
    #[arg(short = 't', long)]
    image_target: Option<String>,
    /// Optional arg to speciy the org to build the image for, default to "mystenlabs"
    #[arg(short = 'o', long)]
    org: Option<String>,
}

#[derive(Parser, Debug)]
pub struct ImageQueryArgs {
    #[arg(short, long)]
    repo_name: String,
    #[arg(short, long)]
    watch: bool,
    #[arg(short, long)]
    limit: Option<u32>,
}

#[derive(clap::Subcommand, Debug)]
pub enum ImageAction {
    #[command(name = "build")]
    Build(ImageBuildArgs),
    #[command(name = "query")]
    Query(ImageQueryArgs),
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
    #[command(name = "list", aliases = &["ls"])]
    List {
        #[arg(short, long, aliases = &["repo"])]
        repo_name: String,
        #[arg(short, long, aliases = &["image"])]
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
    repo_region: String,
    ref_type: Option<RefType>,
    ref_val: Option<String>,
    cpu: String,
    memory: String,
    disk: String,
    build_args: Vec<String>,
    force: bool,
    image_target: Option<String>,
    org: String,
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

async fn get_status_table(resp: reqwest::Response) -> Result<tabled::Table> {
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
    Ok(tabled.with(Style::rounded()).to_owned())
}

pub async fn send_image_request(token: &str, action: &ImageAction) -> Result<()> {
    let req = generate_image_request(token, action);

    let resp = req.send().await?;
    debug!("resp: {:?}", resp);

    let status = resp.status();

    if status.is_success() {
        match action {
            ImageAction::Build(ImageBuildArgs {
                repo_name,
                dockerfile,
                image_name,
                image_tag,
                ref_type,
                ref_val,
                repo_region: _,
                build_mode: _,
                cpu: _,
                memory: _,
                disk: _,
                build_args: _,
                force: _,
                image_target,
                org: _,
            }) => {
                let ref_type = ref_type.clone().unwrap_or(RefType::Branch);
                let ref_val = ref_val.clone().unwrap_or("main".to_string());
                let ref_name = format!("{}:{}", ref_type, ref_val);
                let image_name = image_name.clone().unwrap_or("app".to_string());
                let image_tag = image_tag.clone().unwrap_or("".to_string());
                let mut image_info = image_name;
                if !image_tag.is_empty() {
                    image_info += &format!(":{}", image_tag);
                }
                if !image_target.is_none() {
                    image_info += &format!("@{}", image_target.as_ref().unwrap());
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
            ImageAction::Query(ImageQueryArgs {
                repo_name,
                watch,
                limit: _,
            }) => {
                if !*watch {
                    println!("Requested query for repo: {}", repo_name.green());
                    let status_table = get_status_table(resp).await?.to_string();
                    println!("{}", status_table);
                } else {
                    enable_raw_mode()?;
                    loop {
                        let mut reader = EventStream::new();
                        let mut delay = futures_timer::Delay::new(Duration::from_secs(1)).fuse();
                        let mut event = reader.next().fuse();

                        select! {
                            _ = delay => {
                                let req = generate_image_request(token, action);

                                let resp = req.send().await?;
                                let status_table = get_status_table(resp).await?.to_string();
                                std::io::stdout().execute(Clear(ClearType::All))?.execute(MoveTo(0,0))?;
                                print!("press 'q' or 'esc' to quit");
                                for (i, line )in status_table.lines().enumerate() {
                                    std::io::stdout().execute(MoveTo(0,(i + 1) as u16))?;
                                    println!("{}", line);
                                }
                            },
                            maybe_event = event => {
                                println!("checking event");
                                match maybe_event {
                                    Some(Ok(event)) => {
                                        if event == Event::Key(KeyCode::Char('q').into()) {
                                            std::io::stdout().execute(Clear(ClearType::All))?.execute(MoveTo(0,0))?;
                                            println!("q pressed, quitting 🫡");
                                            break
                                        } else if event == Event::Key(KeyCode::Esc.into()) {
                                            std::io::stdout().execute(Clear(ClearType::All))?.execute(MoveTo(0,0))?;
                                            println!("esc pressed, quitting 🫡");
                                            break;
                                        }

                                    }
                                    Some(Err(e)) => println!("Error: {:?}\r", e),
                                    None => println!("no event"),
                                }
                            }
                        };
                    }
                    disable_raw_mode()?;
                }
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
        ImageAction::Build(ImageBuildArgs {
            repo_name,
            dockerfile,
            image_name,
            repo_region,
            image_tag,
            ref_type,
            ref_val,
            build_mode,
            cpu,
            memory,
            disk,
            build_args,
            force,
            image_target,
            org,
        }) => {
            let full_url = format!("{}{}", api_server, ENDPOINT);
            debug!("full_url: {}", full_url);
            let req = client.post(full_url);
            let mut cpu = cpu.clone().unwrap_or("2".to_string());
            let mut memory = memory.clone().unwrap_or("4Gi".to_string());
            let mut disk = disk.clone().unwrap_or("20Gi".to_string());
            if let Some(build_mode) = build_mode {
                match build_mode {
                    BuildMode::Light => {
                        cpu = "2".to_string();
                        memory = "4Gi".to_string();
                        disk = "20Gi".to_string();
                    }
                    BuildMode::Moderate => {
                        cpu = "4".to_string();
                        memory = "8Gi".to_string();
                        disk = "40Gi".to_string();
                    }
                    BuildMode::Beast => {
                        cpu = "6".to_string();
                        memory = "16Gi".to_string();
                        disk = "40Gi".to_string();
                    }
                }
            }
            let mut region = "us-central1".to_string();
            if let Some(repo_region) = repo_region {
                match repo_region {
                    RepoRegion::UsCentral1 => region = "us-central1".to_string(),
                    RepoRegion::UsWest1 => region = "us-west1".to_string(),
                    RepoRegion::UsEast1 => region = "us-east1".to_string(),
                }
            }
            let body = RequestBuildRequest {
                repo_name: repo_name.clone(),
                dockerfile: dockerfile.clone(),
                image_name: image_name.clone(),
                image_tag: image_tag.clone(),
                ref_type: ref_type.clone(),
                ref_val: ref_val.clone(),
                repo_region: region,
                cpu,
                memory,
                disk,
                build_args: build_args.clone(),
                force: *force,
                image_target: image_target.clone(),
                org: org.clone().unwrap_or("mystenlabs".to_string()),
            };
            debug!("req body: {:?}", body);
            req.json(&body).headers(generate_headers_with_auth(token))
        }
        ImageAction::Query(ImageQueryArgs {
            repo_name,
            limit,
            watch: _,
        }) => {
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
