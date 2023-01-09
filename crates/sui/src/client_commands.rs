// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use core::fmt;
use std::sync::Arc;
use std::{
    collections::BTreeSet,
    fmt::{Debug, Display, Formatter, Write},
    path::{Path, PathBuf},
    time::Instant,
};

use crate::config::{Config, PersistedConfig, SuiClientConfig, SuiEnv};
use anyhow::{anyhow, ensure};
use bip32::DerivationPath;
use clap::*;
use colored::Colorize;
use fastcrypto::{
    encoding::{Base64, Encoding},
    traits::ToFromBytes,
};
use move_core_types::language_storage::TypeTag;
use move_package::BuildConfig as MoveBuildConfig;
use prettytable::Table;
use prettytable::{row, table};
use serde::Serialize;
use serde_json::{json, Value};
use sui_framework::build_move_package;
use sui_source_validation::{BytecodeSourceVerifier, SourceMode};
use sui_types::error::SuiError;

use sui_framework_build::compiled_package::BuildConfig;
use sui_json::SuiJsonValue;
use sui_json_rpc_types::{
    DynamicFieldPage, GetObjectDataResponse, SuiObjectInfo, SuiParsedObject, SuiTransactionResponse,
};
use sui_json_rpc_types::{GetRawObjectDataResponse, SuiData};
use sui_json_rpc_types::{SuiCertifiedTransaction, SuiExecutionStatus, SuiTransactionEffects};
use sui_keys::keystore::AccountKeystore;
use sui_sdk::SuiClient;
use sui_sdk::TransactionExecutionResult;
use sui_types::dynamic_field::DynamicFieldType;
use sui_types::intent::Intent;
use sui_types::multisig::GenericSignature;
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    gas_coin::GasCoin,
    messages::{Transaction, VerifiedTransaction},
    object::Owner,
    parse_sui_type_tag, SUI_FRAMEWORK_ADDRESS,
};
use sui_types::{crypto::SignatureScheme, intent::IntentMessage};
use tokio::sync::RwLock;
use tracing::{info, warn};

pub const EXAMPLE_NFT_NAME: &str = "Example NFT";
pub const EXAMPLE_NFT_DESCRIPTION: &str = "An NFT created by the Sui Command Line Tool";
pub const EXAMPLE_NFT_URL: &str =
    "ipfs://bafkreibngqhl3gaa7daob4i2vccziay2jjlp435cf66vhono7nrvww53ty";

#[derive(Parser)]
#[clap(rename_all = "kebab-case")]
pub enum SuiClientCommands {
    /// Switch active address and network(e.g., devnet, local rpc server)
    #[clap(name = "switch")]
    Switch {
        /// An Sui address to be used as the active address for subsequent
        /// commands.
        #[clap(long)]
        address: Option<SuiAddress>,
        /// The RPC server URL (e.g., local rpc server, devnet rpc server, etc) to be
        /// used for subsequent commands.
        #[clap(long)]
        env: Option<String>,
    },
    /// Add new Sui environment.
    #[clap(name = "new-env")]
    NewEnv {
        #[clap(long)]
        alias: String,
        #[clap(long, value_hint = ValueHint::Url)]
        rpc: String,
        #[clap(long, value_hint = ValueHint::Url)]
        ws: Option<String>,
    },
    /// List all Sui environments
    Envs,

    /// Default address used for commands when none specified
    #[clap(name = "active-address")]
    ActiveAddress,

    /// Default environment used for commands when none specified
    #[clap(name = "active-env")]
    ActiveEnv,

    /// Get object info
    #[clap(name = "object")]
    Object {
        /// Object ID of the object to fetch
        #[clap(name = "object_id")]
        id: ObjectID,

        /// Return the bcs serialized version of the object
        #[clap(long)]
        bcs: bool,
    },

    /// Publish Move modules
    #[clap(name = "publish")]
    Publish {
        /// Path to directory containing a Move package
        #[clap(
            name = "package_path",
            global = true,
            parse(from_os_str),
            default_value = "."
        )]
        package_path: PathBuf,

        /// Package build options
        #[clap(flatten)]
        build_config: MoveBuildConfig,

        /// ID of the gas object for gas payment, in 20 bytes Hex string
        /// If not provided, a gas object with at least gas_budget value will be selected
        #[clap(long)]
        gas: Option<ObjectID>,

        /// Gas budget for running module initializers
        #[clap(long)]
        gas_budget: u64,

        /// (Deprecated) This flag is deprecated, dependency verification is on by default.
        #[clap(long)]
        verify_dependencies: bool,

        /// Publish the package without checking whether compiling dependencies from source results
        /// in bytecode matching the dependencies found on-chain.
        #[clap(long)]
        skip_dependency_verification: bool,

        /// Also publish transitive dependencies that have not already been published.
        #[clap(long)]
        with_unpublished_dependencies: bool,
    },

    /// Verify local Move packages against on-chain packages, and optionally their dependencies.
    #[clap(name = "verify-source")]
    VerifySource {
        /// Path to directory containing a Move package
        #[clap(
            name = "package_path",
            global = true,
            parse(from_os_str),
            default_value = "."
        )]
        package_path: PathBuf,

        /// Package build options
        #[clap(flatten)]
        build_config: MoveBuildConfig,

        /// Verify on-chain dependencies.
        #[clap(long)]
        verify_deps: bool,

        /// Don't verify source (only valid if --verify-deps is enabled).
        #[clap(long)]
        skip_source: bool,

        /// If specified, override the addresses for the package's own modules with this address.
        /// Only works for unpublished modules (whose addresses are currently 0x0).
        #[clap(long)]
        address_override: Option<ObjectID>,
    },

    /// Call Move function
    #[clap(name = "call")]
    Call {
        /// Object ID of the package, which contains the module
        #[clap(long)]
        package: ObjectID,
        /// The name of the module in the package
        #[clap(long)]
        module: String,
        /// Function name in module
        #[clap(long)]
        function: String,
        /// Function name in module
        #[clap(
        long,
        parse(try_from_str = parse_sui_type_tag),
        multiple_occurrences = false,
        multiple_values = true
        )]
        type_args: Vec<TypeTag>,
        /// Simplified ordered args like in the function syntax
        /// ObjectIDs, Addresses must be hex strings
        #[clap(long, multiple_occurrences = false, multiple_values = true)]
        args: Vec<SuiJsonValue>,
        /// ID of the gas object for gas payment, in 20 bytes Hex string
        #[clap(long)]
        /// If not provided, a gas object with at least gas_budget value will be selected
        #[clap(long)]
        gas: Option<ObjectID>,
        /// Gas budget for this call
        #[clap(long)]
        gas_budget: u64,
    },

    /// Transfer object
    #[clap(name = "transfer")]
    Transfer {
        /// Recipient address
        #[clap(long)]
        to: SuiAddress,

        /// Object to transfer, in 20 bytes Hex string
        #[clap(long)]
        object_id: ObjectID,

        /// ID of the gas object for gas payment, in 20 bytes Hex string
        /// If not provided, a gas object with at least gas_budget value will be selected
        #[clap(long)]
        gas: Option<ObjectID>,

        /// Gas budget for this transfer
        #[clap(long)]
        gas_budget: u64,
    },
    /// Transfer SUI, and pay gas with the same SUI coin object.
    /// If amount is specified, only the amount is transferred; otherwise the entire object
    /// is transferred.
    #[clap(name = "transfer-sui")]
    TransferSui {
        /// Recipient address
        #[clap(long)]
        to: SuiAddress,

        /// Sui coin object to transfer, ID in 20 bytes Hex string. This is also the gas object.
        #[clap(long)]
        sui_coin_object_id: ObjectID,

        /// Gas budget for this transfer
        #[clap(long)]
        gas_budget: u64,

        /// The amount to transfer, if not specified, the entire coin object will be transferred.
        #[clap(long)]
        amount: Option<u64>,
    },
    /// Pay coins to recipients following specified amounts, with input coins.
    /// Length of recipients must be the same as that of amounts.
    #[clap(name = "pay")]
    Pay {
        /// The input coins to be used for pay recipients, following the specified amounts.
        #[clap(long, multiple_occurrences = false, multiple_values = true)]
        input_coins: Vec<ObjectID>,

        /// The recipient addresses, must be of same length as amounts
        #[clap(long, multiple_occurrences = false, multiple_values = true)]
        recipients: Vec<SuiAddress>,

        /// The amounts to be paid, following the order of recipients.
        #[clap(long, multiple_occurrences = false, multiple_values = true)]
        amounts: Vec<u64>,

        /// ID of the gas object for gas payment, in 20 bytes Hex string
        /// If not provided, a gas object with at least gas_budget value will be selected
        #[clap(long)]
        gas: Option<ObjectID>,

        /// Gas budget for this transaction
        #[clap(long)]
        gas_budget: u64,
    },

    /// Pay SUI coins to recipients following following specified amounts, with input coins.
    /// Length of recipients must be the same as that of amounts.
    /// The input coins also include the coin for gas payment, so no extra gas coin is required.
    #[clap(name = "pay_sui")]
    PaySui {
        /// The input coins to be used for pay recipients, including the gas coin.
        #[clap(long, multiple_occurrences = false, multiple_values = true)]
        input_coins: Vec<ObjectID>,

        /// The recipient addresses, must be of same length as amounts.
        #[clap(long, multiple_occurrences = false, multiple_values = true)]
        recipients: Vec<SuiAddress>,

        /// The amounts to be paid, following the order of recipients.
        #[clap(long, multiple_occurrences = false, multiple_values = true)]
        amounts: Vec<u64>,

        /// Gas budget for this transaction
        #[clap(long)]
        gas_budget: u64,
    },

    /// Pay all residual SUI coins to the recipient with input coins, after deducting the gas cost.
    /// The input coins also include the coin for gas payment, so no extra gas coin is required.
    #[clap(name = "pay_all_sui")]
    PayAllSui {
        /// The input coins to be used for pay recipients, including the gas coin.
        #[clap(long, multiple_occurrences = false, multiple_values = true)]
        input_coins: Vec<ObjectID>,

        /// The recipient address.
        #[clap(long, multiple_occurrences = false)]
        recipient: SuiAddress,

        /// Gas budget for this transaction
        #[clap(long)]
        gas_budget: u64,
    },

    /// Obtain the Addresses managed by the client.
    #[clap(name = "addresses")]
    Addresses,

    /// Generate new address and keypair with keypair scheme flag {ed25519 | secp256k1 | secp256r1}
    /// with optional derivation path, default to m/44'/784'/0'/0'/0' for ed25519 or m/54'/784'/0'/0/0 for secp256k1 or m/74'/784'/0'/0/0 for secp256r1.
    #[clap(name = "new-address")]
    NewAddress {
        key_scheme: SignatureScheme,
        derivation_path: Option<DerivationPath>,
    },

    /// Obtain all objects owned by the address
    #[clap(name = "objects")]
    Objects {
        /// Address owning the objects
        /// Shows all objects owned by `sui client active-address` if no argument is passed
        #[clap(name = "owner_address")]
        address: Option<SuiAddress>,
    },

    /// Obtain all gas objects owned by the address.
    #[clap(name = "gas")]
    Gas {
        /// Address owning the objects
        #[clap(name = "owner_address")]
        address: Option<SuiAddress>,
    },

    /// Query a dynamic field by its address.
    #[clap(name = "dynamic-field")]
    DynamicFieldQuery {
        ///The ID of the parent object
        #[clap(name = "object_id")]
        id: ObjectID,
        /// Optional paging cursor
        #[clap(long)]
        cursor: Option<ObjectID>,
        /// Maximum item returned per page
        #[clap(long, default_value = "50")]
        limit: usize,
    },

    /// Split a coin object into multiple coins.
    #[clap(group(ArgGroup::new("split").required(true).args(&["amounts", "count"])))]
    SplitCoin {
        /// Coin to Split, in 20 bytes Hex string
        #[clap(long)]
        coin_id: ObjectID,
        /// Specific amounts to split out from the coin
        #[clap(long, multiple_occurrences = false, multiple_values = true)]
        amounts: Option<Vec<u64>>,
        /// Count of equal-size coins to split into
        #[clap(long)]
        count: Option<u64>,
        /// ID of the gas object for gas payment, in 20 bytes Hex string
        /// If not provided, a gas object with at least gas_budget value will be selected
        #[clap(long)]
        gas: Option<ObjectID>,
        /// Gas budget for this call
        #[clap(long)]
        gas_budget: u64,
    },

    /// Merge two coin objects into one coin
    MergeCoin {
        /// Coin to merge into, in 20 bytes Hex string
        #[clap(long)]
        primary_coin: ObjectID,
        /// Coin to be merged, in 20 bytes Hex string
        #[clap(long)]
        coin_to_merge: ObjectID,
        /// ID of the gas object for gas payment, in 20 bytes Hex string
        /// If not provided, a gas object with at least gas_budget value will be selected
        #[clap(long)]
        gas: Option<ObjectID>,
        /// Gas budget for this call
        #[clap(long)]
        gas_budget: u64,
    },

    /// Create an example NFT
    #[clap(name = "create-example-nft")]
    CreateExampleNFT {
        /// Name of the NFT
        #[clap(long)]
        name: Option<String>,

        /// Description of the NFT
        #[clap(long)]
        description: Option<String>,

        /// Display url(e.g., an image url) of the NFT
        #[clap(long)]
        url: Option<String>,

        /// ID of the gas object for gas payment, in 20 bytes Hex string
        /// If not provided, a gas object with at least gas_budget value will be selected
        #[clap(long)]
        gas: Option<ObjectID>,

        /// Gas budget for this transfer
        #[clap(long)]
        gas_budget: Option<u64>,
    },

    /// Serialize a transfer that can be signed. This is useful when user prefers to take the data to sign elsewhere.
    #[clap(name = "serialize-transfer-sui")]
    SerializeTransferSui {
        /// Recipient address
        #[clap(long)]
        to: SuiAddress,

        /// Sui coin object to transfer, ID in 20 bytes Hex string. This is also the gas object.
        #[clap(long)]
        sui_coin_object_id: ObjectID,

        /// Gas budget for this transfer
        #[clap(long)]
        gas_budget: u64,

        /// The amount to transfer, if not specified, the entire coin object will be transferred.
        #[clap(long)]
        amount: Option<u64>,
    },

    /// Execute a Signed Transaction. This is useful when the user prefers to sign elsewhere and use this command to execute.
    ExecuteSignedTx {
        /// BCS serialized transaction data bytes without its type tag, as base-64 encoded string.
        #[clap(long)]
        tx_bytes: String,

        /// Base64 encoded signature `flag || signature || pubkey`.
        #[clap(long)]
        signature: String,
    },
}

impl SuiClientCommands {
    pub async fn execute(
        self,
        context: &mut WalletContext,
    ) -> Result<SuiClientCommandResult, anyhow::Error> {
        let ret = Ok(match self {
            SuiClientCommands::Publish {
                package_path,
                gas,
                build_config,
                gas_budget,
                verify_dependencies,
                skip_dependency_verification,
                with_unpublished_dependencies,
            } => {
                let sender = context.try_get_object_owner(&gas).await?;
                let sender = sender.unwrap_or(context.active_address()?);

                let compiled_package = build_move_package(
                    &package_path,
                    BuildConfig {
                        config: build_config,
                        run_bytecode_verifier: true,
                        print_diags_to_stderr: true,
                    },
                )?;

                if !compiled_package.is_framework() {
                    if let Some(already_published) = compiled_package.published_root_module() {
                        return Err(SuiError::ModulePublishFailure {
                            error: format!(
                                "Modules must all have 0x0 as their addresses. \
                                 Violated by module {:?}",
                                already_published.self_id(),
                            ),
                        }
                        .into());
                    }
                }

                let client = context.get_client().await?;
                let compiled_modules =
                    compiled_package.get_package_bytes(with_unpublished_dependencies);

                if verify_dependencies {
                    eprintln!(
                        "{}",
                        "Dependency verification is on by default. --verify-dependencies is \
                         deprecated and will be removed in the next release."
                            .bold()
                            .yellow(),
                    );
                }

                if !skip_dependency_verification {
                    BytecodeSourceVerifier::new(client.read_api(), false)
                        .verify_package_deps(&compiled_package.package)
                        .await?;
                    eprintln!(
                        "{}",
                        "Successfully verified dependencies on-chain against source."
                            .bold()
                            .green(),
                    );
                } else {
                    eprintln!("{}", "Skipping dependency verification".bold().yellow());
                }

                let data = client
                    .transaction_builder()
                    .publish(sender, compiled_modules, gas, gas_budget)
                    .await?;
                let signature =
                    context
                        .config
                        .keystore
                        .sign_secure(&sender, &data, Intent::default())?;
                let response = context
                    .execute_transaction(
                        Transaction::from_data(data, Intent::default(), signature).verify()?,
                    )
                    .await?;

                SuiClientCommandResult::Publish(response)
            }

            SuiClientCommands::Object { id, bcs } => {
                // Fetch the object ref
                let client = context.get_client().await?;
                let object_read = client.read_api().get_parsed_object(id).await?;
                SuiClientCommandResult::Object(object_read, bcs)
            }

            SuiClientCommands::DynamicFieldQuery { id, cursor, limit } => {
                let client = context.get_client().await?;
                let df_read = client
                    .read_api()
                    .get_dynamic_fields(id, cursor, Some(limit))
                    .await?;
                SuiClientCommandResult::DynamicFieldQuery(df_read)
            }

            SuiClientCommands::Call {
                package,
                module,
                function,
                type_args,
                gas,
                gas_budget,
                args,
            } => {
                let (cert, effects) = call_move(
                    package, &module, &function, type_args, gas, gas_budget, args, context,
                )
                .await?;
                SuiClientCommandResult::Call(cert, effects)
            }

            SuiClientCommands::Transfer {
                to,
                object_id,
                gas,
                gas_budget,
            } => {
                let from = context.get_object_owner(&object_id).await?;
                let time_start = Instant::now();

                let client = context.get_client().await?;
                let data = client
                    .transaction_builder()
                    .transfer_object(from, object_id, gas, gas_budget, to)
                    .await?;
                let signature =
                    context
                        .config
                        .keystore
                        .sign_secure(&from, &data, Intent::default())?;
                let response = context
                    .execute_transaction(
                        Transaction::from_data(data, Intent::default(), signature).verify()?,
                    )
                    .await?;
                let cert = response.certificate;
                let effects = response.effects;

                let time_total = time_start.elapsed().as_micros();
                if matches!(effects.status, SuiExecutionStatus::Failure { .. }) {
                    return Err(anyhow!("Error transferring object: {:#?}", effects.status));
                }
                SuiClientCommandResult::Transfer(time_total, cert, effects)
            }

            SuiClientCommands::TransferSui {
                to,
                sui_coin_object_id: object_id,
                gas_budget,
                amount,
            } => {
                let from = context.get_object_owner(&object_id).await?;

                let client = context.get_client().await?;
                let data = client
                    .transaction_builder()
                    .transfer_sui(from, object_id, gas_budget, to, amount)
                    .await?;
                let signature =
                    context
                        .config
                        .keystore
                        .sign_secure(&from, &data, Intent::default())?;
                let response = context
                    .execute_transaction(
                        Transaction::from_data(data, Intent::default(), signature).verify()?,
                    )
                    .await?;
                let cert = response.certificate;
                let effects = response.effects;

                if matches!(effects.status, SuiExecutionStatus::Failure { .. }) {
                    return Err(anyhow!("Error transferring SUI: {:#?}", effects.status));
                }
                SuiClientCommandResult::TransferSui(cert, effects)
            }

            SuiClientCommands::Pay {
                input_coins,
                recipients,
                amounts,
                gas,
                gas_budget,
            } => {
                ensure!(
                    !input_coins.is_empty(),
                    "Pay transaction requires a non-empty list of input coins"
                );
                ensure!(
                    !recipients.is_empty(),
                    "Pay transaction requires a non-empty list of recipient addresses"
                );
                ensure!(
                    recipients.len() == amounts.len(),
                    format!(
                        "Found {:?} recipient addresses, but {:?} recipient amounts",
                        recipients.len(),
                        amounts.len()
                    ),
                );
                let from = context.get_object_owner(&input_coins[0]).await?;
                let client = context.get_client().await?;
                let data = client
                    .transaction_builder()
                    .pay(from, input_coins, recipients, amounts, gas, gas_budget)
                    .await?;
                let signature =
                    context
                        .config
                        .keystore
                        .sign_secure(&from, &data, Intent::default())?;
                let response = context
                    .execute_transaction(
                        Transaction::from_data(data, Intent::default(), signature).verify()?,
                    )
                    .await?;
                let cert = response.certificate;
                let effects = response.effects;
                if matches!(effects.status, SuiExecutionStatus::Failure { .. }) {
                    return Err(anyhow!(
                        "Error executing Pay transaction: {:#?}",
                        effects.status
                    ));
                }
                SuiClientCommandResult::Pay(cert, effects)
            }

            SuiClientCommands::PaySui {
                input_coins,
                recipients,
                amounts,
                gas_budget,
            } => {
                ensure!(
                    !input_coins.is_empty(),
                    "PaySui transaction requires a non-empty list of input coins"
                );
                ensure!(
                    !recipients.is_empty(),
                    "PaySui transaction requires a non-empty list of recipient addresses"
                );
                ensure!(
                    recipients.len() == amounts.len(),
                    format!(
                        "Found {:?} recipient addresses, but {:?} recipient amounts",
                        recipients.len(),
                        amounts.len()
                    ),
                );
                let signer = context.get_object_owner(&input_coins[0]).await?;
                let client = context.get_client().await?;
                let data = client
                    .transaction_builder()
                    .pay_sui(signer, input_coins, recipients, amounts, gas_budget)
                    .await?;
                let signature =
                    context
                        .config
                        .keystore
                        .sign_secure(&signer, &data, Intent::default())?;
                let response = context
                    .execute_transaction(
                        Transaction::from_data(data, Intent::default(), signature).verify()?,
                    )
                    .await?;

                let cert = response.certificate;
                let effects = response.effects;
                if matches!(effects.status, SuiExecutionStatus::Failure { .. }) {
                    return Err(anyhow!(
                        "Error executing PaySui transaction: {:#?}",
                        effects.status
                    ));
                }
                SuiClientCommandResult::PaySui(cert, effects)
            }

            SuiClientCommands::PayAllSui {
                input_coins,
                recipient,
                gas_budget,
            } => {
                ensure!(
                    !input_coins.is_empty(),
                    "PayAllSui transaction requires a non-empty list of input coins"
                );
                let signer = context.get_object_owner(&input_coins[0]).await?;
                let client = context.get_client().await?;
                let data = client
                    .transaction_builder()
                    .pay_all_sui(signer, input_coins, recipient, gas_budget)
                    .await?;

                let signature =
                    context
                        .config
                        .keystore
                        .sign_secure(&signer, &data, Intent::default())?;
                let response = context
                    .execute_transaction(
                        Transaction::from_data(data, Intent::default(), signature).verify()?,
                    )
                    .await?;

                let cert = response.certificate;
                let effects = response.effects;
                if matches!(effects.status, SuiExecutionStatus::Failure { .. }) {
                    return Err(anyhow!(
                        "Error executing PayAllSui transaction: {:#?}",
                        effects.status
                    ));
                }
                SuiClientCommandResult::PayAllSui(cert, effects)
            }

            SuiClientCommands::Addresses => {
                SuiClientCommandResult::Addresses(context.config.keystore.addresses())
            }

            SuiClientCommands::Objects { address } => {
                let address = address.unwrap_or(context.active_address()?);
                let client = context.get_client().await?;
                let address_object = client
                    .read_api()
                    .get_objects_owned_by_address(address)
                    .await?;
                SuiClientCommandResult::Objects(address_object)
            }

            SuiClientCommands::NewAddress {
                key_scheme,
                derivation_path,
            } => {
                let (address, phrase, scheme) = context
                    .config
                    .keystore
                    .generate_and_add_new_key(key_scheme, derivation_path)?;
                SuiClientCommandResult::NewAddress((address, phrase, scheme))
            }
            SuiClientCommands::Gas { address } => {
                let address = address.unwrap_or(context.active_address()?);
                let coins = context
                    .gas_objects(address)
                    .await?
                    .iter()
                    // Ok to unwrap() since `get_gas_objects` guarantees gas
                    .map(|(_val, object, _object_ref)| GasCoin::try_from(object).unwrap())
                    .collect();
                SuiClientCommandResult::Gas(coins)
            }
            SuiClientCommands::SplitCoin {
                coin_id,
                amounts,
                count,
                gas,
                gas_budget,
            } => {
                let signer = context.get_object_owner(&coin_id).await?;
                let client = context.get_client().await?;
                let data = match (amounts, count) {
                    (Some(amounts), None) => {
                        client
                            .transaction_builder()
                            .split_coin(signer, coin_id, amounts, gas, gas_budget)
                            .await?
                    }
                    (None, Some(count)) => {
                        if count == 0 {
                            return Err(anyhow!("Coin split count must be greater than 0"));
                        }
                        client
                            .transaction_builder()
                            .split_coin_equal(signer, coin_id, count, gas, gas_budget)
                            .await?
                    }
                    _ => {
                        return Err(anyhow!("Exactly one of `count` and `amounts` must be present for split-coin command."));
                    }
                };
                let signature =
                    context
                        .config
                        .keystore
                        .sign_secure(&signer, &data, Intent::default())?;
                let response = context
                    .execute_transaction(
                        Transaction::from_data(data, Intent::default(), signature).verify()?,
                    )
                    .await?;
                SuiClientCommandResult::SplitCoin(response)
            }
            SuiClientCommands::MergeCoin {
                primary_coin,
                coin_to_merge,
                gas,
                gas_budget,
            } => {
                let client = context.get_client().await?;
                let signer = context.get_object_owner(&primary_coin).await?;
                let data = client
                    .transaction_builder()
                    .merge_coins(signer, primary_coin, coin_to_merge, gas, gas_budget)
                    .await?;
                let signature =
                    context
                        .config
                        .keystore
                        .sign_secure(&signer, &data, Intent::default())?;
                let response = context
                    .execute_transaction(
                        Transaction::from_data(data, Intent::default(), signature).verify()?,
                    )
                    .await?;

                SuiClientCommandResult::MergeCoin(response)
            }
            SuiClientCommands::Switch { address, env } => {
                match (address, &env) {
                    (None, Some(env)) => {
                        Self::switch_env(&mut context.config, env)?;
                    }
                    (Some(addr), None) => {
                        if !context.config.keystore.addresses().contains(&addr) {
                            return Err(anyhow!("Address {} not managed by wallet", addr));
                        }
                        context.config.active_address = Some(addr);
                    }
                    _ => return Err(anyhow!("No address or env specified. Please Specify one.")),
                }
                context.config.save()?;
                SuiClientCommandResult::Switch(SwitchResponse { address, env })
            }
            SuiClientCommands::ActiveAddress => {
                SuiClientCommandResult::ActiveAddress(context.active_address().ok())
            }
            SuiClientCommands::CreateExampleNFT {
                name,
                description,
                url,
                gas,
                gas_budget,
            } => {
                let args_json = json!([
                    unwrap_or(&name, EXAMPLE_NFT_NAME),
                    unwrap_or(&description, EXAMPLE_NFT_DESCRIPTION),
                    unwrap_or(&url, EXAMPLE_NFT_URL)
                ]);
                let mut args = vec![];
                for a in args_json.as_array().unwrap() {
                    args.push(SuiJsonValue::new(a.clone()).unwrap());
                }
                let (_, effects) = call_move(
                    ObjectID::from(SUI_FRAMEWORK_ADDRESS),
                    "devnet_nft",
                    "mint",
                    vec![],
                    gas,
                    gas_budget.unwrap_or(100_000),
                    args,
                    context,
                )
                .await?;
                let nft_id = effects
                    .created
                    .first()
                    .ok_or_else(|| anyhow!("Failed to create NFT"))?
                    .reference
                    .object_id;
                let client = context.get_client().await?;
                let object_read = client.read_api().get_parsed_object(nft_id).await?;
                SuiClientCommandResult::CreateExampleNFT(object_read)
            }

            SuiClientCommands::SerializeTransferSui {
                to,
                sui_coin_object_id: object_id,
                gas_budget,
                amount,
            } => {
                let from = context.get_object_owner(&object_id).await?;
                let client = context.get_client().await?;
                let data = client
                    .transaction_builder()
                    .transfer_sui(from, object_id, gas_budget, to, amount)
                    .await?;
                let data1 = data.clone();
                let intent_msg = IntentMessage::new(Intent::default(), data);
                SuiClientCommandResult::SerializeTransferSui(
                    Base64::encode(bcs::to_bytes(&intent_msg)?.as_slice()),
                    Base64::encode(&bcs::to_bytes(&data1).unwrap()),
                )
            }

            SuiClientCommands::ExecuteSignedTx {
                tx_bytes,
                signature,
            } => {
                let data = bcs::from_bytes(
                    &Base64::try_from(tx_bytes)
                        .map_err(|e| anyhow!(e))?
                        .to_vec()
                        .map_err(|e| anyhow!(e))?,
                )?;
                let bytes = &Base64::try_from(signature)
                    .map_err(|e| anyhow!(e))?
                    .to_vec()
                    .map_err(|e| anyhow!(e))?;

                let sig = GenericSignature::from_bytes(bytes)?;
                let verified =
                    Transaction::from_generic_sig_data(data, Intent::default(), sig).verify()?;
                let response = context.execute_transaction(verified).await?;
                SuiClientCommandResult::ExecuteSignedTx(response)
            }
            SuiClientCommands::NewEnv { alias, rpc, ws } => {
                if context.config.envs.iter().any(|env| env.alias == alias) {
                    return Err(anyhow!(
                        "Environment config with name [{alias}] already exists."
                    ));
                }
                let env = SuiEnv { alias, rpc, ws };

                // Check urls are valid and server is reachable
                env.create_rpc_client(None).await?;
                context.config.envs.push(env.clone());
                context.config.save()?;
                SuiClientCommandResult::NewEnv(env)
            }
            SuiClientCommands::ActiveEnv => {
                SuiClientCommandResult::ActiveEnv(context.config.active_env.clone())
            }
            SuiClientCommands::Envs => SuiClientCommandResult::Envs(
                context.config.envs.clone(),
                context.config.active_env.clone(),
            ),
            SuiClientCommands::VerifySource {
                package_path,
                build_config,
                verify_deps,
                skip_source,
                address_override,
            } => {
                if skip_source && !verify_deps {
                    return Err(anyhow!(
                        "Source skipped and not verifying deps: Nothing to verify."
                    ));
                }

                let compiled_package = build_move_package(
                    &package_path,
                    BuildConfig {
                        config: build_config,
                        run_bytecode_verifier: true,
                        print_diags_to_stderr: true,
                    },
                )?;

                let client = context.get_client().await?;

                BytecodeSourceVerifier::new(client.read_api(), false)
                    .verify_package(
                        &compiled_package.package,
                        verify_deps,
                        match (skip_source, address_override) {
                            (true, _) => SourceMode::Skip,
                            (false, None) => SourceMode::Verify,
                            (false, Some(addr)) => SourceMode::VerifyAt(addr.into()),
                        },
                    )
                    .await?;

                SuiClientCommandResult::VerifySource
            }
        });
        ret
    }

    pub fn switch_env(config: &mut SuiClientConfig, env: &str) -> Result<(), anyhow::Error> {
        let env = Some(env.into());
        ensure!(config.get_env(&env).is_some(), "Environment config not found for [{env:?}], add new environment config using the `sui client new-env` command.");
        config.active_env = env;
        Ok(())
    }
}

pub struct WalletContext {
    pub config: PersistedConfig<SuiClientConfig>,
    request_timeout: Option<std::time::Duration>,
    client: Arc<RwLock<Option<SuiClient>>>,
}

impl WalletContext {
    pub async fn new(
        config_path: &Path,
        request_timeout: Option<std::time::Duration>,
    ) -> Result<Self, anyhow::Error> {
        let config: SuiClientConfig = PersistedConfig::read(config_path).map_err(|err| {
            err.context(format!(
                "Cannot open wallet config file at {:?}",
                config_path
            ))
        })?;

        let config = config.persisted(config_path);
        let context = Self {
            config,
            request_timeout,
            client: Default::default(),
        };
        Ok(context)
    }

    pub async fn get_client(&self) -> Result<SuiClient, anyhow::Error> {
        let read = self.client.read().await;

        Ok(if let Some(client) = read.as_ref() {
            client.clone()
        } else {
            drop(read);
            let client = self
                .config
                .get_active_env()?
                .create_rpc_client(self.request_timeout)
                .await?;

            if let Err(e) = client.check_api_version() {
                warn!("{e}");
                println!("{}", format!("[warn] {e}").yellow().bold());
            }
            self.client.write().await.insert(client).clone()
        })
    }

    pub fn active_address(&mut self) -> Result<SuiAddress, anyhow::Error> {
        if self.config.keystore.addresses().is_empty() {
            return Err(anyhow!(
                "No managed addresses. Create new address with `new-address` command."
            ));
        }

        // Ok to unwrap because we checked that config addresses not empty
        // Set it if not exists
        self.config.active_address = Some(
            self.config
                .active_address
                .unwrap_or(*self.config.keystore.addresses().get(0).unwrap()),
        );

        Ok(self.config.active_address.unwrap())
    }

    /// Get the latest object reference given a object id
    pub async fn get_object_ref(
        &self,
        object_id: ObjectID,
    ) -> Result<GetRawObjectDataResponse, anyhow::Error> {
        let client = self.get_client().await?;
        Ok(client.read_api().get_object(object_id).await?)
    }

    /// Get all the gas objects (and conveniently, gas amounts) for the address
    pub async fn gas_objects(
        &self,
        address: SuiAddress,
    ) -> Result<Vec<(u64, SuiParsedObject, SuiObjectInfo)>, anyhow::Error> {
        let client = self.get_client().await?;
        let object_refs = client
            .read_api()
            .get_objects_owned_by_address(address)
            .await?;

        // TODO: We should ideally fetch the objects from local cache
        let mut values_objects = Vec::new();
        for oref in object_refs {
            let response = client.read_api().get_parsed_object(oref.object_id).await?;
            match response {
                GetObjectDataResponse::Exists(o) => {
                    if matches!( o.data.type_(), Some(v)  if *v == GasCoin::type_().to_string()) {
                        // Okay to unwrap() since we already checked type
                        let gas_coin = GasCoin::try_from(&o)?;
                        values_objects.push((gas_coin.value(), o, oref));
                    }
                }
                _ => continue,
            }
        }

        Ok(values_objects)
    }

    pub async fn get_object_owner(&self, id: &ObjectID) -> Result<SuiAddress, anyhow::Error> {
        let client = self.get_client().await?;
        let object = client.read_api().get_object(*id).await?.into_object()?;
        Ok(object.owner.get_owner_address()?)
    }

    pub async fn try_get_object_owner(
        &self,
        id: &Option<ObjectID>,
    ) -> Result<Option<SuiAddress>, anyhow::Error> {
        if let Some(id) = id {
            Ok(Some(self.get_object_owner(id).await?))
        } else {
            Ok(None)
        }
    }

    /// Find a gas object which fits the budget
    pub async fn gas_for_owner_budget(
        &self,
        address: SuiAddress,
        budget: u64,
        forbidden_gas_objects: BTreeSet<ObjectID>,
    ) -> Result<(u64, SuiParsedObject), anyhow::Error> {
        for o in self.gas_objects(address).await.unwrap() {
            if o.0 >= budget && !forbidden_gas_objects.contains(&o.1.id()) {
                return Ok((o.0, o.1));
            }
        }
        Err(anyhow!(
            "No non-argument gas objects found with value >= budget {budget}"
        ))
    }

    pub async fn execute_transaction(
        &self,
        tx: VerifiedTransaction,
    ) -> anyhow::Result<SuiTransactionResponse> {
        let tx_digest = *tx.digest();

        let client = self.get_client().await?;
        let result = client
            .quorum_driver()
            .execute_transaction(
                tx,
                Some(sui_types::messages::ExecuteTransactionRequestType::WaitForLocalExecution),
            )
            .await;
        match result {
            Ok(TransactionExecutionResult {
                tx_digest: _,
                tx_cert,
                effects,
                confirmed_local_execution: _,
                timestamp_ms,
                parsed_data,
            }) => Ok(SuiTransactionResponse {
                certificate: tx_cert.unwrap(), // check is done in execute_transaction, safe to unwrap
                effects: effects.unwrap(), // check is done in execute_transaction, safe to unwrap
                timestamp_ms,
                parsed_data,
            }),
            Err(err) => Err(anyhow!(
                "Failed to execute transaction {tx_digest:?} with error {err:?}"
            )),
        }
    }
}

impl Display for SuiClientCommandResult {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut writer = String::new();
        match self {
            SuiClientCommandResult::Publish(response) => {
                write!(
                    writer,
                    "{}",
                    write_cert_and_effects(&response.certificate, &response.effects)?
                )?;
                if let Some(parsed_resp) = &response.parsed_data {
                    writeln!(writer, "{}", parsed_resp)?;
                }
            }
            SuiClientCommandResult::Object(object_read, bcs) => {
                let object = if *bcs {
                    match object_read.object() {
                        Ok(v) => {
                            let bcs_bytes = bcs::to_bytes(v).unwrap();
                            format!("{:?}\nNumber of bytes: {}", bcs_bytes, bcs_bytes.len())
                        }
                        Err(err) => format!("{err}").red().to_string(),
                    }
                } else {
                    unwrap_err_to_string(|| Ok(object_read.object()?))
                };
                writeln!(writer, "{}", object)?;
            }
            SuiClientCommandResult::Call(cert, effects) => {
                write!(writer, "{}", write_cert_and_effects(cert, effects)?)?;
            }
            SuiClientCommandResult::Transfer(time_elapsed, cert, effects) => {
                writeln!(writer, "Transfer confirmed after {} us", time_elapsed)?;
                write!(writer, "{}", write_cert_and_effects(cert, effects)?)?;
            }
            SuiClientCommandResult::TransferSui(cert, effects) => {
                write!(writer, "{}", write_cert_and_effects(cert, effects)?)?;
            }
            SuiClientCommandResult::Pay(cert, effects) => {
                write!(writer, "{}", write_cert_and_effects(cert, effects)?)?;
            }
            SuiClientCommandResult::PaySui(cert, effects) => {
                write!(writer, "{}", write_cert_and_effects(cert, effects)?)?;
            }
            SuiClientCommandResult::PayAllSui(cert, effects) => {
                write!(writer, "{}", write_cert_and_effects(cert, effects)?)?;
            }
            SuiClientCommandResult::Addresses(addresses) => {
                writeln!(writer, "Showing {} results.", addresses.len())?;
                for address in addresses {
                    writeln!(writer, "{}", address)?;
                }
            }
            SuiClientCommandResult::Objects(object_refs) => {
                writeln!(
                    writer,
                    " {0: ^42} | {1: ^10} | {2: ^44} | {3: ^15} | {4: ^40}",
                    "Object ID", "Version", "Digest", "Owner Type", "Object Type"
                )?;
                writeln!(writer, "{}", ["-"; 165].join(""))?;
                for oref in object_refs {
                    let owner_type = match oref.owner {
                        Owner::AddressOwner(_) => "AddressOwner",
                        Owner::ObjectOwner(_) => "object_owner",
                        Owner::Shared { .. } => "Shared",
                        Owner::Immutable => "Immutable",
                    };
                    writeln!(
                        writer,
                        " {0: ^42} | {1: ^10} | {2: ^44} | {3: ^15} | {4: ^40}",
                        oref.object_id,
                        oref.version.value(),
                        Base64::encode(oref.digest),
                        owner_type,
                        oref.type_
                    )?
                }
                writeln!(writer, "Showing {} results.", object_refs.len())?;
            }
            SuiClientCommandResult::DynamicFieldQuery(df_refs) => {
                let mut table: Table = table!([
                    "Name",
                    "Type",
                    "Object Type",
                    "Object Id",
                    "Version",
                    "Digest"
                ]);
                for df_ref in df_refs.data.iter() {
                    let df_type = match df_ref.type_ {
                        DynamicFieldType::DynamicField => "DynamicField",
                        DynamicFieldType::DynamicObject => "DynamicObject",
                    };
                    table.add_row(row![
                        df_ref.name,
                        df_type,
                        df_ref.object_type,
                        df_ref.object_id,
                        df_ref.version.value(),
                        Base64::encode(df_ref.digest)
                    ]);
                }
                write!(writer, "{table}")?;
                writeln!(writer, "Showing {} results.", df_refs.data.len())?;
                if let Some(cursor) = df_refs.next_cursor {
                    writeln!(writer, "Next cursor: {cursor}")?;
                }
            }
            SuiClientCommandResult::SyncClientState => {
                writeln!(writer, "Client state sync complete.")?;
            }
            // Do not use writer for new address output, which may get sent to logs.
            #[allow(clippy::print_in_format_impl)]
            SuiClientCommandResult::NewAddress((address, recovery_phrase, scheme)) => {
                println!(
                    "Created new keypair for address with scheme {:?}: [{address}]",
                    scheme
                );
                println!("Secret Recovery Phrase : [{recovery_phrase}]");
            }
            SuiClientCommandResult::Gas(gases) => {
                // TODO: generalize formatting of CLI
                writeln!(writer, " {0: ^42} | {1: ^11}", "Object ID", "Gas Value")?;
                writeln!(
                    writer,
                    "----------------------------------------------------------------------"
                )?;
                for gas in gases {
                    writeln!(writer, " {0: ^42} | {1: ^11}", gas.id(), gas.value())?;
                }
            }
            SuiClientCommandResult::SplitCoin(response) => {
                write!(
                    writer,
                    "{}",
                    write_cert_and_effects(&response.certificate, &response.effects)?
                )?;
                if let Some(parsed_resp) = &response.parsed_data {
                    writeln!(writer, "{}", parsed_resp)?;
                }
            }
            SuiClientCommandResult::MergeCoin(response) => {
                write!(
                    writer,
                    "{}",
                    write_cert_and_effects(&response.certificate, &response.effects)?
                )?;
                if let Some(parsed_resp) = &response.parsed_data {
                    writeln!(writer, "{}", parsed_resp)?;
                }
            }
            SuiClientCommandResult::Switch(response) => {
                write!(writer, "{}", response)?;
            }
            SuiClientCommandResult::ActiveAddress(response) => {
                match response {
                    Some(r) => write!(writer, "{}", r)?,
                    None => write!(writer, "None")?,
                };
            }
            SuiClientCommandResult::CreateExampleNFT(object_read) => {
                // TODO: display the content of the object
                let object = unwrap_err_to_string(|| Ok(object_read.object()?));
                writeln!(writer, "{}\n", "Successfully created an ExampleNFT:".bold())?;
                writeln!(writer, "{}", object)?;
            }
            SuiClientCommandResult::ExecuteSignedTx(response) => {
                write!(
                    writer,
                    "{}",
                    write_cert_and_effects(&response.certificate, &response.effects)?
                )?;
                if let Some(parsed_resp) = &response.parsed_data {
                    writeln!(writer, "{}", parsed_resp)?;
                }
            }
            SuiClientCommandResult::SerializeTransferSui(data_to_sign, data_to_execute) => {
                writeln!(writer, "Intent message to sign: {}", data_to_sign)?;
                writeln!(writer, "Raw transaction to execute: {}", data_to_execute)?;
            }
            SuiClientCommandResult::ActiveEnv(env) => {
                write!(writer, "{}", env.as_deref().unwrap_or("None"))?;
            }
            SuiClientCommandResult::NewEnv(env) => {
                writeln!(writer, "Added new Sui env [{}] to config.", env.alias)?;
            }
            SuiClientCommandResult::Envs(envs, active) => {
                for env in envs {
                    write!(writer, "{} => {}", env.alias, env.rpc)?;
                    if Some(env.alias.as_str()) == active.as_deref() {
                        write!(writer, " (active)")?;
                    }
                    writeln!(writer)?;
                }
            }
            SuiClientCommandResult::VerifySource => {
                writeln!(writer, "Source verification succeeded!")?;
            }
        }
        write!(f, "{}", writer.trim_end_matches('\n'))
    }
}

pub async fn call_move(
    package: ObjectID,
    module: &str,
    function: &str,
    type_args: Vec<TypeTag>,
    gas: Option<ObjectID>,
    gas_budget: u64,
    args: Vec<SuiJsonValue>,
    context: &mut WalletContext,
) -> Result<(SuiCertifiedTransaction, SuiTransactionEffects), anyhow::Error> {
    // Convert all numeric input to String, this will allow number input from the CLI without failing SuiJSON's checks.
    let args = args
        .into_iter()
        .map(|value| SuiJsonValue::new(convert_number_to_string(value.to_json_value())))
        .collect::<Result<_, _>>()?;

    let gas_owner = context.try_get_object_owner(&gas).await?;
    let sender = gas_owner.unwrap_or(context.active_address()?);

    let client = context.get_client().await?;
    let data = client
        .transaction_builder()
        .move_call(
            sender,
            package,
            module,
            function,
            type_args
                .into_iter()
                .map(|arg| arg.try_into())
                .collect::<Result<Vec<_>, _>>()?,
            args,
            gas,
            gas_budget,
        )
        .await?;
    let signature = context
        .config
        .keystore
        .sign_secure(&sender, &data, Intent::default())?;
    let transaction = Transaction::from_data(data, Intent::default(), signature).verify()?;

    let response = context.execute_transaction(transaction).await?;
    let cert = response.certificate;
    let effects = response.effects;

    if matches!(effects.status, SuiExecutionStatus::Failure { .. }) {
        return Err(anyhow!("Error calling module: {:#?}", effects.status));
    }
    Ok((cert, effects))
}

fn convert_number_to_string(value: Value) -> Value {
    match value {
        Value::Number(n) => Value::String(n.to_string()),
        Value::Array(a) => Value::Array(a.into_iter().map(convert_number_to_string).collect()),
        Value::Object(o) => Value::Object(
            o.into_iter()
                .map(|(k, v)| (k, convert_number_to_string(v)))
                .collect(),
        ),
        _ => value,
    }
}

fn unwrap_or<'a>(val: &'a Option<String>, default: &'a str) -> &'a str {
    match val {
        Some(v) => v,
        None => default,
    }
}

fn write_cert_and_effects(
    cert: &SuiCertifiedTransaction,
    effects: &SuiTransactionEffects,
) -> Result<String, fmt::Error> {
    let mut writer = String::new();
    writeln!(writer, "{}", "----- Certificate ----".bold())?;
    write!(writer, "{}", cert)?;
    writeln!(writer, "{}", "----- Transaction Effects ----".bold())?;
    write!(writer, "{}", effects)?;
    Ok(writer)
}

impl Debug for SuiClientCommandResult {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let s = unwrap_err_to_string(|| match self {
            SuiClientCommandResult::Object(object_read, bcs) => {
                let object = object_read.object()?;
                if *bcs {
                    Ok(serde_json::to_string_pretty(&bcs::to_bytes(&object)?)?)
                } else {
                    Ok(serde_json::to_string_pretty(&object)?)
                }
            }
            _ => Ok(serde_json::to_string_pretty(self)?),
        });
        write!(f, "{}", s)
    }
}

fn unwrap_err_to_string<T: Display, F: FnOnce() -> Result<T, anyhow::Error>>(func: F) -> String {
    match func() {
        Ok(s) => format!("{s}"),
        Err(err) => format!("{err}").red().to_string(),
    }
}

impl SuiClientCommandResult {
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

#[derive(Serialize)]
#[serde(untagged)]
pub enum SuiClientCommandResult {
    Publish(SuiTransactionResponse),
    VerifySource,
    Object(GetObjectDataResponse, bool),
    Call(SuiCertifiedTransaction, SuiTransactionEffects),
    Transfer(
        // Skipping serialisation for elapsed time.
        #[serde(skip)] u128,
        SuiCertifiedTransaction,
        SuiTransactionEffects,
    ),
    TransferSui(SuiCertifiedTransaction, SuiTransactionEffects),
    Pay(SuiCertifiedTransaction, SuiTransactionEffects),
    PaySui(SuiCertifiedTransaction, SuiTransactionEffects),
    PayAllSui(SuiCertifiedTransaction, SuiTransactionEffects),
    Addresses(Vec<SuiAddress>),
    Objects(Vec<SuiObjectInfo>),
    DynamicFieldQuery(DynamicFieldPage),
    SyncClientState,
    NewAddress((SuiAddress, String, SignatureScheme)),
    Gas(Vec<GasCoin>),
    SplitCoin(SuiTransactionResponse),
    MergeCoin(SuiTransactionResponse),
    Switch(SwitchResponse),
    ActiveAddress(Option<SuiAddress>),
    ActiveEnv(Option<String>),
    Envs(Vec<SuiEnv>, Option<String>),
    CreateExampleNFT(GetObjectDataResponse),
    SerializeTransferSui(String, String),
    ExecuteSignedTx(SuiTransactionResponse),
    NewEnv(SuiEnv),
}

#[derive(Serialize, Clone, Debug)]
pub struct SwitchResponse {
    /// Active address
    pub address: Option<SuiAddress>,
    pub env: Option<String>,
}

impl Display for SwitchResponse {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut writer = String::new();
        if let Some(addr) = self.address {
            writeln!(writer, "Active address switched to {addr}")?;
        }
        if let Some(env) = &self.env {
            writeln!(writer, "Active environment switched to [{env}]")?;
        }
        write!(f, "{}", writer)
    }
}
