// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Generates the node-local `LocalExecutionService` gRPC service.
//!
//! This is a small sui-repo-local service (not part of the public `sui-rpc` SDK), defined the same
//! way as `sui.validator.Validator` and `consensus.ObserverService`: hand-written request/response
//! types (in `sui-types`) plus a manual tonic service definition compiled here.

use std::env;
use std::path::PathBuf;

use tonic_build::manual::{Builder, Method, Service};

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let out_dir = PathBuf::from(env::var("OUT_DIR")?);

    let service = Service::builder()
        .name("LocalExecutionService")
        .package("sui.node")
        .comment("Node-local execution observability endpoints.")
        .method(
            Method::builder()
                .name("wait_for_local_effects")
                .route_name("WaitForLocalEffects")
                .input_type("sui_types::local_execution::WaitForLocalEffectsRequest")
                .output_type("sui_types::local_execution::WaitForLocalEffectsResponse")
                .codec_path("mysten_network::codec::BcsCodec")
                .build(),
        )
        .build();

    Builder::new().out_dir(&out_dir).compile(&[service]);

    println!("cargo:rerun-if-changed=build.rs");
    Ok(())
}
