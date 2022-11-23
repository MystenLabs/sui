// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, Context, Result};
use camino::Utf8PathBuf;
use clap::Parser;
use fastcrypto::encoding::{Encoding, Hex};
use multiaddr::Multiaddr;
use signature::{Signer, Verifier};
use std::{fs, path::PathBuf};
use sui_config::{
    genesis::{Builder, Genesis},
    SUI_GENESIS_FILENAME,
};
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    crypto::{
        generate_proof_of_possession, AuthorityKeyPair, AuthorityPublicKey,
        AuthorityPublicKeyBytes, AuthoritySignature, KeypairTraits, NetworkKeyPair, SuiKeyPair,
        ToFromBytes,
    },
    object::Object,
};

use crate::keytool::{
    read_authority_keypair_from_file, read_keypair_from_file, read_network_keypair_from_file,
};

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
        worker_key_file: PathBuf,
        #[clap(long)]
        account_key_file: PathBuf,
        #[clap(long)]
        network_key_file: PathBuf,
        #[clap(long)]
        network_address: Multiaddr,
        #[clap(long)]
        narwhal_primary_address: Multiaddr,
        #[clap(long)]
        narwhal_worker_address: Multiaddr,
        #[clap(long)]
        narwhal_internal_worker_address: Option<Multiaddr>,
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
            worker_key_file,
            account_key_file,
            network_key_file,
            network_address,
            narwhal_primary_address,
            narwhal_worker_address,
            narwhal_internal_worker_address,
            narwhal_consensus_address,
        } => {
            let mut builder = Builder::load(&dir)?;
            let keypair: AuthorityKeyPair = read_authority_keypair_from_file(validator_key_file)?;
            let account_keypair: SuiKeyPair = read_keypair_from_file(account_key_file)?;
            let worker_keypair: NetworkKeyPair = read_network_keypair_from_file(worker_key_file)?;
            let network_keypair: NetworkKeyPair = read_network_keypair_from_file(network_key_file)?;
            let pop = generate_proof_of_possession(&keypair, (&account_keypair.public()).into());
            builder = builder.add_validator(
                sui_config::ValidatorInfo {
                    name,
                    protocol_key: keypair.public().into(),
                    worker_key: worker_keypair.public().clone(),
                    account_key: account_keypair.public(),
                    network_key: network_keypair.public().clone(),
                    stake: 1,
                    delegation: 0,
                    gas_price: 1,
                    commission_rate: 0,
                    network_address,
                    narwhal_primary_address,
                    narwhal_worker_address,
                    narwhal_internal_worker_address,
                    narwhal_consensus_address,
                },
                pop,
            );
            builder.save(dir)?;
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
                Hex::encode(genesis.sha3())
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
                validator.protocol_key() == AuthorityPublicKeyBytes::from(keypair.public())
            }) {
                return Err(anyhow::anyhow!(
                    "provided keypair does not correspond to a validator in the validator set"
                ));
            }

            // Sign the genesis bytes
            let signature: AuthoritySignature = keypair.try_sign(&built_genesis_bytes)?;

            let signature_dir = dir.join(GENESIS_BUILDER_SIGNATURE_DIR);
            std::fs::create_dir_all(&signature_dir)?;

            let hex_name = Hex::encode(AuthorityPublicKeyBytes::from(keypair.public()));
            fs::write(signature_dir.join(hex_name), signature)?;

            println!("Successfully verified {SUI_GENESIS_FILENAME}");
            println!(
                "{SUI_GENESIS_FILENAME} sha3-256: {}",
                Hex::encode(built_genesis.sha3())
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
                let public_key = AuthorityPublicKeyBytes::from_bytes(
                    &Hex::decode(name).map_err(|e| anyhow!(e))?[..],
                )?;
                signatures.insert(public_key, signature);
            }

            for validator in genesis.validator_set() {
                let signature = signatures
                    .remove(&validator.protocol_key())
                    .ok_or_else(|| {
                        anyhow::anyhow!("missing signature for validator {}", validator.name())
                    })?;

                let pk: AuthorityPublicKey = validator.protocol_key().try_into()?;

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
                Hex::encode(genesis.sha3())
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::keytool::{write_authority_keypair_to_file, write_keypair_to_file};
    use anyhow::Result;
    use sui_config::{utils, ValidatorInfo};
    use sui_types::crypto::{get_key_pair_from_rng, AccountKeyPair, AuthorityKeyPair, SuiKeyPair};

    #[test]
    fn ceremony() -> Result<()> {
        let dir = tempfile::TempDir::new().unwrap();

        let validators = (0..10)
            .map(|i| {
                let keypair: AuthorityKeyPair = get_key_pair_from_rng(&mut rand::rngs::OsRng).1;
                let worker_keypair: NetworkKeyPair =
                    get_key_pair_from_rng(&mut rand::rngs::OsRng).1;
                let network_keypair: NetworkKeyPair =
                    get_key_pair_from_rng(&mut rand::rngs::OsRng).1;
                let account_keypair: AccountKeyPair =
                    get_key_pair_from_rng(&mut rand::rngs::OsRng).1;
                let info = ValidatorInfo {
                    name: format!("validator-{i}"),
                    protocol_key: keypair.public().into(),
                    worker_key: worker_keypair.public().clone(),
                    account_key: account_keypair.public().clone().into(),
                    network_key: network_keypair.public().clone(),
                    stake: 1,
                    delegation: 0,
                    gas_price: 1,
                    commission_rate: 0,
                    network_address: utils::new_network_address(),
                    narwhal_primary_address: utils::new_network_address(),
                    narwhal_worker_address: utils::new_network_address(),
                    narwhal_internal_worker_address: None,
                    narwhal_consensus_address: utils::new_network_address(),
                };
                let key_file = dir.path().join(format!("{}-0.key", info.name));
                write_authority_keypair_to_file(&keypair, &key_file).unwrap();

                let worker_key_file = dir.path().join(format!("{}.key", info.name));
                write_keypair_to_file(
                    &SuiKeyPair::Ed25519SuiKeyPair(worker_keypair),
                    &worker_key_file,
                )
                .unwrap();

                let network_key_file = dir.path().join(format!("{}-1.key", info.name));
                write_keypair_to_file(
                    &SuiKeyPair::Ed25519SuiKeyPair(network_keypair),
                    &network_key_file,
                )
                .unwrap();

                let account_key_file = dir.path().join(format!("{}-2.key", info.name));
                write_keypair_to_file(
                    &SuiKeyPair::Ed25519SuiKeyPair(account_keypair),
                    &account_key_file,
                )
                .unwrap();

                (
                    key_file,
                    worker_key_file,
                    network_key_file,
                    account_key_file,
                    info,
                )
            })
            .collect::<Vec<_>>();

        // Initialize
        let command = Ceremony {
            path: Some(dir.path().into()),
            command: CeremonyCommand::Init,
        };
        command.run()?;

        // Add the validators
        for (key_file, worker_key_file, network_key_file, account_key_file, validator) in
            &validators
        {
            let command = Ceremony {
                path: Some(dir.path().into()),
                command: CeremonyCommand::AddValidator {
                    name: validator.name().to_owned(),
                    validator_key_file: key_file.into(),
                    worker_key_file: worker_key_file.into(),
                    network_key_file: network_key_file.into(),
                    account_key_file: account_key_file.into(),
                    network_address: validator.network_address().to_owned(),
                    narwhal_primary_address: validator.narwhal_primary_address.clone(),
                    narwhal_worker_address: validator.narwhal_worker_address.clone(),
                    narwhal_internal_worker_address: validator
                        .narwhal_internal_worker_address
                        .clone(),
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
        for (key, _worker_key, _network_key, _account_key, _validator) in &validators {
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
