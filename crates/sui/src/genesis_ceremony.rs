// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use camino::Utf8PathBuf;
use clap::Parser;
use multiaddr::Multiaddr;
use signature::{Signer, Verifier};
use std::{fs, path::PathBuf};
use sui_config::{
    genesis::{Builder, Genesis},
    SUI_GENESIS_FILENAME,
};
use sui_types::{
    base_types::{decode_bytes_hex, encode_bytes_hex, ObjectID, SuiAddress},
    crypto::{
        generate_proof_of_possession, AuthorityKeyPair, AuthorityPublicKey,
        AuthorityPublicKeyBytes, AuthoritySignature, KeypairTraits, SuiKeyPair, ToFromBytes,
    },
    object::Object,
};

use crate::keytool::{read_authority_keypair_from_file, read_keypair_from_file};

const GENESIS_BUILDER_SIGNATURE_DIR: &str = "signatures";

#[derive(Parser)]
pub struct Ceremony {
    #[clap(long)]
    path: Option<PathBuf>,

    #[clap(subcommand)]
    command: CeremonyCommand,
}

impl Ceremony {
    pub fn run(self) -> Result<()> {
        run(self)
    }
}

#[derive(Parser)]
pub enum CeremonyCommand {
    Init,

    AddValidator {
        #[clap(long)]
        name: String,
        #[clap(long)]
        validator_key_file: PathBuf,
        #[clap(long)]
        staking_key_file: PathBuf,
        #[clap(long)]
        network_key_file: PathBuf,
        #[clap(long)]
        network_address: Multiaddr,
        #[clap(long)]
        narwhal_primary_to_primary: Multiaddr,
        #[clap(long)]
        narwhal_worker_to_primary: Multiaddr,
        #[clap(long)]
        narwhal_primary_to_worker: Multiaddr,
        #[clap(long)]
        narwhal_worker_to_worker: Multiaddr,
        #[clap(long)]
        narwhal_consensus_address: Multiaddr,
    },

    AddGasObject {
        #[clap(long)]
        address: SuiAddress,
        #[clap(long)]
        object_id: Option<ObjectID>,
        #[clap(long)]
        value: u64,
    },

    Build,

    VerifyAndSign {
        #[clap(long)]
        key_file: PathBuf,
    },

    Finalize,
}

pub fn run(cmd: Ceremony) -> Result<()> {
    let dir = if let Some(path) = cmd.path {
        path
    } else {
        std::env::current_dir()?
    };
    let dir = Utf8PathBuf::try_from(dir)?;

    match cmd.command {
        CeremonyCommand::Init => {
            let builder = Builder::new();
            builder.save(dir)?;
        }

        CeremonyCommand::AddValidator {
            name,
            validator_key_file,
            staking_key_file: _,
            network_key_file,
            network_address,
            narwhal_primary_to_primary,
            narwhal_worker_to_primary,
            narwhal_primary_to_worker,
            narwhal_worker_to_worker,
            narwhal_consensus_address,
        } => {
            let mut builder = Builder::load(&dir)?;
            let keypair: AuthorityKeyPair = read_authority_keypair_from_file(validator_key_file)?;
            let network_keypair: SuiKeyPair = read_keypair_from_file(network_key_file)?;
            builder = builder.add_validator(sui_config::ValidatorInfo {
                name,
                public_key: keypair.public().into(),
                network_key: network_keypair.public(),
                proof_of_possession: generate_proof_of_possession(&keypair),
                stake: 1,
                delegation: 0,
                gas_price: 1,
                network_address,
                narwhal_primary_to_primary,
                narwhal_worker_to_primary,
                narwhal_primary_to_worker,
                narwhal_worker_to_worker,
                narwhal_consensus_address,
            });
            println!("1");
            builder.save(dir)?;
            println!("2");
        }

        CeremonyCommand::AddGasObject {
            address,
            object_id,
            value,
        } => {
            let mut builder = Builder::load(&dir)?;

            let object_id = object_id.unwrap_or_else(ObjectID::random);
            let object = Object::with_id_owner_gas_for_testing(object_id, address, value);
            builder = builder.add_object(object);

            builder.save(dir)?;
        }

        CeremonyCommand::Build => {
            let builder = Builder::load(&dir)?;

            let genesis = builder.build();

            genesis.save(dir.join(SUI_GENESIS_FILENAME))?;

            println!("Successfully built {SUI_GENESIS_FILENAME}");
            println!(
                "{SUI_GENESIS_FILENAME} sha3-256: {}",
                hex::encode(genesis.sha3())
            );
        }

        CeremonyCommand::VerifyAndSign { key_file } => {
            let keypair: AuthorityKeyPair = read_authority_keypair_from_file(key_file)?;
            let loaded_genesis = Genesis::load(dir.join(SUI_GENESIS_FILENAME))?;
            let loaded_genesis_bytes = loaded_genesis.to_bytes();

            let builder = Builder::load(&dir)?;

            let built_genesis = builder.build();
            let built_genesis_bytes = built_genesis.to_bytes();

            if built_genesis != loaded_genesis || built_genesis_bytes != loaded_genesis_bytes {
                return Err(anyhow::anyhow!(
                    "loaded genesis does not match built genesis"
                ));
            }

            if !built_genesis.validator_set().iter().any(|validator| {
                validator.public_key() == AuthorityPublicKeyBytes::from(keypair.public())
            }) {
                return Err(anyhow::anyhow!(
                    "provided keypair does not correspond to a validator in the validator set"
                ));
            }

            // Sign the genesis bytes
            let signature: AuthoritySignature = keypair.try_sign(&built_genesis_bytes)?;

            let signature_dir = dir.join(GENESIS_BUILDER_SIGNATURE_DIR);
            std::fs::create_dir_all(&signature_dir)?;

            let hex_name = encode_bytes_hex(&AuthorityPublicKeyBytes::from(keypair.public()));
            fs::write(signature_dir.join(hex_name), signature)?;

            println!("Successfully verified {SUI_GENESIS_FILENAME}");
            println!(
                "{SUI_GENESIS_FILENAME} sha3-256: {}",
                hex::encode(built_genesis.sha3())
            );
        }

        CeremonyCommand::Finalize => {
            let genesis = Genesis::load(dir.join(SUI_GENESIS_FILENAME))?;
            let genesis_bytes = genesis.to_bytes();

            let mut signatures = std::collections::BTreeMap::new();

            for entry in dir.join(GENESIS_BUILDER_SIGNATURE_DIR).read_dir_utf8()? {
                let entry = entry?;
                if entry.file_name().starts_with('.') {
                    continue;
                }

                let path = entry.path();
                let signature_bytes = fs::read(path)?;
                let signature: AuthoritySignature =
                    AuthoritySignature::from_bytes(&signature_bytes)?;
                let name = path
                    .file_name()
                    .ok_or_else(|| anyhow::anyhow!("Invalid signature file"))?;
                let public_key =
                    AuthorityPublicKeyBytes::from_bytes(&decode_bytes_hex::<Vec<u8>>(name)?[..])?;
                signatures.insert(public_key, signature);
            }

            for validator in genesis.validator_set() {
                let signature = signatures.remove(&validator.public_key()).ok_or_else(|| {
                    anyhow::anyhow!("missing signature for validator {}", validator.name())
                })?;

                let pk: AuthorityPublicKey = validator.public_key().try_into()?;

                pk.verify(&genesis_bytes, &signature).with_context(|| {
                    format!(
                        "failed to validate signature for validator {}",
                        validator.name()
                    )
                })?;
            }

            if !signatures.is_empty() {
                return Err(anyhow::anyhow!(
                    "found extra signatures from entities not in the validator set"
                ));
            }

            println!("Successfully finalized Genesis!");
            println!(
                "{SUI_GENESIS_FILENAME} sha3-256: {}",
                hex::encode(genesis.sha3())
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::keytool::write_keypair_to_file;
    use anyhow::Result;
    use sui_config::{utils, ValidatorInfo};
    use sui_types::crypto::{get_key_pair_from_rng, AccountKeyPair, AuthorityKeyPair, SuiKeyPair};

    #[test]
    fn ceremony() -> Result<()> {
        let dir = tempfile::TempDir::new().unwrap();

        let validators = (0..10)
            .map(|i| {
                let keypair: AuthorityKeyPair = get_key_pair_from_rng(&mut rand::rngs::OsRng).1;
                let network_keypair: AccountKeyPair =
                    get_key_pair_from_rng(&mut rand::rngs::OsRng).1;
                let staking_keypair: AccountKeyPair =
                    get_key_pair_from_rng(&mut rand::rngs::OsRng).1;
                let info = ValidatorInfo {
                    name: format!("validator-{i}"),
                    public_key: keypair.public().into(),
                    network_key: keypair.public().clone().into(),
                    proof_of_possession: generate_proof_of_possession(&keypair),
                    stake: 1,
                    delegation: 0,
                    gas_price: 1,
                    network_address: utils::new_network_address(),
                    narwhal_primary_to_primary: utils::new_network_address(),
                    narwhal_worker_to_primary: utils::new_network_address(),
                    narwhal_primary_to_worker: utils::new_network_address(),
                    narwhal_worker_to_worker: utils::new_network_address(),
                    narwhal_consensus_address: utils::new_network_address(),
                };
                let key_file = dir.path().join(format!("{}.key", info.name));
                write_keypair_to_file(&SuiKeyPair::Ed25519SuiKeyPair(keypair), &key_file).unwrap();

                let network_key_file = dir.path().join(format!("{}.key", info.name));
                write_keypair_to_file(
                    &SuiKeyPair::Ed25519SuiKeyPair(network_keypair),
                    &network_key_file,
                )
                .unwrap();

                let staking_key_file = dir.path().join(format!("{}.key", info.name));
                write_keypair_to_file(
                    &SuiKeyPair::Ed25519SuiKeyPair(staking_keypair),
                    &staking_key_file,
                )
                .unwrap();

                (key_file, network_key_file, staking_key_file, info)
            })
            .collect::<Vec<_>>();

        // Initialize
        let command = Ceremony {
            path: Some(dir.path().into()),
            command: CeremonyCommand::Init,
        };
        command.run()?;

        // Add the validators
        for (key_file, network_key_file, staking_key_file, validator) in &validators {
            let command = Ceremony {
                path: Some(dir.path().into()),
                command: CeremonyCommand::AddValidator {
                    name: validator.name().to_owned(),
                    validator_key_file: key_file.into(),
                    network_key_file: network_key_file.into(),
                    staking_key_file: staking_key_file.into(),
                    network_address: validator.network_address().to_owned(),
                    narwhal_primary_to_primary: validator.narwhal_primary_to_primary.clone(),
                    narwhal_worker_to_primary: validator.narwhal_worker_to_primary.clone(),
                    narwhal_primary_to_worker: validator.narwhal_primary_to_worker.clone(),
                    narwhal_worker_to_worker: validator.narwhal_worker_to_worker.clone(),
                    narwhal_consensus_address: validator.narwhal_consensus_address.clone(),
                },
            };
            command.run()?;
        }

        // Build the Genesis object
        let command = Ceremony {
            path: Some(dir.path().into()),
            command: CeremonyCommand::Build,
        };
        command.run()?;

        // Have all the validators verify and sign genesis
        for (key, _network_key, _staking_key, _validator) in &validators {
            let command = Ceremony {
                path: Some(dir.path().into()),
                command: CeremonyCommand::VerifyAndSign {
                    key_file: key.into(),
                },
            };
            command.run()?;
        }

        // Finalize the Ceremony
        let command = Ceremony {
            path: Some(dir.path().into()),
            command: CeremonyCommand::Finalize,
        };
        command.run()?;

        Ok(())
    }
}
