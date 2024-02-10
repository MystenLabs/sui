// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use clap::*;
use fastcrypto::traits::EncodeDecodeBase64;
use std::path::PathBuf;
use sui_bridge::crypto::BridgeAuthorityKeyPair;
use sui_types::crypto::get_key_pair;

#[derive(Parser)]
#[clap(rename_all = "kebab-case")]
struct Args {
    #[clap(subcommand)]
    command: BridgeValidatorCommand,
}

#[derive(Parser)]
#[clap(rename_all = "kebab-case")]
pub enum BridgeValidatorCommand {
    #[clap(name = "create-bridge-validator-key")]
    CreateBridgeValidatorKey { path: PathBuf },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    match args.command {
        BridgeValidatorCommand::CreateBridgeValidatorKey { path } => {
            generate_bridge_authority_key_and_write_to_file(&path)?;
            println!("Bridge validator key generated at {}", path.display());
        }
    }

    Ok(())
}

/// Generate Bridge Authority key (Secp256k1KeyPair) and write to a file as base64 encoded `privkey`.
fn generate_bridge_authority_key_and_write_to_file(path: &PathBuf) -> Result<(), anyhow::Error> {
    let (_, kp): (_, BridgeAuthorityKeyPair) = get_key_pair();
    let base64_encoded = kp.encode_base64();
    std::fs::write(path, base64_encoded)
        .map_err(|err| anyhow!("Failed to write encoded key to path: {:?}", err))
}
