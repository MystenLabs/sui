// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::config::{AccountInfo, Config, WalletConfig};
use core::fmt;
use std::fmt::{Debug, Display, Formatter};
use sui_core::authority_client::AuthorityClient;
use sui_core::client::{Client, ClientAddressManager, ClientState};
use sui_types::base_types::{decode_bytes_hex, ObjectID, ObjectRef, SuiAddress};
use sui_types::crypto::get_key_pair;
use sui_types::gas_coin::GasCoin;
use sui_types::messages::{CertifiedTransaction, ExecutionStatus, TransactionEffects};
use sui_types::object::ObjectRead::Exists;

use colored::Colorize;
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::TypeTag;
use move_core_types::parser::{parse_transaction_argument, parse_type_tag};
use move_core_types::transaction_argument::{convert_txn_args, TransactionArgument};
use serde::ser::Error;
use serde::Serialize;
use std::time::Instant;
use structopt::clap::AppSettings;
use structopt::StructOpt;
use sui_types::error::SuiError;
use sui_types::object::ObjectRead;
use tracing::info;

#[derive(StructOpt)]
#[structopt(name = "", rename_all = "kebab-case")]
#[structopt(setting(AppSettings::NoBinaryName))]
pub struct WalletOpts {
    #[structopt(subcommand)]
    pub command: WalletCommands,
    /// Return command outputs in json format.
    #[structopt(long, global = true)]
    pub json: bool,
}

#[derive(StructOpt)]
#[structopt(rename_all = "kebab-case")]
#[structopt(setting(AppSettings::NoBinaryName))]
pub enum WalletCommands {
    /// Get obj info
    #[structopt(name = "object")]
    Object {
        /// Object ID of the object to fetch
        #[structopt(long)]
        id: ObjectID,
    },

    /// Publish Move modules
    #[structopt(name = "publish")]
    Publish {
        /// Path to directory containing a Move package
        #[structopt(long)]
        path: String,

        /// ID of the gas object for gas payment, in 20 bytes Hex string
        #[structopt(long)]
        gas: ObjectID,

        /// gas budget for running module initializers
        #[structopt(default_value = "0")]
        gas_budget: u64,
    },

    /// Call Move function
    #[structopt(name = "call")]
    Call {
        /// Object ID of the package, which contains the module
        #[structopt(long)]
        package: ObjectID,
        /// The name of the module in the package
        #[structopt(long)]
        module: Identifier,
        /// Function name in module
        #[structopt(long)]
        function: Identifier,
        /// Function name in module
        #[structopt(long, parse(try_from_str = parse_type_tag))]
        type_args: Vec<TypeTag>,
        /// Object args object IDs
        #[structopt(long)]
        object_args: Vec<ObjectID>,
        /// Pure arguments to the functions, which conform to move_core_types::transaction_argument
        /// Special case formatting rules:
        /// Use one string with CSV token embedded, for example "54u8,0x43"
        /// When specifying FastX addresses, specify as vector. Example x\"01FE4E6F9F57935C5150A486B5B78AC2B94E2C5CD9352C132691D99B3E8E095C\"
        #[structopt(long, parse(try_from_str = parse_transaction_argument))]
        pure_args: Vec<TransactionArgument>,
        /// ID of the gas object for gas payment, in 20 bytes Hex string
        #[structopt(long)]
        gas: ObjectID,
        /// Gas budget for this call
        #[structopt(long)]
        gas_budget: u64,
    },

    /// Transfer an object
    #[structopt(name = "transfer")]
    Transfer {
        /// Recipient address
        #[structopt(long, parse(try_from_str = decode_bytes_hex))]
        to: SuiAddress,

        /// Object to transfer, in 20 bytes Hex string
        #[structopt(long)]
        object_id: ObjectID,

        /// ID of the gas object for gas payment, in 20 bytes Hex string
        #[structopt(long)]
        gas: ObjectID,
    },
    /// Synchronize client state with authorities.
    #[structopt(name = "sync")]
    SyncClientState {
        #[structopt(long, parse(try_from_str = decode_bytes_hex))]
        address: SuiAddress,
    },

    /// Obtain the Addresses managed by the wallet.
    #[structopt(name = "addresses")]
    Addresses,

    /// Generate new address and keypair.
    #[structopt(name = "new-address")]
    NewAddress,

    /// Obtain all objects owned by the address.
    #[structopt(name = "objects")]
    Objects {
        /// Address owning the objects
        #[structopt(long, parse(try_from_str = decode_bytes_hex))]
        address: SuiAddress,
    },

    /// Obtain all gas objects owned by the address.
    #[structopt(name = "gas")]
    Gas {
        /// Address owning the objects
        #[structopt(long, parse(try_from_str = decode_bytes_hex))]
        address: SuiAddress,
    },
}

impl WalletCommands {
    pub async fn execute(
        &mut self,
        context: &mut WalletContext,
    ) -> Result<WalletCommandResult, anyhow::Error> {
        Ok(match self {
            WalletCommands::Publish {
                path,
                gas,
                gas_budget,
            } => {
                // Find owner of gas object
                let sender = &context.address_manager.get_object_owner(*gas).await?;
                let client_state = context.get_or_create_client_state(sender)?;
                let gas_obj_ref = client_state.object_ref(*gas)?;

                let (cert, effects) = client_state
                    .publish(path.clone(), gas_obj_ref, *gas_budget)
                    .await?;
                WalletCommandResult::Publish(cert, effects)
            }

            WalletCommands::Object { id } => {
                // Fetch the object ref
                let object_read = context.address_manager.get_object_info(*id).await?;
                WalletCommandResult::Object(object_read)
            }
            WalletCommands::Call {
                package,
                module,
                function,
                type_args,
                object_args,
                pure_args,
                gas,
                gas_budget,
            } => {
                let sender = &context.address_manager.get_object_owner(*gas).await?;
                let client_state = context.get_or_create_client_state(sender)?;

                let package_obj_info = client_state.get_object_info(*package).await?;
                let package_obj_ref = package_obj_info.object()?.to_object_reference();

                // Fetch the object info for the gas obj
                let gas_obj_ref = client_state.object_ref(*gas)?;

                // Fetch the objects for the object args
                let mut object_args_refs = Vec::new();
                for obj_id in object_args {
                    let obj_info = client_state.get_object_info(*obj_id).await?;
                    object_args_refs.push(obj_info.object()?.to_object_reference());
                }

                let (cert, effects) = client_state
                    .move_call(
                        package_obj_ref,
                        module.to_owned(),
                        function.to_owned(),
                        type_args.clone(),
                        gas_obj_ref,
                        object_args_refs,
                        convert_txn_args(pure_args),
                        *gas_budget,
                    )
                    .await?;
                WalletCommandResult::Call(cert, effects)
            }

            WalletCommands::Transfer { to, object_id, gas } => {
                let from = &context.address_manager.get_object_owner(*gas).await?;
                let client_state = context.get_or_create_client_state(from)?;
                let time_start = Instant::now();
                let (cert, effects) = client_state.transfer_object(*object_id, *gas, *to).await?;
                let time_total = time_start.elapsed().as_micros();

                WalletCommandResult::Transfer(time_total, cert, effects)
            }

            WalletCommands::Addresses => WalletCommandResult::Addresses(
                context
                    .address_manager
                    .get_managed_address_states()
                    .keys()
                    .copied()
                    .collect(),
            ),

            WalletCommands::Objects { address } => {
                let client_state = context.get_or_create_client_state(address)?;
                WalletCommandResult::Objects(
                    client_state
                        .object_refs()
                        .map(|(_, object_ref)| object_ref)
                        .collect(),
                )
            }

            WalletCommands::SyncClientState { address } => {
                let client_state = context.get_or_create_client_state(address)?;
                client_state.sync_client_state().await?;
                WalletCommandResult::SyncClientState
            }
            WalletCommands::NewAddress => {
                let (address, key) = get_key_pair();
                context.config.accounts.push(AccountInfo {
                    address,
                    key_pair: key,
                });
                context.config.save()?;
                // Create an address to be managed
                context.get_or_create_client_state(&address)?;
                WalletCommandResult::NewAddress(address)
            }
            WalletCommands::Gas { address } => {
                let client_state = context.get_or_create_client_state(address)?;

                client_state.sync_client_state().await?;
                let object_ids = client_state.get_owned_objects();

                // TODO: generalize formatting of CLI
                info!(
                    " {0: ^40} | {1: ^10} | {2: ^11}",
                    "Object ID", "Version", "Gas Value"
                );
                info!("----------------------------------------------------------------------",);
                // TODO: We should ideally fetch the objects from local cache
                for obj in object_ids {
                    match context.address_manager.get_object_info(obj).await? {
                        Exists(_, o, _) => {
                            if let Some(v) = o.type_() {
                                if *v == GasCoin::type_() {
                                    // Okay to unwrap() since we already checked type
                                    let gas_coin =
                                        GasCoin::try_from(o.data.try_as_move().unwrap())?;
                                    info!(
                                        " {0: ^40} | {1: ^10} | {2: ^11}",
                                        gas_coin.id(),
                                        u64::from(gas_coin.version()),
                                        gas_coin.value()
                                    );
                                }
                            }
                        }
                        _ => continue,
                    }
                }
            }
        })
    }
}

pub struct WalletContext {
    pub config: WalletConfig,
    pub address_manager: ClientAddressManager<AuthorityClient>,
}

impl WalletContext {
    pub fn new(config: WalletConfig) -> Result<Self, anyhow::Error> {
        let path = config.db_folder_path.clone();
        let addresses = config
            .accounts
            .iter()
            .map(|info| info.address)
            .collect::<Vec<_>>();

        let committee = config.make_committee();
        let authority_clients = config.make_authority_clients();
        let mut context = Self {
            config,
            address_manager: ClientAddressManager::new(path, committee, authority_clients),
        };
        // Pre-populate client state for each address in the config.
        for address in addresses {
            context.get_or_create_client_state(&address)?;
        }
        Ok(context)
    }

    fn get_or_create_client_state(
        &mut self,
        owner: &SuiAddress,
    ) -> Result<&mut ClientState<AuthorityClient>, SuiError> {
        let kp = Box::pin(self.config.get_account_cfg_info(owner)?.key_pair.copy());
        self.address_manager.get_or_create_state_mut(*owner, kp)
    }
}

impl Display for WalletCommandResult {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            WalletCommandResult::Publish(cert, effects) => {
                if matches!(effects.status, ExecutionStatus::Failure { .. }) {
                    return Err(fmt::Error::custom(format!(
                        "Error publishing module: {:#?}",
                        effects.status
                    )));
                }
                vec![
                    "----- Certificate ----".bold().to_string(),
                    format!("{}", cert),
                    "----- Transaction Effects ----".bold().to_string(),
                    format!("{}", effects),
                ]
            }
            WalletCommandResult::Object(object_read) => {
                let object = object_read.object().map_err(fmt::Error::custom)?;
                vec![format!("{}", object)]
            }
            WalletCommandResult::Call(cert, effects) => {
                if matches!(effects.status, ExecutionStatus::Failure { .. }) {
                    return Err(fmt::Error::custom(format!(
                        "Error calling module: {:#?}",
                        effects.status
                    )));
                }
                vec![
                    "----- Certificate ----".bold().to_string(),
                    format!("{}", cert),
                    "----- Transaction Effects ----".bold().to_string(),
                    format!("{}", effects),
                ]
            }
            WalletCommandResult::Transfer(time_elapsed, cert, effects) => {
                if matches!(effects.status, ExecutionStatus::Failure { .. }) {
                    return Err(fmt::Error::custom(format!(
                        "Error transferring object: {:#?}",
                        effects.status
                    )));
                }
                vec![
                    format!("Transfer confirmed after {} us", time_elapsed),
                    "----- Certificate ----".bold().to_string(),
                    format!("{}", cert),
                    "----- Transaction Effects ----".bold().to_string(),
                    format!("{}", effects),
                ]
            }

            WalletCommandResult::Addresses(addresses) => {
                let mut results = Vec::new();
                results.push(format!("Showing {} results.", addresses.len()));
                results.extend(
                    addresses
                        .iter()
                        .map(|address| format!("{}", address))
                        .collect::<Vec<_>>(),
                );
                results
            }

            WalletCommandResult::Objects(object_refs) => {
                let mut results = Vec::new();
                results.push(format!("Showing {} results.", object_refs.len()));
                results.extend(
                    object_refs
                        .iter()
                        .map(|object_ref| format!("{:?}", object_ref))
                        .collect::<Vec<_>>(),
                );
                results
            }
            WalletCommandResult::SyncClientState => {
                vec!["Client state sync complete.".to_string()]
            }
            WalletCommandResult::NewAddress(address) => {
                vec![format!("Created new keypair for address : {}", &address)]
            }
        }
        .join("\n");
        write!(f, "{}", s)
    }
}

impl Debug for WalletCommandResult {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            WalletCommandResult::Object(object_read) => {
                let object = object_read.object().map_err(fmt::Error::custom)?;
                let layout = object_read.layout().map_err(fmt::Error::custom)?;
                object
                    .to_json(layout)
                    .map_err(fmt::Error::custom)?
                    .to_string()
            }
            _ => serde_json::to_string(self).map_err(fmt::Error::custom)?,
        };
        write!(f, "{}", s)
    }
}

impl WalletCommandResult {
    pub fn print(&self, pretty: bool) {
        let line = if pretty {
            format!("{}", self)
        } else {
            format!("{:?}", self)
        };
        // Log line by line
        for line in line.lines() {
            info!("{}", line)
        }
    }
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum WalletCommandResult {
    Publish(CertifiedTransaction, TransactionEffects),
    Object(ObjectRead),
    Call(CertifiedTransaction, TransactionEffects),
    Transfer(
        // Skipping serialisation for elapsed time.
        #[serde(skip)] u128,
        CertifiedTransaction,
        TransactionEffects,
    ),
    Addresses(Vec<SuiAddress>),
    Objects(Vec<ObjectRef>),
    SyncClientState,
    NewAddress(SuiAddress),
}
