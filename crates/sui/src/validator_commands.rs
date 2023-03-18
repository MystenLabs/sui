// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::ident_str;
use std::{
    fmt::{Debug, Display, Formatter, Write},
    fs,
    path::PathBuf,
};
use sui_config::genesis::GenesisValidatorInfo;
use sui_types::multiaddr::Multiaddr;

use crate::client_commands::{write_transaction_response, WalletContext};
use crate::fire_drill::get_gas_obj_ref;
use clap::*;
use colored::Colorize;
use fastcrypto::traits::KeyPair;
use fastcrypto::traits::ToFromBytes;
use serde::Serialize;
use shared_crypto::intent::Intent;
use sui_json_rpc_types::{SuiTransactionResponse, SuiTransactionResponseOptions};
use sui_keys::keystore::AccountKeystore;
use sui_keys::{
    key_derive::generate_new_key,
    keypair_file::{
        read_authority_keypair_from_file, read_keypair_from_file, read_network_keypair_from_file,
        write_authority_keypair_to_file, write_keypair_to_file,
    },
};
use sui_sdk::SuiClient;
use sui_types::crypto::{
    generate_proof_of_possession, get_authority_key_pair, AuthorityPublicKeyBytes,
};
use sui_types::messages::Transaction;
use sui_types::messages::{CallArg, TransactionData};
use sui_types::{
    crypto::{AuthorityKeyPair, NetworkKeyPair, SignatureScheme, SuiKeyPair},
    SUI_FRAMEWORK_OBJECT_ID, SUI_SYSTEM_OBJ_CALL_ARG,
};
use tracing::info;

#[derive(Parser)]
#[clap(rename_all = "kebab-case")]
pub enum SuiValidatorCommand {
    #[clap(name = "make-validator-info")]
    MakeValidatorInfo {
        name: String,
        description: String,
        image_url: String,
        project_url: String,
        host_name: String,
        gas_price: u64,
    },
    #[clap(name = "become-candidate")]
    BecomeCandidate {
        #[clap(name = "validator-info-path")]
        file: PathBuf,
    },
    #[clap(name = "join-committee")]
    JoinCommittee,
    #[clap(name = "leave-committee")]
    LeaveCommittee,
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum SuiValidatorCommandResponse {
    MakeValidatorInfo,
    BecomeCandidate(SuiTransactionResponse),
    JoinCommittee(SuiTransactionResponse),
    LeaveCommittee(SuiTransactionResponse),
}

fn make_key_files(
    file_name: PathBuf,
    is_protocol_key: bool,
    key: Option<SuiKeyPair>,
) -> anyhow::Result<()> {
    if file_name.exists() {
        println!("Use existing {:?} key file.", file_name);
        return Ok(());
    } else if is_protocol_key {
        let (_, keypair) = get_authority_key_pair();
        write_authority_keypair_to_file(&keypair, file_name.clone())?;
        println!("Generated new key file: {:?}.", file_name);
    } else {
        let kp = match key {
            Some(key) => {
                println!(
                    "Generated new key file {:?} based on sui.keystore file.",
                    file_name
                );
                key
            }
            None => {
                let (_, kp, _, _) = generate_new_key(SignatureScheme::ED25519, None)?;
                println!("Generated new key file: {:?}.", file_name);
                kp
            }
        };
        write_keypair_to_file(&kp, &file_name)?;
    }
    println!("Generated new key file: {:?}.", file_name);
    Ok(())
}

impl SuiValidatorCommand {
    pub async fn execute(
        self,
        context: &mut WalletContext,
    ) -> Result<SuiValidatorCommandResponse, anyhow::Error> {
        let client = context.get_client().await?;
        let sui_address = context.active_address()?;
        let ret = Ok(match self {
            SuiValidatorCommand::MakeValidatorInfo {
                name,
                description,
                image_url,
                project_url,
                host_name,
                gas_price,
            } => {
                let dir = std::env::current_dir()?;
                let protocol_key_file_name = dir.join("protocol.key");
                let account_key = match context.config.keystore.get_key(&sui_address)? {
                    SuiKeyPair::Ed25519(account_key) => SuiKeyPair::Ed25519(account_key.copy()),
                    _ => panic!(
                        "Other account key types supported yet, please use Ed25519 keys for now."
                    ),
                };
                let account_key_file_name = dir.join("account.key");
                let network_key_file_name = dir.join("network.key");
                let worker_key_file_name = dir.join("worker.key");
                make_key_files(protocol_key_file_name.clone(), true, None)?;
                make_key_files(account_key_file_name.clone(), false, Some(account_key))?;
                make_key_files(network_key_file_name.clone(), false, None)?;
                make_key_files(worker_key_file_name.clone(), false, None)?;

                let keypair: AuthorityKeyPair =
                    read_authority_keypair_from_file(protocol_key_file_name)?;
                let account_keypair: SuiKeyPair = read_keypair_from_file(account_key_file_name)?;
                let worker_keypair: NetworkKeyPair =
                    read_network_keypair_from_file(worker_key_file_name)?;
                let network_keypair: NetworkKeyPair =
                    read_network_keypair_from_file(network_key_file_name)?;
                let pop =
                    generate_proof_of_possession(&keypair, (&account_keypair.public()).into());
                let validator_info = GenesisValidatorInfo {
                    info: sui_config::ValidatorInfo {
                        name,
                        protocol_key: keypair.public().into(),
                        worker_key: worker_keypair.public().clone(),
                        account_key: account_keypair.public(),
                        network_key: network_keypair.public().clone(),
                        gas_price,
                        commission_rate: 0,
                        network_address: Multiaddr::try_from(format!(
                            "/dns/{}/tcp/8080/http",
                            host_name
                        ))?,
                        p2p_address: Multiaddr::try_from(format!("/dns/{}/udp/8084", host_name))?,
                        narwhal_primary_address: Multiaddr::try_from(format!(
                            "/dns/{}/udp/8081",
                            host_name
                        ))?,
                        narwhal_worker_address: Multiaddr::try_from(format!(
                            "/dns/{}/udp/8082",
                            host_name
                        ))?,
                        description,
                        image_url,
                        project_url,
                    },
                    proof_of_possession: pop,
                };
                // TODO set key files permisssion
                let validator_info_file_name = dir.join("validator.info");
                let validator_info_bytes = serde_yaml::to_vec(&validator_info)?;
                fs::write(validator_info_file_name.clone(), validator_info_bytes)?;
                println!(
                    "Generated validator info file: {:?}.",
                    validator_info_file_name
                );
                SuiValidatorCommandResponse::MakeValidatorInfo
            }
            SuiValidatorCommand::BecomeCandidate { file } => {
                let validator_info_bytes = fs::read(file)?;
                // Note: we should probably rename the struct or evolve it accordingly.
                let validator_info: GenesisValidatorInfo =
                    serde_yaml::from_slice(&validator_info_bytes)?;
                let validator = validator_info.info;

                let args = vec![
                    CallArg::Pure(
                        bcs::to_bytes(&AuthorityPublicKeyBytes::from_bytes(
                            validator.protocol_key().as_bytes(),
                        )?)
                        .unwrap(),
                    ),
                    CallArg::Pure(
                        bcs::to_bytes(&validator.network_key().as_bytes().to_vec()).unwrap(),
                    ),
                    CallArg::Pure(
                        bcs::to_bytes(&validator.worker_key().as_bytes().to_vec()).unwrap(),
                    ),
                    CallArg::Pure(
                        bcs::to_bytes(&validator_info.proof_of_possession.as_ref().to_vec())
                            .unwrap(),
                    ),
                    CallArg::Pure(
                        bcs::to_bytes(&validator.name().to_owned().into_bytes()).unwrap(),
                    ),
                    CallArg::Pure(
                        bcs::to_bytes(&validator.description.clone().into_bytes()).unwrap(),
                    ),
                    CallArg::Pure(
                        bcs::to_bytes(&validator.image_url.clone().into_bytes()).unwrap(),
                    ),
                    CallArg::Pure(
                        bcs::to_bytes(&validator.project_url.clone().into_bytes()).unwrap(),
                    ),
                    CallArg::Pure(bcs::to_bytes(validator.network_address()).unwrap()),
                    CallArg::Pure(bcs::to_bytes(validator.p2p_address()).unwrap()),
                    CallArg::Pure(bcs::to_bytes(validator.narwhal_primary_address()).unwrap()),
                    CallArg::Pure(bcs::to_bytes(validator.narwhal_worker_address()).unwrap()),
                    CallArg::Pure(bcs::to_bytes(&validator.gas_price()).unwrap()),
                    CallArg::Pure(bcs::to_bytes(&validator.commission_rate()).unwrap()),
                ];
                let response =
                    call_0x5(context, "request_add_validator_candidate", args, &client).await?;
                SuiValidatorCommandResponse::BecomeCandidate(response)
            }

            SuiValidatorCommand::JoinCommittee => {
                let response = call_0x5(context, "request_add_validator", vec![], &client).await?;
                SuiValidatorCommandResponse::JoinCommittee(response)
            }

            SuiValidatorCommand::LeaveCommittee => {
                let response =
                    call_0x5(context, "request_remove_validator", vec![], &client).await?;
                SuiValidatorCommandResponse::LeaveCommittee(response)
            }
        });
        ret
    }
}

async fn call_0x5(
    context: &mut WalletContext,
    function: &'static str,
    call_args: Vec<CallArg>,
    sui_client: &SuiClient,
) -> anyhow::Result<SuiTransactionResponse> {
    let sender = context.active_address()?;
    let gas_obj_ref = get_gas_obj_ref(sender, sui_client).await?;
    let mut args = vec![SUI_SYSTEM_OBJ_CALL_ARG];
    args.extend(call_args);
    let rgp = sui_client
        .governance_api()
        .get_reference_gas_price()
        .await?;
    // 10k is a herustic number for gas unit
    let gas_budget = 10_000 * rgp;
    let tx_data = TransactionData::new_move_call(
        sender,
        SUI_FRAMEWORK_OBJECT_ID,
        ident_str!("sui_system").to_owned(),
        ident_str!(function).to_owned(),
        vec![],
        gas_obj_ref,
        args,
        gas_budget,
        rgp,
    )
    .unwrap();
    let signature = context
        .config
        .keystore
        .sign_secure(&sender, &tx_data, Intent::default())?;
    let transaction =
        Transaction::from_data(tx_data, Intent::default(), vec![signature]).verify()?;
    sui_client
        .quorum_driver()
        .execute_transaction(
            transaction,
            SuiTransactionResponseOptions::full_content(),
            Some(sui_types::messages::ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await
        .map_err(|err| anyhow::anyhow!(err.to_string()))
}

impl Display for SuiValidatorCommandResponse {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut writer = String::new();
        match self {
            SuiValidatorCommandResponse::MakeValidatorInfo => {}
            SuiValidatorCommandResponse::BecomeCandidate(response) => {
                write!(writer, "{}", write_transaction_response(response)?)?;
            }
            SuiValidatorCommandResponse::JoinCommittee(response) => {
                write!(writer, "{}", write_transaction_response(response)?)?;
            }
            SuiValidatorCommandResponse::LeaveCommittee(response) => {
                write!(writer, "{}", write_transaction_response(response)?)?;
            }
        }
        write!(f, "{}", writer.trim_end_matches('\n'))
    }
}

impl Debug for SuiValidatorCommandResponse {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let string = serde_json::to_string_pretty(self);
        let s = match string {
            Ok(s) => s,
            Err(err) => format!("{err}").red().to_string(),
        };
        write!(f, "{}", s)
    }
}

impl SuiValidatorCommandResponse {
    pub fn print(&self, pretty: bool) {
        let line = if pretty {
            format!("{self}")
        } else {
            format!("{:?}", self)
        };
        // Log line by line
        for line in line.lines() {
            // Logs write to a file on the side.  Print to stdout and also log to file, for tests to pass.
            println!("{line}");
            info!("{line}")
        }
    }
}
