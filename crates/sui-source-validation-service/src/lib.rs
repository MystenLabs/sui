// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    fs,
    path::{Path, PathBuf},
};

use actix_web::{dev::Server, web, App, HttpRequest, HttpServer, Responder};
use serde::Deserialize;

use move_package::BuildConfig as MoveBuildConfig;
use sui_move::build::resolve_lock_file_path;
use sui_move_build::{BuildConfig, SuiPackageHooks};
use sui_sdk::wallet_context::WalletContext;
use sui_source_validation::{BytecodeSourceVerifier, SourceMode};

#[derive(Deserialize, Debug)]
pub struct Config {
    pub packages: Vec<Packages>,
}

#[derive(Deserialize, Debug)]
pub struct Packages {
    repository: String,
    paths: Vec<String>,
}

pub async fn verify_package(
    context: &WalletContext,
    package_path: impl AsRef<Path>,
) -> anyhow::Result<()> {
    move_package::package_hooks::register_package_hooks(Box::new(SuiPackageHooks));
    let config = resolve_lock_file_path(
        MoveBuildConfig::default(),
        Some(package_path.as_ref().to_path_buf()),
    )
    .unwrap();
    let build_config = BuildConfig {
        config,
        run_bytecode_verifier: false, /* no need to run verifier if code is on-chain */
        print_diags_to_stderr: false,
    };
    let compiled_package = build_config
        .build(package_path.as_ref().to_path_buf())
        .unwrap();

    let client = context.get_client().await?;
    BytecodeSourceVerifier::new(client.read_api())
        .verify_package(
            &compiled_package,
            /* verify_deps */ false,
            SourceMode::Verify,
        )
        .await
        .map_err(anyhow::Error::from)
}

pub fn parse_config(config_path: impl AsRef<Path>) -> anyhow::Result<Config> {
    let contents = fs::read_to_string(config_path)?;
    Ok(toml::from_str(&contents)?)
}

pub async fn clone_repositories(config: &Config) -> anyhow::Result<()> {
    for p in &config.packages {
        let _ = p.repository;
        let _ = p.paths;
    }
    Ok(())
}

pub async fn initialize(context: &WalletContext, config: &Config) -> anyhow::Result<()> {
    clone_repositories(config).await?;
    verify_packages(context, vec![]).await?;
    Ok(())
}

pub async fn verify_packages(
    context: &WalletContext,
    package_paths: Vec<PathBuf>,
) -> anyhow::Result<()> {
    for p in package_paths {
        verify_package(context, p).await?
    }
    Ok(())
}

pub fn serve() -> anyhow::Result<Server> {
    Ok(
        HttpServer::new(|| App::new().route("/api", web::get().to(api_route)))
            .bind("0.0.0.0:8000")?
            .run(),
    )
}

async fn api_route(_request: HttpRequest) -> impl Responder {
    "{\"source\": \"code\"}"
}
