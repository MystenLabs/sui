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
use sui_sdk::TransactionExecutionResult;
use sui_types::dynamic_field::DynamicFieldType;
use sui_types::intent::Intent;
use sui_types::sui_system_state::SuiSystemState;
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    gas_coin::GasCoin,
    messages::{Transaction, VerifiedTransaction},
    object::Owner,
    parse_sui_type_tag, SUI_FRAMEWORK_ADDRESS,
};
use sui_types::{
    crypto::{Signature, SignatureScheme},
    intent::IntentMessage,
};
use tokio::sync::RwLock;
use tracing::{info, warn};

use sui_sdk::SuiClient;

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

        /// Confirms that compiling dependencies from source results in bytecode matching the
        /// dependency found on-chain.
        #[clap(long)]
        verify_dependencies: bool,
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
        let contents = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 5, 127, 128, 0, 0, 0, 0, 0, 0, 0, 138, 22, 7, 136, 2, 0, 0, 0, 136, 169, 67, 199, 0, 0, 0, 0, 18, 192, 74, 79, 3, 0, 0, 0, 12, 128, 220, 52, 2, 0, 0, 0, 40, 232, 76, 64, 207, 27, 18, 187, 73, 97, 157, 141, 89, 185, 138, 181, 114, 153, 44, 252, 71, 96, 128, 34, 99, 90, 27, 132, 148, 171, 185, 103, 58, 148, 107, 132, 174, 108, 43, 141, 112, 15, 161, 130, 70, 80, 163, 130, 155, 150, 199, 85, 154, 122, 132, 217, 12, 0, 220, 225, 28, 7, 38, 15, 142, 229, 10, 241, 204, 113, 2, 207, 239, 34, 6, 12, 178, 121, 183, 238, 118, 0, 125, 171, 19, 3, 37, 234, 46, 221, 70, 21, 129, 33, 202, 29, 158, 172, 41, 214, 75, 235, 206, 240, 131, 205, 190, 248, 139, 80, 182, 194, 2, 127, 103, 31, 32, 222, 32, 51, 250, 26, 70, 119, 71, 135, 145, 255, 241, 36, 173, 42, 129, 85, 81, 219, 106, 130, 81, 122, 247, 189, 9, 129, 107, 249, 43, 45, 59, 67, 175, 32, 66, 26, 197, 206, 184, 194, 121, 245, 210, 213, 193, 215, 7, 68, 184, 216, 18, 107, 67, 3, 36, 43, 94, 18, 68, 44, 146, 166, 90, 170, 96, 179, 48, 147, 97, 69, 0, 211, 22, 33, 22, 66, 207, 12, 179, 125, 113, 205, 179, 207, 54, 196, 9, 242, 53, 81, 38, 12, 15, 82, 145, 111, 201, 115, 9, 4, 35, 146, 85, 82, 226, 78, 19, 213, 88, 37, 116, 2, 3, 148, 57, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 49, 52, 0, 0, 0, 34, 53, 27, 115, 103, 112, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 115, 103, 112, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 115, 103, 112, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 228, 81, 54, 19, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 228, 81, 54, 19, 0, 0, 0, 0, 228, 81, 54, 19, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 232, 76, 64, 207, 27, 18, 187, 73, 97, 157, 141, 89, 185, 138, 181, 114, 153, 44, 252, 71, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 213, 217, 170, 135, 155, 120, 220, 31, 81, 109, 113, 171, 151, 145, 137, 8, 110, 255, 117, 47, 0, 0, 0, 0, 0, 0, 0, 0, 171, 130, 53, 218, 206, 61, 104, 199, 251, 72, 17, 13, 99, 203, 244, 214, 253, 129, 206, 16, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 241, 73, 108, 229, 243, 126, 88, 109, 2, 28, 226, 107, 67, 77, 69, 2, 248, 4, 110, 19, 96, 129, 39, 221, 36, 115, 102, 19, 3, 59, 145, 114, 34, 48, 40, 143, 129, 65, 74, 253, 108, 79, 171, 195, 45, 76, 238, 224, 61, 209, 123, 76, 72, 136, 163, 17, 230, 196, 30, 228, 159, 208, 32, 30, 177, 134, 106, 137, 86, 5, 16, 213, 52, 133, 132, 106, 204, 203, 86, 224, 59, 239, 0, 61, 190, 174, 116, 138, 41, 240, 209, 184, 157, 157, 41, 2, 241, 254, 176, 115, 223, 120, 142, 55, 119, 219, 191, 7, 224, 215, 203, 136, 185, 197, 197, 196, 174, 32, 174, 44, 82, 57, 201, 34, 110, 199, 255, 217, 213, 14, 14, 78, 172, 178, 172, 236, 246, 111, 138, 81, 167, 115, 31, 145, 15, 174, 48, 207, 184, 208, 32, 124, 8, 55, 39, 53, 90, 43, 166, 21, 190, 80, 45, 157, 220, 62, 219, 216, 50, 119, 211, 93, 208, 7, 230, 184, 205, 150, 43, 231, 239, 36, 130, 48, 138, 117, 239, 130, 223, 151, 223, 11, 152, 69, 0, 179, 138, 100, 2, 40, 23, 77, 237, 52, 147, 238, 75, 223, 117, 240, 212, 113, 187, 134, 179, 87, 194, 103, 111, 76, 223, 155, 48, 131, 221, 106, 49, 196, 124, 172, 213, 135, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 50, 51, 0, 0, 0, 34, 53, 27, 108, 104, 114, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 108, 104, 114, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 108, 104, 114, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 125, 52, 60, 25, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 125, 52, 60, 25, 0, 0, 0, 0, 125, 52, 60, 25, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 241, 73, 108, 229, 243, 126, 88, 109, 2, 28, 226, 107, 67, 77, 69, 2, 248, 4, 110, 19, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 98, 143, 253, 14, 81, 233, 166, 234, 50, 193, 60, 39, 57, 163, 26, 143, 52, 75, 85, 125, 0, 0, 0, 0, 0, 0, 0, 0, 26, 206, 101, 245, 77, 101, 169, 98, 81, 179, 244, 107, 250, 114, 10, 182, 90, 126, 190, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 226, 230, 31, 246, 124, 225, 138, 116, 120, 50, 82, 243, 246, 208, 87, 225, 29, 226, 82, 112, 96, 130, 227, 51, 197, 166, 161, 197, 230, 35, 181, 75, 167, 120, 44, 9, 162, 25, 71, 161, 116, 225, 68, 116, 158, 153, 137, 168, 219, 134, 116, 24, 12, 116, 141, 206, 127, 250, 135, 70, 35, 250, 128, 45, 161, 6, 34, 74, 194, 13, 157, 89, 110, 150, 148, 157, 226, 186, 68, 109, 221, 199, 15, 235, 168, 251, 158, 215, 117, 224, 200, 190, 126, 19, 196, 205, 215, 158, 68, 253, 65, 111, 170, 37, 105, 153, 52, 214, 219, 198, 25, 33, 227, 116, 117, 151, 186, 32, 94, 203, 19, 240, 55, 63, 197, 173, 201, 50, 217, 13, 34, 239, 31, 38, 129, 230, 190, 244, 22, 90, 240, 175, 187, 158, 123, 37, 121, 3, 77, 123, 32, 34, 84, 123, 49, 114, 218, 249, 186, 242, 111, 3, 66, 205, 178, 155, 35, 220, 52, 195, 3, 129, 152, 114, 86, 84, 110, 85, 91, 33, 233, 169, 88, 48, 142, 173, 214, 142, 48, 51, 201, 255, 102, 216, 11, 141, 119, 227, 133, 73, 241, 75, 101, 180, 43, 109, 113, 27, 124, 173, 151, 91, 90, 76, 129, 35, 207, 121, 172, 33, 79, 52, 198, 147, 228, 91, 203, 109, 189, 71, 187, 68, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 49, 53, 0, 0, 0, 34, 53, 27, 115, 103, 112, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 115, 103, 112, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 115, 103, 112, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 125, 52, 60, 25, 0, 0, 0, 0, 96, 119, 176, 16, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 221, 171, 236, 41, 0, 0, 0, 0, 125, 52, 60, 25, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 226, 230, 31, 246, 124, 225, 138, 116, 120, 50, 82, 243, 246, 208, 87, 225, 29, 226, 82, 112, 1, 0, 0, 0, 0, 0, 0, 0, 96, 119, 176, 16, 0, 0, 0, 0, 96, 1, 0, 0, 0, 0, 0, 0, 249, 117, 176, 16, 0, 0, 0, 0, 109, 63, 252, 82, 19, 237, 77, 246, 128, 44, 212, 83, 93, 60, 24, 246, 109, 133, 186, 181, 0, 0, 0, 0, 0, 0, 0, 0, 44, 213, 100, 255, 100, 125, 183, 1, 175, 231, 177, 232, 163, 241, 163, 27, 192, 113, 253, 11, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 106, 216, 192, 178, 155, 23, 164, 123, 243, 231, 46, 147, 205, 186, 127, 219, 78, 187, 59, 237, 96, 133, 219, 58, 246, 192, 183, 111, 36, 159, 232, 54, 149, 110, 196, 204, 209, 192, 180, 152, 240, 125, 205, 228, 118, 98, 180, 81, 136, 170, 85, 124, 144, 90, 135, 217, 23, 106, 198, 241, 86, 241, 213, 21, 242, 183, 87, 27, 182, 8, 120, 37, 162, 203, 22, 197, 206, 227, 121, 147, 250, 98, 234, 65, 16, 116, 86, 8, 196, 176, 214, 109, 149, 12, 70, 38, 193, 76, 253, 142, 34, 184, 46, 15, 82, 0, 204, 139, 245, 81, 179, 65, 89, 229, 239, 111, 221, 32, 204, 182, 208, 107, 18, 251, 45, 207, 144, 75, 178, 175, 132, 136, 150, 71, 50, 56, 216, 153, 141, 177, 77, 171, 245, 132, 70, 174, 7, 99, 76, 165, 32, 137, 114, 8, 135, 213, 33, 154, 106, 99, 68, 114, 244, 212, 40, 54, 247, 158, 236, 118, 212, 142, 46, 237, 214, 11, 240, 183, 108, 137, 1, 177, 245, 48, 137, 131, 140, 236, 120, 53, 228, 174, 5, 130, 155, 102, 36, 55, 113, 121, 24, 199, 238, 21, 190, 5, 248, 228, 229, 96, 107, 49, 248, 115, 16, 162, 246, 143, 73, 254, 25, 12, 107, 29, 34, 212, 95, 14, 16, 241, 95, 77, 11, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 48, 0, 0, 0, 34, 53, 27, 101, 119, 114, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 101, 119, 114, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 101, 119, 114, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 155, 140, 42, 7, 0, 0, 0, 0, 52, 232, 92, 135, 0, 0, 0, 0, 42, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 207, 116, 135, 142, 0, 0, 0, 0, 155, 140, 42, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 42, 0, 0, 0, 0, 0, 0, 0, 106, 216, 192, 178, 155, 23, 164, 123, 243, 231, 46, 147, 205, 186, 127, 219, 78, 187, 59, 237, 1, 0, 0, 0, 0, 0, 0, 0, 52, 232, 92, 135, 0, 0, 0, 0, 100, 11, 0, 0, 0, 0, 0, 0, 249, 219, 92, 135, 0, 0, 0, 0, 1, 179, 177, 221, 24, 163, 183, 117, 254, 14, 13, 75, 135, 60, 10, 160, 208, 205, 42, 207, 0, 0, 0, 0, 0, 0, 0, 0, 60, 43, 48, 124, 50, 57, 246, 22, 67, 175, 94, 154, 9, 215, 208, 201, 91, 254, 20, 220, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 107, 161, 237, 21, 3, 70, 168, 230, 203, 99, 46, 183, 52, 197, 73, 132, 68, 124, 94, 151, 96, 134, 93, 89, 54, 96, 173, 180, 134, 208, 243, 184, 113, 193, 103, 55, 65, 135, 126, 166, 241, 146, 248, 85, 81, 224, 53, 253, 37, 3, 116, 202, 35, 185, 31, 69, 224, 232, 157, 66, 175, 112, 226, 249, 234, 105, 172, 4, 122, 14, 64, 27, 47, 96, 135, 9, 125, 136, 183, 155, 121, 43, 172, 243, 78, 93, 36, 153, 131, 31, 193, 197, 247, 200, 46, 93, 115, 42, 198, 121, 246, 115, 48, 137, 200, 68, 4, 147, 158, 215, 80, 117, 12, 5, 103, 105, 7, 32, 23, 180, 254, 11, 119, 244, 193, 48, 85, 127, 124, 187, 162, 38, 32, 225, 90, 128, 240, 14, 56, 146, 59, 14, 250, 210, 93, 19, 205, 53, 154, 142, 32, 36, 115, 4, 141, 109, 99, 167, 56, 211, 4, 196, 69, 153, 153, 150, 28, 108, 160, 171, 223, 227, 215, 15, 25, 144, 64, 130, 43, 63, 189, 197, 124, 48, 149, 30, 227, 5, 33, 13, 17, 8, 9, 9, 78, 53, 110, 220, 68, 33, 191, 205, 192, 183, 117, 111, 112, 29, 58, 214, 156, 243, 33, 191, 225, 73, 202, 161, 249, 192, 60, 17, 91, 237, 68, 35, 71, 219, 81, 219, 23, 121, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 51, 53, 0, 0, 0, 34, 53, 27, 110, 114, 116, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 110, 114, 116, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 110, 114, 116, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 125, 52, 60, 25, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 125, 52, 60, 25, 0, 0, 0, 0, 125, 52, 60, 25, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 107, 161, 237, 21, 3, 70, 168, 230, 203, 99, 46, 183, 52, 197, 73, 132, 68, 124, 94, 151, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 54, 242, 165, 226, 62, 55, 7, 218, 31, 165, 203, 55, 52, 73, 187, 237, 77, 118, 148, 66, 0, 0, 0, 0, 0, 0, 0, 0, 15, 232, 160, 31, 59, 238, 104, 6, 41, 57, 135, 252, 88, 138, 28, 108, 170, 137, 140, 112, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 128, 82, 17, 232, 33, 168, 110, 56, 24, 238, 75, 151, 191, 230, 60, 19, 119, 178, 23, 154, 96, 137, 45, 9, 17, 115, 148, 197, 217, 96, 182, 238, 243, 46, 149, 171, 18, 153, 55, 127, 136, 139, 170, 32, 25, 185, 58, 183, 140, 244, 173, 102, 125, 113, 229, 232, 125, 27, 141, 14, 165, 168, 166, 174, 8, 210, 86, 84, 217, 17, 178, 69, 178, 34, 36, 34, 138, 156, 138, 253, 3, 46, 7, 45, 108, 104, 209, 126, 20, 82, 100, 216, 197, 29, 120, 177, 82, 150, 21, 15, 33, 147, 233, 53, 225, 196, 236, 218, 111, 185, 208, 203, 232, 118, 165, 245, 67, 32, 202, 123, 197, 182, 78, 229, 195, 29, 47, 45, 127, 179, 231, 209, 152, 238, 62, 223, 78, 216, 71, 57, 10, 35, 13, 137, 208, 166, 87, 77, 253, 178, 32, 38, 171, 104, 156, 26, 231, 156, 121, 206, 70, 82, 215, 47, 47, 165, 214, 68, 75, 181, 55, 137, 0, 123, 105, 72, 19, 95, 181, 118, 39, 237, 31, 48, 160, 23, 35, 144, 188, 82, 55, 219, 218, 102, 46, 226, 234, 113, 164, 30, 147, 127, 21, 66, 236, 158, 101, 19, 213, 59, 156, 149, 126, 40, 164, 7, 16, 170, 114, 162, 75, 139, 243, 236, 89, 98, 176, 188, 35, 68, 15, 195, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 51, 57, 0, 0, 0, 34, 53, 27, 109, 105, 97, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 109, 105, 97, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 109, 105, 97, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 125, 52, 60, 25, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 125, 52, 60, 25, 0, 0, 0, 0, 125, 52, 60, 25, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 128, 82, 17, 232, 33, 168, 110, 56, 24, 238, 75, 151, 191, 230, 60, 19, 119, 178, 23, 154, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 36, 44, 112, 226, 96, 186, 221, 72, 52, 64, 180, 227, 218, 214, 62, 157, 136, 15, 247, 32, 0, 0, 0, 0, 0, 0, 0, 0, 22, 165, 173, 118, 178, 177, 164, 77, 177, 88, 137, 130, 133, 82, 74, 224, 189, 146, 83, 168, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 163, 238, 36, 181, 206, 105, 96, 137, 155, 104, 123, 92, 66, 55, 8, 29, 156, 137, 25, 91, 96, 140, 102, 214, 104, 68, 197, 166, 41, 181, 73, 185, 247, 153, 105, 153, 23, 108, 189, 149, 243, 51, 110, 24, 194, 92, 176, 118, 216, 213, 127, 110, 198, 135, 204, 5, 62, 23, 181, 239, 108, 92, 6, 180, 166, 111, 189, 79, 93, 9, 152, 134, 209, 71, 73, 48, 215, 106, 76, 135, 24, 163, 78, 178, 31, 116, 135, 234, 59, 69, 164, 188, 220, 187, 10, 198, 255, 171, 183, 21, 162, 228, 101, 111, 128, 77, 211, 79, 59, 107, 77, 242, 249, 105, 134, 176, 48, 32, 225, 3, 158, 209, 110, 5, 248, 184, 112, 35, 184, 105, 243, 78, 86, 88, 22, 75, 213, 252, 249, 242, 124, 13, 254, 57, 37, 248, 57, 135, 142, 219, 32, 5, 225, 201, 125, 158, 99, 203, 229, 203, 57, 32, 142, 130, 109, 67, 74, 137, 24, 154, 216, 164, 159, 184, 230, 243, 169, 25, 152, 239, 100, 203, 46, 48, 139, 224, 148, 15, 51, 164, 88, 185, 180, 149, 117, 212, 251, 221, 82, 4, 44, 253, 80, 60, 89, 46, 205, 5, 154, 201, 237, 100, 150, 216, 243, 175, 25, 144, 33, 119, 88, 152, 252, 165, 234, 189, 100, 39, 74, 16, 152, 141, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 49, 56, 0, 0, 0, 34, 53, 27, 97, 109, 115, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 97, 109, 115, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 97, 109, 115, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 228, 81, 54, 19, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 228, 81, 54, 19, 0, 0, 0, 0, 228, 81, 54, 19, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 163, 238, 36, 181, 206, 105, 96, 137, 155, 104, 123, 92, 66, 55, 8, 29, 156, 137, 25, 91, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 23, 37, 221, 221, 30, 171, 227, 167, 186, 215, 232, 243, 13, 176, 89, 15, 51, 2, 224, 65, 0, 0, 0, 0, 0, 0, 0, 0, 26, 117, 196, 223, 54, 127, 192, 23, 93, 119, 146, 5, 167, 102, 103, 31, 140, 247, 119, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 179, 1, 108, 252, 177, 72, 49, 15, 118, 4, 186, 54, 127, 153, 107, 53, 201, 144, 9, 227, 96, 140, 233, 134, 76, 220, 11, 28, 98, 46, 7, 58, 55, 69, 95, 223, 173, 243, 132, 181, 254, 39, 239, 12, 95, 67, 115, 238, 193, 242, 143, 46, 199, 116, 124, 117, 115, 71, 95, 189, 254, 213, 118, 19, 182, 183, 200, 110, 172, 21, 43, 52, 225, 229, 45, 7, 148, 48, 4, 0, 203, 70, 79, 73, 79, 123, 137, 155, 133, 254, 239, 126, 134, 182, 16, 87, 18, 0, 64, 30, 5, 84, 92, 49, 52, 252, 145, 149, 207, 4, 214, 204, 46, 221, 58, 40, 2, 32, 244, 36, 163, 3, 47, 132, 114, 133, 252, 104, 85, 150, 211, 83, 70, 164, 184, 86, 96, 213, 203, 51, 169, 7, 11, 41, 196, 133, 59, 70, 228, 209, 32, 120, 58, 96, 109, 134, 100, 63, 108, 176, 69, 110, 0, 196, 2, 175, 119, 158, 125, 105, 135, 49, 242, 66, 121, 187, 102, 223, 9, 158, 3, 44, 113, 48, 161, 91, 238, 248, 15, 11, 100, 64, 202, 197, 61, 63, 119, 100, 213, 0, 52, 247, 163, 86, 2, 212, 116, 66, 43, 161, 30, 151, 137, 37, 39, 75, 215, 225, 212, 109, 28, 0, 212, 196, 141, 15, 234, 10, 90, 234, 7, 229, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 51, 48, 0, 0, 0, 34, 53, 27, 115, 121, 100, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 115, 121, 100, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 115, 121, 100, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 228, 81, 54, 19, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 228, 81, 54, 19, 0, 0, 0, 0, 228, 81, 54, 19, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 179, 1, 108, 252, 177, 72, 49, 15, 118, 4, 186, 54, 127, 153, 107, 53, 201, 144, 9, 227, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 207, 126, 165, 113, 164, 239, 166, 94, 181, 205, 88, 191, 67, 168, 42, 177, 24, 220, 142, 160, 0, 0, 0, 0, 0, 0, 0, 0, 1, 160, 127, 25, 127, 144, 229, 142, 13, 144, 234, 187, 75, 215, 206, 48, 144, 14, 176, 50, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 174, 146, 19, 175, 146, 2, 168, 163, 32, 117, 208, 90, 102, 171, 85, 226, 36, 242, 84, 42, 96, 141, 85, 173, 220, 125, 158, 127, 17, 25, 35, 233, 97, 176, 195, 20, 227, 133, 196, 167, 215, 164, 193, 71, 157, 138, 86, 149, 129, 243, 77, 70, 174, 121, 170, 160, 195, 42, 105, 221, 51, 209, 97, 224, 248, 104, 66, 15, 54, 13, 112, 116, 70, 138, 55, 125, 154, 131, 15, 56, 170, 245, 179, 81, 234, 241, 159, 9, 92, 14, 236, 141, 22, 135, 7, 76, 186, 22, 209, 136, 213, 159, 71, 44, 42, 121, 75, 252, 245, 168, 234, 13, 182, 13, 202, 215, 183, 32, 175, 14, 241, 108, 42, 196, 50, 91, 57, 87, 16, 231, 163, 14, 247, 136, 129, 242, 71, 98, 145, 171, 166, 187, 42, 7, 137, 54, 100, 60, 189, 37, 32, 151, 211, 39, 24, 182, 152, 76, 65, 111, 122, 113, 225, 185, 36, 244, 65, 127, 223, 39, 113, 15, 82, 100, 68, 81, 122, 146, 86, 202, 161, 250, 253, 48, 178, 45, 26, 83, 13, 253, 148, 16, 142, 80, 175, 74, 104, 12, 180, 30, 30, 219, 240, 106, 173, 105, 20, 24, 138, 243, 205, 7, 163, 210, 189, 132, 238, 39, 241, 27, 207, 36, 235, 98, 164, 187, 68, 221, 40, 246, 44, 106, 11, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 53, 0, 0, 0, 34, 53, 27, 111, 114, 100, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 111, 114, 100, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 111, 114, 100, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 69, 111, 48, 13, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 69, 111, 48, 13, 0, 0, 0, 0, 69, 111, 48, 13, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 174, 146, 19, 175, 146, 2, 168, 163, 32, 117, 208, 90, 102, 171, 85, 226, 36, 242, 84, 42, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 168, 54, 94, 38, 58, 129, 81, 197, 21, 112, 107, 15, 243, 91, 61, 99, 115, 172, 181, 244, 0, 0, 0, 0, 0, 0, 0, 0, 54, 54, 164, 132, 154, 6, 172, 71, 75, 183, 226, 154, 152, 234, 231, 174, 64, 113, 144, 44, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 136, 30, 145, 86, 246, 230, 34, 117, 53, 79, 226, 146, 113, 110, 224, 156, 114, 119, 114, 115, 96, 141, 123, 40, 58, 110, 2, 12, 152, 105, 207, 252, 41, 24, 61, 233, 52, 10, 49, 149, 78, 2, 201, 245, 122, 183, 174, 240, 51, 114, 223, 191, 63, 242, 152, 23, 31, 46, 33, 207, 226, 194, 214, 81, 250, 48, 247, 213, 90, 12, 111, 105, 33, 72, 51, 114, 181, 183, 218, 178, 115, 47, 245, 93, 146, 160, 113, 124, 138, 250, 17, 76, 23, 212, 216, 79, 44, 33, 10, 255, 72, 15, 75, 175, 78, 39, 214, 164, 178, 247, 41, 232, 178, 76, 238, 146, 36, 32, 125, 93, 31, 49, 168, 178, 32, 153, 174, 191, 220, 29, 220, 20, 44, 135, 235, 121, 185, 159, 41, 254, 20, 85, 41, 5, 117, 235, 211, 97, 196, 252, 32, 152, 89, 74, 92, 1, 237, 114, 19, 70, 208, 109, 134, 55, 224, 174, 211, 163, 91, 192, 239, 193, 210, 134, 219, 161, 83, 57, 252, 80, 255, 194, 241, 48, 136, 224, 105, 105, 39, 151, 217, 225, 207, 203, 205, 3, 251, 98, 108, 190, 96, 75, 58, 70, 106, 122, 93, 171, 87, 55, 102, 125, 23, 149, 167, 156, 254, 251, 0, 200, 52, 207, 134, 195, 243, 64, 122, 79, 192, 233, 99, 224, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 51, 51, 0, 0, 0, 34, 53, 27, 110, 114, 116, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 110, 114, 116, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 110, 114, 116, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 69, 111, 48, 13, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 69, 111, 48, 13, 0, 0, 0, 0, 69, 111, 48, 13, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 136, 30, 145, 86, 246, 230, 34, 117, 53, 79, 226, 146, 113, 110, 224, 156, 114, 119, 114, 115, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 197, 35, 75, 52, 1, 174, 221, 90, 63, 237, 224, 40, 59, 112, 97, 5, 206, 14, 242, 126, 0, 0, 0, 0, 0, 0, 0, 0, 106, 231, 171, 129, 85, 238, 56, 147, 74, 51, 225, 125, 75, 186, 100, 137, 47, 206, 49, 198, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 33, 70, 108, 156, 152, 153, 157, 203, 212, 67, 188, 182, 144, 240, 59, 40, 154, 172, 168, 227, 96, 141, 142, 15, 240, 110, 248, 35, 119, 100, 245, 33, 133, 114, 48, 105, 114, 25, 116, 21, 50, 109, 160, 132, 62, 195, 38, 218, 79, 118, 224, 99, 70, 202, 211, 234, 13, 85, 8, 220, 142, 152, 7, 97, 77, 178, 205, 75, 201, 15, 99, 220, 69, 37, 5, 134, 198, 193, 47, 66, 68, 178, 149, 230, 218, 130, 221, 134, 52, 89, 25, 81, 123, 203, 12, 91, 88, 14, 244, 29, 105, 122, 218, 172, 24, 105, 145, 93, 103, 84, 219, 183, 32, 155, 8, 67, 89, 32, 194, 76, 44, 208, 49, 53, 238, 158, 193, 4, 193, 204, 78, 109, 96, 242, 55, 153, 131, 200, 74, 235, 247, 192, 249, 13, 1, 63, 8, 9, 98, 62, 32, 11, 214, 246, 42, 251, 165, 43, 163, 64, 87, 30, 140, 10, 55, 169, 66, 13, 172, 66, 243, 103, 90, 21, 171, 22, 255, 90, 189, 236, 219, 31, 247, 48, 176, 145, 146, 158, 67, 146, 233, 238, 222, 55, 236, 125, 22, 237, 120, 253, 98, 142, 158, 203, 170, 225, 9, 189, 41, 23, 19, 125, 172, 120, 71, 87, 32, 134, 210, 154, 89, 54, 53, 146, 229, 26, 160, 76, 172, 202, 87, 10, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 49, 55, 0, 0, 0, 34, 53, 27, 97, 109, 115, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 97, 109, 115, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 97, 109, 115, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 69, 111, 48, 13, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 69, 111, 48, 13, 0, 0, 0, 0, 69, 111, 48, 13, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 33, 70, 108, 156, 152, 153, 157, 203, 212, 67, 188, 182, 144, 240, 59, 40, 154, 172, 168, 227, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 117, 47, 250, 185, 64, 85, 124, 106, 96, 154, 49, 56, 223, 209, 173, 225, 27, 252, 42, 207, 0, 0, 0, 0, 0, 0, 0, 0, 101, 199, 83, 224, 142, 183, 28, 108, 3, 43, 207, 223, 204, 96, 43, 77, 45, 151, 226, 124, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 104, 153, 146, 140, 137, 19, 160, 121, 37, 243, 109, 203, 88, 212, 167, 236, 130, 159, 90, 43, 96, 142, 14, 87, 218, 104, 92, 144, 81, 249, 208, 213, 122, 41, 22, 19, 0, 31, 209, 73, 65, 235, 170, 200, 190, 232, 64, 69, 186, 91, 93, 55, 211, 157, 190, 21, 216, 54, 210, 162, 128, 33, 128, 51, 23, 107, 180, 188, 211, 14, 37, 21, 75, 254, 13, 177, 27, 65, 155, 0, 23, 118, 216, 228, 31, 73, 190, 172, 21, 172, 107, 190, 74, 14, 88, 38, 234, 18, 52, 241, 249, 204, 252, 159, 151, 179, 23, 78, 189, 55, 184, 213, 88, 29, 37, 23, 133, 32, 78, 154, 185, 0, 184, 126, 133, 56, 131, 150, 26, 49, 212, 128, 171, 217, 5, 196, 58, 58, 214, 164, 105, 254, 57, 194, 130, 204, 247, 95, 54, 164, 32, 155, 156, 145, 1, 64, 82, 145, 95, 76, 108, 165, 157, 119, 52, 235, 193, 236, 194, 122, 67, 161, 115, 29, 243, 118, 187, 108, 106, 17, 24, 145, 21, 48, 143, 185, 254, 249, 18, 129, 28, 121, 181, 23, 125, 76, 240, 64, 173, 219, 173, 121, 127, 24, 187, 140, 33, 10, 33, 20, 122, 197, 175, 152, 189, 15, 23, 37, 56, 112, 18, 139, 70, 156, 210, 68, 242, 4, 230, 207, 58, 210, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 50, 50, 0, 0, 0, 34, 53, 27, 108, 104, 114, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 108, 104, 114, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 108, 104, 114, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 228, 81, 54, 19, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 228, 81, 54, 19, 0, 0, 0, 0, 228, 81, 54, 19, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 104, 153, 146, 140, 137, 19, 160, 121, 37, 243, 109, 203, 88, 212, 167, 236, 130, 159, 90, 43, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 147, 84, 222, 6, 28, 99, 92, 253, 19, 228, 252, 35, 178, 188, 210, 99, 176, 173, 37, 166, 0, 0, 0, 0, 0, 0, 0, 0, 225, 8, 13, 17, 139, 89, 135, 19, 84, 215, 240, 239, 185, 116, 9, 197, 125, 87, 63, 207, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 8, 112, 140, 200, 27, 191, 192, 176, 235, 17, 246, 238, 218, 199, 152, 222, 144, 158, 40, 110, 96, 144, 17, 91, 78, 167, 241, 216, 70, 17, 76, 93, 38, 251, 121, 216, 124, 22, 9, 20, 64, 11, 45, 37, 49, 109, 72, 59, 116, 127, 79, 35, 104, 5, 48, 237, 83, 200, 182, 10, 221, 166, 139, 252, 207, 147, 161, 41, 110, 7, 0, 118, 30, 217, 4, 236, 55, 127, 177, 183, 94, 199, 47, 85, 48, 43, 137, 107, 202, 150, 118, 61, 143, 101, 159, 117, 215, 2, 188, 64, 196, 138, 253, 186, 232, 63, 177, 4, 205, 15, 1, 251, 241, 218, 17, 191, 218, 32, 201, 134, 122, 57, 66, 77, 229, 36, 255, 26, 96, 25, 112, 33, 236, 102, 206, 107, 25, 138, 29, 4, 133, 16, 153, 86, 201, 90, 19, 209, 27, 40, 32, 137, 111, 91, 65, 66, 25, 212, 111, 39, 115, 46, 250, 118, 10, 7, 103, 146, 230, 103, 245, 92, 12, 226, 109, 153, 8, 167, 122, 97, 173, 245, 17, 48, 131, 67, 141, 51, 65, 18, 207, 69, 7, 50, 100, 230, 134, 131, 196, 163, 190, 217, 216, 148, 127, 122, 152, 72, 84, 63, 16, 74, 42, 58, 116, 160, 156, 6, 208, 70, 158, 201, 178, 179, 188, 209, 237, 132, 186, 204, 227, 111, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 50, 55, 0, 0, 0, 34, 53, 27, 115, 106, 99, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 115, 106, 99, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 115, 106, 99, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 125, 52, 60, 25, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 125, 52, 60, 25, 0, 0, 0, 0, 125, 52, 60, 25, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 8, 112, 140, 200, 27, 191, 192, 176, 235, 17, 246, 238, 218, 199, 152, 222, 144, 158, 40, 110, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 239, 156, 168, 192, 128, 30, 205, 100, 35, 44, 156, 131, 193, 11, 183, 93, 15, 7, 165, 109, 0, 0, 0, 0, 0, 0, 0, 0, 176, 146, 98, 63, 194, 102, 183, 170, 20, 112, 248, 141, 26, 150, 95, 26, 72, 144, 6, 148, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 122, 214, 187, 208, 129, 9, 37, 222, 5, 77, 51, 137, 54, 172, 127, 37, 223, 68, 65, 239, 96, 144, 188, 49, 5, 44, 35, 90, 114, 156, 167, 65, 159, 133, 76, 114, 241, 174, 205, 197, 22, 205, 24, 183, 128, 104, 195, 224, 214, 255, 134, 98, 221, 113, 231, 207, 48, 210, 126, 221, 28, 53, 73, 9, 242, 199, 236, 82, 125, 13, 206, 44, 211, 184, 87, 252, 84, 229, 174, 225, 87, 175, 175, 238, 242, 106, 159, 215, 199, 31, 194, 179, 91, 45, 189, 238, 163, 218, 11, 213, 62, 95, 28, 239, 224, 246, 191, 66, 129, 238, 137, 86, 233, 153, 94, 89, 198, 32, 227, 174, 216, 44, 32, 249, 107, 73, 43, 168, 57, 61, 45, 205, 58, 14, 40, 179, 116, 35, 41, 18, 49, 113, 20, 96, 71, 77, 117, 38, 165, 16, 32, 181, 50, 108, 237, 165, 171, 226, 110, 195, 180, 12, 9, 205, 3, 96, 129, 7, 60, 216, 36, 127, 235, 232, 192, 7, 2, 90, 82, 93, 12, 168, 243, 48, 165, 119, 231, 209, 195, 90, 129, 198, 33, 151, 145, 211, 245, 255, 16, 51, 14, 8, 128, 85, 159, 88, 121, 181, 230, 166, 88, 151, 94, 138, 131, 122, 25, 3, 233, 201, 119, 83, 165, 113, 122, 244, 110, 134, 235, 71, 222, 135, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 50, 54, 0, 0, 0, 34, 53, 27, 115, 106, 99, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 115, 106, 99, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 115, 106, 99, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 228, 81, 54, 19, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 228, 81, 54, 19, 0, 0, 0, 0, 228, 81, 54, 19, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 122, 214, 187, 208, 129, 9, 37, 222, 5, 77, 51, 137, 54, 172, 127, 37, 223, 68, 65, 239, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 246, 86, 70, 61, 233, 222, 58, 44, 77, 47, 178, 157, 184, 157, 149, 168, 96, 168, 183, 47, 0, 0, 0, 0, 0, 0, 0, 0, 96, 131, 158, 253, 17, 250, 56, 191, 61, 42, 71, 168, 39, 198, 187, 70, 182, 45, 85, 201, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 138, 4, 23, 152, 181, 43, 155, 240, 31, 97, 102, 142, 135, 42, 145, 58, 125, 18, 50, 223, 96, 144, 220, 242, 35, 41, 201, 192, 12, 200, 110, 245, 118, 207, 110, 2, 79, 155, 240, 230, 178, 8, 137, 134, 181, 81, 83, 247, 86, 0, 175, 123, 249, 37, 2, 172, 143, 87, 139, 103, 73, 177, 62, 204, 122, 180, 60, 105, 154, 6, 208, 92, 204, 71, 172, 172, 135, 239, 22, 241, 62, 200, 119, 150, 35, 43, 185, 146, 39, 199, 45, 192, 240, 82, 50, 167, 44, 90, 199, 90, 56, 244, 74, 181, 51, 192, 182, 104, 33, 104, 18, 6, 42, 205, 88, 209, 6, 32, 106, 87, 118, 5, 201, 33, 59, 102, 71, 249, 111, 12, 2, 182, 133, 176, 231, 160, 120, 253, 239, 247, 58, 142, 142, 138, 125, 32, 208, 26, 103, 131, 32, 102, 228, 76, 104, 209, 88, 252, 48, 137, 184, 144, 43, 14, 37, 213, 86, 150, 175, 87, 28, 165, 155, 223, 189, 132, 221, 123, 7, 110, 57, 152, 124, 48, 151, 44, 17, 57, 4, 150, 215, 193, 99, 111, 70, 17, 34, 208, 101, 76, 196, 64, 114, 133, 102, 174, 146, 157, 23, 78, 208, 58, 140, 32, 185, 137, 159, 229, 15, 224, 36, 27, 234, 214, 233, 159, 146, 120, 14, 29, 135, 57, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 51, 49, 0, 0, 0, 34, 53, 27, 115, 121, 100, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 115, 121, 100, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 115, 121, 100, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 125, 52, 60, 25, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 125, 52, 60, 25, 0, 0, 0, 0, 125, 52, 60, 25, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 138, 4, 23, 152, 181, 43, 155, 240, 31, 97, 102, 142, 135, 42, 145, 58, 125, 18, 50, 223, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 218, 5, 158, 24, 15, 202, 234, 101, 210, 59, 125, 102, 130, 17, 145, 50, 167, 159, 118, 151, 0, 0, 0, 0, 0, 0, 0, 0, 13, 225, 51, 7, 80, 18, 9, 236, 124, 88, 195, 151, 108, 127, 84, 176, 19, 14, 107, 77, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 62, 135, 120, 109, 247, 82, 93, 235, 16, 136, 7, 7, 227, 35, 154, 3, 36, 243, 106, 91, 96, 146, 28, 141, 93, 65, 36, 154, 0, 228, 96, 112, 59, 221, 61, 33, 139, 84, 46, 130, 119, 37, 205, 53, 88, 13, 145, 107, 118, 164, 177, 119, 159, 11, 249, 195, 120, 86, 94, 213, 28, 116, 82, 144, 220, 195, 210, 174, 97, 11, 27, 153, 7, 64, 149, 191, 146, 80, 214, 43, 16, 208, 191, 84, 105, 212, 49, 26, 39, 204, 32, 125, 13, 137, 143, 227, 84, 114, 86, 24, 174, 134, 8, 126, 202, 171, 27, 165, 149, 121, 16, 135, 200, 59, 44, 112, 5, 32, 178, 52, 65, 219, 109, 40, 206, 101, 120, 216, 80, 95, 44, 254, 83, 235, 73, 46, 191, 44, 98, 216, 86, 108, 175, 48, 14, 255, 203, 36, 203, 251, 32, 143, 245, 190, 101, 140, 51, 78, 178, 149, 214, 141, 251, 13, 83, 253, 239, 93, 49, 161, 163, 158, 77, 88, 215, 33, 137, 110, 134, 103, 164, 184, 110, 48, 130, 86, 226, 109, 73, 216, 172, 129, 134, 2, 40, 117, 168, 120, 188, 248, 128, 54, 139, 179, 34, 141, 112, 110, 121, 171, 134, 67, 111, 182, 219, 134, 42, 46, 115, 14, 166, 20, 19, 143, 243, 99, 149, 31, 137, 45, 241, 154, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 50, 48, 0, 0, 0, 34, 53, 27, 108, 104, 114, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 108, 104, 114, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 108, 104, 114, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 155, 140, 42, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 155, 140, 42, 7, 0, 0, 0, 0, 155, 140, 42, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 62, 135, 120, 109, 247, 82, 93, 235, 16, 136, 7, 7, 227, 35, 154, 3, 36, 243, 106, 91, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 244, 60, 115, 196, 17, 188, 251, 62, 101, 181, 121, 60, 122, 187, 205, 35, 214, 65, 5, 182, 0, 0, 0, 0, 0, 0, 0, 0, 185, 248, 114, 169, 86, 207, 136, 31, 121, 206, 98, 115, 183, 105, 145, 36, 180, 81, 103, 116, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 252, 138, 236, 86, 103, 105, 177, 77, 107, 84, 104, 209, 218, 154, 63, 0, 8, 56, 25, 100, 96, 146, 208, 30, 203, 17, 56, 249, 176, 115, 195, 219, 128, 131, 56, 145, 236, 170, 175, 36, 29, 39, 101, 210, 141, 161, 98, 114, 127, 198, 8, 143, 111, 7, 53, 51, 67, 121, 2, 225, 162, 60, 33, 91, 50, 100, 111, 65, 135, 16, 70, 149, 24, 32, 130, 90, 174, 154, 85, 209, 69, 203, 97, 59, 65, 31, 187, 164, 9, 85, 255, 73, 151, 89, 30, 249, 22, 159, 101, 5, 13, 141, 241, 1, 31, 172, 86, 63, 109, 30, 0, 99, 64, 195, 87, 18, 43, 32, 2, 103, 22, 82, 47, 95, 111, 77, 93, 202, 137, 180, 134, 84, 242, 166, 29, 159, 105, 162, 111, 236, 11, 79, 84, 123, 103, 97, 226, 252, 235, 162, 32, 27, 41, 97, 36, 136, 251, 130, 40, 30, 47, 199, 198, 205, 250, 154, 153, 178, 178, 199, 133, 245, 238, 127, 132, 253, 178, 113, 37, 61, 208, 51, 204, 48, 173, 164, 9, 223, 139, 195, 35, 216, 218, 54, 124, 50, 17, 47, 238, 254, 17, 103, 71, 117, 196, 233, 200, 221, 159, 66, 20, 101, 238, 207, 218, 215, 65, 83, 64, 53, 29, 123, 214, 209, 113, 5, 236, 79, 138, 191, 121, 132, 11, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 57, 0, 0, 0, 34, 53, 27, 100, 102, 119, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 100, 102, 119, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 100, 102, 119, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 69, 111, 48, 13, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 69, 111, 48, 13, 0, 0, 0, 0, 69, 111, 48, 13, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 252, 138, 236, 86, 103, 105, 177, 77, 107, 84, 104, 209, 218, 154, 63, 0, 8, 56, 25, 100, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 160, 165, 67, 130, 32, 131, 96, 192, 227, 95, 106, 208, 63, 182, 207, 230, 31, 52, 147, 159, 0, 0, 0, 0, 0, 0, 0, 0, 66, 52, 149, 30, 55, 162, 250, 175, 53, 105, 92, 19, 147, 58, 113, 36, 47, 56, 118, 225, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 138, 69, 130, 156, 233, 25, 164, 29, 154, 68, 108, 67, 209, 218, 67, 171, 6, 58, 125, 245, 96, 146, 216, 155, 22, 61, 43, 225, 196, 175, 93, 39, 32, 146, 15, 93, 223, 140, 9, 152, 16, 187, 108, 237, 70, 33, 138, 76, 133, 74, 117, 165, 48, 126, 187, 169, 130, 71, 64, 245, 194, 238, 105, 43, 40, 56, 119, 231, 235, 11, 172, 105, 18, 142, 48, 138, 64, 247, 244, 176, 15, 121, 105, 208, 233, 67, 115, 43, 108, 205, 78, 81, 170, 73, 39, 79, 57, 9, 204, 90, 161, 139, 29, 100, 191, 139, 97, 158, 159, 110, 19, 100, 243, 113, 17, 173, 105, 32, 159, 115, 123, 64, 32, 150, 250, 214, 240, 247, 105, 26, 101, 30, 132, 237, 74, 13, 203, 217, 248, 197, 40, 11, 147, 154, 48, 58, 62, 147, 88, 200, 32, 26, 197, 50, 99, 226, 38, 151, 251, 181, 26, 165, 42, 226, 12, 80, 253, 6, 46, 105, 163, 205, 135, 69, 123, 255, 252, 71, 127, 230, 181, 118, 22, 48, 153, 19, 105, 237, 249, 125, 236, 205, 161, 190, 6, 187, 6, 170, 92, 250, 230, 192, 71, 99, 230, 71, 230, 246, 98, 105, 212, 195, 202, 222, 38, 111, 100, 252, 51, 243, 89, 191, 83, 215, 50, 91, 120, 65, 186, 216, 54, 194, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 50, 52, 0, 0, 0, 34, 53, 27, 115, 106, 99, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 115, 106, 99, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 115, 106, 99, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 155, 140, 42, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 155, 140, 42, 7, 0, 0, 0, 0, 155, 140, 42, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 138, 69, 130, 156, 233, 25, 164, 29, 154, 68, 108, 67, 209, 218, 67, 171, 6, 58, 125, 245, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 20, 112, 247, 157, 115, 17, 128, 2, 237, 78, 59, 104, 108, 54, 10, 202, 3, 156, 64, 96, 0, 0, 0, 0, 0, 0, 0, 0, 58, 175, 237, 253, 174, 135, 110, 4, 178, 131, 192, 112, 50, 244, 122, 150, 243, 238, 172, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 179, 251, 79, 253, 214, 182, 228, 194, 68, 3, 17, 191, 142, 125, 64, 29, 179, 244, 14, 143, 96, 146, 242, 151, 26, 142, 240, 160, 140, 91, 146, 127, 134, 227, 187, 129, 246, 168, 254, 147, 249, 110, 217, 202, 76, 130, 35, 173, 247, 191, 209, 242, 138, 234, 216, 58, 115, 7, 94, 59, 98, 15, 10, 83, 43, 239, 162, 81, 40, 4, 246, 222, 239, 62, 204, 108, 92, 27, 41, 230, 114, 222, 122, 74, 80, 12, 80, 208, 18, 127, 5, 19, 117, 159, 139, 129, 136, 161, 133, 57, 53, 129, 63, 69, 92, 197, 85, 192, 233, 123, 63, 255, 104, 187, 209, 127, 231, 32, 33, 153, 173, 197, 158, 239, 222, 169, 145, 171, 148, 107, 246, 47, 161, 152, 250, 170, 230, 158, 16, 95, 109, 34, 208, 160, 213, 241, 111, 70, 52, 122, 32, 108, 245, 68, 178, 1, 242, 165, 254, 69, 80, 54, 65, 216, 53, 243, 84, 69, 87, 181, 30, 151, 62, 105, 76, 79, 10, 150, 211, 233, 219, 79, 141, 48, 168, 12, 132, 76, 9, 207, 163, 51, 3, 9, 119, 201, 88, 109, 176, 173, 66, 18, 102, 130, 177, 202, 238, 50, 10, 40, 5, 220, 62, 113, 173, 70, 219, 117, 3, 174, 216, 121, 241, 241, 43, 75, 214, 145, 244, 165, 5, 212, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 51, 50, 0, 0, 0, 34, 53, 27, 110, 114, 116, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 110, 114, 116, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 110, 114, 116, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 155, 140, 42, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 155, 140, 42, 7, 0, 0, 0, 0, 155, 140, 42, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 179, 251, 79, 253, 214, 182, 228, 194, 68, 3, 17, 191, 142, 125, 64, 29, 179, 244, 14, 143, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 13, 99, 200, 166, 124, 209, 214, 88, 89, 116, 162, 156, 145, 106, 46, 143, 243, 104, 83, 60, 0, 0, 0, 0, 0, 0, 0, 0, 24, 211, 151, 117, 168, 138, 143, 231, 174, 180, 162, 113, 234, 21, 100, 177, 181, 56, 100, 208, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 102, 132, 91, 188, 57, 46, 8, 114, 23, 183, 7, 201, 206, 178, 176, 119, 72, 216, 172, 96, 150, 12, 75, 244, 124, 76, 191, 31, 138, 233, 147, 67, 78, 140, 137, 0, 119, 9, 155, 27, 62, 96, 120, 225, 22, 50, 186, 173, 53, 10, 4, 218, 170, 75, 72, 50, 172, 222, 248, 128, 131, 216, 74, 16, 247, 246, 35, 89, 18, 117, 214, 176, 164, 33, 231, 127, 139, 75, 212, 87, 81, 115, 115, 206, 16, 75, 255, 254, 14, 224, 66, 233, 38, 127, 129, 42, 97, 132, 37, 182, 234, 62, 10, 91, 178, 91, 87, 130, 228, 158, 241, 156, 73, 108, 41, 236, 32, 6, 123, 97, 207, 17, 90, 131, 114, 49, 108, 129, 7, 85, 216, 14, 24, 246, 247, 17, 69, 45, 187, 85, 52, 65, 115, 190, 148, 181, 3, 125, 102, 32, 99, 98, 115, 255, 52, 129, 6, 227, 53, 219, 74, 15, 208, 77, 8, 236, 178, 52, 224, 72, 219, 179, 15, 242, 82, 90, 159, 66, 50, 97, 35, 208, 48, 182, 221, 209, 8, 64, 106, 37, 248, 170, 248, 105, 51, 220, 114, 137, 169, 113, 217, 10, 224, 38, 112, 225, 214, 63, 64, 102, 133, 239, 186, 248, 199, 34, 97, 106, 168, 79, 95, 132, 13, 42, 201, 234, 119, 217, 71, 71, 198, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 51, 52, 0, 0, 0, 34, 53, 27, 110, 114, 116, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 110, 114, 116, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 110, 114, 116, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 228, 81, 54, 19, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 228, 81, 54, 19, 0, 0, 0, 0, 228, 81, 54, 19, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 255, 102, 132, 91, 188, 57, 46, 8, 114, 23, 183, 7, 201, 206, 178, 176, 119, 72, 216, 172, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 60, 246, 71, 99, 96, 190, 179, 142, 207, 248, 253, 67, 120, 49, 250, 38, 56, 21, 89, 17, 0, 0, 0, 0, 0, 0, 0, 0, 128, 171, 46, 199, 55, 181, 155, 82, 125, 154, 60, 88, 96, 142, 73, 62, 224, 129, 194, 8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 189, 140, 56, 205, 22, 242, 253, 73, 29, 123, 20, 28, 60, 240, 92, 54, 194, 158, 161, 53, 96, 151, 162, 106, 19, 123, 189, 229, 133, 68, 1, 30, 106, 173, 133, 129, 220, 72, 20, 131, 157, 229, 128, 164, 29, 64, 131, 96, 79, 190, 92, 65, 156, 32, 69, 191, 236, 91, 30, 57, 12, 36, 21, 19, 136, 156, 64, 238, 69, 19, 36, 63, 241, 90, 29, 39, 200, 156, 208, 77, 237, 96, 172, 242, 97, 70, 184, 4, 175, 177, 131, 164, 75, 184, 134, 148, 74, 68, 0, 151, 172, 111, 60, 26, 38, 155, 37, 202, 191, 103, 92, 235, 66, 32, 234, 64, 165, 32, 220, 7, 249, 107, 99, 98, 112, 14, 182, 255, 230, 79, 52, 128, 51, 179, 224, 76, 147, 99, 252, 147, 249, 179, 34, 29, 75, 59, 85, 96, 243, 151, 32, 106, 168, 219, 60, 3, 191, 127, 216, 116, 69, 36, 195, 123, 215, 161, 161, 158, 43, 77, 47, 154, 107, 200, 61, 219, 168, 137, 33, 108, 115, 105, 32, 48, 138, 109, 182, 106, 233, 138, 63, 153, 179, 54, 101, 89, 181, 71, 220, 77, 164, 4, 114, 51, 57, 193, 22, 46, 91, 200, 30, 114, 94, 102, 49, 74, 135, 202, 36, 167, 93, 184, 239, 103, 68, 238, 46, 84, 73, 93, 211, 61, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 49, 51, 0, 0, 0, 34, 53, 27, 115, 103, 112, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 115, 103, 112, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 115, 103, 112, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 69, 111, 48, 13, 0, 0, 0, 0, 163, 140, 230, 14, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 232, 251, 22, 28, 0, 0, 0, 0, 69, 111, 48, 13, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 189, 140, 56, 205, 22, 242, 253, 73, 29, 123, 20, 28, 60, 240, 92, 54, 194, 158, 161, 53, 1, 0, 0, 0, 0, 0, 0, 0, 163, 140, 230, 14, 0, 0, 0, 0, 51, 1, 0, 0, 0, 0, 0, 0, 112, 139, 230, 14, 0, 0, 0, 0, 88, 254, 156, 54, 144, 119, 105, 32, 220, 41, 203, 94, 39, 165, 27, 166, 102, 229, 43, 6, 0, 0, 0, 0, 0, 0, 0, 0, 198, 101, 64, 31, 199, 223, 196, 72, 90, 99, 213, 237, 219, 119, 224, 62, 64, 145, 77, 249, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 219, 77, 62, 9, 213, 253, 199, 127, 78, 185, 152, 39, 30, 133, 74, 199, 111, 138, 246, 211, 96, 152, 60, 97, 50, 62, 139, 100, 117, 184, 108, 155, 188, 240, 47, 162, 56, 85, 161, 73, 28, 43, 208, 48, 81, 139, 208, 26, 44, 245, 157, 146, 141, 60, 143, 93, 136, 43, 6, 98, 50, 207, 6, 22, 103, 222, 43, 180, 20, 21, 195, 101, 51, 91, 25, 252, 78, 34, 52, 60, 106, 129, 113, 172, 204, 102, 181, 184, 17, 177, 95, 69, 234, 176, 244, 238, 216, 118, 41, 142, 119, 52, 48, 90, 121, 131, 99, 80, 93, 91, 235, 214, 239, 62, 77, 109, 245, 32, 126, 23, 157, 94, 8, 21, 165, 187, 95, 251, 216, 68, 242, 65, 51, 113, 246, 71, 171, 101, 58, 170, 28, 236, 189, 98, 106, 22, 43, 23, 26, 60, 32, 188, 77, 185, 200, 110, 29, 216, 104, 133, 230, 204, 70, 121, 182, 255, 2, 24, 59, 120, 5, 239, 180, 78, 133, 182, 90, 69, 69, 122, 78, 187, 113, 48, 137, 5, 208, 56, 50, 102, 33, 40, 39, 3, 169, 46, 40, 252, 173, 11, 139, 31, 147, 53, 185, 114, 54, 190, 200, 34, 206, 91, 121, 18, 236, 201, 144, 21, 208, 35, 56, 240, 37, 6, 163, 212, 177, 69, 232, 184, 3, 191, 11, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 50, 0, 0, 0, 34, 53, 27, 101, 119, 114, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 101, 119, 114, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 101, 119, 114, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 228, 81, 54, 19, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 228, 81, 54, 19, 0, 0, 0, 0, 228, 81, 54, 19, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 219, 77, 62, 9, 213, 253, 199, 127, 78, 185, 152, 39, 30, 133, 74, 199, 111, 138, 246, 211, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 92, 208, 195, 140, 42, 159, 251, 241, 233, 218, 164, 50, 0, 255, 233, 131, 183, 7, 75, 106, 0, 0, 0, 0, 0, 0, 0, 0, 101, 25, 103, 90, 30, 111, 179, 209, 241, 48, 32, 251, 199, 252, 46, 64, 112, 244, 216, 79, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 148, 69, 218, 42, 230, 83, 97, 123, 55, 212, 87, 172, 76, 5, 198, 129, 202, 227, 117, 61, 96, 153, 13, 40, 191, 251, 130, 217, 141, 218, 203, 87, 112, 236, 102, 56, 133, 145, 111, 179, 24, 225, 255, 169, 4, 239, 173, 41, 138, 236, 35, 251, 181, 219, 51, 69, 25, 144, 238, 16, 149, 207, 169, 42, 23, 59, 147, 159, 33, 3, 113, 66, 182, 66, 236, 37, 91, 255, 34, 15, 146, 221, 2, 173, 159, 105, 158, 254, 227, 122, 178, 61, 77, 58, 67, 118, 83, 118, 133, 66, 13, 40, 75, 156, 107, 123, 19, 130, 6, 168, 99, 245, 58, 22, 27, 29, 46, 32, 190, 170, 51, 186, 163, 168, 165, 161, 0, 178, 107, 240, 153, 58, 31, 175, 64, 224, 203, 205, 24, 252, 29, 85, 133, 179, 86, 39, 15, 13, 68, 49, 32, 218, 86, 117, 148, 40, 220, 11, 185, 112, 145, 157, 49, 199, 86, 254, 235, 155, 136, 173, 16, 167, 75, 29, 190, 36, 15, 178, 105, 198, 134, 68, 222, 48, 151, 121, 249, 246, 167, 88, 176, 135, 74, 18, 64, 155, 147, 128, 124, 32, 71, 5, 63, 180, 243, 4, 6, 142, 171, 230, 39, 133, 148, 238, 165, 3, 176, 133, 41, 225, 34, 83, 0, 131, 78, 91, 240, 229, 131, 56, 204, 140, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 49, 49, 0, 0, 0, 34, 53, 27, 100, 102, 119, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 100, 102, 119, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 100, 102, 119, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 125, 52, 60, 25, 0, 0, 0, 0, 178, 169, 220, 29, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 47, 222, 24, 55, 0, 0, 0, 0, 125, 52, 60, 25, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 148, 69, 218, 42, 230, 83, 97, 123, 55, 212, 87, 172, 76, 5, 198, 129, 202, 227, 117, 61, 1, 0, 0, 0, 0, 0, 0, 0, 178, 169, 220, 29, 0, 0, 0, 0, 114, 2, 0, 0, 0, 0, 0, 0, 64, 167, 220, 29, 0, 0, 0, 0, 169, 138, 186, 51, 125, 182, 113, 74, 134, 112, 9, 53, 218, 210, 209, 86, 16, 51, 237, 47, 0, 0, 0, 0, 0, 0, 0, 0, 154, 117, 13, 217, 204, 174, 191, 211, 62, 165, 223, 29, 252, 19, 85, 129, 4, 253, 79, 56, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 185, 50, 221, 0, 235, 52, 62, 4, 219, 6, 157, 13, 86, 74, 250, 201, 145, 123, 216, 48, 96, 162, 128, 36, 136, 6, 47, 50, 228, 17, 171, 110, 23, 41, 82, 10, 197, 234, 103, 191, 186, 52, 163, 119, 19, 55, 210, 110, 79, 59, 27, 177, 85, 174, 32, 56, 1, 5, 154, 93, 124, 124, 205, 133, 116, 154, 181, 204, 24, 0, 111, 55, 247, 15, 137, 112, 189, 83, 97, 63, 223, 24, 78, 231, 159, 58, 11, 239, 135, 141, 163, 104, 22, 62, 216, 120, 211, 50, 25, 47, 207, 237, 110, 66, 23, 94, 122, 92, 74, 171, 160, 17, 17, 158, 243, 22, 165, 32, 25, 254, 167, 149, 211, 244, 23, 189, 120, 217, 20, 126, 109, 114, 110, 112, 176, 117, 133, 189, 193, 104, 37, 193, 36, 198, 226, 40, 198, 17, 64, 5, 32, 169, 222, 224, 200, 145, 137, 249, 168, 95, 170, 74, 140, 77, 40, 98, 220, 57, 1, 120, 81, 158, 127, 144, 136, 221, 156, 246, 229, 211, 134, 15, 114, 48, 144, 230, 122, 83, 218, 252, 189, 210, 172, 212, 80, 99, 222, 92, 85, 185, 193, 120, 113, 46, 93, 160, 101, 236, 249, 30, 197, 162, 210, 62, 15, 143, 215, 100, 192, 3, 186, 212, 43, 19, 79, 115, 227, 233, 65, 225, 228, 45, 11, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 55, 0, 0, 0, 34, 53, 27, 111, 114, 100, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 111, 114, 100, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 111, 114, 100, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 125, 52, 60, 25, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 125, 52, 60, 25, 0, 0, 0, 0, 125, 52, 60, 25, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 185, 50, 221, 0, 235, 52, 62, 4, 219, 6, 157, 13, 86, 74, 250, 201, 145, 123, 216, 48, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 110, 13, 232, 221, 187, 163, 180, 25, 192, 45, 98, 34, 200, 98, 125, 198, 233, 225, 197, 250, 0, 0, 0, 0, 0, 0, 0, 0, 77, 181, 107, 206, 195, 206, 101, 65, 255, 102, 115, 17, 250, 211, 115, 66, 225, 247, 190, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 232, 93, 217, 185, 16, 234, 46, 88, 236, 243, 198, 206, 222, 215, 248, 252, 87, 74, 41, 110, 96, 162, 141, 102, 167, 205, 63, 255, 132, 170, 219, 172, 97, 240, 243, 244, 6, 60, 168, 132, 155, 127, 218, 28, 54, 28, 55, 222, 23, 101, 187, 184, 146, 212, 115, 64, 197, 112, 90, 235, 113, 187, 160, 6, 188, 166, 16, 132, 229, 15, 9, 80, 34, 121, 60, 244, 18, 193, 105, 66, 230, 179, 18, 173, 205, 140, 131, 214, 210, 102, 136, 210, 228, 170, 49, 172, 172, 170, 136, 205, 133, 182, 109, 133, 18, 109, 228, 122, 85, 139, 146, 21, 223, 44, 166, 194, 161, 32, 189, 55, 124, 237, 217, 148, 67, 165, 136, 66, 236, 222, 45, 116, 136, 242, 142, 249, 15, 118, 240, 212, 14, 149, 8, 230, 88, 23, 96, 180, 124, 61, 32, 116, 32, 29, 119, 206, 142, 208, 208, 120, 60, 243, 252, 152, 29, 208, 244, 25, 44, 105, 89, 124, 221, 141, 30, 189, 122, 39, 140, 196, 253, 204, 174, 48, 185, 23, 163, 71, 241, 80, 152, 211, 197, 223, 210, 65, 137, 251, 126, 46, 45, 0, 216, 129, 187, 113, 220, 0, 244, 77, 14, 0, 173, 124, 251, 153, 63, 209, 167, 155, 28, 10, 72, 6, 63, 201, 180, 8, 87, 51, 62, 188, 11, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 52, 0, 0, 0, 34, 53, 27, 111, 114, 100, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 111, 114, 100, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 111, 114, 100, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 155, 140, 42, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 155, 140, 42, 7, 0, 0, 0, 0, 155, 140, 42, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 232, 93, 217, 185, 16, 234, 46, 88, 236, 243, 198, 206, 222, 215, 248, 252, 87, 74, 41, 110, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 32, 186, 175, 135, 145, 245, 194, 148, 229, 41, 194, 179, 205, 206, 95, 75, 37, 19, 50, 147, 0, 0, 0, 0, 0, 0, 0, 0, 11, 34, 168, 108, 33, 58, 11, 64, 240, 135, 179, 164, 34, 142, 0, 42, 206, 56, 253, 39, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 196, 193, 42, 87, 154, 171, 32, 255, 205, 111, 39, 26, 234, 17, 212, 128, 254, 88, 212, 67, 96, 164, 250, 219, 193, 117, 233, 34, 233, 160, 13, 222, 222, 134, 0, 83, 70, 46, 62, 23, 210, 26, 17, 130, 149, 237, 157, 156, 28, 5, 125, 31, 90, 101, 219, 102, 38, 42, 167, 50, 219, 78, 156, 49, 124, 118, 213, 95, 146, 3, 114, 77, 229, 113, 115, 244, 92, 227, 48, 86, 137, 46, 8, 102, 49, 104, 16, 101, 35, 243, 222, 173, 200, 38, 39, 16, 85, 74, 99, 11, 161, 37, 214, 134, 84, 4, 89, 154, 189, 211, 168, 166, 84, 121, 185, 203, 31, 32, 207, 249, 255, 248, 140, 61, 225, 69, 68, 157, 238, 35, 239, 92, 156, 15, 178, 208, 8, 237, 249, 42, 76, 237, 146, 122, 112, 93, 0, 49, 34, 3, 32, 57, 147, 102, 239, 9, 243, 222, 86, 28, 0, 172, 126, 118, 116, 162, 110, 109, 34, 229, 150, 196, 62, 106, 136, 175, 204, 221, 214, 228, 28, 12, 137, 48, 135, 101, 18, 35, 186, 177, 181, 82, 7, 66, 51, 5, 22, 224, 162, 160, 162, 58, 60, 34, 250, 161, 13, 79, 166, 241, 55, 139, 48, 244, 185, 239, 129, 105, 61, 233, 246, 1, 212, 222, 45, 34, 216, 177, 116, 109, 125, 180, 11, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 49, 0, 0, 0, 34, 53, 27, 101, 119, 114, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 101, 119, 114, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 101, 119, 114, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 69, 111, 48, 13, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 69, 111, 48, 13, 0, 0, 0, 0, 69, 111, 48, 13, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 196, 193, 42, 87, 154, 171, 32, 255, 205, 111, 39, 26, 234, 17, 212, 128, 254, 88, 212, 67, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 118, 157, 146, 115, 14, 147, 228, 221, 175, 69, 3, 175, 158, 228, 140, 38, 189, 52, 198, 214, 0, 0, 0, 0, 0, 0, 0, 0, 166, 207, 69, 11, 102, 227, 39, 222, 10, 41, 71, 221, 255, 36, 102, 164, 8, 241, 8, 157, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 222, 209, 104, 28, 254, 42, 165, 57, 151, 152, 143, 112, 252, 225, 64, 72, 66, 127, 79, 12, 96, 165, 37, 89, 186, 64, 228, 211, 63, 177, 149, 190, 212, 65, 188, 111, 134, 14, 8, 33, 220, 31, 2, 141, 149, 205, 191, 54, 134, 74, 227, 41, 236, 241, 247, 173, 211, 194, 110, 40, 201, 181, 134, 142, 4, 201, 234, 42, 150, 14, 42, 193, 252, 87, 65, 151, 115, 90, 43, 75, 248, 91, 9, 108, 119, 246, 181, 175, 212, 61, 40, 203, 78, 190, 242, 220, 204, 215, 191, 31, 54, 147, 150, 143, 27, 66, 108, 14, 130, 88, 71, 9, 55, 208, 152, 56, 105, 32, 64, 112, 213, 72, 54, 82, 193, 175, 43, 27, 195, 221, 0, 167, 32, 67, 12, 145, 133, 26, 33, 64, 205, 72, 130, 213, 55, 12, 192, 36, 253, 108, 32, 63, 8, 98, 185, 172, 90, 57, 103, 160, 11, 29, 48, 140, 132, 129, 161, 255, 101, 0, 40, 27, 35, 205, 149, 100, 110, 34, 96, 107, 176, 25, 122, 48, 164, 41, 128, 166, 211, 218, 45, 7, 76, 224, 127, 133, 139, 147, 2, 215, 166, 52, 174, 240, 41, 168, 227, 213, 133, 173, 134, 244, 40, 52, 11, 7, 244, 162, 61, 188, 28, 97, 34, 50, 15, 170, 160, 22, 120, 63, 134, 66, 11, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 56, 0, 0, 0, 34, 53, 27, 100, 102, 119, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 100, 102, 119, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 100, 102, 119, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 155, 140, 42, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 155, 140, 42, 7, 0, 0, 0, 0, 155, 140, 42, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 222, 209, 104, 28, 254, 42, 165, 57, 151, 152, 143, 112, 252, 225, 64, 72, 66, 127, 79, 12, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 39, 101, 23, 35, 50, 164, 225, 210, 211, 146, 126, 224, 223, 160, 227, 41, 70, 167, 59, 201, 0, 0, 0, 0, 0, 0, 0, 0, 110, 163, 144, 206, 163, 48, 99, 3, 249, 125, 126, 226, 4, 36, 8, 30, 76, 139, 189, 68, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 225, 125, 195, 57, 236, 113, 89, 50, 115, 174, 253, 198, 39, 212, 190, 56, 38, 91, 207, 60, 96, 166, 8, 39, 80, 42, 251, 28, 208, 234, 224, 210, 57, 237, 94, 236, 213, 222, 57, 206, 27, 188, 210, 55, 254, 59, 114, 254, 6, 54, 43, 235, 187, 92, 124, 72, 221, 127, 134, 21, 149, 196, 228, 215, 140, 178, 27, 54, 50, 3, 239, 154, 83, 82, 179, 237, 66, 201, 235, 125, 156, 138, 78, 147, 101, 161, 218, 128, 236, 182, 108, 50, 38, 128, 196, 210, 116, 249, 83, 189, 203, 167, 189, 248, 5, 142, 128, 77, 92, 120, 187, 92, 67, 185, 140, 210, 56, 32, 129, 13, 176, 202, 61, 129, 147, 219, 242, 139, 42, 182, 111, 162, 254, 231, 173, 151, 203, 172, 167, 199, 66, 36, 25, 21, 35, 142, 100, 9, 61, 39, 32, 196, 227, 84, 232, 4, 70, 42, 214, 202, 189, 232, 50, 102, 109, 224, 67, 72, 207, 68, 229, 239, 17, 145, 231, 43, 243, 201, 72, 7, 127, 100, 154, 48, 135, 151, 233, 161, 21, 118, 43, 192, 39, 151, 145, 235, 133, 91, 43, 239, 114, 35, 62, 137, 31, 79, 65, 21, 36, 156, 149, 216, 53, 5, 255, 183, 130, 159, 188, 135, 176, 87, 136, 114, 24, 221, 85, 151, 222, 189, 59, 41, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 50, 49, 0, 0, 0, 34, 53, 27, 108, 104, 114, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 108, 104, 114, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 108, 104, 114, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 69, 111, 48, 13, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 69, 111, 48, 13, 0, 0, 0, 0, 69, 111, 48, 13, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 225, 125, 195, 57, 236, 113, 89, 50, 115, 174, 253, 198, 39, 212, 190, 56, 38, 91, 207, 60, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 73, 2, 209, 41, 110, 253, 181, 202, 227, 150, 191, 144, 118, 201, 48, 119, 149, 203, 254, 252, 0, 0, 0, 0, 0, 0, 0, 0, 81, 58, 167, 63, 75, 141, 76, 22, 156, 83, 228, 161, 185, 130, 145, 192, 125, 31, 57, 123, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 18, 125, 166, 160, 158, 140, 211, 99, 86, 99, 87, 106, 147, 120, 183, 17, 214, 252, 4, 169, 96, 168, 67, 78, 156, 151, 11, 5, 53, 145, 59, 168, 50, 158, 77, 7, 99, 108, 228, 110, 185, 196, 4, 207, 100, 14, 233, 44, 47, 57, 42, 144, 105, 127, 174, 210, 201, 84, 222, 81, 52, 91, 184, 25, 7, 97, 38, 247, 234, 19, 32, 244, 37, 104, 116, 198, 29, 17, 129, 23, 200, 14, 101, 36, 206, 137, 115, 208, 81, 183, 126, 243, 85, 149, 167, 75, 238, 48, 195, 73, 132, 246, 177, 2, 113, 15, 111, 25, 168, 20, 110, 118, 17, 63, 204, 174, 152, 32, 183, 198, 55, 200, 194, 9, 235, 18, 229, 215, 142, 140, 27, 239, 103, 183, 200, 127, 212, 133, 203, 154, 139, 24, 202, 124, 79, 146, 18, 254, 47, 85, 32, 226, 47, 90, 26, 135, 198, 56, 156, 180, 166, 136, 51, 194, 2, 225, 188, 133, 23, 100, 96, 182, 108, 10, 216, 219, 71, 51, 67, 162, 150, 151, 35, 48, 174, 157, 139, 112, 62, 206, 39, 18, 61, 70, 56, 33, 139, 82, 117, 150, 155, 193, 164, 2, 243, 197, 4, 108, 231, 58, 158, 199, 155, 22, 89, 55, 225, 217, 165, 116, 167, 43, 211, 195, 98, 141, 86, 66, 214, 69, 165, 73, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 50, 57, 0, 0, 0, 34, 53, 27, 115, 121, 100, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 115, 121, 100, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 115, 121, 100, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 69, 111, 48, 13, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 69, 111, 48, 13, 0, 0, 0, 0, 69, 111, 48, 13, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 18, 125, 166, 160, 158, 140, 211, 99, 86, 99, 87, 106, 147, 120, 183, 17, 214, 252, 4, 169, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 251, 41, 77, 31, 141, 92, 55, 207, 81, 198, 142, 246, 113, 184, 218, 221, 59, 56, 83, 198, 0, 0, 0, 0, 0, 0, 0, 0, 148, 238, 81, 178, 87, 56, 232, 92, 150, 202, 88, 62, 252, 165, 113, 234, 95, 121, 204, 205, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 166, 25, 129, 90, 73, 212, 177, 91, 211, 173, 56, 203, 82, 156, 182, 194, 251, 243, 247, 172, 96, 170, 193, 187, 5, 137, 168, 8, 63, 175, 109, 188, 59, 223, 168, 177, 224, 172, 17, 178, 118, 162, 74, 194, 132, 53, 29, 12, 7, 22, 79, 148, 111, 12, 21, 14, 242, 235, 2, 139, 77, 63, 3, 76, 73, 223, 102, 139, 18, 7, 84, 29, 196, 228, 254, 189, 181, 103, 204, 87, 7, 175, 177, 211, 113, 63, 197, 90, 88, 233, 179, 19, 24, 213, 231, 10, 23, 105, 172, 189, 197, 46, 23, 237, 59, 164, 235, 66, 207, 66, 138, 67, 222, 69, 79, 12, 102, 32, 69, 143, 111, 44, 247, 100, 214, 67, 17, 130, 31, 60, 37, 154, 40, 224, 97, 212, 20, 148, 145, 18, 75, 244, 14, 138, 208, 30, 156, 90, 138, 123, 32, 108, 3, 211, 248, 156, 119, 99, 92, 84, 124, 184, 229, 11, 200, 232, 160, 21, 167, 61, 49, 182, 172, 29, 156, 224, 122, 74, 37, 87, 99, 178, 79, 48, 174, 137, 201, 181, 133, 227, 159, 190, 148, 11, 148, 32, 214, 248, 14, 13, 207, 212, 98, 8, 228, 249, 136, 20, 128, 90, 226, 168, 210, 133, 206, 153, 206, 102, 235, 199, 241, 203, 157, 137, 145, 186, 116, 41, 124, 122, 27, 252, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 49, 48, 0, 0, 0, 34, 53, 27, 100, 102, 119, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 100, 102, 119, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 100, 102, 119, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 228, 81, 54, 19, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 228, 81, 54, 19, 0, 0, 0, 0, 228, 81, 54, 19, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 166, 25, 129, 90, 73, 212, 177, 91, 211, 173, 56, 203, 82, 156, 182, 194, 251, 243, 247, 172, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 176, 179, 21, 101, 12, 136, 59, 75, 44, 135, 244, 67, 86, 15, 197, 140, 28, 72, 149, 139, 0, 0, 0, 0, 0, 0, 0, 0, 62, 190, 163, 195, 8, 156, 208, 113, 77, 245, 159, 7, 227, 244, 31, 143, 253, 217, 10, 118, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 121, 255, 244, 9, 60, 44, 241, 72, 8, 84, 138, 153, 90, 206, 137, 238, 105, 28, 205, 194, 96, 172, 145, 177, 5, 85, 74, 116, 2, 214, 17, 30, 154, 225, 253, 206, 238, 63, 30, 51, 183, 250, 180, 203, 129, 62, 72, 149, 63, 8, 223, 119, 13, 251, 116, 254, 34, 39, 201, 2, 30, 202, 155, 198, 203, 188, 208, 166, 167, 19, 71, 145, 250, 179, 128, 106, 123, 250, 162, 209, 145, 7, 83, 4, 36, 33, 124, 221, 199, 219, 26, 244, 115, 247, 18, 130, 79, 97, 89, 106, 76, 131, 216, 125, 240, 89, 50, 120, 165, 164, 85, 73, 121, 84, 197, 45, 30, 32, 234, 160, 60, 222, 250, 216, 53, 77, 52, 108, 233, 121, 196, 78, 37, 176, 159, 69, 8, 8, 127, 45, 16, 122, 108, 69, 36, 50, 87, 202, 56, 236, 32, 147, 226, 110, 143, 28, 68, 61, 75, 104, 222, 4, 157, 44, 31, 170, 231, 250, 136, 100, 103, 109, 148, 199, 241, 245, 84, 106, 202, 130, 224, 73, 200, 48, 139, 136, 96, 241, 0, 254, 56, 3, 6, 123, 176, 228, 74, 50, 22, 33, 162, 5, 192, 117, 134, 137, 96, 106, 115, 69, 122, 180, 188, 218, 53, 202, 140, 38, 214, 58, 98, 39, 77, 251, 43, 85, 84, 8, 158, 127, 149, 174, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 49, 57, 0, 0, 0, 34, 53, 27, 97, 109, 115, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 97, 109, 115, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 97, 109, 115, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 125, 52, 60, 25, 0, 0, 0, 0, 255, 219, 210, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 156, 177, 22, 27, 0, 0, 0, 0, 125, 52, 60, 25, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 121, 255, 244, 9, 60, 44, 241, 72, 8, 84, 138, 153, 90, 206, 137, 238, 105, 28, 205, 194, 1, 0, 0, 0, 0, 0, 0, 0, 31, 125, 218, 1, 0, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0, 23, 125, 218, 1, 0, 0, 0, 0, 229, 82, 76, 122, 182, 56, 159, 148, 91, 19, 141, 37, 226, 243, 53, 186, 240, 193, 33, 123, 0, 0, 0, 0, 0, 0, 0, 0, 201, 60, 91, 137, 235, 104, 221, 254, 28, 48, 237, 80, 65, 119, 57, 66, 130, 0, 253, 147, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 92, 147, 140, 156, 151, 155, 223, 46, 222, 250, 240, 247, 68, 221, 88, 193, 183, 85, 156, 1, 96, 173, 38, 226, 40, 95, 222, 188, 171, 182, 157, 81, 171, 216, 243, 42, 150, 20, 228, 235, 70, 131, 88, 235, 43, 193, 87, 178, 212, 238, 36, 30, 186, 165, 239, 29, 193, 36, 1, 136, 58, 41, 34, 164, 32, 173, 17, 118, 136, 16, 247, 128, 51, 125, 36, 195, 77, 43, 170, 163, 245, 107, 170, 86, 61, 144, 174, 158, 228, 208, 27, 102, 25, 136, 174, 156, 209, 246, 130, 69, 100, 62, 21, 129, 143, 52, 190, 243, 163, 121, 203, 78, 229, 122, 227, 25, 129, 32, 200, 120, 108, 172, 141, 86, 186, 220, 154, 255, 196, 54, 54, 106, 5, 189, 189, 228, 18, 17, 47, 42, 148, 83, 129, 146, 157, 234, 79, 25, 159, 122, 32, 55, 247, 179, 162, 123, 248, 27, 71, 222, 28, 199, 64, 144, 115, 22, 100, 7, 218, 148, 15, 58, 106, 61, 37, 228, 187, 92, 215, 109, 101, 100, 8, 48, 176, 204, 196, 86, 23, 57, 59, 37, 194, 35, 221, 197, 180, 234, 157, 4, 141, 177, 104, 22, 83, 150, 216, 3, 63, 150, 65, 187, 194, 100, 226, 78, 194, 44, 82, 155, 87, 124, 221, 13, 151, 172, 171, 50, 15, 44, 174, 209, 11, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 54, 0, 0, 0, 34, 53, 27, 111, 114, 100, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 111, 114, 100, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 111, 114, 100, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 228, 81, 54, 19, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 228, 81, 54, 19, 0, 0, 0, 0, 228, 81, 54, 19, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 92, 147, 140, 156, 151, 155, 223, 46, 222, 250, 240, 247, 68, 221, 88, 193, 183, 85, 156, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 235, 138, 66, 137, 247, 75, 106, 141, 64, 50, 28, 4, 38, 77, 232, 109, 46, 131, 116, 84, 0, 0, 0, 0, 0, 0, 0, 0, 173, 186, 178, 198, 1, 208, 32, 161, 107, 169, 137, 208, 5, 165, 208, 117, 247, 251, 112, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 148, 210, 132, 36, 100, 12, 29, 216, 194, 84, 83, 19, 140, 172, 203, 159, 94, 111, 85, 223, 96, 173, 253, 71, 31, 195, 210, 180, 12, 214, 231, 31, 31, 224, 81, 237, 178, 236, 151, 207, 252, 231, 251, 180, 146, 136, 52, 237, 63, 139, 206, 174, 63, 82, 252, 101, 211, 231, 150, 3, 165, 220, 4, 38, 222, 160, 4, 104, 120, 12, 176, 204, 153, 106, 10, 112, 197, 194, 86, 206, 146, 110, 252, 116, 105, 78, 81, 127, 17, 215, 175, 85, 234, 138, 250, 229, 30, 79, 166, 176, 199, 123, 230, 78, 19, 119, 102, 138, 9, 83, 30, 205, 208, 122, 255, 17, 54, 32, 188, 173, 177, 44, 73, 152, 155, 70, 56, 116, 72, 136, 20, 45, 159, 129, 194, 94, 55, 179, 141, 32, 178, 180, 35, 6, 162, 218, 226, 210, 30, 119, 32, 160, 4, 176, 218, 102, 190, 212, 198, 70, 171, 22, 103, 191, 74, 126, 140, 206, 214, 228, 234, 152, 46, 3, 71, 190, 83, 114, 125, 164, 125, 154, 4, 48, 163, 17, 154, 117, 47, 44, 14, 102, 17, 103, 74, 133, 56, 17, 200, 21, 133, 182, 127, 25, 17, 114, 245, 170, 130, 245, 14, 23, 224, 19, 168, 4, 139, 180, 17, 228, 127, 33, 206, 28, 211, 79, 132, 203, 75, 52, 111, 13, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 49, 54, 0, 0, 0, 34, 53, 27, 97, 109, 115, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 97, 109, 115, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 97, 109, 115, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 155, 140, 42, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 155, 140, 42, 7, 0, 0, 0, 0, 155, 140, 42, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 148, 210, 132, 36, 100, 12, 29, 216, 194, 84, 83, 19, 140, 172, 203, 159, 94, 111, 85, 223, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 75, 254, 16, 116, 101, 34, 169, 5, 32, 112, 85, 54, 139, 185, 136, 207, 65, 201, 163, 164, 0, 0, 0, 0, 0, 0, 0, 0, 247, 248, 13, 26, 28, 153, 14, 164, 84, 149, 137, 150, 239, 98, 120, 227, 43, 39, 224, 206, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 236, 25, 167, 237, 3, 135, 42, 163, 145, 113, 133, 67, 94, 185, 131, 135, 169, 18, 197, 122, 96, 174, 53, 71, 222, 156, 219, 27, 165, 82, 235, 94, 112, 219, 239, 143, 220, 163, 178, 172, 188, 217, 137, 172, 160, 198, 78, 72, 200, 160, 59, 34, 219, 252, 73, 11, 109, 130, 156, 198, 149, 10, 207, 77, 119, 65, 37, 89, 25, 1, 112, 10, 93, 7, 240, 175, 113, 250, 255, 236, 192, 191, 255, 152, 12, 21, 160, 214, 53, 225, 11, 185, 129, 74, 102, 184, 144, 15, 132, 42, 160, 194, 243, 181, 73, 14, 79, 26, 115, 4, 39, 225, 196, 151, 30, 255, 158, 32, 26, 146, 61, 70, 150, 164, 152, 131, 33, 155, 92, 222, 92, 98, 159, 115, 170, 218, 169, 202, 101, 217, 69, 16, 18, 161, 16, 152, 135, 37, 246, 228, 32, 153, 216, 177, 32, 95, 223, 53, 174, 83, 68, 150, 205, 72, 85, 30, 147, 238, 204, 9, 115, 99, 132, 95, 43, 209, 2, 41, 132, 168, 176, 102, 20, 48, 177, 190, 193, 154, 206, 30, 239, 64, 202, 147, 43, 8, 133, 240, 64, 112, 144, 48, 64, 231, 0, 45, 126, 127, 137, 28, 178, 236, 203, 163, 233, 249, 154, 79, 219, 199, 10, 251, 33, 122, 190, 247, 112, 163, 232, 150, 82, 101, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 50, 56, 0, 0, 0, 34, 53, 27, 115, 121, 100, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 115, 121, 100, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 115, 121, 100, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 155, 140, 42, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 155, 140, 42, 7, 0, 0, 0, 0, 155, 140, 42, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 236, 25, 167, 237, 3, 135, 42, 163, 145, 113, 133, 67, 94, 185, 131, 135, 169, 18, 197, 122, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 36, 241, 48, 133, 231, 30, 98, 125, 95, 111, 151, 67, 140, 29, 160, 66, 112, 208, 153, 155, 0, 0, 0, 0, 0, 0, 0, 0, 235, 79, 58, 196, 91, 187, 250, 87, 131, 15, 91, 225, 122, 241, 10, 57, 128, 43, 149, 108, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 9, 14, 235, 145, 239, 52, 108, 30, 244, 154, 225, 124, 149, 251, 85, 28, 27, 42, 120, 206, 96, 176, 209, 43, 129, 8, 150, 120, 128, 198, 119, 67, 189, 80, 122, 85, 87, 65, 159, 17, 197, 70, 109, 2, 24, 221, 32, 22, 49, 75, 118, 144, 238, 224, 230, 229, 43, 4, 238, 222, 40, 130, 64, 198, 81, 154, 131, 191, 210, 25, 73, 151, 142, 29, 225, 23, 82, 163, 92, 96, 76, 2, 194, 18, 178, 233, 180, 132, 25, 215, 87, 87, 173, 112, 65, 148, 76, 26, 190, 32, 87, 56, 61, 169, 71, 53, 191, 148, 129, 254, 155, 172, 98, 236, 2, 234, 170, 32, 98, 33, 196, 45, 64, 87, 253, 59, 183, 231, 215, 179, 33, 229, 251, 196, 237, 169, 228, 143, 58, 96, 88, 16, 75, 145, 58, 122, 41, 194, 142, 247, 32, 41, 19, 96, 18, 7, 206, 168, 153, 88, 123, 5, 153, 244, 88, 252, 41, 15, 89, 120, 232, 94, 227, 106, 228, 35, 237, 159, 130, 139, 95, 244, 251, 48, 152, 125, 13, 151, 172, 223, 218, 154, 147, 81, 218, 251, 120, 162, 103, 122, 242, 248, 248, 164, 195, 101, 180, 32, 39, 8, 125, 232, 12, 17, 131, 185, 17, 229, 4, 164, 120, 246, 187, 66, 91, 46, 252, 241, 100, 6, 77, 228, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 51, 54, 0, 0, 0, 34, 53, 27, 109, 105, 97, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 109, 105, 97, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 109, 105, 97, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 155, 140, 42, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 155, 140, 42, 7, 0, 0, 0, 0, 155, 140, 42, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 9, 14, 235, 145, 239, 52, 108, 30, 244, 154, 225, 124, 149, 251, 85, 28, 27, 42, 120, 206, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 152, 34, 153, 76, 194, 170, 210, 19, 201, 174, 118, 32, 4, 189, 39, 238, 160, 125, 72, 17, 0, 0, 0, 0, 0, 0, 0, 0, 149, 220, 38, 215, 112, 62, 203, 57, 209, 138, 247, 236, 50, 61, 231, 137, 223, 138, 210, 144, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 106, 16, 46, 28, 238, 176, 217, 31, 126, 248, 196, 191, 47, 132, 41, 230, 27, 85, 211, 80, 96, 176, 233, 106, 215, 59, 40, 74, 147, 120, 245, 56, 32, 137, 53, 77, 101, 193, 230, 75, 71, 25, 16, 49, 65, 27, 248, 172, 176, 244, 172, 123, 87, 218, 44, 218, 243, 63, 81, 163, 214, 69, 196, 173, 243, 241, 100, 210, 112, 2, 178, 184, 113, 176, 209, 214, 175, 135, 164, 0, 76, 133, 42, 135, 87, 193, 249, 50, 229, 136, 12, 210, 61, 170, 83, 192, 190, 97, 184, 123, 149, 111, 227, 2, 140, 99, 89, 163, 128, 12, 148, 187, 10, 187, 8, 223, 137, 32, 102, 226, 164, 177, 18, 93, 16, 47, 12, 44, 165, 13, 20, 230, 44, 177, 12, 60, 184, 139, 8, 27, 63, 180, 115, 91, 137, 207, 208, 146, 239, 92, 32, 208, 98, 45, 230, 34, 179, 203, 48, 199, 151, 46, 158, 38, 5, 19, 152, 45, 48, 51, 135, 138, 187, 103, 33, 202, 52, 95, 199, 184, 156, 30, 136, 48, 128, 42, 126, 180, 124, 53, 231, 97, 114, 18, 46, 24, 197, 200, 121, 172, 191, 112, 3, 155, 206, 21, 46, 210, 131, 129, 83, 99, 75, 163, 206, 87, 125, 200, 87, 151, 180, 125, 95, 74, 119, 238, 110, 85, 131, 64, 232, 70, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 51, 55, 0, 0, 0, 34, 53, 27, 109, 105, 97, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 109, 105, 97, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 109, 105, 97, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 69, 111, 48, 13, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 69, 111, 48, 13, 0, 0, 0, 0, 69, 111, 48, 13, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 106, 16, 46, 28, 238, 176, 217, 31, 126, 248, 196, 191, 47, 132, 41, 230, 27, 85, 211, 80, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 65, 227, 46, 196, 111, 165, 4, 108, 5, 49, 229, 128, 123, 255, 11, 28, 197, 95, 75, 18, 0, 0, 0, 0, 0, 0, 0, 0, 245, 211, 165, 232, 136, 138, 179, 191, 207, 107, 23, 191, 245, 55, 47, 253, 0, 176, 139, 243, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 69, 47, 198, 18, 250, 222, 181, 244, 98, 150, 38, 196, 5, 250, 32, 156, 152, 185, 89, 170, 96, 180, 165, 195, 89, 30, 166, 40, 243, 30, 252, 241, 251, 212, 41, 15, 245, 216, 12, 219, 15, 250, 248, 1, 242, 138, 104, 177, 218, 107, 5, 79, 243, 235, 129, 242, 38, 250, 94, 217, 154, 69, 76, 187, 116, 106, 220, 233, 145, 18, 167, 221, 55, 249, 46, 125, 116, 235, 166, 9, 70, 157, 227, 78, 123, 26, 91, 117, 92, 121, 124, 91, 71, 36, 153, 163, 24, 214, 76, 211, 47, 4, 207, 232, 162, 178, 59, 251, 93, 197, 46, 190, 238, 44, 180, 195, 214, 32, 69, 234, 130, 9, 102, 171, 112, 161, 231, 34, 125, 188, 0, 23, 100, 78, 42, 205, 229, 108, 98, 156, 143, 255, 87, 3, 152, 245, 178, 111, 147, 8, 32, 137, 255, 59, 0, 54, 144, 18, 144, 160, 32, 220, 11, 166, 168, 191, 163, 212, 228, 96, 198, 76, 251, 86, 59, 185, 152, 138, 3, 201, 206, 167, 151, 48, 128, 5, 28, 146, 242, 243, 149, 246, 250, 255, 89, 113, 162, 146, 3, 67, 85, 56, 59, 148, 5, 200, 199, 140, 33, 120, 225, 108, 65, 167, 144, 93, 137, 129, 76, 22, 113, 89, 60, 45, 37, 119, 9, 196, 55, 139, 71, 89, 11, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 51, 0, 0, 0, 34, 53, 27, 101, 119, 114, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 101, 119, 114, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 101, 119, 114, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 125, 52, 60, 25, 0, 0, 0, 0, 128, 150, 152, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 253, 202, 212, 25, 0, 0, 0, 0, 125, 52, 60, 25, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 69, 47, 198, 18, 250, 222, 181, 244, 98, 150, 38, 196, 5, 250, 32, 156, 152, 185, 89, 170, 1, 0, 0, 0, 0, 0, 0, 0, 128, 150, 152, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 128, 150, 152, 0, 0, 0, 0, 0, 67, 145, 206, 30, 13, 139, 49, 79, 216, 140, 202, 53, 186, 176, 244, 146, 156, 87, 128, 203, 0, 0, 0, 0, 0, 0, 0, 0, 71, 122, 105, 170, 97, 118, 207, 10, 155, 252, 135, 50, 34, 72, 222, 83, 151, 78, 123, 167, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 205, 222, 202, 236, 40, 7, 218, 105, 251, 215, 185, 141, 215, 188, 23, 17, 245, 21, 204, 104, 96, 180, 188, 157, 41, 104, 184, 147, 81, 191, 184, 53, 56, 68, 70, 214, 20, 168, 107, 224, 57, 54, 105, 102, 73, 146, 49, 80, 253, 195, 172, 44, 159, 125, 218, 6, 204, 84, 172, 75, 203, 22, 171, 132, 93, 129, 86, 66, 73, 4, 85, 245, 3, 16, 10, 104, 100, 209, 136, 179, 132, 154, 87, 175, 144, 26, 72, 239, 221, 100, 240, 99, 85, 12, 125, 6, 137, 35, 46, 52, 44, 94, 50, 156, 182, 90, 195, 125, 21, 248, 199, 146, 34, 240, 251, 159, 97, 32, 61, 218, 23, 160, 253, 63, 29, 186, 77, 188, 113, 209, 29, 223, 156, 237, 207, 147, 33, 47, 47, 16, 59, 20, 231, 45, 203, 109, 50, 148, 218, 174, 32, 155, 118, 157, 157, 135, 89, 200, 7, 142, 99, 187, 247, 144, 120, 16, 109, 217, 185, 53, 164, 29, 203, 10, 215, 71, 218, 202, 202, 100, 94, 9, 244, 48, 165, 31, 44, 89, 202, 119, 45, 68, 7, 144, 95, 116, 250, 186, 74, 142, 243, 29, 58, 111, 147, 6, 50, 237, 145, 148, 161, 113, 134, 43, 152, 247, 35, 219, 234, 151, 118, 246, 234, 91, 199, 245, 132, 177, 152, 255, 175, 112, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 49, 50, 0, 0, 0, 34, 53, 27, 115, 103, 112, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 115, 103, 112, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 115, 103, 112, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 155, 140, 42, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 155, 140, 42, 7, 0, 0, 0, 0, 155, 140, 42, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 205, 222, 202, 236, 40, 7, 218, 105, 251, 215, 185, 141, 215, 188, 23, 17, 245, 21, 204, 104, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 75, 43, 248, 235, 90, 187, 135, 56, 30, 153, 190, 191, 210, 193, 116, 245, 27, 205, 108, 36, 0, 0, 0, 0, 0, 0, 0, 0, 67, 126, 195, 165, 101, 18, 133, 233, 165, 82, 88, 72, 72, 221, 239, 142, 41, 61, 164, 229, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 193, 111, 182, 252, 65, 152, 143, 196, 68, 57, 200, 158, 221, 169, 248, 96, 34, 44, 56, 194, 96, 182, 91, 25, 107, 140, 166, 125, 151, 219, 123, 78, 203, 108, 245, 59, 143, 238, 114, 181, 195, 17, 19, 96, 251, 209, 108, 9, 171, 136, 232, 94, 2, 111, 166, 179, 152, 248, 203, 11, 156, 26, 167, 210, 46, 172, 86, 142, 40, 25, 28, 189, 240, 101, 104, 194, 243, 123, 105, 123, 204, 206, 175, 101, 55, 126, 202, 154, 152, 170, 60, 31, 241, 27, 48, 136, 55, 120, 225, 193, 45, 165, 81, 101, 173, 88, 117, 67, 45, 64, 90, 159, 202, 83, 49, 94, 130, 32, 153, 126, 241, 90, 79, 81, 91, 155, 145, 11, 72, 187, 136, 70, 111, 174, 231, 78, 151, 38, 7, 234, 204, 90, 25, 36, 241, 133, 215, 24, 131, 10, 32, 87, 112, 185, 138, 23, 238, 40, 179, 97, 238, 132, 2, 228, 88, 72, 52, 32, 30, 173, 45, 64, 3, 188, 4, 209, 26, 65, 46, 223, 93, 127, 13, 48, 171, 87, 212, 130, 179, 72, 176, 143, 193, 110, 123, 20, 5, 77, 210, 80, 150, 14, 187, 201, 73, 228, 192, 232, 44, 71, 119, 155, 194, 126, 139, 242, 132, 136, 159, 89, 210, 186, 13, 215, 201, 90, 170, 150, 140, 110, 22, 141, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 51, 56, 0, 0, 0, 34, 53, 27, 109, 105, 97, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 109, 105, 97, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 109, 105, 97, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 228, 81, 54, 19, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 228, 81, 54, 19, 0, 0, 0, 0, 228, 81, 54, 19, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 193, 111, 182, 252, 65, 152, 143, 196, 68, 57, 200, 158, 221, 169, 248, 96, 34, 44, 56, 194, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 50, 183, 219, 97, 7, 248, 12, 156, 55, 14, 110, 252, 16, 171, 226, 229, 151, 13, 33, 213, 0, 0, 0, 0, 0, 0, 0, 0, 68, 91, 145, 68, 127, 31, 118, 248, 190, 2, 137, 22, 117, 14, 60, 238, 8, 109, 179, 29, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 179, 224, 79, 15, 26, 109, 128, 116, 176, 107, 219, 20, 75, 72, 68, 5, 215, 133, 138, 51, 96, 184, 204, 193, 10, 54, 145, 71, 128, 72, 139, 24, 129, 211, 149, 97, 84, 91, 91, 109, 139, 123, 139, 197, 163, 104, 96, 42, 242, 86, 84, 82, 7, 0, 231, 227, 141, 121, 75, 15, 55, 4, 251, 121, 58, 126, 139, 112, 122, 3, 76, 24, 175, 109, 44, 220, 61, 232, 126, 34, 253, 163, 167, 105, 2, 212, 10, 76, 35, 103, 185, 165, 73, 182, 53, 231, 116, 15, 39, 104, 68, 10, 66, 211, 174, 100, 200, 19, 43, 106, 164, 39, 113, 49, 250, 134, 205, 32, 117, 117, 211, 218, 145, 155, 218, 237, 237, 159, 143, 45, 42, 163, 230, 221, 195, 179, 176, 212, 11, 133, 181, 36, 227, 159, 103, 89, 212, 118, 50, 1, 32, 247, 36, 219, 99, 11, 172, 236, 58, 226, 234, 45, 225, 49, 190, 204, 155, 206, 153, 85, 144, 61, 198, 71, 49, 198, 68, 73, 14, 39, 127, 232, 238, 48, 138, 131, 234, 127, 219, 20, 189, 6, 169, 11, 199, 111, 206, 25, 144, 67, 210, 44, 202, 181, 0, 152, 244, 153, 179, 206, 5, 96, 104, 197, 209, 232, 247, 113, 221, 35, 213, 20, 129, 217, 94, 62, 249, 79, 174, 105, 218, 189, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 50, 53, 0, 0, 0, 34, 53, 27, 115, 106, 99, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 115, 106, 99, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 115, 106, 99, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 69, 111, 48, 13, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 69, 111, 48, 13, 0, 0, 0, 0, 69, 111, 48, 13, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 179, 224, 79, 15, 26, 109, 128, 116, 176, 107, 219, 20, 75, 72, 68, 5, 215, 133, 138, 51, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 188, 202, 200, 109, 137, 213, 158, 201, 190, 20, 215, 198, 3, 17, 30, 50, 180, 57, 11, 93, 0, 0, 0, 0, 0, 0, 0, 0, 167, 115, 67, 73, 187, 179, 170, 149, 124, 163, 95, 36, 179, 7, 246, 106, 68, 207, 242, 234, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 40, 179, 224, 79, 15, 26, 109, 128, 116, 176, 107, 219, 20, 75, 72, 68, 5, 215, 133, 138, 51, 96, 184, 204, 193, 10, 54, 145, 71, 128, 72, 139, 24, 129, 211, 149, 97, 84, 91, 91, 109, 139, 123, 139, 197, 163, 104, 96, 42, 242, 86, 84, 82, 7, 0, 231, 227, 141, 121, 75, 15, 55, 4, 251, 121, 58, 126, 139, 112, 122, 3, 76, 24, 175, 109, 44, 220, 61, 232, 126, 34, 253, 163, 167, 105, 2, 212, 10, 76, 35, 103, 185, 165, 73, 182, 53, 231, 116, 15, 39, 104, 68, 10, 66, 211, 174, 100, 200, 19, 43, 106, 164, 39, 113, 49, 250, 134, 205, 32, 117, 117, 211, 218, 145, 155, 218, 237, 237, 159, 143, 45, 42, 163, 230, 221, 195, 179, 176, 212, 11, 133, 181, 36, 227, 159, 103, 89, 212, 118, 50, 1, 32, 247, 36, 219, 99, 11, 172, 236, 58, 226, 234, 45, 225, 49, 190, 204, 155, 206, 153, 85, 144, 61, 198, 71, 49, 198, 68, 73, 14, 39, 127, 232, 238, 48, 138, 131, 234, 127, 219, 20, 189, 6, 169, 11, 199, 111, 206, 25, 144, 67, 210, 44, 202, 181, 0, 152, 244, 153, 179, 206, 5, 96, 104, 197, 209, 232, 247, 113, 221, 35, 213, 20, 129, 217, 94, 62, 249, 79, 174, 105, 218, 189, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 50, 53, 0, 0, 0, 34, 53, 27, 115, 106, 99, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 115, 106, 99, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 115, 106, 99, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 69, 111, 48, 13, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 193, 111, 182, 252, 65, 152, 143, 196, 68, 57, 200, 158, 221, 169, 248, 96, 34, 44, 56, 194, 96, 182, 91, 25, 107, 140, 166, 125, 151, 219, 123, 78, 203, 108, 245, 59, 143, 238, 114, 181, 195, 17, 19, 96, 251, 209, 108, 9, 171, 136, 232, 94, 2, 111, 166, 179, 152, 248, 203, 11, 156, 26, 167, 210, 46, 172, 86, 142, 40, 25, 28, 189, 240, 101, 104, 194, 243, 123, 105, 123, 204, 206, 175, 101, 55, 126, 202, 154, 152, 170, 60, 31, 241, 27, 48, 136, 55, 120, 225, 193, 45, 165, 81, 101, 173, 88, 117, 67, 45, 64, 90, 159, 202, 83, 49, 94, 130, 32, 153, 126, 241, 90, 79, 81, 91, 155, 145, 11, 72, 187, 136, 70, 111, 174, 231, 78, 151, 38, 7, 234, 204, 90, 25, 36, 241, 133, 215, 24, 131, 10, 32, 87, 112, 185, 138, 23, 238, 40, 179, 97, 238, 132, 2, 228, 88, 72, 52, 32, 30, 173, 45, 64, 3, 188, 4, 209, 26, 65, 46, 223, 93, 127, 13, 48, 171, 87, 212, 130, 179, 72, 176, 143, 193, 110, 123, 20, 5, 77, 210, 80, 150, 14, 187, 201, 73, 228, 192, 232, 44, 71, 119, 155, 194, 126, 139, 242, 132, 136, 159, 89, 210, 186, 13, 215, 201, 90, 170, 150, 140, 110, 22, 141, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 51, 56, 0, 0, 0, 34, 53, 27, 109, 105, 97, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 109, 105, 97, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 109, 105, 97, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 228, 81, 54, 19, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 205, 222, 202, 236, 40, 7, 218, 105, 251, 215, 185, 141, 215, 188, 23, 17, 245, 21, 204, 104, 96, 180, 188, 157, 41, 104, 184, 147, 81, 191, 184, 53, 56, 68, 70, 214, 20, 168, 107, 224, 57, 54, 105, 102, 73, 146, 49, 80, 253, 195, 172, 44, 159, 125, 218, 6, 204, 84, 172, 75, 203, 22, 171, 132, 93, 129, 86, 66, 73, 4, 85, 245, 3, 16, 10, 104, 100, 209, 136, 179, 132, 154, 87, 175, 144, 26, 72, 239, 221, 100, 240, 99, 85, 12, 125, 6, 137, 35, 46, 52, 44, 94, 50, 156, 182, 90, 195, 125, 21, 248, 199, 146, 34, 240, 251, 159, 97, 32, 61, 218, 23, 160, 253, 63, 29, 186, 77, 188, 113, 209, 29, 223, 156, 237, 207, 147, 33, 47, 47, 16, 59, 20, 231, 45, 203, 109, 50, 148, 218, 174, 32, 155, 118, 157, 157, 135, 89, 200, 7, 142, 99, 187, 247, 144, 120, 16, 109, 217, 185, 53, 164, 29, 203, 10, 215, 71, 218, 202, 202, 100, 94, 9, 244, 48, 165, 31, 44, 89, 202, 119, 45, 68, 7, 144, 95, 116, 250, 186, 74, 142, 243, 29, 58, 111, 147, 6, 50, 237, 145, 148, 161, 113, 134, 43, 152, 247, 35, 219, 234, 151, 118, 246, 234, 91, 199, 245, 132, 177, 152, 255, 175, 112, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 49, 50, 0, 0, 0, 34, 53, 27, 115, 103, 112, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 115, 103, 112, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 115, 103, 112, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 155, 140, 42, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 69, 47, 198, 18, 250, 222, 181, 244, 98, 150, 38, 196, 5, 250, 32, 156, 152, 185, 89, 170, 96, 180, 165, 195, 89, 30, 166, 40, 243, 30, 252, 241, 251, 212, 41, 15, 245, 216, 12, 219, 15, 250, 248, 1, 242, 138, 104, 177, 218, 107, 5, 79, 243, 235, 129, 242, 38, 250, 94, 217, 154, 69, 76, 187, 116, 106, 220, 233, 145, 18, 167, 221, 55, 249, 46, 125, 116, 235, 166, 9, 70, 157, 227, 78, 123, 26, 91, 117, 92, 121, 124, 91, 71, 36, 153, 163, 24, 214, 76, 211, 47, 4, 207, 232, 162, 178, 59, 251, 93, 197, 46, 190, 238, 44, 180, 195, 214, 32, 69, 234, 130, 9, 102, 171, 112, 161, 231, 34, 125, 188, 0, 23, 100, 78, 42, 205, 229, 108, 98, 156, 143, 255, 87, 3, 152, 245, 178, 111, 147, 8, 32, 137, 255, 59, 0, 54, 144, 18, 144, 160, 32, 220, 11, 166, 168, 191, 163, 212, 228, 96, 198, 76, 251, 86, 59, 185, 152, 138, 3, 201, 206, 167, 151, 48, 128, 5, 28, 146, 242, 243, 149, 246, 250, 255, 89, 113, 162, 146, 3, 67, 85, 56, 59, 148, 5, 200, 199, 140, 33, 120, 225, 108, 65, 167, 144, 93, 137, 129, 76, 22, 113, 89, 60, 45, 37, 119, 9, 196, 55, 139, 71, 89, 11, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 51, 0, 0, 0, 34, 53, 27, 101, 119, 114, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 101, 119, 114, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 101, 119, 114, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 125, 52, 60, 25, 0, 0, 0, 0, 128, 150, 152, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 106, 16, 46, 28, 238, 176, 217, 31, 126, 248, 196, 191, 47, 132, 41, 230, 27, 85, 211, 80, 96, 176, 233, 106, 215, 59, 40, 74, 147, 120, 245, 56, 32, 137, 53, 77, 101, 193, 230, 75, 71, 25, 16, 49, 65, 27, 248, 172, 176, 244, 172, 123, 87, 218, 44, 218, 243, 63, 81, 163, 214, 69, 196, 173, 243, 241, 100, 210, 112, 2, 178, 184, 113, 176, 209, 214, 175, 135, 164, 0, 76, 133, 42, 135, 87, 193, 249, 50, 229, 136, 12, 210, 61, 170, 83, 192, 190, 97, 184, 123, 149, 111, 227, 2, 140, 99, 89, 163, 128, 12, 148, 187, 10, 187, 8, 223, 137, 32, 102, 226, 164, 177, 18, 93, 16, 47, 12, 44, 165, 13, 20, 230, 44, 177, 12, 60, 184, 139, 8, 27, 63, 180, 115, 91, 137, 207, 208, 146, 239, 92, 32, 208, 98, 45, 230, 34, 179, 203, 48, 199, 151, 46, 158, 38, 5, 19, 152, 45, 48, 51, 135, 138, 187, 103, 33, 202, 52, 95, 199, 184, 156, 30, 136, 48, 128, 42, 126, 180, 124, 53, 231, 97, 114, 18, 46, 24, 197, 200, 121, 172, 191, 112, 3, 155, 206, 21, 46, 210, 131, 129, 83, 99, 75, 163, 206, 87, 125, 200, 87, 151, 180, 125, 95, 74, 119, 238, 110, 85, 131, 64, 232, 70, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 51, 55, 0, 0, 0, 34, 53, 27, 109, 105, 97, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 109, 105, 97, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 109, 105, 97, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 69, 111, 48, 13, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 9, 14, 235, 145, 239, 52, 108, 30, 244, 154, 225, 124, 149, 251, 85, 28, 27, 42, 120, 206, 96, 176, 209, 43, 129, 8, 150, 120, 128, 198, 119, 67, 189, 80, 122, 85, 87, 65, 159, 17, 197, 70, 109, 2, 24, 221, 32, 22, 49, 75, 118, 144, 238, 224, 230, 229, 43, 4, 238, 222, 40, 130, 64, 198, 81, 154, 131, 191, 210, 25, 73, 151, 142, 29, 225, 23, 82, 163, 92, 96, 76, 2, 194, 18, 178, 233, 180, 132, 25, 215, 87, 87, 173, 112, 65, 148, 76, 26, 190, 32, 87, 56, 61, 169, 71, 53, 191, 148, 129, 254, 155, 172, 98, 236, 2, 234, 170, 32, 98, 33, 196, 45, 64, 87, 253, 59, 183, 231, 215, 179, 33, 229, 251, 196, 237, 169, 228, 143, 58, 96, 88, 16, 75, 145, 58, 122, 41, 194, 142, 247, 32, 41, 19, 96, 18, 7, 206, 168, 153, 88, 123, 5, 153, 244, 88, 252, 41, 15, 89, 120, 232, 94, 227, 106, 228, 35, 237, 159, 130, 139, 95, 244, 251, 48, 152, 125, 13, 151, 172, 223, 218, 154, 147, 81, 218, 251, 120, 162, 103, 122, 242, 248, 248, 164, 195, 101, 180, 32, 39, 8, 125, 232, 12, 17, 131, 185, 17, 229, 4, 164, 120, 246, 187, 66, 91, 46, 252, 241, 100, 6, 77, 228, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 51, 54, 0, 0, 0, 34, 53, 27, 109, 105, 97, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 109, 105, 97, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 109, 105, 97, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 155, 140, 42, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 236, 25, 167, 237, 3, 135, 42, 163, 145, 113, 133, 67, 94, 185, 131, 135, 169, 18, 197, 122, 96, 174, 53, 71, 222, 156, 219, 27, 165, 82, 235, 94, 112, 219, 239, 143, 220, 163, 178, 172, 188, 217, 137, 172, 160, 198, 78, 72, 200, 160, 59, 34, 219, 252, 73, 11, 109, 130, 156, 198, 149, 10, 207, 77, 119, 65, 37, 89, 25, 1, 112, 10, 93, 7, 240, 175, 113, 250, 255, 236, 192, 191, 255, 152, 12, 21, 160, 214, 53, 225, 11, 185, 129, 74, 102, 184, 144, 15, 132, 42, 160, 194, 243, 181, 73, 14, 79, 26, 115, 4, 39, 225, 196, 151, 30, 255, 158, 32, 26, 146, 61, 70, 150, 164, 152, 131, 33, 155, 92, 222, 92, 98, 159, 115, 170, 218, 169, 202, 101, 217, 69, 16, 18, 161, 16, 152, 135, 37, 246, 228, 32, 153, 216, 177, 32, 95, 223, 53, 174, 83, 68, 150, 205, 72, 85, 30, 147, 238, 204, 9, 115, 99, 132, 95, 43, 209, 2, 41, 132, 168, 176, 102, 20, 48, 177, 190, 193, 154, 206, 30, 239, 64, 202, 147, 43, 8, 133, 240, 64, 112, 144, 48, 64, 231, 0, 45, 126, 127, 137, 28, 178, 236, 203, 163, 233, 249, 154, 79, 219, 199, 10, 251, 33, 122, 190, 247, 112, 163, 232, 150, 82, 101, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 50, 56, 0, 0, 0, 34, 53, 27, 115, 121, 100, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 115, 121, 100, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 115, 121, 100, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 155, 140, 42, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 148, 210, 132, 36, 100, 12, 29, 216, 194, 84, 83, 19, 140, 172, 203, 159, 94, 111, 85, 223, 96, 173, 253, 71, 31, 195, 210, 180, 12, 214, 231, 31, 31, 224, 81, 237, 178, 236, 151, 207, 252, 231, 251, 180, 146, 136, 52, 237, 63, 139, 206, 174, 63, 82, 252, 101, 211, 231, 150, 3, 165, 220, 4, 38, 222, 160, 4, 104, 120, 12, 176, 204, 153, 106, 10, 112, 197, 194, 86, 206, 146, 110, 252, 116, 105, 78, 81, 127, 17, 215, 175, 85, 234, 138, 250, 229, 30, 79, 166, 176, 199, 123, 230, 78, 19, 119, 102, 138, 9, 83, 30, 205, 208, 122, 255, 17, 54, 32, 188, 173, 177, 44, 73, 152, 155, 70, 56, 116, 72, 136, 20, 45, 159, 129, 194, 94, 55, 179, 141, 32, 178, 180, 35, 6, 162, 218, 226, 210, 30, 119, 32, 160, 4, 176, 218, 102, 190, 212, 198, 70, 171, 22, 103, 191, 74, 126, 140, 206, 214, 228, 234, 152, 46, 3, 71, 190, 83, 114, 125, 164, 125, 154, 4, 48, 163, 17, 154, 117, 47, 44, 14, 102, 17, 103, 74, 133, 56, 17, 200, 21, 133, 182, 127, 25, 17, 114, 245, 170, 130, 245, 14, 23, 224, 19, 168, 4, 139, 180, 17, 228, 127, 33, 206, 28, 211, 79, 132, 203, 75, 52, 111, 13, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 49, 54, 0, 0, 0, 34, 53, 27, 97, 109, 115, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 97, 109, 115, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 97, 109, 115, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 155, 140, 42, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 92, 147, 140, 156, 151, 155, 223, 46, 222, 250, 240, 247, 68, 221, 88, 193, 183, 85, 156, 1, 96, 173, 38, 226, 40, 95, 222, 188, 171, 182, 157, 81, 171, 216, 243, 42, 150, 20, 228, 235, 70, 131, 88, 235, 43, 193, 87, 178, 212, 238, 36, 30, 186, 165, 239, 29, 193, 36, 1, 136, 58, 41, 34, 164, 32, 173, 17, 118, 136, 16, 247, 128, 51, 125, 36, 195, 77, 43, 170, 163, 245, 107, 170, 86, 61, 144, 174, 158, 228, 208, 27, 102, 25, 136, 174, 156, 209, 246, 130, 69, 100, 62, 21, 129, 143, 52, 190, 243, 163, 121, 203, 78, 229, 122, 227, 25, 129, 32, 200, 120, 108, 172, 141, 86, 186, 220, 154, 255, 196, 54, 54, 106, 5, 189, 189, 228, 18, 17, 47, 42, 148, 83, 129, 146, 157, 234, 79, 25, 159, 122, 32, 55, 247, 179, 162, 123, 248, 27, 71, 222, 28, 199, 64, 144, 115, 22, 100, 7, 218, 148, 15, 58, 106, 61, 37, 228, 187, 92, 215, 109, 101, 100, 8, 48, 176, 204, 196, 86, 23, 57, 59, 37, 194, 35, 221, 197, 180, 234, 157, 4, 141, 177, 104, 22, 83, 150, 216, 3, 63, 150, 65, 187, 194, 100, 226, 78, 194, 44, 82, 155, 87, 124, 221, 13, 151, 172, 171, 50, 15, 44, 174, 209, 11, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 54, 0, 0, 0, 34, 53, 27, 111, 114, 100, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 111, 114, 100, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 111, 114, 100, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 228, 81, 54, 19, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 121, 255, 244, 9, 60, 44, 241, 72, 8, 84, 138, 153, 90, 206, 137, 238, 105, 28, 205, 194, 96, 172, 145, 177, 5, 85, 74, 116, 2, 214, 17, 30, 154, 225, 253, 206, 238, 63, 30, 51, 183, 250, 180, 203, 129, 62, 72, 149, 63, 8, 223, 119, 13, 251, 116, 254, 34, 39, 201, 2, 30, 202, 155, 198, 203, 188, 208, 166, 167, 19, 71, 145, 250, 179, 128, 106, 123, 250, 162, 209, 145, 7, 83, 4, 36, 33, 124, 221, 199, 219, 26, 244, 115, 247, 18, 130, 79, 97, 89, 106, 76, 131, 216, 125, 240, 89, 50, 120, 165, 164, 85, 73, 121, 84, 197, 45, 30, 32, 234, 160, 60, 222, 250, 216, 53, 77, 52, 108, 233, 121, 196, 78, 37, 176, 159, 69, 8, 8, 127, 45, 16, 122, 108, 69, 36, 50, 87, 202, 56, 236, 32, 147, 226, 110, 143, 28, 68, 61, 75, 104, 222, 4, 157, 44, 31, 170, 231, 250, 136, 100, 103, 109, 148, 199, 241, 245, 84, 106, 202, 130, 224, 73, 200, 48, 139, 136, 96, 241, 0, 254, 56, 3, 6, 123, 176, 228, 74, 50, 22, 33, 162, 5, 192, 117, 134, 137, 96, 106, 115, 69, 122, 180, 188, 218, 53, 202, 140, 38, 214, 58, 98, 39, 77, 251, 43, 85, 84, 8, 158, 127, 149, 174, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 49, 57, 0, 0, 0, 34, 53, 27, 97, 109, 115, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 97, 109, 115, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 97, 109, 115, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 125, 52, 60, 25, 0, 0, 0, 0, 255, 219, 210, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 166, 25, 129, 90, 73, 212, 177, 91, 211, 173, 56, 203, 82, 156, 182, 194, 251, 243, 247, 172, 96, 170, 193, 187, 5, 137, 168, 8, 63, 175, 109, 188, 59, 223, 168, 177, 224, 172, 17, 178, 118, 162, 74, 194, 132, 53, 29, 12, 7, 22, 79, 148, 111, 12, 21, 14, 242, 235, 2, 139, 77, 63, 3, 76, 73, 223, 102, 139, 18, 7, 84, 29, 196, 228, 254, 189, 181, 103, 204, 87, 7, 175, 177, 211, 113, 63, 197, 90, 88, 233, 179, 19, 24, 213, 231, 10, 23, 105, 172, 189, 197, 46, 23, 237, 59, 164, 235, 66, 207, 66, 138, 67, 222, 69, 79, 12, 102, 32, 69, 143, 111, 44, 247, 100, 214, 67, 17, 130, 31, 60, 37, 154, 40, 224, 97, 212, 20, 148, 145, 18, 75, 244, 14, 138, 208, 30, 156, 90, 138, 123, 32, 108, 3, 211, 248, 156, 119, 99, 92, 84, 124, 184, 229, 11, 200, 232, 160, 21, 167, 61, 49, 182, 172, 29, 156, 224, 122, 74, 37, 87, 99, 178, 79, 48, 174, 137, 201, 181, 133, 227, 159, 190, 148, 11, 148, 32, 214, 248, 14, 13, 207, 212, 98, 8, 228, 249, 136, 20, 128, 90, 226, 168, 210, 133, 206, 153, 206, 102, 235, 199, 241, 203, 157, 137, 145, 186, 116, 41, 124, 122, 27, 252, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 49, 48, 0, 0, 0, 34, 53, 27, 100, 102, 119, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 100, 102, 119, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 100, 102, 119, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 228, 81, 54, 19, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 18, 125, 166, 160, 158, 140, 211, 99, 86, 99, 87, 106, 147, 120, 183, 17, 214, 252, 4, 169, 96, 168, 67, 78, 156, 151, 11, 5, 53, 145, 59, 168, 50, 158, 77, 7, 99, 108, 228, 110, 185, 196, 4, 207, 100, 14, 233, 44, 47, 57, 42, 144, 105, 127, 174, 210, 201, 84, 222, 81, 52, 91, 184, 25, 7, 97, 38, 247, 234, 19, 32, 244, 37, 104, 116, 198, 29, 17, 129, 23, 200, 14, 101, 36, 206, 137, 115, 208, 81, 183, 126, 243, 85, 149, 167, 75, 238, 48, 195, 73, 132, 246, 177, 2, 113, 15, 111, 25, 168, 20, 110, 118, 17, 63, 204, 174, 152, 32, 183, 198, 55, 200, 194, 9, 235, 18, 229, 215, 142, 140, 27, 239, 103, 183, 200, 127, 212, 133, 203, 154, 139, 24, 202, 124, 79, 146, 18, 254, 47, 85, 32, 226, 47, 90, 26, 135, 198, 56, 156, 180, 166, 136, 51, 194, 2, 225, 188, 133, 23, 100, 96, 182, 108, 10, 216, 219, 71, 51, 67, 162, 150, 151, 35, 48, 174, 157, 139, 112, 62, 206, 39, 18, 61, 70, 56, 33, 139, 82, 117, 150, 155, 193, 164, 2, 243, 197, 4, 108, 231, 58, 158, 199, 155, 22, 89, 55, 225, 217, 165, 116, 167, 43, 211, 195, 98, 141, 86, 66, 214, 69, 165, 73, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 50, 57, 0, 0, 0, 34, 53, 27, 115, 121, 100, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 115, 121, 100, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 115, 121, 100, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 69, 111, 48, 13, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 225, 125, 195, 57, 236, 113, 89, 50, 115, 174, 253, 198, 39, 212, 190, 56, 38, 91, 207, 60, 96, 166, 8, 39, 80, 42, 251, 28, 208, 234, 224, 210, 57, 237, 94, 236, 213, 222, 57, 206, 27, 188, 210, 55, 254, 59, 114, 254, 6, 54, 43, 235, 187, 92, 124, 72, 221, 127, 134, 21, 149, 196, 228, 215, 140, 178, 27, 54, 50, 3, 239, 154, 83, 82, 179, 237, 66, 201, 235, 125, 156, 138, 78, 147, 101, 161, 218, 128, 236, 182, 108, 50, 38, 128, 196, 210, 116, 249, 83, 189, 203, 167, 189, 248, 5, 142, 128, 77, 92, 120, 187, 92, 67, 185, 140, 210, 56, 32, 129, 13, 176, 202, 61, 129, 147, 219, 242, 139, 42, 182, 111, 162, 254, 231, 173, 151, 203, 172, 167, 199, 66, 36, 25, 21, 35, 142, 100, 9, 61, 39, 32, 196, 227, 84, 232, 4, 70, 42, 214, 202, 189, 232, 50, 102, 109, 224, 67, 72, 207, 68, 229, 239, 17, 145, 231, 43, 243, 201, 72, 7, 127, 100, 154, 48, 135, 151, 233, 161, 21, 118, 43, 192, 39, 151, 145, 235, 133, 91, 43, 239, 114, 35, 62, 137, 31, 79, 65, 21, 36, 156, 149, 216, 53, 5, 255, 183, 130, 159, 188, 135, 176, 87, 136, 114, 24, 221, 85, 151, 222, 189, 59, 41, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 50, 49, 0, 0, 0, 34, 53, 27, 108, 104, 114, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 108, 104, 114, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 108, 104, 114, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 69, 111, 48, 13, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 222, 209, 104, 28, 254, 42, 165, 57, 151, 152, 143, 112, 252, 225, 64, 72, 66, 127, 79, 12, 96, 165, 37, 89, 186, 64, 228, 211, 63, 177, 149, 190, 212, 65, 188, 111, 134, 14, 8, 33, 220, 31, 2, 141, 149, 205, 191, 54, 134, 74, 227, 41, 236, 241, 247, 173, 211, 194, 110, 40, 201, 181, 134, 142, 4, 201, 234, 42, 150, 14, 42, 193, 252, 87, 65, 151, 115, 90, 43, 75, 248, 91, 9, 108, 119, 246, 181, 175, 212, 61, 40, 203, 78, 190, 242, 220, 204, 215, 191, 31, 54, 147, 150, 143, 27, 66, 108, 14, 130, 88, 71, 9, 55, 208, 152, 56, 105, 32, 64, 112, 213, 72, 54, 82, 193, 175, 43, 27, 195, 221, 0, 167, 32, 67, 12, 145, 133, 26, 33, 64, 205, 72, 130, 213, 55, 12, 192, 36, 253, 108, 32, 63, 8, 98, 185, 172, 90, 57, 103, 160, 11, 29, 48, 140, 132, 129, 161, 255, 101, 0, 40, 27, 35, 205, 149, 100, 110, 34, 96, 107, 176, 25, 122, 48, 164, 41, 128, 166, 211, 218, 45, 7, 76, 224, 127, 133, 139, 147, 2, 215, 166, 52, 174, 240, 41, 168, 227, 213, 133, 173, 134, 244, 40, 52, 11, 7, 244, 162, 61, 188, 28, 97, 34, 50, 15, 170, 160, 22, 120, 63, 134, 66, 11, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 56, 0, 0, 0, 34, 53, 27, 100, 102, 119, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 100, 102, 119, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 100, 102, 119, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 155, 140, 42, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 196, 193, 42, 87, 154, 171, 32, 255, 205, 111, 39, 26, 234, 17, 212, 128, 254, 88, 212, 67, 96, 164, 250, 219, 193, 117, 233, 34, 233, 160, 13, 222, 222, 134, 0, 83, 70, 46, 62, 23, 210, 26, 17, 130, 149, 237, 157, 156, 28, 5, 125, 31, 90, 101, 219, 102, 38, 42, 167, 50, 219, 78, 156, 49, 124, 118, 213, 95, 146, 3, 114, 77, 229, 113, 115, 244, 92, 227, 48, 86, 137, 46, 8, 102, 49, 104, 16, 101, 35, 243, 222, 173, 200, 38, 39, 16, 85, 74, 99, 11, 161, 37, 214, 134, 84, 4, 89, 154, 189, 211, 168, 166, 84, 121, 185, 203, 31, 32, 207, 249, 255, 248, 140, 61, 225, 69, 68, 157, 238, 35, 239, 92, 156, 15, 178, 208, 8, 237, 249, 42, 76, 237, 146, 122, 112, 93, 0, 49, 34, 3, 32, 57, 147, 102, 239, 9, 243, 222, 86, 28, 0, 172, 126, 118, 116, 162, 110, 109, 34, 229, 150, 196, 62, 106, 136, 175, 204, 221, 214, 228, 28, 12, 137, 48, 135, 101, 18, 35, 186, 177, 181, 82, 7, 66, 51, 5, 22, 224, 162, 160, 162, 58, 60, 34, 250, 161, 13, 79, 166, 241, 55, 139, 48, 244, 185, 239, 129, 105, 61, 233, 246, 1, 212, 222, 45, 34, 216, 177, 116, 109, 125, 180, 11, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 49, 0, 0, 0, 34, 53, 27, 101, 119, 114, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 101, 119, 114, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 101, 119, 114, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 69, 111, 48, 13, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 232, 93, 217, 185, 16, 234, 46, 88, 236, 243, 198, 206, 222, 215, 248, 252, 87, 74, 41, 110, 96, 162, 141, 102, 167, 205, 63, 255, 132, 170, 219, 172, 97, 240, 243, 244, 6, 60, 168, 132, 155, 127, 218, 28, 54, 28, 55, 222, 23, 101, 187, 184, 146, 212, 115, 64, 197, 112, 90, 235, 113, 187, 160, 6, 188, 166, 16, 132, 229, 15, 9, 80, 34, 121, 60, 244, 18, 193, 105, 66, 230, 179, 18, 173, 205, 140, 131, 214, 210, 102, 136, 210, 228, 170, 49, 172, 172, 170, 136, 205, 133, 182, 109, 133, 18, 109, 228, 122, 85, 139, 146, 21, 223, 44, 166, 194, 161, 32, 189, 55, 124, 237, 217, 148, 67, 165, 136, 66, 236, 222, 45, 116, 136, 242, 142, 249, 15, 118, 240, 212, 14, 149, 8, 230, 88, 23, 96, 180, 124, 61, 32, 116, 32, 29, 119, 206, 142, 208, 208, 120, 60, 243, 252, 152, 29, 208, 244, 25, 44, 105, 89, 124, 221, 141, 30, 189, 122, 39, 140, 196, 253, 204, 174, 48, 185, 23, 163, 71, 241, 80, 152, 211, 197, 223, 210, 65, 137, 251, 126, 46, 45, 0, 216, 129, 187, 113, 220, 0, 244, 77, 14, 0, 173, 124, 251, 153, 63, 209, 167, 155, 28, 10, 72, 6, 63, 201, 180, 8, 87, 51, 62, 188, 11, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 52, 0, 0, 0, 34, 53, 27, 111, 114, 100, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 111, 114, 100, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 111, 114, 100, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 155, 140, 42, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 185, 50, 221, 0, 235, 52, 62, 4, 219, 6, 157, 13, 86, 74, 250, 201, 145, 123, 216, 48, 96, 162, 128, 36, 136, 6, 47, 50, 228, 17, 171, 110, 23, 41, 82, 10, 197, 234, 103, 191, 186, 52, 163, 119, 19, 55, 210, 110, 79, 59, 27, 177, 85, 174, 32, 56, 1, 5, 154, 93, 124, 124, 205, 133, 116, 154, 181, 204, 24, 0, 111, 55, 247, 15, 137, 112, 189, 83, 97, 63, 223, 24, 78, 231, 159, 58, 11, 239, 135, 141, 163, 104, 22, 62, 216, 120, 211, 50, 25, 47, 207, 237, 110, 66, 23, 94, 122, 92, 74, 171, 160, 17, 17, 158, 243, 22, 165, 32, 25, 254, 167, 149, 211, 244, 23, 189, 120, 217, 20, 126, 109, 114, 110, 112, 176, 117, 133, 189, 193, 104, 37, 193, 36, 198, 226, 40, 198, 17, 64, 5, 32, 169, 222, 224, 200, 145, 137, 249, 168, 95, 170, 74, 140, 77, 40, 98, 220, 57, 1, 120, 81, 158, 127, 144, 136, 221, 156, 246, 229, 211, 134, 15, 114, 48, 144, 230, 122, 83, 218, 252, 189, 210, 172, 212, 80, 99, 222, 92, 85, 185, 193, 120, 113, 46, 93, 160, 101, 236, 249, 30, 197, 162, 210, 62, 15, 143, 215, 100, 192, 3, 186, 212, 43, 19, 79, 115, 227, 233, 65, 225, 228, 45, 11, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 55, 0, 0, 0, 34, 53, 27, 111, 114, 100, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 111, 114, 100, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 111, 114, 100, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 125, 52, 60, 25, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 148, 69, 218, 42, 230, 83, 97, 123, 55, 212, 87, 172, 76, 5, 198, 129, 202, 227, 117, 61, 96, 153, 13, 40, 191, 251, 130, 217, 141, 218, 203, 87, 112, 236, 102, 56, 133, 145, 111, 179, 24, 225, 255, 169, 4, 239, 173, 41, 138, 236, 35, 251, 181, 219, 51, 69, 25, 144, 238, 16, 149, 207, 169, 42, 23, 59, 147, 159, 33, 3, 113, 66, 182, 66, 236, 37, 91, 255, 34, 15, 146, 221, 2, 173, 159, 105, 158, 254, 227, 122, 178, 61, 77, 58, 67, 118, 83, 118, 133, 66, 13, 40, 75, 156, 107, 123, 19, 130, 6, 168, 99, 245, 58, 22, 27, 29, 46, 32, 190, 170, 51, 186, 163, 168, 165, 161, 0, 178, 107, 240, 153, 58, 31, 175, 64, 224, 203, 205, 24, 252, 29, 85, 133, 179, 86, 39, 15, 13, 68, 49, 32, 218, 86, 117, 148, 40, 220, 11, 185, 112, 145, 157, 49, 199, 86, 254, 235, 155, 136, 173, 16, 167, 75, 29, 190, 36, 15, 178, 105, 198, 134, 68, 222, 48, 151, 121, 249, 246, 167, 88, 176, 135, 74, 18, 64, 155, 147, 128, 124, 32, 71, 5, 63, 180, 243, 4, 6, 142, 171, 230, 39, 133, 148, 238, 165, 3, 176, 133, 41, 225, 34, 83, 0, 131, 78, 91, 240, 229, 131, 56, 204, 140, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 49, 49, 0, 0, 0, 34, 53, 27, 100, 102, 119, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 100, 102, 119, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 100, 102, 119, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 125, 52, 60, 25, 0, 0, 0, 0, 178, 169, 220, 29, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 219, 77, 62, 9, 213, 253, 199, 127, 78, 185, 152, 39, 30, 133, 74, 199, 111, 138, 246, 211, 96, 152, 60, 97, 50, 62, 139, 100, 117, 184, 108, 155, 188, 240, 47, 162, 56, 85, 161, 73, 28, 43, 208, 48, 81, 139, 208, 26, 44, 245, 157, 146, 141, 60, 143, 93, 136, 43, 6, 98, 50, 207, 6, 22, 103, 222, 43, 180, 20, 21, 195, 101, 51, 91, 25, 252, 78, 34, 52, 60, 106, 129, 113, 172, 204, 102, 181, 184, 17, 177, 95, 69, 234, 176, 244, 238, 216, 118, 41, 142, 119, 52, 48, 90, 121, 131, 99, 80, 93, 91, 235, 214, 239, 62, 77, 109, 245, 32, 126, 23, 157, 94, 8, 21, 165, 187, 95, 251, 216, 68, 242, 65, 51, 113, 246, 71, 171, 101, 58, 170, 28, 236, 189, 98, 106, 22, 43, 23, 26, 60, 32, 188, 77, 185, 200, 110, 29, 216, 104, 133, 230, 204, 70, 121, 182, 255, 2, 24, 59, 120, 5, 239, 180, 78, 133, 182, 90, 69, 69, 122, 78, 187, 113, 48, 137, 5, 208, 56, 50, 102, 33, 40, 39, 3, 169, 46, 40, 252, 173, 11, 139, 31, 147, 53, 185, 114, 54, 190, 200, 34, 206, 91, 121, 18, 236, 201, 144, 21, 208, 35, 56, 240, 37, 6, 163, 212, 177, 69, 232, 184, 3, 191, 11, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 50, 0, 0, 0, 34, 53, 27, 101, 119, 114, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 101, 119, 114, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 101, 119, 114, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 228, 81, 54, 19, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 189, 140, 56, 205, 22, 242, 253, 73, 29, 123, 20, 28, 60, 240, 92, 54, 194, 158, 161, 53, 96, 151, 162, 106, 19, 123, 189, 229, 133, 68, 1, 30, 106, 173, 133, 129, 220, 72, 20, 131, 157, 229, 128, 164, 29, 64, 131, 96, 79, 190, 92, 65, 156, 32, 69, 191, 236, 91, 30, 57, 12, 36, 21, 19, 136, 156, 64, 238, 69, 19, 36, 63, 241, 90, 29, 39, 200, 156, 208, 77, 237, 96, 172, 242, 97, 70, 184, 4, 175, 177, 131, 164, 75, 184, 134, 148, 74, 68, 0, 151, 172, 111, 60, 26, 38, 155, 37, 202, 191, 103, 92, 235, 66, 32, 234, 64, 165, 32, 220, 7, 249, 107, 99, 98, 112, 14, 182, 255, 230, 79, 52, 128, 51, 179, 224, 76, 147, 99, 252, 147, 249, 179, 34, 29, 75, 59, 85, 96, 243, 151, 32, 106, 168, 219, 60, 3, 191, 127, 216, 116, 69, 36, 195, 123, 215, 161, 161, 158, 43, 77, 47, 154, 107, 200, 61, 219, 168, 137, 33, 108, 115, 105, 32, 48, 138, 109, 182, 106, 233, 138, 63, 153, 179, 54, 101, 89, 181, 71, 220, 77, 164, 4, 114, 51, 57, 193, 22, 46, 91, 200, 30, 114, 94, 102, 49, 74, 135, 202, 36, 167, 93, 184, 239, 103, 68, 238, 46, 84, 73, 93, 211, 61, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 49, 51, 0, 0, 0, 34, 53, 27, 115, 103, 112, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 115, 103, 112, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 115, 103, 112, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 69, 111, 48, 13, 0, 0, 0, 0, 163, 140, 230, 14, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 255, 102, 132, 91, 188, 57, 46, 8, 114, 23, 183, 7, 201, 206, 178, 176, 119, 72, 216, 172, 96, 150, 12, 75, 244, 124, 76, 191, 31, 138, 233, 147, 67, 78, 140, 137, 0, 119, 9, 155, 27, 62, 96, 120, 225, 22, 50, 186, 173, 53, 10, 4, 218, 170, 75, 72, 50, 172, 222, 248, 128, 131, 216, 74, 16, 247, 246, 35, 89, 18, 117, 214, 176, 164, 33, 231, 127, 139, 75, 212, 87, 81, 115, 115, 206, 16, 75, 255, 254, 14, 224, 66, 233, 38, 127, 129, 42, 97, 132, 37, 182, 234, 62, 10, 91, 178, 91, 87, 130, 228, 158, 241, 156, 73, 108, 41, 236, 32, 6, 123, 97, 207, 17, 90, 131, 114, 49, 108, 129, 7, 85, 216, 14, 24, 246, 247, 17, 69, 45, 187, 85, 52, 65, 115, 190, 148, 181, 3, 125, 102, 32, 99, 98, 115, 255, 52, 129, 6, 227, 53, 219, 74, 15, 208, 77, 8, 236, 178, 52, 224, 72, 219, 179, 15, 242, 82, 90, 159, 66, 50, 97, 35, 208, 48, 182, 221, 209, 8, 64, 106, 37, 248, 170, 248, 105, 51, 220, 114, 137, 169, 113, 217, 10, 224, 38, 112, 225, 214, 63, 64, 102, 133, 239, 186, 248, 199, 34, 97, 106, 168, 79, 95, 132, 13, 42, 201, 234, 119, 217, 71, 71, 198, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 51, 52, 0, 0, 0, 34, 53, 27, 110, 114, 116, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 110, 114, 116, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 110, 114, 116, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 228, 81, 54, 19, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 179, 251, 79, 253, 214, 182, 228, 194, 68, 3, 17, 191, 142, 125, 64, 29, 179, 244, 14, 143, 96, 146, 242, 151, 26, 142, 240, 160, 140, 91, 146, 127, 134, 227, 187, 129, 246, 168, 254, 147, 249, 110, 217, 202, 76, 130, 35, 173, 247, 191, 209, 242, 138, 234, 216, 58, 115, 7, 94, 59, 98, 15, 10, 83, 43, 239, 162, 81, 40, 4, 246, 222, 239, 62, 204, 108, 92, 27, 41, 230, 114, 222, 122, 74, 80, 12, 80, 208, 18, 127, 5, 19, 117, 159, 139, 129, 136, 161, 133, 57, 53, 129, 63, 69, 92, 197, 85, 192, 233, 123, 63, 255, 104, 187, 209, 127, 231, 32, 33, 153, 173, 197, 158, 239, 222, 169, 145, 171, 148, 107, 246, 47, 161, 152, 250, 170, 230, 158, 16, 95, 109, 34, 208, 160, 213, 241, 111, 70, 52, 122, 32, 108, 245, 68, 178, 1, 242, 165, 254, 69, 80, 54, 65, 216, 53, 243, 84, 69, 87, 181, 30, 151, 62, 105, 76, 79, 10, 150, 211, 233, 219, 79, 141, 48, 168, 12, 132, 76, 9, 207, 163, 51, 3, 9, 119, 201, 88, 109, 176, 173, 66, 18, 102, 130, 177, 202, 238, 50, 10, 40, 5, 220, 62, 113, 173, 70, 219, 117, 3, 174, 216, 121, 241, 241, 43, 75, 214, 145, 244, 165, 5, 212, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 51, 50, 0, 0, 0, 34, 53, 27, 110, 114, 116, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 110, 114, 116, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 110, 114, 116, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 155, 140, 42, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 138, 69, 130, 156, 233, 25, 164, 29, 154, 68, 108, 67, 209, 218, 67, 171, 6, 58, 125, 245, 96, 146, 216, 155, 22, 61, 43, 225, 196, 175, 93, 39, 32, 146, 15, 93, 223, 140, 9, 152, 16, 187, 108, 237, 70, 33, 138, 76, 133, 74, 117, 165, 48, 126, 187, 169, 130, 71, 64, 245, 194, 238, 105, 43, 40, 56, 119, 231, 235, 11, 172, 105, 18, 142, 48, 138, 64, 247, 244, 176, 15, 121, 105, 208, 233, 67, 115, 43, 108, 205, 78, 81, 170, 73, 39, 79, 57, 9, 204, 90, 161, 139, 29, 100, 191, 139, 97, 158, 159, 110, 19, 100, 243, 113, 17, 173, 105, 32, 159, 115, 123, 64, 32, 150, 250, 214, 240, 247, 105, 26, 101, 30, 132, 237, 74, 13, 203, 217, 248, 197, 40, 11, 147, 154, 48, 58, 62, 147, 88, 200, 32, 26, 197, 50, 99, 226, 38, 151, 251, 181, 26, 165, 42, 226, 12, 80, 253, 6, 46, 105, 163, 205, 135, 69, 123, 255, 252, 71, 127, 230, 181, 118, 22, 48, 153, 19, 105, 237, 249, 125, 236, 205, 161, 190, 6, 187, 6, 170, 92, 250, 230, 192, 71, 99, 230, 71, 230, 246, 98, 105, 212, 195, 202, 222, 38, 111, 100, 252, 51, 243, 89, 191, 83, 215, 50, 91, 120, 65, 186, 216, 54, 194, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 50, 52, 0, 0, 0, 34, 53, 27, 115, 106, 99, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 115, 106, 99, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 115, 106, 99, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 155, 140, 42, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 252, 138, 236, 86, 103, 105, 177, 77, 107, 84, 104, 209, 218, 154, 63, 0, 8, 56, 25, 100, 96, 146, 208, 30, 203, 17, 56, 249, 176, 115, 195, 219, 128, 131, 56, 145, 236, 170, 175, 36, 29, 39, 101, 210, 141, 161, 98, 114, 127, 198, 8, 143, 111, 7, 53, 51, 67, 121, 2, 225, 162, 60, 33, 91, 50, 100, 111, 65, 135, 16, 70, 149, 24, 32, 130, 90, 174, 154, 85, 209, 69, 203, 97, 59, 65, 31, 187, 164, 9, 85, 255, 73, 151, 89, 30, 249, 22, 159, 101, 5, 13, 141, 241, 1, 31, 172, 86, 63, 109, 30, 0, 99, 64, 195, 87, 18, 43, 32, 2, 103, 22, 82, 47, 95, 111, 77, 93, 202, 137, 180, 134, 84, 242, 166, 29, 159, 105, 162, 111, 236, 11, 79, 84, 123, 103, 97, 226, 252, 235, 162, 32, 27, 41, 97, 36, 136, 251, 130, 40, 30, 47, 199, 198, 205, 250, 154, 153, 178, 178, 199, 133, 245, 238, 127, 132, 253, 178, 113, 37, 61, 208, 51, 204, 48, 173, 164, 9, 223, 139, 195, 35, 216, 218, 54, 124, 50, 17, 47, 238, 254, 17, 103, 71, 117, 196, 233, 200, 221, 159, 66, 20, 101, 238, 207, 218, 215, 65, 83, 64, 53, 29, 123, 214, 209, 113, 5, 236, 79, 138, 191, 121, 132, 11, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 57, 0, 0, 0, 34, 53, 27, 100, 102, 119, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 100, 102, 119, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 100, 102, 119, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 69, 111, 48, 13, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 62, 135, 120, 109, 247, 82, 93, 235, 16, 136, 7, 7, 227, 35, 154, 3, 36, 243, 106, 91, 96, 146, 28, 141, 93, 65, 36, 154, 0, 228, 96, 112, 59, 221, 61, 33, 139, 84, 46, 130, 119, 37, 205, 53, 88, 13, 145, 107, 118, 164, 177, 119, 159, 11, 249, 195, 120, 86, 94, 213, 28, 116, 82, 144, 220, 195, 210, 174, 97, 11, 27, 153, 7, 64, 149, 191, 146, 80, 214, 43, 16, 208, 191, 84, 105, 212, 49, 26, 39, 204, 32, 125, 13, 137, 143, 227, 84, 114, 86, 24, 174, 134, 8, 126, 202, 171, 27, 165, 149, 121, 16, 135, 200, 59, 44, 112, 5, 32, 178, 52, 65, 219, 109, 40, 206, 101, 120, 216, 80, 95, 44, 254, 83, 235, 73, 46, 191, 44, 98, 216, 86, 108, 175, 48, 14, 255, 203, 36, 203, 251, 32, 143, 245, 190, 101, 140, 51, 78, 178, 149, 214, 141, 251, 13, 83, 253, 239, 93, 49, 161, 163, 158, 77, 88, 215, 33, 137, 110, 134, 103, 164, 184, 110, 48, 130, 86, 226, 109, 73, 216, 172, 129, 134, 2, 40, 117, 168, 120, 188, 248, 128, 54, 139, 179, 34, 141, 112, 110, 121, 171, 134, 67, 111, 182, 219, 134, 42, 46, 115, 14, 166, 20, 19, 143, 243, 99, 149, 31, 137, 45, 241, 154, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 50, 48, 0, 0, 0, 34, 53, 27, 108, 104, 114, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 108, 104, 114, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 108, 104, 114, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 155, 140, 42, 7, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 138, 4, 23, 152, 181, 43, 155, 240, 31, 97, 102, 142, 135, 42, 145, 58, 125, 18, 50, 223, 96, 144, 220, 242, 35, 41, 201, 192, 12, 200, 110, 245, 118, 207, 110, 2, 79, 155, 240, 230, 178, 8, 137, 134, 181, 81, 83, 247, 86, 0, 175, 123, 249, 37, 2, 172, 143, 87, 139, 103, 73, 177, 62, 204, 122, 180, 60, 105, 154, 6, 208, 92, 204, 71, 172, 172, 135, 239, 22, 241, 62, 200, 119, 150, 35, 43, 185, 146, 39, 199, 45, 192, 240, 82, 50, 167, 44, 90, 199, 90, 56, 244, 74, 181, 51, 192, 182, 104, 33, 104, 18, 6, 42, 205, 88, 209, 6, 32, 106, 87, 118, 5, 201, 33, 59, 102, 71, 249, 111, 12, 2, 182, 133, 176, 231, 160, 120, 253, 239, 247, 58, 142, 142, 138, 125, 32, 208, 26, 103, 131, 32, 102, 228, 76, 104, 209, 88, 252, 48, 137, 184, 144, 43, 14, 37, 213, 86, 150, 175, 87, 28, 165, 155, 223, 189, 132, 221, 123, 7, 110, 57, 152, 124, 48, 151, 44, 17, 57, 4, 150, 215, 193, 99, 111, 70, 17, 34, 208, 101, 76, 196, 64, 114, 133, 102, 174, 146, 157, 23, 78, 208, 58, 140, 32, 185, 137, 159, 229, 15, 224, 36, 27, 234, 214, 233, 159, 146, 120, 14, 29, 135, 57, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 51, 49, 0, 0, 0, 34, 53, 27, 115, 121, 100, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 115, 121, 100, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 115, 121, 100, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 125, 52, 60, 25, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 122, 214, 187, 208, 129, 9, 37, 222, 5, 77, 51, 137, 54, 172, 127, 37, 223, 68, 65, 239, 96, 144, 188, 49, 5, 44, 35, 90, 114, 156, 167, 65, 159, 133, 76, 114, 241, 174, 205, 197, 22, 205, 24, 183, 128, 104, 195, 224, 214, 255, 134, 98, 221, 113, 231, 207, 48, 210, 126, 221, 28, 53, 73, 9, 242, 199, 236, 82, 125, 13, 206, 44, 211, 184, 87, 252, 84, 229, 174, 225, 87, 175, 175, 238, 242, 106, 159, 215, 199, 31, 194, 179, 91, 45, 189, 238, 163, 218, 11, 213, 62, 95, 28, 239, 224, 246, 191, 66, 129, 238, 137, 86, 233, 153, 94, 89, 198, 32, 227, 174, 216, 44, 32, 249, 107, 73, 43, 168, 57, 61, 45, 205, 58, 14, 40, 179, 116, 35, 41, 18, 49, 113, 20, 96, 71, 77, 117, 38, 165, 16, 32, 181, 50, 108, 237, 165, 171, 226, 110, 195, 180, 12, 9, 205, 3, 96, 129, 7, 60, 216, 36, 127, 235, 232, 192, 7, 2, 90, 82, 93, 12, 168, 243, 48, 165, 119, 231, 209, 195, 90, 129, 198, 33, 151, 145, 211, 245, 255, 16, 51, 14, 8, 128, 85, 159, 88, 121, 181, 230, 166, 88, 151, 94, 138, 131, 122, 25, 3, 233, 201, 119, 83, 165, 113, 122, 244, 110, 134, 235, 71, 222, 135, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 50, 54, 0, 0, 0, 34, 53, 27, 115, 106, 99, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 115, 106, 99, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 115, 106, 99, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 228, 81, 54, 19, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 8, 112, 140, 200, 27, 191, 192, 176, 235, 17, 246, 238, 218, 199, 152, 222, 144, 158, 40, 110, 96, 144, 17, 91, 78, 167, 241, 216, 70, 17, 76, 93, 38, 251, 121, 216, 124, 22, 9, 20, 64, 11, 45, 37, 49, 109, 72, 59, 116, 127, 79, 35, 104, 5, 48, 237, 83, 200, 182, 10, 221, 166, 139, 252, 207, 147, 161, 41, 110, 7, 0, 118, 30, 217, 4, 236, 55, 127, 177, 183, 94, 199, 47, 85, 48, 43, 137, 107, 202, 150, 118, 61, 143, 101, 159, 117, 215, 2, 188, 64, 196, 138, 253, 186, 232, 63, 177, 4, 205, 15, 1, 251, 241, 218, 17, 191, 218, 32, 201, 134, 122, 57, 66, 77, 229, 36, 255, 26, 96, 25, 112, 33, 236, 102, 206, 107, 25, 138, 29, 4, 133, 16, 153, 86, 201, 90, 19, 209, 27, 40, 32, 137, 111, 91, 65, 66, 25, 212, 111, 39, 115, 46, 250, 118, 10, 7, 103, 146, 230, 103, 245, 92, 12, 226, 109, 153, 8, 167, 122, 97, 173, 245, 17, 48, 131, 67, 141, 51, 65, 18, 207, 69, 7, 50, 100, 230, 134, 131, 196, 163, 190, 217, 216, 148, 127, 122, 152, 72, 84, 63, 16, 74, 42, 58, 116, 160, 156, 6, 208, 70, 158, 201, 178, 179, 188, 209, 237, 132, 186, 204, 227, 111, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 50, 55, 0, 0, 0, 34, 53, 27, 115, 106, 99, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 115, 106, 99, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 115, 106, 99, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 125, 52, 60, 25, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 104, 153, 146, 140, 137, 19, 160, 121, 37, 243, 109, 203, 88, 212, 167, 236, 130, 159, 90, 43, 96, 142, 14, 87, 218, 104, 92, 144, 81, 249, 208, 213, 122, 41, 22, 19, 0, 31, 209, 73, 65, 235, 170, 200, 190, 232, 64, 69, 186, 91, 93, 55, 211, 157, 190, 21, 216, 54, 210, 162, 128, 33, 128, 51, 23, 107, 180, 188, 211, 14, 37, 21, 75, 254, 13, 177, 27, 65, 155, 0, 23, 118, 216, 228, 31, 73, 190, 172, 21, 172, 107, 190, 74, 14, 88, 38, 234, 18, 52, 241, 249, 204, 252, 159, 151, 179, 23, 78, 189, 55, 184, 213, 88, 29, 37, 23, 133, 32, 78, 154, 185, 0, 184, 126, 133, 56, 131, 150, 26, 49, 212, 128, 171, 217, 5, 196, 58, 58, 214, 164, 105, 254, 57, 194, 130, 204, 247, 95, 54, 164, 32, 155, 156, 145, 1, 64, 82, 145, 95, 76, 108, 165, 157, 119, 52, 235, 193, 236, 194, 122, 67, 161, 115, 29, 243, 118, 187, 108, 106, 17, 24, 145, 21, 48, 143, 185, 254, 249, 18, 129, 28, 121, 181, 23, 125, 76, 240, 64, 173, 219, 173, 121, 127, 24, 187, 140, 33, 10, 33, 20, 122, 197, 175, 152, 189, 15, 23, 37, 56, 112, 18, 139, 70, 156, 210, 68, 242, 4, 230, 207, 58, 210, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 50, 50, 0, 0, 0, 34, 53, 27, 108, 104, 114, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 108, 104, 114, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 108, 104, 114, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 228, 81, 54, 19, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 33, 70, 108, 156, 152, 153, 157, 203, 212, 67, 188, 182, 144, 240, 59, 40, 154, 172, 168, 227, 96, 141, 142, 15, 240, 110, 248, 35, 119, 100, 245, 33, 133, 114, 48, 105, 114, 25, 116, 21, 50, 109, 160, 132, 62, 195, 38, 218, 79, 118, 224, 99, 70, 202, 211, 234, 13, 85, 8, 220, 142, 152, 7, 97, 77, 178, 205, 75, 201, 15, 99, 220, 69, 37, 5, 134, 198, 193, 47, 66, 68, 178, 149, 230, 218, 130, 221, 134, 52, 89, 25, 81, 123, 203, 12, 91, 88, 14, 244, 29, 105, 122, 218, 172, 24, 105, 145, 93, 103, 84, 219, 183, 32, 155, 8, 67, 89, 32, 194, 76, 44, 208, 49, 53, 238, 158, 193, 4, 193, 204, 78, 109, 96, 242, 55, 153, 131, 200, 74, 235, 247, 192, 249, 13, 1, 63, 8, 9, 98, 62, 32, 11, 214, 246, 42, 251, 165, 43, 163, 64, 87, 30, 140, 10, 55, 169, 66, 13, 172, 66, 243, 103, 90, 21, 171, 22, 255, 90, 189, 236, 219, 31, 247, 48, 176, 145, 146, 158, 67, 146, 233, 238, 222, 55, 236, 125, 22, 237, 120, 253, 98, 142, 158, 203, 170, 225, 9, 189, 41, 23, 19, 125, 172, 120, 71, 87, 32, 134, 210, 154, 89, 54, 53, 146, 229, 26, 160, 76, 172, 202, 87, 10, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 49, 55, 0, 0, 0, 34, 53, 27, 97, 109, 115, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 97, 109, 115, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 97, 109, 115, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 69, 111, 48, 13, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 136, 30, 145, 86, 246, 230, 34, 117, 53, 79, 226, 146, 113, 110, 224, 156, 114, 119, 114, 115, 96, 141, 123, 40, 58, 110, 2, 12, 152, 105, 207, 252, 41, 24, 61, 233, 52, 10, 49, 149, 78, 2, 201, 245, 122, 183, 174, 240, 51, 114, 223, 191, 63, 242, 152, 23, 31, 46, 33, 207, 226, 194, 214, 81, 250, 48, 247, 213, 90, 12, 111, 105, 33, 72, 51, 114, 181, 183, 218, 178, 115, 47, 245, 93, 146, 160, 113, 124, 138, 250, 17, 76, 23, 212, 216, 79, 44, 33, 10, 255, 72, 15, 75, 175, 78, 39, 214, 164, 178, 247, 41, 232, 178, 76, 238, 146, 36, 32, 125, 93, 31, 49, 168, 178, 32, 153, 174, 191, 220, 29, 220, 20, 44, 135, 235, 121, 185, 159, 41, 254, 20, 85, 41, 5, 117, 235, 211, 97, 196, 252, 32, 152, 89, 74, 92, 1, 237, 114, 19, 70, 208, 109, 134, 55, 224, 174, 211, 163, 91, 192, 239, 193, 210, 134, 219, 161, 83, 57, 252, 80, 255, 194, 241, 48, 136, 224, 105, 105, 39, 151, 217, 225, 207, 203, 205, 3, 251, 98, 108, 190, 96, 75, 58, 70, 106, 122, 93, 171, 87, 55, 102, 125, 23, 149, 167, 156, 254, 251, 0, 200, 52, 207, 134, 195, 243, 64, 122, 79, 192, 233, 99, 224, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 51, 51, 0, 0, 0, 34, 53, 27, 110, 114, 116, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 110, 114, 116, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 110, 114, 116, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 69, 111, 48, 13, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 174, 146, 19, 175, 146, 2, 168, 163, 32, 117, 208, 90, 102, 171, 85, 226, 36, 242, 84, 42, 96, 141, 85, 173, 220, 125, 158, 127, 17, 25, 35, 233, 97, 176, 195, 20, 227, 133, 196, 167, 215, 164, 193, 71, 157, 138, 86, 149, 129, 243, 77, 70, 174, 121, 170, 160, 195, 42, 105, 221, 51, 209, 97, 224, 248, 104, 66, 15, 54, 13, 112, 116, 70, 138, 55, 125, 154, 131, 15, 56, 170, 245, 179, 81, 234, 241, 159, 9, 92, 14, 236, 141, 22, 135, 7, 76, 186, 22, 209, 136, 213, 159, 71, 44, 42, 121, 75, 252, 245, 168, 234, 13, 182, 13, 202, 215, 183, 32, 175, 14, 241, 108, 42, 196, 50, 91, 57, 87, 16, 231, 163, 14, 247, 136, 129, 242, 71, 98, 145, 171, 166, 187, 42, 7, 137, 54, 100, 60, 189, 37, 32, 151, 211, 39, 24, 182, 152, 76, 65, 111, 122, 113, 225, 185, 36, 244, 65, 127, 223, 39, 113, 15, 82, 100, 68, 81, 122, 146, 86, 202, 161, 250, 253, 48, 178, 45, 26, 83, 13, 253, 148, 16, 142, 80, 175, 74, 104, 12, 180, 30, 30, 219, 240, 106, 173, 105, 20, 24, 138, 243, 205, 7, 163, 210, 189, 132, 238, 39, 241, 27, 207, 36, 235, 98, 164, 187, 68, 221, 40, 246, 44, 106, 11, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 53, 0, 0, 0, 34, 53, 27, 111, 114, 100, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 111, 114, 100, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 111, 114, 100, 45, 115, 117, 105, 118, 97, 108, 45, 49, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 69, 111, 48, 13, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 179, 1, 108, 252, 177, 72, 49, 15, 118, 4, 186, 54, 127, 153, 107, 53, 201, 144, 9, 227, 96, 140, 233, 134, 76, 220, 11, 28, 98, 46, 7, 58, 55, 69, 95, 223, 173, 243, 132, 181, 254, 39, 239, 12, 95, 67, 115, 238, 193, 242, 143, 46, 199, 116, 124, 117, 115, 71, 95, 189, 254, 213, 118, 19, 182, 183, 200, 110, 172, 21, 43, 52, 225, 229, 45, 7, 148, 48, 4, 0, 203, 70, 79, 73, 79, 123, 137, 155, 133, 254, 239, 126, 134, 182, 16, 87, 18, 0, 64, 30, 5, 84, 92, 49, 52, 252, 145, 149, 207, 4, 214, 204, 46, 221, 58, 40, 2, 32, 244, 36, 163, 3, 47, 132, 114, 133, 252, 104, 85, 150, 211, 83, 70, 164, 184, 86, 96, 213, 203, 51, 169, 7, 11, 41, 196, 133, 59, 70, 228, 209, 32, 120, 58, 96, 109, 134, 100, 63, 108, 176, 69, 110, 0, 196, 2, 175, 119, 158, 125, 105, 135, 49, 242, 66, 121, 187, 102, 223, 9, 158, 3, 44, 113, 48, 161, 91, 238, 248, 15, 11, 100, 64, 202, 197, 61, 63, 119, 100, 213, 0, 52, 247, 163, 86, 2, 212, 116, 66, 43, 161, 30, 151, 137, 37, 39, 75, 215, 225, 212, 109, 28, 0, 212, 196, 141, 15, 234, 10, 90, 234, 7, 229, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 51, 48, 0, 0, 0, 34, 53, 27, 115, 121, 100, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 115, 121, 100, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 115, 121, 100, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 228, 81, 54, 19, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 163, 238, 36, 181, 206, 105, 96, 137, 155, 104, 123, 92, 66, 55, 8, 29, 156, 137, 25, 91, 96, 140, 102, 214, 104, 68, 197, 166, 41, 181, 73, 185, 247, 153, 105, 153, 23, 108, 189, 149, 243, 51, 110, 24, 194, 92, 176, 118, 216, 213, 127, 110, 198, 135, 204, 5, 62, 23, 181, 239, 108, 92, 6, 180, 166, 111, 189, 79, 93, 9, 152, 134, 209, 71, 73, 48, 215, 106, 76, 135, 24, 163, 78, 178, 31, 116, 135, 234, 59, 69, 164, 188, 220, 187, 10, 198, 255, 171, 183, 21, 162, 228, 101, 111, 128, 77, 211, 79, 59, 107, 77, 242, 249, 105, 134, 176, 48, 32, 225, 3, 158, 209, 110, 5, 248, 184, 112, 35, 184, 105, 243, 78, 86, 88, 22, 75, 213, 252, 249, 242, 124, 13, 254, 57, 37, 248, 57, 135, 142, 219, 32, 5, 225, 201, 125, 158, 99, 203, 229, 203, 57, 32, 142, 130, 109, 67, 74, 137, 24, 154, 216, 164, 159, 184, 230, 243, 169, 25, 152, 239, 100, 203, 46, 48, 139, 224, 148, 15, 51, 164, 88, 185, 180, 149, 117, 212, 251, 221, 82, 4, 44, 253, 80, 60, 89, 46, 205, 5, 154, 201, 237, 100, 150, 216, 243, 175, 25, 144, 33, 119, 88, 152, 252, 165, 234, 189, 100, 39, 74, 16, 152, 141, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 49, 56, 0, 0, 0, 34, 53, 27, 97, 109, 115, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 97, 109, 115, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 97, 109, 115, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 228, 81, 54, 19, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 128, 82, 17, 232, 33, 168, 110, 56, 24, 238, 75, 151, 191, 230, 60, 19, 119, 178, 23, 154, 96, 137, 45, 9, 17, 115, 148, 197, 217, 96, 182, 238, 243, 46, 149, 171, 18, 153, 55, 127, 136, 139, 170, 32, 25, 185, 58, 183, 140, 244, 173, 102, 125, 113, 229, 232, 125, 27, 141, 14, 165, 168, 166, 174, 8, 210, 86, 84, 217, 17, 178, 69, 178, 34, 36, 34, 138, 156, 138, 253, 3, 46, 7, 45, 108, 104, 209, 126, 20, 82, 100, 216, 197, 29, 120, 177, 82, 150, 21, 15, 33, 147, 233, 53, 225, 196, 236, 218, 111, 185, 208, 203, 232, 118, 165, 245, 67, 32, 202, 123, 197, 182, 78, 229, 195, 29, 47, 45, 127, 179, 231, 209, 152, 238, 62, 223, 78, 216, 71, 57, 10, 35, 13, 137, 208, 166, 87, 77, 253, 178, 32, 38, 171, 104, 156, 26, 231, 156, 121, 206, 70, 82, 215, 47, 47, 165, 214, 68, 75, 181, 55, 137, 0, 123, 105, 72, 19, 95, 181, 118, 39, 237, 31, 48, 160, 23, 35, 144, 188, 82, 55, 219, 218, 102, 46, 226, 234, 113, 164, 30, 147, 127, 21, 66, 236, 158, 101, 19, 213, 59, 156, 149, 126, 40, 164, 7, 16, 170, 114, 162, 75, 139, 243, 236, 89, 98, 176, 188, 35, 68, 15, 195, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 51, 57, 0, 0, 0, 34, 53, 27, 109, 105, 97, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 109, 105, 97, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 109, 105, 97, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 125, 52, 60, 25, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 107, 161, 237, 21, 3, 70, 168, 230, 203, 99, 46, 183, 52, 197, 73, 132, 68, 124, 94, 151, 96, 134, 93, 89, 54, 96, 173, 180, 134, 208, 243, 184, 113, 193, 103, 55, 65, 135, 126, 166, 241, 146, 248, 85, 81, 224, 53, 253, 37, 3, 116, 202, 35, 185, 31, 69, 224, 232, 157, 66, 175, 112, 226, 249, 234, 105, 172, 4, 122, 14, 64, 27, 47, 96, 135, 9, 125, 136, 183, 155, 121, 43, 172, 243, 78, 93, 36, 153, 131, 31, 193, 197, 247, 200, 46, 93, 115, 42, 198, 121, 246, 115, 48, 137, 200, 68, 4, 147, 158, 215, 80, 117, 12, 5, 103, 105, 7, 32, 23, 180, 254, 11, 119, 244, 193, 48, 85, 127, 124, 187, 162, 38, 32, 225, 90, 128, 240, 14, 56, 146, 59, 14, 250, 210, 93, 19, 205, 53, 154, 142, 32, 36, 115, 4, 141, 109, 99, 167, 56, 211, 4, 196, 69, 153, 153, 150, 28, 108, 160, 171, 223, 227, 215, 15, 25, 144, 64, 130, 43, 63, 189, 197, 124, 48, 149, 30, 227, 5, 33, 13, 17, 8, 9, 9, 78, 53, 110, 220, 68, 33, 191, 205, 192, 183, 117, 111, 112, 29, 58, 214, 156, 243, 33, 191, 225, 73, 202, 161, 249, 192, 60, 17, 91, 237, 68, 35, 71, 219, 81, 219, 23, 121, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 51, 53, 0, 0, 0, 34, 53, 27, 110, 114, 116, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 110, 114, 116, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 110, 114, 116, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 125, 52, 60, 25, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 106, 216, 192, 178, 155, 23, 164, 123, 243, 231, 46, 147, 205, 186, 127, 219, 78, 187, 59, 237, 96, 133, 219, 58, 246, 192, 183, 111, 36, 159, 232, 54, 149, 110, 196, 204, 209, 192, 180, 152, 240, 125, 205, 228, 118, 98, 180, 81, 136, 170, 85, 124, 144, 90, 135, 217, 23, 106, 198, 241, 86, 241, 213, 21, 242, 183, 87, 27, 182, 8, 120, 37, 162, 203, 22, 197, 206, 227, 121, 147, 250, 98, 234, 65, 16, 116, 86, 8, 196, 176, 214, 109, 149, 12, 70, 38, 193, 76, 253, 142, 34, 184, 46, 15, 82, 0, 204, 139, 245, 81, 179, 65, 89, 229, 239, 111, 221, 32, 204, 182, 208, 107, 18, 251, 45, 207, 144, 75, 178, 175, 132, 136, 150, 71, 50, 56, 216, 153, 141, 177, 77, 171, 245, 132, 70, 174, 7, 99, 76, 165, 32, 137, 114, 8, 135, 213, 33, 154, 106, 99, 68, 114, 244, 212, 40, 54, 247, 158, 236, 118, 212, 142, 46, 237, 214, 11, 240, 183, 108, 137, 1, 177, 245, 48, 137, 131, 140, 236, 120, 53, 228, 174, 5, 130, 155, 102, 36, 55, 113, 121, 24, 199, 238, 21, 190, 5, 248, 228, 229, 96, 107, 49, 248, 115, 16, 162, 246, 143, 73, 254, 25, 12, 107, 29, 34, 212, 95, 14, 16, 241, 95, 77, 11, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 48, 0, 0, 0, 34, 53, 27, 101, 119, 114, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 101, 119, 114, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 101, 119, 114, 45, 115, 117, 105, 118, 97, 108, 45, 48, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 155, 140, 42, 7, 0, 0, 0, 0, 52, 232, 92, 135, 0, 0, 0, 0, 42, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 226, 230, 31, 246, 124, 225, 138, 116, 120, 50, 82, 243, 246, 208, 87, 225, 29, 226, 82, 112, 96, 130, 227, 51, 197, 166, 161, 197, 230, 35, 181, 75, 167, 120, 44, 9, 162, 25, 71, 161, 116, 225, 68, 116, 158, 153, 137, 168, 219, 134, 116, 24, 12, 116, 141, 206, 127, 250, 135, 70, 35, 250, 128, 45, 161, 6, 34, 74, 194, 13, 157, 89, 110, 150, 148, 157, 226, 186, 68, 109, 221, 199, 15, 235, 168, 251, 158, 215, 117, 224, 200, 190, 126, 19, 196, 205, 215, 158, 68, 253, 65, 111, 170, 37, 105, 153, 52, 214, 219, 198, 25, 33, 227, 116, 117, 151, 186, 32, 94, 203, 19, 240, 55, 63, 197, 173, 201, 50, 217, 13, 34, 239, 31, 38, 129, 230, 190, 244, 22, 90, 240, 175, 187, 158, 123, 37, 121, 3, 77, 123, 32, 34, 84, 123, 49, 114, 218, 249, 186, 242, 111, 3, 66, 205, 178, 155, 35, 220, 52, 195, 3, 129, 152, 114, 86, 84, 110, 85, 91, 33, 233, 169, 88, 48, 142, 173, 214, 142, 48, 51, 201, 255, 102, 216, 11, 141, 119, 227, 133, 73, 241, 75, 101, 180, 43, 109, 113, 27, 124, 173, 151, 91, 90, 76, 129, 35, 207, 121, 172, 33, 79, 52, 198, 147, 228, 91, 203, 109, 189, 71, 187, 68, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 49, 53, 0, 0, 0, 34, 53, 27, 115, 103, 112, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 115, 103, 112, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 115, 103, 112, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 125, 52, 60, 25, 0, 0, 0, 0, 96, 119, 176, 16, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 241, 73, 108, 229, 243, 126, 88, 109, 2, 28, 226, 107, 67, 77, 69, 2, 248, 4, 110, 19, 96, 129, 39, 221, 36, 115, 102, 19, 3, 59, 145, 114, 34, 48, 40, 143, 129, 65, 74, 253, 108, 79, 171, 195, 45, 76, 238, 224, 61, 209, 123, 76, 72, 136, 163, 17, 230, 196, 30, 228, 159, 208, 32, 30, 177, 134, 106, 137, 86, 5, 16, 213, 52, 133, 132, 106, 204, 203, 86, 224, 59, 239, 0, 61, 190, 174, 116, 138, 41, 240, 209, 184, 157, 157, 41, 2, 241, 254, 176, 115, 223, 120, 142, 55, 119, 219, 191, 7, 224, 215, 203, 136, 185, 197, 197, 196, 174, 32, 174, 44, 82, 57, 201, 34, 110, 199, 255, 217, 213, 14, 14, 78, 172, 178, 172, 236, 246, 111, 138, 81, 167, 115, 31, 145, 15, 174, 48, 207, 184, 208, 32, 124, 8, 55, 39, 53, 90, 43, 166, 21, 190, 80, 45, 157, 220, 62, 219, 216, 50, 119, 211, 93, 208, 7, 230, 184, 205, 150, 43, 231, 239, 36, 130, 48, 138, 117, 239, 130, 223, 151, 223, 11, 152, 69, 0, 179, 138, 100, 2, 40, 23, 77, 237, 52, 147, 238, 75, 223, 117, 240, 212, 113, 187, 134, 179, 87, 194, 103, 111, 76, 223, 155, 48, 131, 221, 106, 49, 196, 124, 172, 213, 135, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 50, 51, 0, 0, 0, 34, 53, 27, 108, 104, 114, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 108, 104, 114, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 108, 104, 114, 45, 115, 117, 105, 118, 97, 108, 45, 51, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 125, 52, 60, 25, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 232, 76, 64, 207, 27, 18, 187, 73, 97, 157, 141, 89, 185, 138, 181, 114, 153, 44, 252, 71, 96, 128, 34, 99, 90, 27, 132, 148, 171, 185, 103, 58, 148, 107, 132, 174, 108, 43, 141, 112, 15, 161, 130, 70, 80, 163, 130, 155, 150, 199, 85, 154, 122, 132, 217, 12, 0, 220, 225, 28, 7, 38, 15, 142, 229, 10, 241, 204, 113, 2, 207, 239, 34, 6, 12, 178, 121, 183, 238, 118, 0, 125, 171, 19, 3, 37, 234, 46, 221, 70, 21, 129, 33, 202, 29, 158, 172, 41, 214, 75, 235, 206, 240, 131, 205, 190, 248, 139, 80, 182, 194, 2, 127, 103, 31, 32, 222, 32, 51, 250, 26, 70, 119, 71, 135, 145, 255, 241, 36, 173, 42, 129, 85, 81, 219, 106, 130, 81, 122, 247, 189, 9, 129, 107, 249, 43, 45, 59, 67, 175, 32, 66, 26, 197, 206, 184, 194, 121, 245, 210, 213, 193, 215, 7, 68, 184, 216, 18, 107, 67, 3, 36, 43, 94, 18, 68, 44, 146, 166, 90, 170, 96, 179, 48, 147, 97, 69, 0, 211, 22, 33, 22, 66, 207, 12, 179, 125, 113, 205, 179, 207, 54, 196, 9, 242, 53, 81, 38, 12, 15, 82, 145, 111, 201, 115, 9, 4, 35, 146, 85, 82, 226, 78, 19, 213, 88, 37, 116, 2, 3, 148, 57, 12, 118, 97, 108, 105, 100, 97, 116, 111, 114, 45, 49, 52, 0, 0, 0, 34, 53, 27, 115, 103, 112, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 6, 31, 144, 224, 3, 33, 53, 27, 115, 103, 112, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 145, 33, 53, 27, 115, 103, 112, 45, 115, 117, 105, 118, 97, 108, 45, 50, 46, 116, 101, 115, 116, 110, 101, 116, 46, 115, 117, 105, 46, 105, 111, 145, 2, 31, 146, 228, 81, 54, 19, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 240, 122, 252, 106, 245, 90, 0, 0, 99, 3, 130, 16, 243, 90, 0, 0, 0, 64, 122, 16, 243, 90, 0, 0, 100, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 128, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 228, 2, 10, 0, 0, 0, 0, 0];
        let result = bcs::from_bytes::<SuiSystemState>(&contents)
            .expect("Sui System State object deserialization cannot fail");
        println!("@@@@@@@@@@@@@ Sui System State: {result:?}");
        let ret = Ok(match self {
            SuiClientCommands::Publish {
                package_path,
                gas,
                build_config,
                gas_budget,
                verify_dependencies,
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

                let compiled_modules = compiled_package.get_package_bytes();

                let client = context.get_client().await?;
                if verify_dependencies {
                    BytecodeSourceVerifier::new(client.read_api(), false)
                        .verify_package_deps(&compiled_package.package)
                        .await?;
                    println!("Successfully verified dependencies on-chain against source.");
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
                let signature = Signature::from_bytes(
                    &Base64::try_from(signature)
                        .map_err(|e| anyhow!(e))?
                        .to_vec()
                        .map_err(|e| anyhow!(e))?,
                )?;
                let verified =
                    Transaction::from_data(data, Intent::default(), signature).verify()?;

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
            SuiClientCommandResult::NewAddress((address, recovery_phrase, scheme)) => {
                writeln!(
                    writer,
                    "Created new keypair for address with scheme {:?}: [{address}]",
                    scheme
                )?;
                writeln!(writer, "Secret Recovery Phrase : [{recovery_phrase}]")?;
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
