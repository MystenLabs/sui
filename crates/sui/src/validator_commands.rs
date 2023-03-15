// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    fs,
    fmt::{Debug, Display, Formatter, Write},
    path::PathBuf,
};
use move_core_types::ident_str;
use sui_config::genesis::GenesisValidatorInfo;

use crate::client_commands::{WalletContext, write_transaction_response};
use crate::fire_drill::get_gas_obj_ref;
use clap::*;
use colored::Colorize;
use fastcrypto::traits::KeyPair;
use fastcrypto::{
    traits::ToFromBytes,
};
use serde::Serialize;
use sui_types::{SUI_FRAMEWORK_OBJECT_ID, SUI_SYSTEM_OBJ_CALL_ARG};

use shared_crypto::intent::Intent;
use sui_json_rpc_types::{
    SuiTransactionResponse, SuiTransactionResponseOptions
};
use sui_keys::keystore::AccountKeystore;
use sui_sdk::SuiClient;
use sui_types::crypto::{generate_proof_of_possession, get_authority_key_pair, AuthorityPublicKeyBytes};
use sui_types::messages::{CallArg, TransactionData};
use sui_types::messages::Transaction;
use tracing::info;


#[derive(Parser)]
#[clap(rename_all = "kebab-case")]
pub enum SuiValidatorCommand {
    #[clap(name = "join-committee")]
    JoinCommittee {
        #[clap(name = "validator-info-path")]
        file: PathBuf,
    },
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum SuiValidatorCommandResponse {
    JoinCommittee(SuiTransactionResponse),
}

impl SuiValidatorCommand {
    pub async fn execute(
        self,
        context: &mut WalletContext,
    ) -> Result<SuiValidatorCommandResponse, anyhow::Error> {
        let ret = Ok(match self {
            SuiValidatorCommand::JoinCommittee {
                file
            } => {
                let client = context.get_client().await?;
                let validator_info_bytes = fs::read(file)?;
                // Note: we should probably rename the struct or evolve it accordingly.
                let validator_info: GenesisValidatorInfo =
                    serde_yaml::from_slice(&validator_info_bytes)?;
                let validator = validator_info.info;
                
                // FIXME remove these
                let new_protocol_key_pair = get_authority_key_pair().1;
                let pop = generate_proof_of_possession(&new_protocol_key_pair, context.active_address()?);

                let args = vec![
                    CallArg::Pure(bcs::to_bytes(&AuthorityPublicKeyBytes::from_bytes(new_protocol_key_pair.public().as_bytes())?).unwrap()),
                    CallArg::Pure(bcs::to_bytes(&validator.network_key().as_bytes().to_vec()).unwrap()),
                    CallArg::Pure(bcs::to_bytes(&validator.worker_key().as_bytes().to_vec()).unwrap()),
                    CallArg::Pure(bcs::to_bytes(&pop.as_ref().to_vec()).unwrap()),
                    CallArg::Pure(bcs::to_bytes(&validator.name().to_owned().into_bytes()).unwrap()),
                    CallArg::Pure(bcs::to_bytes(&validator.description.clone().into_bytes()).unwrap()),
                    CallArg::Pure(bcs::to_bytes(&validator.image_url.clone().into_bytes()).unwrap()),
                    CallArg::Pure(bcs::to_bytes(&validator.project_url.clone().into_bytes()).unwrap()),
                    CallArg::Pure(bcs::to_bytes(&validator.network_address().to_vec()).unwrap()),
                    CallArg::Pure(bcs::to_bytes(&validator.p2p_address().to_vec()).unwrap()),
                    CallArg::Pure(bcs::to_bytes(&validator.narwhal_primary_address().to_vec()).unwrap()),
                    CallArg::Pure(bcs::to_bytes(&validator.narwhal_worker_address().to_vec()).unwrap()),
                    CallArg::Pure(bcs::to_bytes(&validator.gas_price()).unwrap()),
                    CallArg::Pure(bcs::to_bytes(&validator.commission_rate()).unwrap()),
                ];
                let response = call_0x5(context, "request_add_validator_candidate", args, &client).await?;
                SuiValidatorCommandResponse::JoinCommittee(response)
            }
            _ => {
                panic!()
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
    let tx_data = TransactionData::new_move_call(
        sender,
        SUI_FRAMEWORK_OBJECT_ID,
        ident_str!("sui_system").to_owned(),
        ident_str!(function).to_owned(),
        vec![],
        gas_obj_ref,
        args,
        5000,
        // FIXME
        1,
    )
    .unwrap();
    let signature = context
        .config
        .keystore
        .sign_secure(&sender, &tx_data, Intent::default())?;
    let transaction = Transaction::from_data(tx_data, Intent::default(), vec![signature]).verify()?;
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
            SuiValidatorCommandResponse::JoinCommittee(response) => {
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
            Ok(s) => format!("{s}"),
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