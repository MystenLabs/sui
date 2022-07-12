// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use clap::Parser;
use multiaddr::Multiaddr;
use std::path::PathBuf;
use sui_config::{genesis::Builder, SUI_GENESIS_FILENAME};
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    crypto::PublicKeyBytes,
    object::Object,
};

#[derive(Parser)]
pub struct Ceremony {
    #[clap(long)]
    path: PathBuf,

    #[clap(subcommand)]
    command: CeremonyCommand,
}

#[derive(Parser)]
pub enum CeremonyCommand {
    Init,

    AddValidator {
        name: String,
        public_key: PublicKeyBytes,
        network_address: Multiaddr,
        narwhal_primary_to_primary: Multiaddr,
        narwhal_worker_to_primary: Multiaddr,
        narwhal_primary_to_worker: Multiaddr,
        narwhal_worker_to_worker: Multiaddr,
        narwhal_consensus_address: Multiaddr,
    },

    AddGasObject {
        address: SuiAddress,
        object_id: Option<ObjectID>,
        value: u64,
    },

    Finalize,
}

pub fn run(cmd: Ceremony) -> Result<()> {
    match cmd.command {
        CeremonyCommand::Init => {
            let builder = Builder::new();
            builder.save(cmd.path)?;
        }

        //TODO this will need to include Narwhal network information
        CeremonyCommand::AddValidator {
            name,
            public_key,
            network_address,
            narwhal_primary_to_primary,
            narwhal_worker_to_primary,
            narwhal_primary_to_worker,
            narwhal_worker_to_worker,
            narwhal_consensus_address,
        } => {
            let mut builder = Builder::load(&cmd.path)?;
            builder = builder.add_validator(sui_config::ValidatorInfo {
                name,
                public_key,
                stake: 1,
                delegation: 0,
                network_address,
                narwhal_primary_to_primary,
                narwhal_worker_to_primary,
                narwhal_primary_to_worker,
                narwhal_worker_to_worker,
                narwhal_consensus_address,
            });
            builder.save(cmd.path)?;
        }

        CeremonyCommand::AddGasObject {
            address,
            object_id,
            value,
        } => {
            let mut builder = Builder::load(&cmd.path)?;

            let object_id = object_id.unwrap_or_else(ObjectID::random);
            let object = Object::with_id_owner_gas_for_testing(object_id, address, value);
            builder = builder.add_object(object);

            builder.save(cmd.path)?;
        }

        CeremonyCommand::Finalize => {
            let builder = Builder::load(&cmd.path)?;

            let genesis = builder.build();

            genesis.save(cmd.path.join(SUI_GENESIS_FILENAME))?;
        }
    }

    Ok(())
}
