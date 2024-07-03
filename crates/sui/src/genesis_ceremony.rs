// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use camino::Utf8PathBuf;
use clap::Parser;
use fastcrypto::encoding::{Encoding, Hex};
use std::path::PathBuf;
use sui_config::{genesis::UnsignedGenesis, SUI_GENESIS_FILENAME};
use sui_genesis_builder::Builder;
use sui_types::multiaddr::Multiaddr;
use sui_types::{
    base_types::SuiAddress,
    committee::ProtocolVersion,
    crypto::{
        generate_proof_of_possession, AuthorityKeyPair, KeypairTraits, NetworkKeyPair, SuiKeyPair,
    },
    message_envelope::Message,
};

use sui_keys::keypair_file::{
    read_authority_keypair_from_file, read_keypair_from_file, read_network_keypair_from_file,
};

use crate::genesis_inspector::examine_genesis_checkpoint;

#[derive(Parser)]
pub struct Ceremony {
    #[clap(long)]
    path: Option<PathBuf>,

    #[clap(long)]
    protocol_version: Option<u64>,

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

    ValidateState,

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
        p2p_address: Multiaddr,
        #[clap(long)]
        narwhal_primary_address: Multiaddr,
        #[clap(long)]
        narwhal_worker_address: Multiaddr,
        #[clap(long)]
        description: String,
        #[clap(long)]
        image_url: String,
        #[clap(long)]
        project_url: String,
    },

    ListValidators,

    BuildUnsignedCheckpoint,

    ExamineGenesisCheckpoint,

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

    let protocol_version = cmd
        .protocol_version
        .map(ProtocolVersion::new)
        .unwrap_or(ProtocolVersion::MAX);

    match cmd.command {
        CeremonyCommand::Init => {
            let builder = Builder::new().with_protocol_version(protocol_version);
            builder.save(dir)?;
        }

        CeremonyCommand::ValidateState => {
            let builder = Builder::load(&dir)?;
            builder.validate()?;
        }

        CeremonyCommand::AddValidator {
            name,
            validator_key_file,
            worker_key_file,
            account_key_file,
            network_key_file,
            network_address,
            p2p_address,
            narwhal_primary_address,
            narwhal_worker_address,
            description,
            image_url,
            project_url,
        } => {
            let mut builder = Builder::load(&dir)?;
            let keypair: AuthorityKeyPair = read_authority_keypair_from_file(validator_key_file)?;
            let account_keypair: SuiKeyPair = read_keypair_from_file(account_key_file)?;
            let worker_keypair: NetworkKeyPair = read_network_keypair_from_file(worker_key_file)?;
            let network_keypair: NetworkKeyPair = read_network_keypair_from_file(network_key_file)?;
            let pop = generate_proof_of_possession(&keypair, (&account_keypair.public()).into());
            builder = builder.add_validator(
                sui_genesis_builder::validator_info::ValidatorInfo {
                    name,
                    protocol_key: keypair.public().into(),
                    worker_key: worker_keypair.public().clone(),
                    account_address: SuiAddress::from(&account_keypair.public()),
                    network_key: network_keypair.public().clone(),
                    gas_price: sui_config::node::DEFAULT_VALIDATOR_GAS_PRICE,
                    commission_rate: sui_config::node::DEFAULT_COMMISSION_RATE,
                    network_address,
                    p2p_address,
                    narwhal_primary_address,
                    narwhal_worker_address,
                    description,
                    image_url,
                    project_url,
                },
                pop,
            );
            builder.save(dir)?;
        }

        CeremonyCommand::ListValidators => {
            let builder = Builder::load(&dir)?;

            let mut writer = csv::Writer::from_writer(std::io::stdout());

            writer.write_record(["validator-name", "account-address"])?;

            let mut validators = builder
                .validators()
                .values()
                .map(|v| {
                    (
                        v.info.name().to_lowercase(),
                        v.info.account_address.to_string(),
                    )
                })
                .collect::<Vec<_>>();

            validators.sort_by_key(|v| v.0.clone());

            for (name, address) in validators {
                writer.write_record([&name, &address])?;
            }
        }

        CeremonyCommand::BuildUnsignedCheckpoint => {
            let mut builder = Builder::load(&dir)?;
            let UnsignedGenesis { checkpoint, .. } = builder.build_unsigned_genesis_checkpoint();
            println!(
                "Successfully built unsigned checkpoint: {}",
                checkpoint.digest()
            );

            builder.save(dir)?;
        }

        CeremonyCommand::ExamineGenesisCheckpoint => {
            let builder = Builder::load(&dir)?;

            let Some(unsigned_genesis) = builder.unsigned_genesis_checkpoint() else {
                return Err(anyhow::anyhow!(
                    "Unable to examine genesis checkpoint; it hasn't been built yet"
                ));
            };

            examine_genesis_checkpoint(unsigned_genesis);
        }

        CeremonyCommand::VerifyAndSign { key_file } => {
            let keypair: AuthorityKeyPair = read_authority_keypair_from_file(key_file)?;

            let mut builder = Builder::load(&dir)?;

            check_protocol_version(&builder, protocol_version)?;

            // Don't sign unless the unsigned checkpoint has already been created
            if builder.unsigned_genesis_checkpoint().is_none() {
                return Err(anyhow::anyhow!(
                    "Unable to verify and sign genesis checkpoint; it hasn't been built yet"
                ));
            }

            builder = builder.add_validator_signature(&keypair);
            let UnsignedGenesis { checkpoint, .. } = builder.unsigned_genesis_checkpoint().unwrap();
            builder.save(dir)?;

            println!(
                "Successfully verified and signed genesis checkpoint: {}",
                checkpoint.digest()
            );
        }

        CeremonyCommand::Finalize => {
            let builder = Builder::load(&dir)?;
            check_protocol_version(&builder, protocol_version)?;

            let genesis = builder.build();

            genesis.save(dir.join(SUI_GENESIS_FILENAME))?;

            println!("Successfully built {SUI_GENESIS_FILENAME}");
            println!(
                "{SUI_GENESIS_FILENAME} blake2b-256: {}",
                Hex::encode(genesis.hash())
            );
        }
    }

    Ok(())
}

fn check_protocol_version(builder: &Builder, protocol_version: ProtocolVersion) -> Result<()> {
    // It is entirely possible for the user to sign a genesis blob with an unknown
    // protocol version, but if this happens there is almost certainly some confusion
    // (e.g. using a `sui` binary built at the wrong commit).
    if builder.protocol_version() != protocol_version {
        return Err(anyhow::anyhow!(
                        "Serialized protocol version does not match local --protocol-version argument. ({:?} vs {:?})",
                        builder.protocol_version(), protocol_version));
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use anyhow::Result;
    use sui_config::local_ip_utils;
    use sui_genesis_builder::validator_info::ValidatorInfo;
    use sui_keys::keypair_file::{write_authority_keypair_to_file, write_keypair_to_file};
    use sui_macros::nondeterministic;
    use sui_types::crypto::{get_key_pair_from_rng, AccountKeyPair, AuthorityKeyPair, SuiKeyPair};

    #[test]
    #[cfg_attr(msim, ignore)]
    fn ceremony() -> Result<()> {
        let dir = nondeterministic!(tempfile::TempDir::new().unwrap());

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
                    account_address: SuiAddress::from(account_keypair.public()),
                    network_key: network_keypair.public().clone(),
                    gas_price: sui_config::node::DEFAULT_VALIDATOR_GAS_PRICE,
                    commission_rate: sui_config::node::DEFAULT_COMMISSION_RATE,
                    network_address: local_ip_utils::new_local_tcp_address_for_testing(),
                    p2p_address: local_ip_utils::new_local_udp_address_for_testing(),
                    narwhal_primary_address: local_ip_utils::new_local_udp_address_for_testing(),
                    narwhal_worker_address: local_ip_utils::new_local_udp_address_for_testing(),
                    description: String::new(),
                    image_url: String::new(),
                    project_url: String::new(),
                };
                let key_file = dir.path().join(format!("{}-0.key", info.name));
                write_authority_keypair_to_file(&keypair, &key_file).unwrap();

                let worker_key_file = dir.path().join(format!("{}.key", info.name));
                write_keypair_to_file(&SuiKeyPair::Ed25519(worker_keypair), &worker_key_file)
                    .unwrap();

                let network_key_file = dir.path().join(format!("{}-1.key", info.name));
                write_keypair_to_file(&SuiKeyPair::Ed25519(network_keypair), &network_key_file)
                    .unwrap();

                let account_key_file = dir.path().join(format!("{}-2.key", info.name));
                write_keypair_to_file(&SuiKeyPair::Ed25519(account_keypair), &account_key_file)
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
            protocol_version: None,
            command: CeremonyCommand::Init,
        };
        command.run()?;

        // Add the validators
        for (key_file, worker_key_file, network_key_file, account_key_file, validator) in
            &validators
        {
            let command = Ceremony {
                path: Some(dir.path().into()),
                protocol_version: None,
                command: CeremonyCommand::AddValidator {
                    name: validator.name().to_owned(),
                    validator_key_file: key_file.into(),
                    worker_key_file: worker_key_file.into(),
                    network_key_file: network_key_file.into(),
                    account_key_file: account_key_file.into(),
                    network_address: validator.network_address().to_owned(),
                    p2p_address: validator.p2p_address().to_owned(),
                    narwhal_primary_address: validator.narwhal_primary_address.clone(),
                    narwhal_worker_address: validator.narwhal_worker_address.clone(),
                    description: String::new(),
                    image_url: String::new(),
                    project_url: String::new(),
                },
            };
            command.run()?;

            Ceremony {
                path: Some(dir.path().into()),
                protocol_version: None,
                command: CeremonyCommand::ValidateState,
            }
            .run()?;
        }

        // Build the unsigned checkpoint
        let command = Ceremony {
            path: Some(dir.path().into()),
            protocol_version: None,
            command: CeremonyCommand::BuildUnsignedCheckpoint,
        };
        command.run()?;

        // Have all the validators verify and sign genesis
        for (key, _worker_key, _network_key, _account_key, _validator) in &validators {
            let command = Ceremony {
                path: Some(dir.path().into()),
                protocol_version: None,
                command: CeremonyCommand::VerifyAndSign {
                    key_file: key.into(),
                },
            };
            command.run()?;

            Ceremony {
                path: Some(dir.path().into()),
                protocol_version: None,
                command: CeremonyCommand::ValidateState,
            }
            .run()?;
        }

        // Finalize the Ceremony and build the Genesis object
        let command = Ceremony {
            path: Some(dir.path().into()),
            protocol_version: None,
            command: CeremonyCommand::Finalize,
        };
        command.run()?;

        Ok(())
    }
}
