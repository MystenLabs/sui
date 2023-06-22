// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::{Path, PathBuf};

use actix_web::{dev::Server, web, App, HttpRequest, HttpServer, Responder};

use move_package::BuildConfig as MoveBuildConfig;
use sui_move::build::resolve_lock_file_path;
use sui_move_build::{BuildConfig, SuiPackageHooks};
use sui_sdk::wallet_context::WalletContext;
use sui_source_validation::{BytecodeSourceVerifier, SourceMode};

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

pub async fn initialize(
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
