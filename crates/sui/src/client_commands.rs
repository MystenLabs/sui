// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use core::fmt;
use std::{
    collections::BTreeSet,
    fmt::{Debug, Display, Formatter, Write},
    path::{Path, PathBuf},
    time::Instant,
};

use anyhow::{anyhow, ensure};
use bip32::DerivationPath;
use clap::*;
use colored::Colorize;
use fastcrypto::traits::ToFromBytes;
use move_core_types::language_storage::TypeTag;
use move_package::BuildConfig;
use serde::Serialize;
use serde_json::json;
use tracing::info;

use crate::config::{Config, PersistedConfig, SuiClientConfig};
use sui_framework::build_move_package_to_bytes;
use sui_json::SuiJsonValue;
use sui_json_rpc_types::{
    GetObjectDataResponse, SuiObjectInfo, SuiParsedObject, SuiTransactionResponse,
};
use sui_json_rpc_types::{GetRawObjectDataResponse, SuiData};
use sui_json_rpc_types::{SuiCertifiedTransaction, SuiExecutionStatus, SuiTransactionEffects};
use sui_keys::keystore::AccountKeystore;
use sui_sdk::TransactionExecutionResult;
use sui_sdk::{ClientType, SuiClient};
use sui_types::crypto::Signature;
use sui_types::sui_serde::{Base64, Encoding};
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    gas_coin::GasCoin,
    messages::Transaction,
    object::Owner,
    parse_sui_type_tag, SUI_FRAMEWORK_ADDRESS,
};
use sui_types::{crypto::SignatureScheme, intent::Intent};

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
        #[clap(long, value_hint = ValueHint::Url)]
        rpc: Option<String>,
        /// The pubsub Websocket server URL
        #[clap(long, value_hint = ValueHint::Url)]
        ws: Option<String>,
    },

    /// Default address used for commands when none specified
    #[clap(name = "active-address")]
    ActiveAddress,

    /// Get object info
    #[clap(name = "object")]
    Object {
        /// Object ID of the object to fetch
        #[clap(long)]
        id: ObjectID,
    },

    /// Publish Move modules
    #[clap(name = "publish")]
    Publish {
        /// Path to directory containing a Move package
        #[clap(
            long = "path",
            short = 'p',
            global = true,
            parse(from_os_str),
            default_value = "."
        )]
        package_path: PathBuf,

        /// Package build options
        #[clap(flatten)]
        build_config: BuildConfig,

        /// ID of the gas object for gas payment, in 20 bytes Hex string
        /// If not provided, a gas object with at least gas_budget value will be selected
        #[clap(long)]
        gas: Option<ObjectID>,

        /// Gas budget for running module initializers
        #[clap(long)]
        gas_budget: u64,
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
    /// Pay SUI to recipients following specified amounts, with input coins.
    /// Length of recipients must be the same as that of amounts.
    #[clap(name = "pay")]
    Pay {
        /// The input coins to be used for pay recipients, following the specified amounts.
        #[clap(long, multiple_occurrences = false, multiple_values = true)]
        input_coins: Vec<ObjectID>,

        /// The recipient addresses, must be of same length as amounts
        #[clap(long, multiple_occurrences = false, multiple_values = true)]
        recipients: Vec<SuiAddress>,

        /// The amounts to be transferred, following the order of recipients.
        #[clap(long, multiple_occurrences = false, multiple_values = true)]
        amounts: Vec<u64>,

        /// ID of the gas object for gas payment, in 20 bytes Hex string
        /// If not provided, a gas object with at least gas_budget value will be selected
        #[clap(long)]
        gas: Option<ObjectID>,

        /// Gas budget for this transfer
        #[clap(long)]
        gas_budget: u64,
    },
    /// Synchronize client state with authorities.
    #[clap(name = "sync")]
    SyncClientState {
        #[clap(long)]
        address: Option<SuiAddress>,
    },

    /// Obtain the Addresses managed by the client.
    #[clap(name = "addresses")]
    Addresses,

    /// Generate new address and keypair with keypair scheme flag {ed25519 | secp256k1}
    /// with optional derivation path, default to m/44'/784'/0'/0'/0' for ed25519 or m/54'/784'/0'/0/0 for secp256k1.
    #[clap(name = "new-address")]
    NewAddress {
        key_scheme: SignatureScheme,
        derivation_path: Option<DerivationPath>,
    },

    /// Obtain all objects owned by the address.
    #[clap(name = "objects")]
    Objects {
        /// Address owning the objects
        #[clap(long)]
        address: Option<SuiAddress>,
    },

    /// Obtain all gas objects owned by the address.
    #[clap(name = "gas")]
    Gas {
        /// Address owning the objects
        #[clap(long)]
        address: Option<SuiAddress>,
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
        /// Base64 encoded of the transaction data.
        #[clap(long)]
        tx_data: String,

        /// Signature scheme used to sign the transaction.
        #[clap(long)]
        scheme: SignatureScheme,

        /// Public key that the signature can be verified with.
        #[clap(long)]
        pubkey: String,

        /// Base64 encoded signature committed to the transaction data.
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
            } => {
                let sender = context.try_get_object_owner(&gas).await?;
                let sender = sender.unwrap_or(context.active_address()?);

                let compiled_modules = build_move_package_to_bytes(&package_path, build_config)?;
                let data = context
                    .client
                    .transaction_builder()
                    .publish(sender, compiled_modules, gas, gas_budget)
                    .await?;
                let signature =
                    context
                        .config
                        .keystore
                        .sign_secure(&sender, &data, Intent::default())?;
                let response = context
                    .execute_transaction(Transaction::new(data, Intent::default(), signature))
                    .await?;

                SuiClientCommandResult::Publish(response)
            }

            SuiClientCommands::Object { id } => {
                // Fetch the object ref
                let object_read = context.client.read_api().get_parsed_object(id).await?;
                SuiClientCommandResult::Object(object_read)
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

                let data = context
                    .client
                    .transaction_builder()
                    .transfer_object(from, object_id, gas, gas_budget, to)
                    .await?;
                let signature =
                    context
                        .config
                        .keystore
                        .sign_secure(&from, &data, Intent::default())?;
                let response = context
                    .execute_transaction(Transaction::new(data, Intent::default(), signature))
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

                let data = context
                    .client
                    .transaction_builder()
                    .transfer_sui(from, object_id, gas_budget, to, amount)
                    .await?;
                let signature =
                    context
                        .config
                        .keystore
                        .sign_secure(&from, &data, Intent::default())?;
                let response = context
                    .execute_transaction(Transaction::new(data, Intent::default(), signature))
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
                let data = context
                    .client
                    .transaction_builder()
                    .pay(from, input_coins, recipients, amounts, gas, gas_budget)
                    .await?;
                let signature =
                    context
                        .config
                        .keystore
                        .sign_secure(&from, &data, Intent::default())?;
                let response = context
                    .execute_transaction(Transaction::new(data, Intent::default(), signature))
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

            SuiClientCommands::Addresses => {
                SuiClientCommandResult::Addresses(context.config.keystore.addresses())
            }

            SuiClientCommands::Objects { address } => {
                let address = address.unwrap_or(context.active_address()?);
                let mut address_object = context
                    .client
                    .read_api()
                    .get_objects_owned_by_address(address)
                    .await?;
                let object_objects = context
                    .client
                    .read_api()
                    .get_objects_owned_by_object(address.into())
                    .await?;
                address_object.extend(object_objects);

                SuiClientCommandResult::Objects(address_object)
            }

            SuiClientCommands::SyncClientState { address } => {
                let address = address.unwrap_or(context.active_address()?);
                context
                    .client
                    .wallet_sync_api()
                    .sync_account_state(address)
                    .await?;

                SuiClientCommandResult::SyncClientState
            }
            SuiClientCommands::NewAddress {
                key_scheme,
                derivation_path,
            } => {
                let (address, phrase, scheme) = context
                    .config
                    .keystore
                    .generate_new_key(key_scheme, derivation_path)?;
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
                let data = match (amounts, count) {
                    (Some(amounts), None) => {
                        context
                            .client
                            .transaction_builder()
                            .split_coin(signer, coin_id, amounts, gas, gas_budget)
                            .await?
                    }
                    (None, Some(count)) => {
                        if count == 0 {
                            return Err(anyhow!("Coin split count must be greater than 0"));
                        }
                        context
                            .client
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
                    .execute_transaction(Transaction::new(data, Intent::default(), signature))
                    .await?;
                SuiClientCommandResult::SplitCoin(response)
            }
            SuiClientCommands::MergeCoin {
                primary_coin,
                coin_to_merge,
                gas,
                gas_budget,
            } => {
                let signer = context.get_object_owner(&primary_coin).await?;
                let data = context
                    .client
                    .transaction_builder()
                    .merge_coins(signer, primary_coin, coin_to_merge, gas, gas_budget)
                    .await?;
                let signature =
                    context
                        .config
                        .keystore
                        .sign_secure(&signer, &data, Intent::default())?;
                let response = context
                    .execute_transaction(Transaction::new(data, Intent::default(), signature))
                    .await?;

                SuiClientCommandResult::MergeCoin(response)
            }
            SuiClientCommands::Switch { address, rpc, ws } => {
                if let Some(addr) = address {
                    if !context.config.keystore.addresses().contains(&addr) {
                        return Err(anyhow!("Address {} not managed by wallet", addr));
                    }
                    context.config.active_address = Some(addr);
                }

                Self::switch_server(&mut context.config, &rpc, &ws)?;

                if Option::is_none(&address) && Option::is_none(&rpc) && Option::is_none(&ws) {
                    return Err(anyhow!(
                        "No address or RPC url specified. Please Specify one."
                    ));
                }
                context.config.save()?;
                SuiClientCommandResult::Switch(SwitchResponse { address, rpc, ws })
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
                let object_read = context.client.read_api().get_parsed_object(nft_id).await?;
                SuiClientCommandResult::CreateExampleNFT(object_read)
            }

            SuiClientCommands::SerializeTransferSui {
                to,
                sui_coin_object_id: object_id,
                gas_budget,
                amount,
            } => {
                let from = context.get_object_owner(&object_id).await?;

                let data = context
                    .client
                    .transaction_builder()
                    .transfer_sui(from, object_id, gas_budget, to, amount)
                    .await?;
                SuiClientCommandResult::SerializeTransferSui(data.to_base64())
            }

            SuiClientCommands::ExecuteSignedTx {
                tx_data,
                scheme,
                pubkey,
                signature,
            } => {
                let data = bcs::from_bytes(&Base64::try_from(tx_data)?.to_vec()?).unwrap();
                let signed_tx = Transaction::new(
                    data,
                    Intent::default(),
                    Signature::from_bytes(
                        &[
                            vec![scheme.flag()],
                            Base64::try_from(signature)?.to_vec()?,
                            Base64::try_from(pubkey)?.to_vec()?,
                        ]
                        .concat(),
                    )?,
                );
                signed_tx.verify_sender_signature()?;

                let response = context.execute_transaction(signed_tx).await?;
                SuiClientCommandResult::ExecuteSignedTx(response)
            }
        });
        ret
    }

    pub fn switch_server(
        config: &mut SuiClientConfig,
        rpc: &Option<String>,
        ws: &Option<String>,
    ) -> Result<(), anyhow::Error> {
        if let Some(rpc) = rpc {
            let ws = match &config.client_type {
                ClientType::RPC(_, Some(ws)) => Some(ws.clone()),
                _ => None,
            };
            config.client_type = ClientType::RPC(rpc.clone(), ws);
        }

        if let Some(ws) = ws {
            let rpc = match &config.client_type {
                ClientType::RPC(rpc, _) => rpc.clone(),
                _ => return Err(anyhow!("RPC server address must be defined")),
            };
            config.client_type = ClientType::RPC(rpc, Some(ws.clone()));
        }
        Ok(())
    }
}

pub struct WalletContext {
    pub config: PersistedConfig<SuiClientConfig>,
    pub client: SuiClient,
}

impl WalletContext {
    pub async fn new(config_path: &Path) -> Result<Self, anyhow::Error> {
        let config: SuiClientConfig = PersistedConfig::read(config_path).map_err(|err| {
            err.context(format!(
                "Cannot open wallet config file at {:?}",
                config_path
            ))
        })?;

        let client = config.client_type.init().await?;
        let config = config.persisted(config_path);
        let context = Self { config, client };
        Ok(context)
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
        self.client.read_api().get_object(object_id).await
    }

    /// Get all the gas objects (and conveniently, gas amounts) for the address
    pub async fn gas_objects(
        &self,
        address: SuiAddress,
    ) -> Result<Vec<(u64, SuiParsedObject, SuiObjectInfo)>, anyhow::Error> {
        let object_refs = self
            .client
            .read_api()
            .get_objects_owned_by_address(address)
            .await?;

        // TODO: We should ideally fetch the objects from local cache
        let mut values_objects = Vec::new();
        for oref in object_refs {
            let response = self
                .client
                .read_api()
                .get_parsed_object(oref.object_id)
                .await?;
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
        let object = self
            .client
            .read_api()
            .get_object(*id)
            .await?
            .into_object()?;
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

    /// This function is compatible with both fullnode and an embedded gateway
    pub async fn execute_transaction(
        &self,
        tx: Transaction,
    ) -> anyhow::Result<SuiTransactionResponse> {
        let tx_digest = *tx.digest();

        let result = self
            .client
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

    pub fn switch_client(&mut self, new_client: SuiClient) {
        self.client = new_client;
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
            SuiClientCommandResult::Object(object_read) => {
                let object = unwrap_err_to_string(|| Ok(object_read.object()?));
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
            SuiClientCommandResult::SerializeTransferSui(res) => {
                write!(writer, "{}", res)?;
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
    let gas_owner = context.try_get_object_owner(&gas).await?;
    let sender = gas_owner.unwrap_or(context.active_address()?);

    let data = context
        .client
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
    let transaction = Transaction::new(data, Intent::default(), signature);

    let response = context.execute_transaction(transaction).await?;
    let cert = response.certificate;
    let effects = response.effects;

    if matches!(effects.status, SuiExecutionStatus::Failure { .. }) {
        return Err(anyhow!("Error calling module: {:#?}", effects.status));
    }
    Ok((cert, effects))
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
            SuiClientCommandResult::Object(object_read) => {
                let object = object_read.object()?;
                Ok(serde_json::to_string_pretty(&object)?)
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
    Object(GetObjectDataResponse),
    Call(SuiCertifiedTransaction, SuiTransactionEffects),
    Transfer(
        // Skipping serialisation for elapsed time.
        #[serde(skip)] u128,
        SuiCertifiedTransaction,
        SuiTransactionEffects,
    ),
    TransferSui(SuiCertifiedTransaction, SuiTransactionEffects),
    Pay(SuiCertifiedTransaction, SuiTransactionEffects),
    Addresses(Vec<SuiAddress>),
    Objects(Vec<SuiObjectInfo>),
    SyncClientState,
    NewAddress((SuiAddress, String, SignatureScheme)),
    Gas(Vec<GasCoin>),
    SplitCoin(SuiTransactionResponse),
    MergeCoin(SuiTransactionResponse),
    Switch(SwitchResponse),
    ActiveAddress(Option<SuiAddress>),
    CreateExampleNFT(GetObjectDataResponse),
    SerializeTransferSui(String),
    ExecuteSignedTx(SuiTransactionResponse),
}

#[derive(Serialize, Clone, Debug)]
pub struct SwitchResponse {
    /// Active address
    pub address: Option<SuiAddress>,
    pub rpc: Option<String>,
    pub ws: Option<String>,
}

impl Display for SwitchResponse {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut writer = String::new();
        if let Some(addr) = self.address {
            writeln!(writer, "Active address switched to {}", addr)?;
        }
        if let Some(rpc) = &self.rpc {
            writeln!(writer, "Active RPC server switched to {}", rpc)?;
        }

        if let Some(ws) = &self.ws {
            writeln!(writer, "Active Websocket server switched to {}", ws)?;
        }
        write!(f, "{}", writer)
    }
}
