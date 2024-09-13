// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, bail, Result};
use move_core_types::ident_str;
use std::{
    collections::{BTreeMap, HashSet},
    fmt::{self, Debug, Display, Formatter, Write},
    fs,
    path::PathBuf,
};
use sui_genesis_builder::validator_info::GenesisValidatorInfo;
use url::{ParseError, Url};

use sui_types::{
    base_types::{ObjectID, ObjectRef, SuiAddress},
    crypto::{AuthorityPublicKey, NetworkPublicKey, Signable, DEFAULT_EPOCH_ID},
    multiaddr::Multiaddr,
    object::Owner,
    sui_system_state::{
        sui_system_state_inner_v1::{UnverifiedValidatorOperationCapV1, ValidatorV1},
        sui_system_state_summary::{SuiSystemStateSummary, SuiValidatorSummary},
    },
    SUI_SYSTEM_PACKAGE_ID,
};
use tap::tap::TapOptional;

use crate::fire_drill::get_gas_obj_ref;
use clap::*;
use colored::Colorize;
use fastcrypto::traits::ToFromBytes;
use fastcrypto::{
    encoding::{Base64, Encoding},
    traits::KeyPair,
};
use serde::Serialize;
use shared_crypto::intent::{Intent, IntentMessage, IntentScope};
use sui_bridge::sui_client::SuiClient as SuiBridgeClient;
use sui_bridge::sui_transaction_builder::{
    build_committee_register_transaction, build_committee_update_url_transaction,
};
use sui_json_rpc_types::{
    SuiObjectDataOptions, SuiTransactionBlockResponse, SuiTransactionBlockResponseOptions,
};
use sui_keys::{
    key_derive::generate_new_key,
    keypair_file::{
        read_authority_keypair_from_file, read_keypair_from_file, read_network_keypair_from_file,
        write_authority_keypair_to_file, write_keypair_to_file,
    },
};
use sui_keys::{keypair_file::read_key, keystore::AccountKeystore};
use sui_sdk::wallet_context::WalletContext;
use sui_sdk::SuiClient;
use sui_types::crypto::{
    generate_proof_of_possession, get_authority_key_pair, AuthorityPublicKeyBytes,
};
use sui_types::crypto::{AuthorityKeyPair, NetworkKeyPair, SignatureScheme, SuiKeyPair};
use sui_types::transaction::{CallArg, ObjectArg, Transaction, TransactionData};

#[path = "unit_tests/validator_tests.rs"]
#[cfg(test)]
mod validator_tests;

const DEFAULT_GAS_BUDGET: u64 = 200_000_000; // 0.2 SUI

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
        #[clap(name = "gas-budget", long)]
        gas_budget: Option<u64>,
    },
    #[clap(name = "join-committee")]
    JoinCommittee {
        /// Gas budget for this transaction.
        #[clap(name = "gas-budget", long)]
        gas_budget: Option<u64>,
    },
    #[clap(name = "leave-committee")]
    LeaveCommittee {
        /// Gas budget for this transaction.
        #[clap(name = "gas-budget", long)]
        gas_budget: Option<u64>,
    },
    #[clap(name = "display-metadata")]
    DisplayMetadata {
        #[clap(name = "validator-address")]
        validator_address: Option<SuiAddress>,
        #[clap(name = "json", long)]
        json: Option<bool>,
    },
    #[clap(name = "update-metadata")]
    UpdateMetadata {
        #[clap(subcommand)]
        metadata: MetadataUpdate,
        /// Gas budget for this transaction.
        #[clap(name = "gas-budget", long)]
        gas_budget: Option<u64>,
    },
    /// Update gas price that is used to calculate Reference Gas Price
    #[clap(name = "update-gas-price")]
    UpdateGasPrice {
        /// Optional when sender is the validator itself and it holds the Cap object.
        /// Required when sender is not the validator itself.
        /// Validator's OperationCap ID can be found by using the `display-metadata` subcommand.
        #[clap(name = "operation-cap-id", long)]
        operation_cap_id: Option<ObjectID>,
        #[clap(name = "gas-price")]
        gas_price: u64,
        /// Gas budget for this transaction.
        #[clap(name = "gas-budget", long)]
        gas_budget: Option<u64>,
    },
    /// Report or un-report a validator.
    #[clap(name = "report-validator")]
    ReportValidator {
        /// Optional when sender is reporter validator itself and it holds the Cap object.
        /// Required when sender is not the reporter validator itself.
        /// Validator's OperationCap ID can be found by using the `display-metadata` subcommand.
        #[clap(name = "operation-cap-id", long)]
        operation_cap_id: Option<ObjectID>,
        /// The Sui Address of the validator is being reported or un-reported
        #[clap(name = "reportee-address")]
        reportee_address: SuiAddress,
        /// If true, undo an existing report.
        #[clap(name = "undo-report", long)]
        undo_report: Option<bool>,
        /// Gas budget for this transaction.
        #[clap(name = "gas-budget", long)]
        gas_budget: Option<u64>,
    },
    /// Serialize the payload that is used to generate Proof of Possession.
    /// This is useful to take the payload offline for an Authority protocol keypair to sign.
    #[clap(name = "serialize-payload-pop")]
    SerializePayloadForPoP {
        /// Authority account address encoded in hex with 0x prefix.
        #[clap(name = "account-address", long)]
        account_address: SuiAddress,
        /// Authority protocol public key encoded in hex.
        #[clap(name = "protocol-public-key", long)]
        protocol_public_key: AuthorityPublicKeyBytes,
    },
    /// Print out the serialized data of a transaction that sets the gas price quote for a validator.
    DisplayGasPriceUpdateRawTxn {
        /// Address of the transaction sender.
        #[clap(name = "sender-address", long)]
        sender_address: SuiAddress,
        /// Object ID of a validator's OperationCap, used for setting gas price and reportng validators.
        #[clap(name = "operation-cap-id", long)]
        operation_cap_id: ObjectID,
        /// Gas price to be set to.
        #[clap(name = "new-gas-price", long)]
        new_gas_price: u64,
        /// Gas budget for this transaction.
        #[clap(name = "gas-budget", long)]
        gas_budget: Option<u64>,
    },
    /// Sui native bridge committee member registration
    #[clap(name = "register-bridge-committee")]
    RegisterBridgeCommittee {
        /// Path to Bridge Authority Key file.
        #[clap(long)]
        bridge_authority_key_path: PathBuf,
        /// Path to Bridge Authority Network Key file.
        #[clap(long)]
        bridge_authority_network_key_path: Option<PathBuf>,
        /// Bridge authority URL which clients collects action signatures from.
        #[clap(long)]
        bridge_authority_url: String,
        /// If true, only print the unsigned transaction and do not execute it.
        /// This is useful for offline signing.
        #[clap(name = "print-only", long, default_value = "false")]
        print_unsigned_transaction_only: bool,
        /// Must present if `print_unsigned_transaction_only` is true.
        #[clap(long)]
        validator_address: Option<SuiAddress>,
        /// Gas budget for this transaction.
        #[clap(name = "gas-budget", long)]
        gas_budget: Option<u64>,
    },
    /// Update sui native bridge committee node url
    UpdateBridgeCommitteeNodeUrl {
        /// New node url to be registered in the on chain bridge object.
        #[clap(long)]
        bridge_authority_url: String,
        /// If true, only print the unsigned transaction and do not execute it.
        /// This is useful for offline signing.
        #[clap(name = "print-only", long, default_value = "false")]
        print_unsigned_transaction_only: bool,
        /// Must be present if `print_unsigned_transaction_only` is true.
        #[clap(long)]
        validator_address: Option<SuiAddress>,
        /// Gas budget for this transaction.
        #[clap(name = "gas-budget", long)]
        gas_budget: Option<u64>,
    },
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum SuiValidatorCommandResponse {
    MakeValidatorInfo,
    DisplayMetadata,
    BecomeCandidate(SuiTransactionBlockResponse),
    JoinCommittee(SuiTransactionBlockResponse),
    LeaveCommittee(SuiTransactionBlockResponse),
    UpdateMetadata(SuiTransactionBlockResponse),
    UpdateGasPrice(SuiTransactionBlockResponse),
    ReportValidator(SuiTransactionBlockResponse),
    SerializedPayload(String),
    DisplayGasPriceUpdateRawTxn {
        data: TransactionData,
        serialized_data: String,
    },
    RegisterBridgeCommittee {
        execution_response: Option<SuiTransactionBlockResponse>,
        serialized_unsigned_transaction: Option<String>,
    },
    UpdateBridgeCommitteeURL {
        execution_response: Option<SuiTransactionBlockResponse>,
        serialized_unsigned_transaction: Option<String>,
    },
}

fn make_key_files(
    file_name: PathBuf,
    is_protocol_key: bool,
    key: Option<SuiKeyPair>,
) -> Result<()> {
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
                let (_, kp, _, _) = generate_new_key(SignatureScheme::ED25519, None, None)?;
                println!("Generated new key file: {:?}.", file_name);
                kp
            }
        };
        write_keypair_to_file(&kp, &file_name)?;
    }
    Ok(())
}

impl SuiValidatorCommand {
    pub async fn execute(
        self,
        context: &mut WalletContext,
    ) -> Result<SuiValidatorCommandResponse, anyhow::Error> {
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
                    info: sui_genesis_builder::validator_info::ValidatorInfo {
                        name,
                        protocol_key: keypair.public().into(),
                        worker_key: worker_keypair.public().clone(),
                        account_address: SuiAddress::from(&account_keypair.public()),
                        network_key: network_keypair.public().clone(),
                        gas_price,
                        commission_rate: sui_config::node::DEFAULT_COMMISSION_RATE,
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
                // TODO set key files permission
                let validator_info_file_name = dir.join("validator.info");
                let validator_info_bytes = serde_yaml::to_string(&validator_info)?;
                fs::write(validator_info_file_name.clone(), validator_info_bytes)?;
                println!(
                    "Generated validator info file: {:?}.",
                    validator_info_file_name
                );
                SuiValidatorCommandResponse::MakeValidatorInfo
            }
            SuiValidatorCommand::BecomeCandidate { file, gas_budget } => {
                let gas_budget = gas_budget.unwrap_or(DEFAULT_GAS_BUDGET);
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
                    call_0x5(context, "request_add_validator_candidate", args, gas_budget).await?;
                SuiValidatorCommandResponse::BecomeCandidate(response)
            }

            SuiValidatorCommand::JoinCommittee { gas_budget } => {
                let gas_budget = gas_budget.unwrap_or(DEFAULT_GAS_BUDGET);
                let response =
                    call_0x5(context, "request_add_validator", vec![], gas_budget).await?;
                SuiValidatorCommandResponse::JoinCommittee(response)
            }

            SuiValidatorCommand::LeaveCommittee { gas_budget } => {
                // Only an active validator can leave committee.
                let _status =
                    check_status(context, HashSet::from([ValidatorStatus::Active])).await?;
                let gas_budget = gas_budget.unwrap_or(DEFAULT_GAS_BUDGET);
                let response =
                    call_0x5(context, "request_remove_validator", vec![], gas_budget).await?;
                SuiValidatorCommandResponse::LeaveCommittee(response)
            }

            SuiValidatorCommand::DisplayMetadata {
                validator_address,
                json,
            } => {
                let validator_address = validator_address.unwrap_or(context.active_address()?);
                // Default display with json serialization for better UX.
                let sui_client = context.get_client().await?;
                display_metadata(&sui_client, validator_address, json.unwrap_or(true)).await?;
                SuiValidatorCommandResponse::DisplayMetadata
            }

            SuiValidatorCommand::UpdateMetadata {
                metadata,
                gas_budget,
            } => {
                let gas_budget = gas_budget.unwrap_or(DEFAULT_GAS_BUDGET);
                let resp = update_metadata(context, metadata, gas_budget).await?;
                SuiValidatorCommandResponse::UpdateMetadata(resp)
            }

            SuiValidatorCommand::UpdateGasPrice {
                operation_cap_id,
                gas_price,
                gas_budget,
            } => {
                let gas_budget = gas_budget.unwrap_or(DEFAULT_GAS_BUDGET);
                let resp =
                    update_gas_price(context, operation_cap_id, gas_price, gas_budget).await?;
                SuiValidatorCommandResponse::UpdateGasPrice(resp)
            }

            SuiValidatorCommand::ReportValidator {
                operation_cap_id,
                reportee_address,
                undo_report,
                gas_budget,
            } => {
                let gas_budget = gas_budget.unwrap_or(DEFAULT_GAS_BUDGET);
                let undo_report = undo_report.unwrap_or(false);
                let resp = report_validator(
                    context,
                    reportee_address,
                    operation_cap_id,
                    undo_report,
                    gas_budget,
                )
                .await?;
                SuiValidatorCommandResponse::ReportValidator(resp)
            }

            SuiValidatorCommand::SerializePayloadForPoP {
                account_address,
                protocol_public_key,
            } => {
                let mut msg: Vec<u8> = Vec::new();
                msg.extend_from_slice(protocol_public_key.as_bytes());
                msg.extend_from_slice(account_address.as_ref());
                let mut intent_msg_bytes = bcs::to_bytes(&IntentMessage::new(
                    Intent::sui_app(IntentScope::ProofOfPossession),
                    msg,
                ))
                .expect("Message serialization should not fail");
                DEFAULT_EPOCH_ID.write(&mut intent_msg_bytes);
                SuiValidatorCommandResponse::SerializedPayload(Base64::encode(&intent_msg_bytes))
            }

            SuiValidatorCommand::DisplayGasPriceUpdateRawTxn {
                sender_address,
                operation_cap_id,
                new_gas_price,
                gas_budget,
            } => {
                let gas_budget = gas_budget.unwrap_or(DEFAULT_GAS_BUDGET);
                let (_status, _summary, cap_obj_ref) =
                    get_cap_object_ref(context, Some(operation_cap_id)).await?;

                let args = vec![
                    CallArg::Object(ObjectArg::ImmOrOwnedObject(cap_obj_ref)),
                    CallArg::Pure(bcs::to_bytes(&new_gas_price).unwrap()),
                ];
                let data = construct_unsigned_0x5_txn(
                    context,
                    sender_address,
                    "request_set_gas_price",
                    args,
                    gas_budget,
                )
                .await?;
                let serialized_data = Base64::encode(bcs::to_bytes(&data)?);
                SuiValidatorCommandResponse::DisplayGasPriceUpdateRawTxn {
                    data,
                    serialized_data,
                }
            }
            SuiValidatorCommand::RegisterBridgeCommittee {
                bridge_authority_key_path,
                bridge_authority_network_key_path,
                bridge_authority_url,
                print_unsigned_transaction_only,
                validator_address,
                gas_budget,
            } => {
                let parsed_url =
                    Url::parse(&bridge_authority_url).map_err(|e: ParseError| anyhow!(e))?;
                if parsed_url.scheme() != "http" && parsed_url.scheme() != "https" {
                    anyhow::bail!(
                        "URL scheme has to be http or https: {}",
                        parsed_url.scheme()
                    );
                }
                // Read bridge keypair
                let ecdsa_keypair = match read_key(&bridge_authority_key_path, true)? {
                    SuiKeyPair::Secp256k1(key) => key,
                    _ => unreachable!("we required secp256k1 key in `read_key`"),
                };
                // Read bridge network keypair
                let ecdsa_network_keypair = bridge_authority_network_key_path.map(|path| {
                    match read_key(&bridge_authority_network_key_path, true)? {
                        SuiKeyPair::Secp256k1(key) => key,
                        _ => unreachable!("we required secp256k1 key in `read_key`"),
                    }
                });
                let address = check_address(
                    context.active_address()?,
                    validator_address,
                    print_unsigned_transaction_only,
                )?;
                // Make sure the address is a validator
                let sui_client = context.get_client().await?;
                let active_validators = sui_client
                    .governance_api()
                    .get_latest_sui_system_state()
                    .await?
                    .active_validators;
                if !active_validators
                    .into_iter()
                    .any(|s| s.sui_address == address)
                {
                    bail!("Address {} is not in the committee", address);
                }
                println!("Starting bridge committee registration for Sui validator: {address}, with bridge public key: {} and url: {}", ecdsa_keypair.public, bridge_authority_url);
                let sui_rpc_url = &context.config.get_active_env().unwrap().rpc;
                let bridge_client = SuiBridgeClient::new(sui_rpc_url).await?;
                let bridge = bridge_client
                    .get_mutable_bridge_object_arg_must_succeed()
                    .await;

                let gas_budget = gas_budget.unwrap_or(DEFAULT_GAS_BUDGET);
                let (_, gas) = context
                    .gas_for_owner_budget(address, gas_budget, Default::default())
                    .await?;

                let gas_price = context.get_reference_gas_price().await?;
                let tx_data = build_committee_register_transaction(
                    address,
                    &gas.object_ref(),
                    bridge,
                    ecdsa_keypair.public().as_bytes().to_vec(),
                    ecdsa_network_keypair.map(|keypair| public().as_bytes().to_vec()),
                    &bridge_authority_url,
                    gas_price,
                    gas_budget,
                )
                .map_err(|e| anyhow!("{e:?}"))?;
                if print_unsigned_transaction_only {
                    let serialized_data = Base64::encode(bcs::to_bytes(&tx_data)?);
                    SuiValidatorCommandResponse::RegisterBridgeCommittee {
                        execution_response: None,
                        serialized_unsigned_transaction: Some(serialized_data),
                    }
                } else {
                    let tx = context.sign_transaction(&tx_data);
                    let response = context.execute_transaction_must_succeed(tx).await;
                    println!(
                        "Committee registration successful. Transaction digest: {}",
                        response.digest
                    );
                    SuiValidatorCommandResponse::RegisterBridgeCommittee {
                        execution_response: Some(response),
                        serialized_unsigned_transaction: None,
                    }
                }
            }
            SuiValidatorCommand::UpdateBridgeCommitteeNodeUrl {
                bridge_authority_url,
                print_unsigned_transaction_only,
                validator_address,
                gas_budget,
            } => {
                let parsed_url =
                    Url::parse(&bridge_authority_url).map_err(|e: ParseError| anyhow!(e))?;
                if parsed_url.scheme() != "http" && parsed_url.scheme() != "https" {
                    anyhow::bail!(
                        "URL scheme has to be http or https: {}",
                        parsed_url.scheme()
                    );
                }
                // Make sure the address is member of the committee
                let address = check_address(
                    context.active_address()?,
                    validator_address,
                    print_unsigned_transaction_only,
                )?;
                let sui_rpc_url = &context.config.get_active_env().unwrap().rpc;
                let bridge_client = SuiBridgeClient::new(sui_rpc_url).await?;
                let committee_members = bridge_client
                    .get_bridge_summary()
                    .await
                    .map_err(|e| anyhow!("{e:?}"))?
                    .committee
                    .members;
                if !committee_members
                    .into_iter()
                    .any(|(_, m)| m.sui_address == address)
                {
                    bail!("Address {} is not in the committee", address);
                }
                println!(
                    "Updating bridge committee node URL for Sui validator: {address}, url: {}",
                    bridge_authority_url
                );

                let bridge = bridge_client
                    .get_mutable_bridge_object_arg_must_succeed()
                    .await;

                let gas_budget = gas_budget.unwrap_or(DEFAULT_GAS_BUDGET);
                let (_, gas) = context
                    .gas_for_owner_budget(address, gas_budget, Default::default())
                    .await?;

                let gas_price = context.get_reference_gas_price().await?;
                let tx_data = build_committee_update_url_transaction(
                    address,
                    &gas.object_ref(),
                    bridge,
                    &bridge_authority_url,
                    gas_price,
                    gas_budget,
                )
                .map_err(|e| anyhow!("{e:?}"))?;
                if print_unsigned_transaction_only {
                    let serialized_data = Base64::encode(bcs::to_bytes(&tx_data)?);
                    SuiValidatorCommandResponse::UpdateBridgeCommitteeURL {
                        execution_response: None,
                        serialized_unsigned_transaction: Some(serialized_data),
                    }
                } else {
                    let tx = context.sign_transaction(&tx_data);
                    let response = context.execute_transaction_must_succeed(tx).await;
                    println!(
                        "Update Bridge validator node URL successful. Transaction digest: {}",
                        response.digest
                    );
                    SuiValidatorCommandResponse::UpdateBridgeCommitteeURL {
                        execution_response: Some(response),
                        serialized_unsigned_transaction: None,
                    }
                }
            }
        });
        ret
    }
}

fn check_address(
    active_address: SuiAddress,
    validator_address: Option<SuiAddress>,
    print_unsigned_transaction_only: bool,
) -> Result<SuiAddress, anyhow::Error> {
    if !print_unsigned_transaction_only {
        if let Some(validator_address) = validator_address {
            if validator_address != active_address {
                bail!(
                    "`--validator-address` must be the same as the current active address: {}",
                    active_address
                );
            }
        }
        Ok(active_address)
    } else {
        validator_address
            .ok_or_else(|| anyhow!("--validator-address must be provided when `print_unsigned_transaction_only` is true"))
    }
}

async fn get_cap_object_ref(
    context: &mut WalletContext,
    operation_cap_id: Option<ObjectID>,
) -> Result<(ValidatorStatus, SuiValidatorSummary, ObjectRef)> {
    let sui_client = context.get_client().await?;
    if let Some(operation_cap_id) = operation_cap_id {
        let (status, summary) =
            get_validator_summary_from_cap_id(&sui_client, operation_cap_id).await?;
        let cap_obj_ref = sui_client
            .read_api()
            .get_object_with_options(
                summary.operation_cap_id,
                SuiObjectDataOptions::default().with_owner(),
            )
            .await?
            .object_ref_if_exists()
            .ok_or_else(|| anyhow!("OperationCap {} does not exist", operation_cap_id))?;
        Ok::<(ValidatorStatus, SuiValidatorSummary, ObjectRef), anyhow::Error>((
            status,
            summary,
            cap_obj_ref,
        ))
    } else {
        // Sender is Reporter Validator itself.
        let validator_address = context.active_address()?;
        let (status, summary) = get_validator_summary(&sui_client, validator_address)
            .await?
            .ok_or_else(|| anyhow::anyhow!("{} is not a validator.", validator_address))?;
        // TODO we should allow validator to perform this operation even though the Cap is not at hand.
        // But for now we need to make sure the cap is owned by the sender.
        let cap_object_id = summary.operation_cap_id;
        let resp = sui_client
            .read_api()
            .get_object_with_options(cap_object_id, SuiObjectDataOptions::default().with_owner())
            .await
            .map_err(|e| anyhow!(e))?;
        // Safe to unwrap as we ask with `with_owner`.
        let owner = resp.owner().unwrap();
        let cap_obj_ref = resp
            .object_ref_if_exists()
            .unwrap_or_else(|| panic!("OperationCap {} shall exist.", cap_object_id));
        if owner != Owner::AddressOwner(context.active_address()?) {
            anyhow::bail!(
                "OperationCap {} is not owned by the sender address {} but {:?}",
                summary.operation_cap_id,
                validator_address,
                owner
            );
        }
        Ok((status, summary, cap_obj_ref))
    }
}

async fn update_gas_price(
    context: &mut WalletContext,
    operation_cap_id: Option<ObjectID>,
    gas_price: u64,
    gas_budget: u64,
) -> Result<SuiTransactionBlockResponse> {
    let (_status, _summary, cap_obj_ref) = get_cap_object_ref(context, operation_cap_id).await?;

    // TODO: Only active/pending validators can set gas price.

    let args = vec![
        CallArg::Object(ObjectArg::ImmOrOwnedObject(cap_obj_ref)),
        CallArg::Pure(bcs::to_bytes(&gas_price).unwrap()),
    ];
    call_0x5(context, "request_set_gas_price", args, gas_budget).await
}

async fn report_validator(
    context: &mut WalletContext,
    reportee_address: SuiAddress,
    operation_cap_id: Option<ObjectID>,
    undo_report: bool,
    gas_budget: u64,
) -> Result<SuiTransactionBlockResponse> {
    let (status, summary, cap_obj_ref) = get_cap_object_ref(context, operation_cap_id).await?;

    let validator_address = summary.sui_address;
    // Only active validators can report/un-report.
    if !matches!(status, ValidatorStatus::Active) {
        anyhow::bail!(
            "Only active Validator can report/un-report Validators, but {} is {:?}.",
            validator_address,
            status
        );
    }
    let args = vec![
        CallArg::Object(ObjectArg::ImmOrOwnedObject(cap_obj_ref)),
        CallArg::Pure(bcs::to_bytes(&reportee_address).unwrap()),
    ];
    let function_name = if undo_report {
        "undo_report_validator"
    } else {
        "report_validator"
    };
    call_0x5(context, function_name, args, gas_budget).await
}

async fn get_validator_summary_from_cap_id(
    client: &SuiClient,
    operation_cap_id: ObjectID,
) -> anyhow::Result<(ValidatorStatus, SuiValidatorSummary)> {
    let resp = client
        .read_api()
        .get_object_with_options(operation_cap_id, SuiObjectDataOptions::default().with_bcs())
        .await?;
    let bcs = resp.move_object_bcs().ok_or_else(|| {
        anyhow::anyhow!(
            "Object {} does not exist or does not return bcs bytes",
            operation_cap_id
        )
    })?;
    let cap = bcs::from_bytes::<UnverifiedValidatorOperationCapV1>(bcs).map_err(|e| {
        anyhow::anyhow!(
            "Can't convert bcs bytes of object {} to UnverifiedValidatorOperationCapV1: {}",
            operation_cap_id,
            e,
        )
    })?;
    let validator_address = cap.authorizer_validator_address;
    let (status, summary) = get_validator_summary(client, validator_address)
        .await?
        .ok_or_else(|| anyhow::anyhow!("{} is not a validator", validator_address))?;
    if summary.operation_cap_id != operation_cap_id {
        anyhow::bail!(
            "Validator {}'s current operation cap id is {}",
            validator_address,
            summary.operation_cap_id
        );
    }
    Ok((status, summary))
}

async fn construct_unsigned_0x5_txn(
    context: &mut WalletContext,
    sender: SuiAddress,
    function: &'static str,
    call_args: Vec<CallArg>,
    gas_budget: u64,
) -> anyhow::Result<TransactionData> {
    let sui_client = context.get_client().await?;
    let mut args = vec![CallArg::SUI_SYSTEM_MUT];
    args.extend(call_args);
    let rgp = sui_client
        .governance_api()
        .get_reference_gas_price()
        .await?;

    let gas_obj_ref = get_gas_obj_ref(sender, &sui_client, gas_budget).await?;
    TransactionData::new_move_call(
        sender,
        SUI_SYSTEM_PACKAGE_ID,
        ident_str!("sui_system").to_owned(),
        ident_str!(function).to_owned(),
        vec![],
        gas_obj_ref,
        args,
        gas_budget,
        rgp,
    )
}

async fn call_0x5(
    context: &mut WalletContext,
    function: &'static str,
    call_args: Vec<CallArg>,
    gas_budget: u64,
) -> anyhow::Result<SuiTransactionBlockResponse> {
    let sender = context.active_address()?;
    let tx_data =
        construct_unsigned_0x5_txn(context, sender, function, call_args, gas_budget).await?;
    let signature =
        context
            .config
            .keystore
            .sign_secure(&sender, &tx_data, Intent::sui_transaction())?;
    let transaction = Transaction::from_data(tx_data, vec![signature]);
    let sui_client = context.get_client().await?;
    sui_client
        .quorum_driver_api()
        .execute_transaction_block(
            transaction,
            SuiTransactionBlockResponseOptions::new()
                .with_input()
                .with_effects(),
            Some(sui_types::quorum_driver_types::ExecuteTransactionRequestType::WaitForLocalExecution),
        )
        .await
        .map_err(|err| anyhow::anyhow!(err.to_string()))
}

impl Display for SuiValidatorCommandResponse {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut writer = String::new();
        match self {
            SuiValidatorCommandResponse::MakeValidatorInfo => {}
            SuiValidatorCommandResponse::DisplayMetadata => {}
            SuiValidatorCommandResponse::BecomeCandidate(response) => {
                write!(writer, "{}", write_transaction_response(response)?)?;
            }
            SuiValidatorCommandResponse::JoinCommittee(response) => {
                write!(writer, "{}", write_transaction_response(response)?)?;
            }
            SuiValidatorCommandResponse::LeaveCommittee(response) => {
                write!(writer, "{}", write_transaction_response(response)?)?;
            }
            SuiValidatorCommandResponse::UpdateMetadata(response) => {
                write!(writer, "{}", write_transaction_response(response)?)?;
            }
            SuiValidatorCommandResponse::UpdateGasPrice(response) => {
                write!(writer, "{}", write_transaction_response(response)?)?;
            }
            SuiValidatorCommandResponse::ReportValidator(response) => {
                write!(writer, "{}", write_transaction_response(response)?)?;
            }
            SuiValidatorCommandResponse::SerializedPayload(response) => {
                write!(writer, "Serialized payload: {}", response)?;
            }
            SuiValidatorCommandResponse::DisplayGasPriceUpdateRawTxn {
                data,
                serialized_data,
            } => {
                write!(
                    writer,
                    "Transaction: {:?}, \nSerialized transaction: {:?}",
                    data, serialized_data
                )?;
            }
            SuiValidatorCommandResponse::RegisterBridgeCommittee {
                execution_response,
                serialized_unsigned_transaction,
            }
            | SuiValidatorCommandResponse::UpdateBridgeCommitteeURL {
                execution_response,
                serialized_unsigned_transaction,
            } => {
                if let Some(response) = execution_response {
                    write!(writer, "{}", write_transaction_response(response)?)?;
                } else {
                    write!(
                        writer,
                        "Serialized transaction for signing: {:?}",
                        serialized_unsigned_transaction
                    )?;
                }
            }
        }
        write!(f, "{}", writer.trim_end_matches('\n'))
    }
}

pub fn write_transaction_response(
    response: &SuiTransactionBlockResponse,
) -> Result<String, fmt::Error> {
    // we requested with for full_content, so the following content should be available.
    let success = response.status_ok().unwrap();
    let lines = vec![
        String::from("----- Transaction Digest ----"),
        response.digest.to_string(),
        String::from("\n----- Transaction Data ----"),
        response.transaction.as_ref().unwrap().to_string(),
        String::from("----- Transaction Effects ----"),
        response.effects.as_ref().unwrap().to_string(),
    ];
    let mut writer = String::new();
    for line in lines {
        let colorized_line = if success { line.green() } else { line.red() };
        writeln!(writer, "{}", colorized_line)?;
    }
    Ok(writer)
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
        match self {
            // Don't print empty responses
            SuiValidatorCommandResponse::MakeValidatorInfo
            | SuiValidatorCommandResponse::DisplayMetadata => {}
            other => {
                let line = if pretty {
                    format!("{other}")
                } else {
                    format!("{:?}", other)
                };
                // Log line by line
                for line in line.lines() {
                    println!("{line}");
                }
            }
        }
    }
}

#[derive(Debug, Hash, PartialEq, Eq)]
pub enum ValidatorStatus {
    Active,
    Pending,
}

pub async fn get_validator_summary(
    client: &SuiClient,
    validator_address: SuiAddress,
) -> anyhow::Result<Option<(ValidatorStatus, SuiValidatorSummary)>> {
    let SuiSystemStateSummary {
        active_validators,
        pending_active_validators_id,
        ..
    } = client
        .governance_api()
        .get_latest_sui_system_state()
        .await?;
    let mut status = None;
    let mut active_validators = active_validators
        .into_iter()
        .map(|s| (s.sui_address, s))
        .collect::<BTreeMap<_, _>>();
    let validator_info = if active_validators.contains_key(&validator_address) {
        status = Some(ValidatorStatus::Active);
        Some(active_validators.remove(&validator_address).unwrap())
    } else {
        // Check panding validators
        get_pending_candidate_summary(validator_address, client, pending_active_validators_id)
            .await?
            .map(|v| v.into_sui_validator_summary())
            .tap_some(|_s| status = Some(ValidatorStatus::Pending))

        // TODO also check candidate and inactive valdiators
    };
    if validator_info.is_none() {
        return Ok(None);
    }
    // status is safe unwrap because it has to be Some when the code recahes here
    // validator_info is safe to unwrap because of the above check
    Ok(Some((status.unwrap(), validator_info.unwrap())))
}

async fn display_metadata(
    client: &SuiClient,
    validator_address: SuiAddress,
    json: bool,
) -> anyhow::Result<()> {
    match get_validator_summary(client, validator_address).await? {
        None => println!(
            "{} is not an active or pending Validator.",
            validator_address
        ),
        Some((status, info)) => {
            println!("{}'s valdiator status: {:?}", validator_address, status);
            if json {
                println!("{}", serde_json::to_string_pretty(&info)?);
            } else {
                println!("{:#?}", info);
            }
        }
    }
    Ok(())
}

async fn get_pending_candidate_summary(
    validator_address: SuiAddress,
    sui_client: &SuiClient,
    pending_active_validators_id: ObjectID,
) -> anyhow::Result<Option<ValidatorV1>> {
    let pending_validators = sui_client
        .read_api()
        .get_dynamic_fields(pending_active_validators_id, None, None)
        .await?
        .data
        .into_iter()
        .map(|dyi| dyi.object_id)
        .collect::<Vec<_>>();
    let resps = sui_client
        .read_api()
        .multi_get_object_with_options(
            pending_validators,
            SuiObjectDataOptions::default().with_bcs(),
        )
        .await?;
    for resp in resps {
        // We always expect an objectId from the response as one of data/error should be included.
        let object_id = resp.object_id()?;
        let bcs = resp.move_object_bcs().ok_or_else(|| {
            anyhow::anyhow!(
                "Object {} does not exist or does not return bcs bytes",
                object_id
            )
        })?;
        let val = bcs::from_bytes::<ValidatorV1>(bcs).map_err(|e| {
            anyhow::anyhow!(
                "Can't convert bcs bytes of object {} to ValidatorV1: {}",
                object_id,
                e,
            )
        })?;
        if val.verified_metadata().sui_address == validator_address {
            return Ok(Some(val));
        }
    }
    Ok(None)
}

#[derive(Subcommand)]
#[clap(rename_all = "kebab-case")]
pub enum MetadataUpdate {
    /// Update name. Effectuate immediately.
    Name { name: String },
    /// Update description. Effectuate immediately.
    Description { description: String },
    /// Update Image URL. Effectuate immediately.
    ImageUrl { image_url: String },
    /// Update Project URL. Effectuate immediately.
    ProjectUrl { project_url: String },
    /// Update Network Address. Effectuate from next epoch.
    NetworkAddress { network_address: Multiaddr },
    /// Update Primary Address. Effectuate from next epoch.
    PrimaryAddress { primary_address: Multiaddr },
    /// Update Worker Address. Effectuate from next epoch.
    WorkerAddress { worker_address: Multiaddr },
    /// Update P2P Address. Effectuate from next epoch.
    P2pAddress { p2p_address: Multiaddr },
    /// Update Network Public Key. Effectuate from next epoch.
    NetworkPubKey {
        #[clap(name = "network-key-path")]
        file: PathBuf,
    },
    /// Update Worker Public Key. Effectuate from next epoch.
    WorkerPubKey {
        #[clap(name = "worker-key-path")]
        file: PathBuf,
    },
    /// Update Protocol Public Key and Proof and Possession. Effectuate from next epoch.
    ProtocolPubKey {
        #[clap(name = "protocol-key-path")]
        file: PathBuf,
    },
}

async fn update_metadata(
    context: &mut WalletContext,
    metadata: MetadataUpdate,
    gas_budget: u64,
) -> anyhow::Result<SuiTransactionBlockResponse> {
    use ValidatorStatus::*;
    match metadata {
        MetadataUpdate::Name { name } => {
            let args = vec![CallArg::Pure(bcs::to_bytes(&name.into_bytes()).unwrap())];
            call_0x5(context, "update_validator_name", args, gas_budget).await
        }
        MetadataUpdate::Description { description } => {
            let args = vec![CallArg::Pure(
                bcs::to_bytes(&description.into_bytes()).unwrap(),
            )];
            call_0x5(context, "update_validator_description", args, gas_budget).await
        }
        MetadataUpdate::ImageUrl { image_url } => {
            let args = vec![CallArg::Pure(
                bcs::to_bytes(&image_url.into_bytes()).unwrap(),
            )];
            call_0x5(context, "update_validator_image_url", args, gas_budget).await
        }
        MetadataUpdate::ProjectUrl { project_url } => {
            let args = vec![CallArg::Pure(
                bcs::to_bytes(&project_url.into_bytes()).unwrap(),
            )];
            call_0x5(context, "update_validator_project_url", args, gas_budget).await
        }
        MetadataUpdate::NetworkAddress { network_address } => {
            // Check the network address to be in TCP.
            if !network_address.is_loosely_valid_tcp_addr() {
                bail!("Network address must be a TCP address");
            }
            let _status = check_status(context, HashSet::from([Pending, Active])).await?;
            let args = vec![CallArg::Pure(bcs::to_bytes(&network_address).unwrap())];
            call_0x5(
                context,
                "update_validator_next_epoch_network_address",
                args,
                gas_budget,
            )
            .await
        }
        MetadataUpdate::PrimaryAddress { primary_address } => {
            primary_address.to_anemo_address().map_err(|_| {
                anyhow!("Invalid primary address, it must look like `/[ip4,ip6,dns]/.../udp/port`")
            })?;
            let _status = check_status(context, HashSet::from([Pending, Active])).await?;
            let args = vec![CallArg::Pure(bcs::to_bytes(&primary_address).unwrap())];
            call_0x5(
                context,
                "update_validator_next_epoch_primary_address",
                args,
                gas_budget,
            )
            .await
        }
        MetadataUpdate::WorkerAddress { worker_address } => {
            worker_address.to_anemo_address().map_err(|_| {
                anyhow!("Invalid worker address, it must look like `/[ip4,ip6,dns]/.../udp/port`")
            })?;
            // Only an active validator can leave committee.
            let _status = check_status(context, HashSet::from([Pending, Active])).await?;
            let args = vec![CallArg::Pure(bcs::to_bytes(&worker_address).unwrap())];
            call_0x5(
                context,
                "update_validator_next_epoch_worker_address",
                args,
                gas_budget,
            )
            .await
        }
        MetadataUpdate::P2pAddress { p2p_address } => {
            p2p_address.to_anemo_address().map_err(|_| {
                anyhow!("Invalid p2p address, it must look like `/[ip4,ip6,dns]/.../udp/port`")
            })?;
            let _status = check_status(context, HashSet::from([Pending, Active])).await?;
            let args = vec![CallArg::Pure(bcs::to_bytes(&p2p_address).unwrap())];
            call_0x5(
                context,
                "update_validator_next_epoch_p2p_address",
                args,
                gas_budget,
            )
            .await
        }
        MetadataUpdate::NetworkPubKey { file } => {
            let _status = check_status(context, HashSet::from([Pending, Active])).await?;
            let network_pub_key: NetworkPublicKey =
                read_network_keypair_from_file(file)?.public().clone();
            let args = vec![CallArg::Pure(
                bcs::to_bytes(&network_pub_key.as_bytes().to_vec()).unwrap(),
            )];
            call_0x5(
                context,
                "update_validator_next_epoch_network_pubkey",
                args,
                gas_budget,
            )
            .await
        }
        MetadataUpdate::WorkerPubKey { file } => {
            let _status = check_status(context, HashSet::from([Pending, Active])).await?;
            let worker_pub_key: NetworkPublicKey =
                read_network_keypair_from_file(file)?.public().clone();
            let args = vec![CallArg::Pure(
                bcs::to_bytes(&worker_pub_key.as_bytes().to_vec()).unwrap(),
            )];
            call_0x5(
                context,
                "update_validator_next_epoch_worker_pubkey",
                args,
                gas_budget,
            )
            .await
        }
        MetadataUpdate::ProtocolPubKey { file } => {
            let _status = check_status(context, HashSet::from([Pending, Active])).await?;
            let sui_address = context.active_address()?;
            let protocol_key_pair: AuthorityKeyPair = read_authority_keypair_from_file(file)?;
            let protocol_pub_key: AuthorityPublicKey = protocol_key_pair.public().clone();
            let pop = generate_proof_of_possession(&protocol_key_pair, sui_address);
            let args = vec![
                CallArg::Pure(
                    bcs::to_bytes(&AuthorityPublicKeyBytes::from_bytes(
                        protocol_pub_key.as_bytes(),
                    )?)
                    .unwrap(),
                ),
                CallArg::Pure(bcs::to_bytes(&pop.as_ref().to_vec()).unwrap()),
            ];
            call_0x5(
                context,
                "update_validator_next_epoch_protocol_pubkey",
                args,
                gas_budget,
            )
            .await
        }
    }
}

async fn check_status(
    context: &mut WalletContext,
    allowed_status: HashSet<ValidatorStatus>,
) -> Result<ValidatorStatus> {
    let sui_client = context.get_client().await?;
    let validator_address = context.active_address()?;
    let summary = get_validator_summary(&sui_client, validator_address).await?;
    if summary.is_none() {
        bail!("{validator_address} is not a Validator.");
    }
    let (status, _summary) = summary.unwrap();
    if allowed_status.contains(&status) {
        return Ok(status);
    }
    bail!("Validator {validator_address} is {:?}, this operation is not supported in this tool or prohibited.", status)
}
