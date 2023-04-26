// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use core::fmt;
use std::{
    fmt::{Debug, Display, Formatter, Write},
    path::PathBuf,
    time::Instant,
};

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
use sui_move::build::resolve_lock_file_path;
use sui_source_validation::{BytecodeSourceVerifier, SourceMode};
use sui_types::digests::TransactionDigest;
use sui_types::error::SuiError;

use shared_crypto::intent::Intent;
use sui_json::SuiJsonValue;
use sui_json_rpc_types::{
    DynamicFieldPage, SuiData, SuiObjectResponse, SuiObjectResponseQuery, SuiRawData,
    SuiTransactionBlockEffectsAPI, SuiTransactionBlockResponse, SuiTransactionBlockResponseOptions,
};
use sui_json_rpc_types::{SuiExecutionStatus, SuiObjectDataOptions};
use sui_keys::keystore::AccountKeystore;
use sui_move_build::{
    build_from_resolution_graph, check_invalid_dependencies, check_unpublished_dependencies,
    gather_published_ids, BuildConfig, CompiledPackage, PackageDependencies, PublishedAtError,
};
use sui_sdk::sui_client_config::{SuiClientConfig, SuiEnv};
use sui_sdk::wallet_context::WalletContext;
use sui_sdk::SuiClient;
use sui_types::crypto::SignatureScheme;
use sui_types::dynamic_field::DynamicFieldType;
use sui_types::move_package::UpgradeCap;
use sui_types::signature::GenericSignature;
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    gas_coin::GasCoin,
    messages::Transaction,
    object::Owner,
    parse_sui_type_tag,
};
use tracing::info;

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

    /// Get the effects of executing the given transaction block
    #[clap(name = "tx-block")]
    TransactionBlock {
        /// Digest of the transaction block
        #[clap(name = "digest")]
        digest: TransactionDigest,
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

        /// Publish the package without checking whether compiling dependencies from source results
        /// in bytecode matching the dependencies found on-chain.
        #[clap(long)]
        skip_dependency_verification: bool,

        /// Also publish transitive dependencies that have not already been published.
        #[clap(long)]
        with_unpublished_dependencies: bool,

        /// Do not sign/submit transaction, output base64-encoded serialized output
        #[clap(long)]
        serialize_output: bool,
    },

    /// Upgrade Move modules
    #[clap(name = "upgrade")]
    Upgrade {
        /// Path to directory containing a Move package
        #[clap(
            name = "package_path",
            global = true,
            parse(from_os_str),
            default_value = "."
        )]
        package_path: PathBuf,

        /// ID of the upgrade capability for the package being upgraded.
        #[clap(long)]
        upgrade_capability: ObjectID,

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

        /// Publish the package without checking whether compiling dependencies from source results
        /// in bytecode matching the dependencies found on-chain.
        #[clap(long)]
        skip_dependency_verification: bool,

        /// Also publish transitive dependencies that have not already been published.
        #[clap(long)]
        with_unpublished_dependencies: bool,

        /// Use the legacy digest calculation algorithm
        #[clap(long)]
        legacy_digest: bool,

        /// Do not sign/submit transaction, output base64-encoded serialized output
        #[clap(long)]
        serialize_output: bool,
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

        /// Do not sign/submit transaction, output base64-encoded serialized output
        #[clap(long)]
        serialize_output: bool,
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

        /// Do not sign/submit transaction, output base64-encoded serialized output
        #[clap(long)]
        serialize_output: bool,
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

        /// Do not sign/submit transaction, output base64-encoded serialized output
        #[clap(long)]
        serialize_output: bool,
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

        /// Do not sign/submit transaction, output base64-encoded serialized output
        #[clap(long)]
        serialize_output: bool,
    },

    /// Pay SUI coins to recipients following following specified amounts, with input coins.
    /// Length of recipients must be the same as that of amounts.
    /// The input coins also include the coin for gas payment, so no extra gas coin is required.
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

        /// Do not sign/submit transaction, output base64-encoded serialized output
        #[clap(long)]
        serialize_output: bool,
    },

    /// Pay all residual SUI coins to the recipient with input coins, after deducting the gas cost.
    /// The input coins also include the coin for gas payment, so no extra gas coin is required.
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

        /// Do not sign/submit transaction, output base64-encoded serialized output
        #[clap(long)]
        serialize_output: bool,
    },

    /// Obtain the Addresses managed by the client.
    #[clap(name = "addresses")]
    Addresses,

    /// Generate new address and keypair with keypair scheme flag {ed25519 | secp256k1 | secp256r1}
    /// with optional derivation path, default to m/44'/784'/0'/0'/0' for ed25519 or
    /// m/54'/784'/0'/0/0 for secp256k1 or m/74'/784'/0'/0/0 for secp256r1. Word length can be
    /// { word12 | word15 | word18 | word21 | word24} default to word12 if not specified.
    #[clap(name = "new-address")]
    NewAddress {
        key_scheme: SignatureScheme,
        word_length: Option<String>,
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
        /// Do not sign/submit transaction, output base64-encoded serialized output
        #[clap(long)]
        serialize_output: bool,
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
        /// Do not sign/submit transaction, output base64-encoded serialized output
        #[clap(long)]
        serialize_output: bool,
    },

    /// Execute a Signed Transaction. This is useful when the user prefers to sign elsewhere and use this command to execute.
    ExecuteSignedTx {
        /// BCS serialized transaction data bytes without its type tag, as base-64 encoded string.
        #[clap(long)]
        tx_bytes: String,

        /// A list of Base64 encoded signatures `flag || signature || pubkey`.
        #[clap(long)]
        signatures: Vec<String>,
    },
}

impl SuiClientCommands {
    pub async fn execute(
        self,
        context: &mut WalletContext,
    ) -> Result<SuiClientCommandResult, anyhow::Error> {
        let ret = Ok(match self {
            SuiClientCommands::Upgrade {
                package_path,
                upgrade_capability,
                build_config,
                gas,
                gas_budget,
                skip_dependency_verification,
                with_unpublished_dependencies,
                legacy_digest,
                serialize_output,
            } => {
                let sender = context.try_get_object_owner(&gas).await?;
                let sender = sender.unwrap_or(context.active_address()?);

                let client = context.get_client().await?;
                let (dependencies, compiled_modules, compiled_package, package_id) =
                    compile_package(
                        &client,
                        build_config,
                        package_path,
                        with_unpublished_dependencies,
                        skip_dependency_verification,
                    )
                    .await?;

                let package_id = package_id.map_err(|e| match e {
                    PublishedAtError::NotPresent => {
                        anyhow!("No 'published-at' field in manifest for package to be upgraded.")
                    }
                    PublishedAtError::Invalid(v) => anyhow!(
                        "Invalid 'published-at' field in manifest of package to be upgraded. \
                         Expected an on-chain address, but found: {v:?}"
                    ),
                })?;

                let resp = context
                    .get_client()
                    .await?
                    .read_api()
                    .get_object_with_options(
                        upgrade_capability,
                        SuiObjectDataOptions::default().with_bcs().with_owner(),
                    )
                    .await?;

                let Some(data) = resp.data else {
                    return Err(anyhow!("Could not find upgrade capability at {upgrade_capability}"))
                };

                let upgrade_cap: UpgradeCap = data
                    .bcs
                    .ok_or_else(|| {
                        anyhow!("Fetch upgrade capability object but no data was returned")
                    })?
                    .try_as_move()
                    .ok_or_else(|| anyhow!("Upgrade capability is not a Move Object"))?
                    .deserialize()?;
                // We keep the existing policy -- no fancy policies or changing the upgrade
                // policy at the moment. To change the policy you can call a Move function in the
                // `package` module to change this policy.
                let upgrade_policy = upgrade_cap.policy;
                let package_digest = compiled_package
                    .get_package_digest(with_unpublished_dependencies, !legacy_digest);

                let data = client
                    .transaction_builder()
                    .upgrade(
                        sender,
                        package_id,
                        compiled_modules,
                        dependencies.published.into_values().collect(),
                        upgrade_capability,
                        upgrade_policy,
                        package_digest.to_vec(),
                        gas,
                        gas_budget,
                    )
                    .await?;
                if serialize_output {
                    return Ok(SuiClientCommandResult::SerializeTx(Base64::encode(
                        bcs::to_bytes(&data).unwrap(),
                    )));
                }
                let signature = context.config.keystore.sign_secure(
                    &sender,
                    &data,
                    Intent::sui_transaction(),
                )?;
                let response = context
                    .execute_transaction_block(
                        Transaction::from_data(data, Intent::sui_transaction(), vec![signature])
                            .verify()?,
                    )
                    .await?;

                SuiClientCommandResult::Upgrade(response)
            }
            SuiClientCommands::Publish {
                package_path,
                gas,
                build_config,
                gas_budget,
                skip_dependency_verification,
                with_unpublished_dependencies,
                serialize_output,
            } => {
                let sender = context.try_get_object_owner(&gas).await?;
                let sender = sender.unwrap_or(context.active_address()?);

                let client = context.get_client().await?;
                let (dependencies, compiled_modules, _, _) = compile_package(
                    &client,
                    build_config,
                    package_path,
                    with_unpublished_dependencies,
                    skip_dependency_verification,
                )
                .await?;

                let data = client
                    .transaction_builder()
                    .publish(
                        sender,
                        compiled_modules,
                        dependencies.published.into_values().collect(),
                        gas,
                        gas_budget,
                    )
                    .await?;
                if serialize_output {
                    return Ok(SuiClientCommandResult::SerializeTx(Base64::encode(
                        bcs::to_bytes(&data).unwrap(),
                    )));
                }

                let signature = context.config.keystore.sign_secure(
                    &sender,
                    &data,
                    Intent::sui_transaction(),
                )?;
                let response = context
                    .execute_transaction_block(
                        Transaction::from_data(data, Intent::sui_transaction(), vec![signature])
                            .verify()?,
                    )
                    .await?;

                SuiClientCommandResult::Publish(response)
            }

            SuiClientCommands::Object { id, bcs } => {
                // Fetch the object ref
                let client = context.get_client().await?;
                if !bcs {
                    let object_read = client
                        .read_api()
                        .get_object_with_options(id, SuiObjectDataOptions::full_content())
                        .await?;
                    SuiClientCommandResult::Object(object_read)
                } else {
                    let raw_object_read = client
                        .read_api()
                        .get_object_with_options(id, SuiObjectDataOptions::bcs_lossless())
                        .await?;
                    SuiClientCommandResult::RawObject(raw_object_read)
                }
            }

            SuiClientCommands::TransactionBlock { digest } => {
                let client = context.get_client().await?;
                let tx_read = client
                    .read_api()
                    .get_transaction_with_options(
                        digest,
                        SuiTransactionBlockResponseOptions {
                            show_input: true,
                            show_raw_input: false,
                            show_effects: true,
                            show_events: true,
                            show_object_changes: true,
                            show_balance_changes: false,
                        },
                    )
                    .await?;
                SuiClientCommandResult::TransactionBlock(tx_read)
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
                serialize_output,
            } => {
                call_move(
                    package,
                    &module,
                    &function,
                    type_args,
                    gas,
                    gas_budget,
                    args,
                    serialize_output,
                    context,
                )
                .await?
            }

            SuiClientCommands::Transfer {
                to,
                object_id,
                gas,
                gas_budget,
                serialize_output,
            } => {
                let from = context.get_object_owner(&object_id).await?;
                let time_start = Instant::now();

                let client = context.get_client().await?;
                let data = client
                    .transaction_builder()
                    .transfer_object(from, object_id, gas, gas_budget, to)
                    .await?;
                if serialize_output {
                    return Ok(SuiClientCommandResult::SerializeTx(Base64::encode(
                        bcs::to_bytes(&data).unwrap(),
                    )));
                }
                let signature =
                    context
                        .config
                        .keystore
                        .sign_secure(&from, &data, Intent::sui_transaction())?;
                let response = context
                    .execute_transaction_block(
                        Transaction::from_data(data, Intent::sui_transaction(), vec![signature])
                            .verify()?,
                    )
                    .await?;
                let effects = response.effects.as_ref().ok_or_else(|| {
                    anyhow!("Effects from SuiTransactionBlockResult should not be empty")
                })?;
                let time_total = time_start.elapsed().as_micros();
                if matches!(effects.status(), SuiExecutionStatus::Failure { .. }) {
                    return Err(anyhow!(
                        "Error transferring object: {:#?}",
                        effects.status()
                    ));
                }
                SuiClientCommandResult::Transfer(time_total, response)
            }

            SuiClientCommands::TransferSui {
                to,
                sui_coin_object_id: object_id,
                gas_budget,
                amount,
                serialize_output,
            } => {
                let from = context.get_object_owner(&object_id).await?;

                let client = context.get_client().await?;
                let data = client
                    .transaction_builder()
                    .transfer_sui(from, object_id, gas_budget, to, amount)
                    .await?;
                if serialize_output {
                    return Ok(SuiClientCommandResult::SerializeTx(Base64::encode(
                        bcs::to_bytes(&data).unwrap(),
                    )));
                }
                let signature =
                    context
                        .config
                        .keystore
                        .sign_secure(&from, &data, Intent::sui_transaction())?;
                let response = context
                    .execute_transaction_block(
                        Transaction::from_data(data, Intent::sui_transaction(), vec![signature])
                            .verify()?,
                    )
                    .await?;
                let effects = response.effects.as_ref().ok_or_else(|| {
                    anyhow!("Effects from SuiTransactionBlockResult should not be empty")
                })?;
                if matches!(effects.status(), SuiExecutionStatus::Failure { .. }) {
                    return Err(anyhow!("Error transferring SUI: {:#?}", effects.status()));
                }
                SuiClientCommandResult::TransferSui(response)
            }

            SuiClientCommands::Pay {
                input_coins,
                recipients,
                amounts,
                gas,
                gas_budget,
                serialize_output,
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
                if serialize_output {
                    return Ok(SuiClientCommandResult::SerializeTx(Base64::encode(
                        bcs::to_bytes(&data).unwrap(),
                    )));
                }
                let signature =
                    context
                        .config
                        .keystore
                        .sign_secure(&from, &data, Intent::sui_transaction())?;
                let response = context
                    .execute_transaction_block(
                        Transaction::from_data(data, Intent::sui_transaction(), vec![signature])
                            .verify()?,
                    )
                    .await?;
                let effects = response.effects.as_ref().ok_or_else(|| {
                    anyhow!("Effects from SuiTransactionBlockResult should not be empty")
                })?;
                if matches!(effects.status(), SuiExecutionStatus::Failure { .. }) {
                    return Err(anyhow!(
                        "Error executing Pay transaction: {:#?}",
                        effects.status()
                    ));
                }
                SuiClientCommandResult::Pay(response)
            }

            SuiClientCommands::PaySui {
                input_coins,
                recipients,
                amounts,
                gas_budget,
                serialize_output,
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
                if serialize_output {
                    return Ok(SuiClientCommandResult::SerializeTx(Base64::encode(
                        bcs::to_bytes(&data).unwrap(),
                    )));
                }
                let signature = context.config.keystore.sign_secure(
                    &signer,
                    &data,
                    Intent::sui_transaction(),
                )?;
                let response = context
                    .execute_transaction_block(
                        Transaction::from_data(data, Intent::sui_transaction(), vec![signature])
                            .verify()?,
                    )
                    .await?;
                let effects = response.effects.as_ref().ok_or_else(|| {
                    anyhow!("Effects from SuiTransactionBlockResult should not be empty")
                })?;
                if matches!(effects.status(), SuiExecutionStatus::Failure { .. }) {
                    return Err(anyhow!(
                        "Error executing PaySui transaction: {:#?}",
                        effects.status()
                    ));
                }
                SuiClientCommandResult::PaySui(response)
            }

            SuiClientCommands::PayAllSui {
                input_coins,
                recipient,
                gas_budget,
                serialize_output,
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
                if serialize_output {
                    return Ok(SuiClientCommandResult::SerializeTx(Base64::encode(
                        bcs::to_bytes(&data).unwrap(),
                    )));
                }
                let signature = context.config.keystore.sign_secure(
                    &signer,
                    &data,
                    Intent::sui_transaction(),
                )?;
                let response = context
                    .execute_transaction_block(
                        Transaction::from_data(data, Intent::sui_transaction(), vec![signature])
                            .verify()?,
                    )
                    .await?;
                let effects = response.effects.as_ref().ok_or_else(|| {
                    anyhow!("Effects from SuiTransactionBlockResult should not be empty")
                })?;
                if matches!(effects.status(), SuiExecutionStatus::Failure { .. }) {
                    return Err(anyhow!(
                        "Error executing PayAllSui transaction: {:#?}",
                        effects.status()
                    ));
                }
                SuiClientCommandResult::PayAllSui(response)
            }

            SuiClientCommands::Addresses => SuiClientCommandResult::Addresses(
                context.config.keystore.addresses(),
                context.active_address().ok(),
            ),

            SuiClientCommands::Objects { address } => {
                let address = address.unwrap_or(context.active_address()?);
                let client = context.get_client().await?;
                let mut objects: Vec<SuiObjectResponse> = Vec::new();
                let mut cursor = None;
                loop {
                    let response = client
                        .read_api()
                        .get_owned_objects(
                            address,
                            Some(SuiObjectResponseQuery::new_with_options(
                                SuiObjectDataOptions::full_content(),
                            )),
                            cursor,
                            None,
                        )
                        .await?;
                    objects.extend(response.data);

                    if response.has_next_page {
                        cursor = response.next_cursor;
                    } else {
                        break;
                    }
                }
                SuiClientCommandResult::Objects(objects)
            }

            SuiClientCommands::NewAddress {
                key_scheme,
                derivation_path,
                word_length,
            } => {
                let (address, phrase, scheme) = context.config.keystore.generate_and_add_new_key(
                    key_scheme,
                    derivation_path,
                    word_length,
                )?;
                SuiClientCommandResult::NewAddress((address, phrase, scheme))
            }
            SuiClientCommands::Gas { address } => {
                let address = address.unwrap_or(context.active_address()?);
                let coins = context
                    .gas_objects(address)
                    .await?
                    .iter()
                    // Ok to unwrap() since `get_gas_objects` guarantees gas
                    .map(|(_val, object)| GasCoin::try_from(object).unwrap())
                    .collect();
                SuiClientCommandResult::Gas(coins)
            }
            SuiClientCommands::SplitCoin {
                coin_id,
                amounts,
                count,
                gas,
                gas_budget,
                serialize_output,
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
                if serialize_output {
                    return Ok(SuiClientCommandResult::SerializeTx(Base64::encode(
                        bcs::to_bytes(&data).unwrap(),
                    )));
                }
                let signature = context.config.keystore.sign_secure(
                    &signer,
                    &data,
                    Intent::sui_transaction(),
                )?;
                let response = context
                    .execute_transaction_block(
                        Transaction::from_data(data, Intent::sui_transaction(), vec![signature])
                            .verify()?,
                    )
                    .await?;
                SuiClientCommandResult::SplitCoin(response)
            }
            SuiClientCommands::MergeCoin {
                primary_coin,
                coin_to_merge,
                gas,
                gas_budget,
                serialize_output,
            } => {
                let client = context.get_client().await?;
                let signer = context.get_object_owner(&primary_coin).await?;
                let data = client
                    .transaction_builder()
                    .merge_coins(signer, primary_coin, coin_to_merge, gas, gas_budget)
                    .await?;
                if serialize_output {
                    return Ok(SuiClientCommandResult::SerializeTx(Base64::encode(
                        bcs::to_bytes(&data).unwrap(),
                    )));
                }
                let signature = context.config.keystore.sign_secure(
                    &signer,
                    &data,
                    Intent::sui_transaction(),
                )?;
                let response = context
                    .execute_transaction_block(
                        Transaction::from_data(data, Intent::sui_transaction(), vec![signature])
                            .verify()?,
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

            SuiClientCommands::ExecuteSignedTx {
                tx_bytes,
                signatures,
            } => {
                let data = bcs::from_bytes(
                    &Base64::try_from(tx_bytes)
                        .map_err(|e| anyhow!(e))?
                        .to_vec()
                        .map_err(|e| anyhow!(e))?,
                )?;

                let mut sigs = Vec::new();
                for sig in signatures {
                    sigs.push(
                        GenericSignature::from_bytes(
                            &Base64::try_from(sig)
                                .map_err(|e| anyhow!(e))?
                                .to_vec()
                                .map_err(|e| anyhow!(e))?,
                        )
                        .map_err(|e| anyhow!(e))?,
                    );
                }
                let verified =
                    Transaction::from_generic_sig_data(data, Intent::sui_transaction(), sigs)
                        .verify()?;

                let response = context.execute_transaction_block(verified).await?;
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

                let build_config =
                    resolve_lock_file_path(build_config, Some(package_path.clone()))?;
                let compiled_package = BuildConfig {
                    config: build_config,
                    run_bytecode_verifier: true,
                    print_diags_to_stderr: true,
                }
                .build(package_path)?;

                let client = context.get_client().await?;

                BytecodeSourceVerifier::new(client.read_api())
                    .verify_package(
                        &compiled_package,
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

async fn compile_package(
    client: &SuiClient,
    build_config: MoveBuildConfig,
    package_path: PathBuf,
    with_unpublished_dependencies: bool,
    skip_dependency_verification: bool,
) -> Result<
    (
        PackageDependencies,
        Vec<Vec<u8>>,
        CompiledPackage,
        Result<ObjectID, PublishedAtError>,
    ),
    anemo::Error,
> {
    let config = resolve_lock_file_path(build_config, Some(package_path.clone()))?;
    let run_bytecode_verifier = true;
    let print_diags_to_stderr = true;
    let config = BuildConfig {
        config,
        run_bytecode_verifier,
        print_diags_to_stderr,
    };
    let resolution_graph = config.resolution_graph(&package_path)?;
    let (package_id, dependencies) = gather_published_ids(&resolution_graph);
    check_invalid_dependencies(&dependencies.invalid)?;
    if !with_unpublished_dependencies {
        check_unpublished_dependencies(&dependencies.unpublished)?;
    };
    let compiled_package = build_from_resolution_graph(
        package_path,
        resolution_graph,
        run_bytecode_verifier,
        print_diags_to_stderr,
    )?;
    if !compiled_package.is_system_package() {
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
    if with_unpublished_dependencies {
        compiled_package.verify_unpublished_dependencies(&dependencies.unpublished)?;
    }
    let compiled_modules = compiled_package.get_package_bytes(with_unpublished_dependencies);
    if !skip_dependency_verification {
        BytecodeSourceVerifier::new(client.read_api())
            .verify_package_deps(&compiled_package)
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
    Ok((dependencies, compiled_modules, compiled_package, package_id))
}

impl Display for SuiClientCommandResult {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut writer = String::new();
        match self {
            SuiClientCommandResult::Upgrade(response)
            | SuiClientCommandResult::Publish(response) => {
                write!(writer, "{}", write_transaction_response(response)?)?;
            }
            SuiClientCommandResult::Object(object_read) => {
                let object = unwrap_err_to_string(|| Ok(object_read.object()?));
                writeln!(writer, "{}", object)?;
            }
            SuiClientCommandResult::TransactionBlock(response) => {
                write!(writer, "{}", write_transaction_response(response)?)?;
            }
            SuiClientCommandResult::RawObject(raw_object_read) => {
                let raw_object = match raw_object_read.object() {
                    Ok(v) => match &v.bcs {
                        Some(SuiRawData::MoveObject(o)) => {
                            format!("{:?}\nNumber of bytes: {}", o.bcs_bytes, o.bcs_bytes.len())
                        }
                        Some(SuiRawData::Package(p)) => {
                            let mut temp = String::new();
                            let mut bcs_bytes = 0usize;
                            for m in &p.module_map {
                                temp.push_str(&format!("{:?}\n", m));
                                bcs_bytes += m.1.len()
                            }
                            format!("{}Number of bytes: {}", temp, bcs_bytes)
                        }
                        None => "Bcs field is None".to_string().red().to_string(),
                    },
                    Err(err) => format!("{err}").red().to_string(),
                };
                writeln!(writer, "{}", raw_object)?;
            }
            SuiClientCommandResult::Call(response) => {
                write!(writer, "{}", write_transaction_response(response)?)?;
            }
            SuiClientCommandResult::Transfer(time_elapsed, response) => {
                writeln!(writer, "Transfer confirmed after {} us", time_elapsed)?;
                write!(writer, "{}", write_transaction_response(response)?)?;
            }
            SuiClientCommandResult::TransferSui(response) => {
                write!(writer, "{}", write_transaction_response(response)?)?;
            }
            SuiClientCommandResult::Pay(response) => {
                write!(writer, "{}", write_transaction_response(response)?)?;
            }
            SuiClientCommandResult::PaySui(response) => {
                write!(writer, "{}", write_transaction_response(response)?)?;
            }
            SuiClientCommandResult::PayAllSui(response) => {
                write!(writer, "{}", write_transaction_response(response)?)?;
            }
            SuiClientCommandResult::Addresses(addresses, active_address) => {
                writeln!(writer, "Showing {} results.", addresses.len())?;
                for address in addresses {
                    if *active_address == Some(*address) {
                        writeln!(writer, "{} <=", address)?;
                    } else {
                        writeln!(writer, "{}", address)?;
                    }
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
                    let obj = oref.clone().into_object();
                    match obj {
                        Ok(obj) => {
                            let owner_type = match obj.owner {
                                Some(Owner::AddressOwner(_)) => "AddressOwner",
                                Some(Owner::ObjectOwner(_)) => "object_owner",
                                Some(Owner::Shared { .. }) => "Shared",
                                Some(Owner::Immutable) => "Immutable",
                                None => "None",
                            };

                            writeln!(
                                writer,
                                " {0: ^42} | {1: ^10} | {2: ^44} | {3: ^15} | {4: ^40}",
                                obj.object_id,
                                obj.version.value(),
                                Base64::encode(obj.digest),
                                owner_type,
                                format!("{:?}", obj.type_)
                            )?
                        }
                        Err(e) => writeln!(writer, "Error: {e:?}")?,
                    }
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
                writeln!(writer, " {0: ^66} | {1: ^11}", "Object ID", "Gas Value")?;
                writeln!(
                    writer,
                    "----------------------------------------------------------------------------------"
                )?;
                for gas in gases {
                    writeln!(writer, " {0: ^66} | {1: ^11}", gas.id(), gas.value())?;
                }
            }
            SuiClientCommandResult::SplitCoin(response) => {
                write!(writer, "{}", write_transaction_response(response)?)?;
            }
            SuiClientCommandResult::MergeCoin(response) => {
                write!(writer, "{}", write_transaction_response(response)?)?;
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
            SuiClientCommandResult::ExecuteSignedTx(response) => {
                write!(writer, "{}", write_transaction_response(response)?)?;
            }
            SuiClientCommandResult::SerializeTx(data) => {
                writeln!(writer, "Raw tx_bytes to execute: {}", data)?;
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
    serialize_output: bool,
    context: &mut WalletContext,
) -> Result<SuiClientCommandResult, anyhow::Error> {
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
    if serialize_output {
        return Ok(SuiClientCommandResult::SerializeTx(Base64::encode(
            bcs::to_bytes(&data).unwrap(),
        )));
    }
    let signature =
        context
            .config
            .keystore
            .sign_secure(&sender, &data, Intent::sui_transaction())?;
    let transaction =
        Transaction::from_data(data, Intent::sui_transaction(), vec![signature]).verify()?;

    let response = context.execute_transaction_block(transaction).await?;
    let effects = response
        .effects
        .as_ref()
        .ok_or_else(|| anyhow!("Effects from SuiTransactionBlockResult should not be empty"))?;
    if matches!(effects.status(), SuiExecutionStatus::Failure { .. }) {
        return Err(anyhow!("Error calling module: {:#?}", effects.status()));
    }
    Ok(SuiClientCommandResult::Call(response))
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

// TODO(chris): only print out the full response when `--verbose` is provided
pub fn write_transaction_response(
    response: &SuiTransactionBlockResponse,
) -> Result<String, fmt::Error> {
    let mut writer = String::new();
    writeln!(writer, "{}", "----- Transaction Digest ----".bold())?;
    writeln!(writer, "{}", response.digest)?;
    writeln!(writer, "{}", "----- Transaction Data ----".bold())?;
    if let Some(t) = &response.transaction {
        writeln!(writer, "{}", t)?;
    }

    writeln!(writer, "{}", "----- Transaction Effects ----".bold())?;
    if let Some(e) = &response.effects {
        writeln!(writer, "{}", e)?;
    }

    writeln!(writer, "{}", "----- Events ----".bold())?;
    if let Some(e) = &response.events {
        writeln!(writer, "{:#?}", json!(e))?;
    }

    writeln!(writer, "{}", "----- Object changes ----".bold())?;
    if let Some(e) = &response.object_changes {
        writeln!(writer, "{:#?}", json!(e))?;
    }

    writeln!(writer, "{}", "----- Balance changes ----".bold())?;
    if let Some(e) = &response.balance_changes {
        writeln!(writer, "{:#?}", json!(e))?;
    }
    Ok(writer)
}

impl Debug for SuiClientCommandResult {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let s = unwrap_err_to_string(|| match self {
            SuiClientCommandResult::Object(object_read) => {
                let object = object_read.object()?;
                Ok(serde_json::to_string_pretty(&object)?)
            }
            SuiClientCommandResult::RawObject(raw_object_read) => {
                let raw_object = raw_object_read.object()?;
                Ok(serde_json::to_string_pretty(&raw_object)?)
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

    pub fn tx_block_response(&self) -> Option<&SuiTransactionBlockResponse> {
        use SuiClientCommandResult::*;
        match self {
            Upgrade(b)
            | Publish(b)
            | TransactionBlock(b)
            | Call(b)
            | Transfer(_, b)
            | TransferSui(b)
            | Pay(b)
            | PaySui(b)
            | PayAllSui(b)
            | SplitCoin(b)
            | MergeCoin(b)
            | ExecuteSignedTx(b) => Some(b),
            _ => None,
        }
    }

    pub fn objects_response(&self) -> Option<Vec<SuiObjectResponse>> {
        use SuiClientCommandResult::*;
        match self {
            Object(o) | RawObject(o) => Some(vec![o.clone()]),
            Objects(o) => Some(o.clone()),
            _ => None,
        }
    }
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum SuiClientCommandResult {
    Upgrade(SuiTransactionBlockResponse),
    Publish(SuiTransactionBlockResponse),
    VerifySource,
    Object(SuiObjectResponse),
    RawObject(SuiObjectResponse),
    TransactionBlock(SuiTransactionBlockResponse),
    Call(SuiTransactionBlockResponse),
    Transfer(
        // Skipping serialisation for elapsed time.
        #[serde(skip)] u128,
        SuiTransactionBlockResponse,
    ),
    TransferSui(SuiTransactionBlockResponse),
    Pay(SuiTransactionBlockResponse),
    PaySui(SuiTransactionBlockResponse),
    PayAllSui(SuiTransactionBlockResponse),
    Addresses(Vec<SuiAddress>, Option<SuiAddress>),
    Objects(Vec<SuiObjectResponse>),
    DynamicFieldQuery(DynamicFieldPage),
    SyncClientState,
    NewAddress((SuiAddress, String, SignatureScheme)),
    Gas(Vec<GasCoin>),
    SplitCoin(SuiTransactionBlockResponse),
    MergeCoin(SuiTransactionBlockResponse),
    Switch(SwitchResponse),
    ActiveAddress(Option<SuiAddress>),
    ActiveEnv(Option<String>),
    Envs(Vec<SuiEnv>, Option<String>),
    /// Return a base64-encoded transaction to be signed elsewhere
    SerializeTx(String),
    ExecuteSignedTx(SuiTransactionBlockResponse),
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
