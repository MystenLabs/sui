// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::config::{Config, WalletConfig};
use crate::sui_json::{resolve_move_function_args, SuiJsonValue};
use core::fmt;
use std::fmt::{Debug, Display, Formatter};
use std::path::Path;
use sui_core::authority_client::AuthorityClient;
use sui_core::client::{Client, ClientAddressManager};
use sui_framework::build_move_package_to_bytes;
use sui_types::base_types::{decode_bytes_hex, ObjectID, ObjectRef, SuiAddress};
use sui_types::crypto::get_key_pair;
use sui_types::gas_coin::GasCoin;
use sui_types::messages::{CertifiedTransaction, ExecutionStatus, TransactionEffects};
use sui_types::move_package::resolve_and_type_check;
use sui_types::object::ObjectRead::Exists;

use crate::keystore::{Keystore, SuiKeystoreSigner};
use anyhow::anyhow;
use colored::Colorize;
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::TypeTag;
use move_core_types::parser::parse_type_tag;
use serde::ser::Error;
use serde::Serialize;
use std::fmt::Write;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use structopt::clap::AppSettings;
use structopt::StructOpt;
use sui_types::error::SuiResult;
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

        /// Gas budget for running module initializers
        #[structopt(long)]
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
        /// Simplified ordered args like in the function syntax
        /// ObjectIDs, Addresses must be hex strings
        #[structopt(long)]
        args: Vec<SuiJsonValue>,
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
                let sender = &context
                    .address_manager
                    .get_object_owner(*gas)
                    .await?
                    .get_single_owner_address()?;
                let gas_obj_ref = context
                    .address_manager
                    .get_object_info(*gas)
                    .await?
                    .into_object()?
                    .to_object_reference();

                let compiled_modules = build_move_package_to_bytes(Path::new(path))?;
                let (cert, effects) = context
                    .address_manager
                    .publish(*sender, compiled_modules, gas_obj_ref, *gas_budget)
                    .await?;

                if matches!(effects.status, ExecutionStatus::Failure { .. }) {
                    return Err(anyhow!("Error publishing module: {:#?}", effects.status));
                };
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
                gas,
                gas_budget,
                args,
            } => {
                let sender = context
                    .address_manager
                    .get_object_owner(*gas)
                    .await?
                    .get_single_owner_address()?;

                let package_obj_info = context.address_manager.get_object_info(*package).await?;
                let package_obj = package_obj_info.object().clone()?;
                let package_obj_ref = package_obj_info.reference().unwrap();

                // These steps can potentially be condensed and moved into the client/manager level
                // Extract the input args
                let (object_ids, pure_args) = resolve_move_function_args(
                    package_obj,
                    module.clone(),
                    function.clone(),
                    args.clone(),
                )?;

                // Fetch all the objects needed for this call
                let mut input_objs = vec![];
                for obj_id in object_ids.clone() {
                    input_objs.push(
                        context
                            .address_manager
                            .get_object_info(obj_id)
                            .await?
                            .into_object()?,
                    );
                }

                // Pass in the objects for a deeper check
                // We can technically move this to impl MovePackage
                resolve_and_type_check(
                    package_obj.clone(),
                    module,
                    function,
                    type_args,
                    input_objs,
                    pure_args.clone(),
                )?;

                // Fetch the object info for the gas obj
                let gas_obj_ref = context
                    .address_manager
                    .get_object_info(*gas)
                    .await?
                    .into_object()?
                    .to_object_reference();

                // Fetch the objects for the object args
                let mut object_args_refs = Vec::new();
                for obj_id in object_ids {
                    let obj_info = context.address_manager.get_object_info(obj_id).await?;
                    object_args_refs.push(obj_info.object()?.to_object_reference());
                }

                let (cert, effects) = context
                    .address_manager
                    .move_call(
                        sender,
                        package_obj_ref,
                        module.to_owned(),
                        function.to_owned(),
                        type_args.clone(),
                        gas_obj_ref,
                        object_args_refs,
                        vec![],
                        pure_args,
                        *gas_budget,
                    )
                    .await?;
                if matches!(effects.status, ExecutionStatus::Failure { .. }) {
                    return Err(anyhow!("Error calling module: {:#?}", effects.status));
                }
                WalletCommandResult::Call(cert, effects)
            }

            WalletCommands::Transfer { to, object_id, gas } => {
                let from = &context
                    .address_manager
                    .get_object_owner(*gas)
                    .await?
                    .get_single_owner_address()?;
                let time_start = Instant::now();
                let (cert, effects) = context
                    .address_manager
                    .transfer_object(*from, *object_id, *gas, *to)
                    .await?;
                let time_total = time_start.elapsed().as_micros();

                if matches!(effects.status, ExecutionStatus::Failure { .. }) {
                    return Err(anyhow!("Error transferring object: {:#?}", effects.status));
                }
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
                WalletCommandResult::Objects(context.address_manager.get_owned_objects(*address))
            }

            WalletCommands::SyncClientState { address } => {
                context.address_manager.sync_client_state(*address).await?;
                WalletCommandResult::SyncClientState
            }
            WalletCommands::NewAddress => {
                let (address, key) = get_key_pair();
                context.config.accounts.push(address);
                context.keystore.lock().unwrap().add_key(key)?;
                context.config.save()?;
                // Create an address to be managed
                context.create_account_state(&address)?;
                WalletCommandResult::NewAddress(address)
            }
            WalletCommands::Gas { address } => {
                context.address_manager.sync_client_state(*address).await?;
                let object_refs = context.address_manager.get_owned_objects(*address);

                // TODO: We should ideally fetch the objects from local cache
                let mut coins = Vec::new();
                for (id, _, _) in object_refs {
                    match context.address_manager.get_object_info(id).await? {
                        Exists(_, o, _) => {
                            if matches!( o.type_(), Some(v)  if *v == GasCoin::type_()) {
                                // Okay to unwrap() since we already checked type
                                let gas_coin = GasCoin::try_from(o.data.try_as_move().unwrap())?;
                                coins.push(gas_coin);
                            }
                        }
                        _ => continue,
                    }
                }
                WalletCommandResult::Gas(coins)
            }
        })
    }
}

pub struct WalletContext {
    pub config: WalletConfig,
    pub keystore: Arc<Mutex<Box<dyn Keystore>>>,
    pub address_manager: ClientAddressManager<AuthorityClient>,
}

impl WalletContext {
    pub fn new(config: WalletConfig) -> Result<Self, anyhow::Error> {
        let path = config.db_folder_path.clone();

        let committee = config.make_committee();
        let authority_clients = config.make_authority_clients();

        let keystore = Arc::new(Mutex::new(config.keystore.init()?));
        let accounts = config.accounts.clone();

        let mut context = Self {
            config,
            keystore,
            address_manager: ClientAddressManager::new(path, committee, authority_clients),
        };
        // Pre-populate client state for each address in the config.
        for address in accounts {
            context.create_account_state(&address)?;
        }
        Ok(context)
    }

    pub fn create_account_state(&mut self, owner: &SuiAddress) -> SuiResult {
        let signer = Box::pin(SuiKeystoreSigner::new(self.keystore.clone(), *owner));
        self.address_manager.create_account_state(*owner, signer)
    }
}

impl Display for WalletCommandResult {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut writer = String::new();
        match self {
            WalletCommandResult::Publish(cert, effects) => {
                write!(writer, "{}", write_cert_and_effects(cert, effects)?)?;
            }
            WalletCommandResult::Object(object_read) => {
                let object = object_read.object().map_err(fmt::Error::custom)?;
                writeln!(writer, "{}", object)?;
            }
            WalletCommandResult::Call(cert, effects) => {
                write!(writer, "{}", write_cert_and_effects(cert, effects)?)?;
            }
            WalletCommandResult::Transfer(time_elapsed, cert, effects) => {
                writeln!(writer, "Transfer confirmed after {} us", time_elapsed)?;
                write!(writer, "{}", write_cert_and_effects(cert, effects)?)?;
            }
            WalletCommandResult::Addresses(addresses) => {
                writeln!(writer, "Showing {} results.", addresses.len())?;
                for address in addresses {
                    writeln!(writer, "{}", address)?;
                }
            }
            WalletCommandResult::Objects(object_refs) => {
                writeln!(writer, "Showing {} results.", object_refs.len())?;
                for object_ref in object_refs {
                    writeln!(writer, "{:?}", object_ref)?;
                }
            }
            WalletCommandResult::SyncClientState => {
                writeln!(writer, "Client state sync complete.")?;
            }
            WalletCommandResult::NewAddress(address) => {
                writeln!(writer, "Created new keypair for address : {}", &address)?;
            }
            WalletCommandResult::Gas(gases) => {
                // TODO: generalize formatting of CLI
                writeln!(
                    writer,
                    " {0: ^40} | {1: ^10} | {2: ^11}",
                    "Object ID", "Version", "Gas Value"
                )?;
                writeln!(
                    writer,
                    "----------------------------------------------------------------------"
                )?;
                for gas in gases {
                    writeln!(
                        writer,
                        " {0: ^40} | {1: ^10} | {2: ^11}",
                        gas.id(),
                        u64::from(gas.version()),
                        gas.value()
                    )?;
                }
            }
        }
        write!(f, "{}", writer)
    }
}

fn write_cert_and_effects(
    cert: &CertifiedTransaction,
    effects: &TransactionEffects,
) -> Result<String, fmt::Error> {
    let mut writer = String::new();
    writeln!(writer, "{}", "----- Certificate ----".bold())?;
    write!(writer, "{}", cert)?;
    writeln!(writer, "{}", "----- Transaction Effects ----".bold())?;
    write!(writer, "{}", effects)?;
    Ok(writer)
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
    Gas(Vec<GasCoin>),
}
