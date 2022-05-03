// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    config::{Config, GatewayType, PersistedConfig, WalletConfig},
    keystore::Keystore,
    sui_json::{resolve_move_function_args, SuiJsonCallArg, SuiJsonValue},
};
use anyhow::anyhow;
use clap::*;
use colored::Colorize;
use core::fmt;
use move_core_types::{identifier::Identifier, language_storage::TypeTag, parser::parse_type_tag};
use serde::Serialize;
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::{Debug, Display, Formatter, Write},
    path::Path,
    sync::{Arc, RwLock},
    time::Instant,
};
use sui_adapter::adapter::resolve_and_type_check;
use sui_core::gateway_state::{
    gateway_responses::{MergeCoinResponse, PublishResponse, SplitCoinResponse, SwitchResponse},
    GatewayClient,
};
use sui_framework::build_move_package_to_bytes;
use sui_types::{
    base_types::{decode_bytes_hex, ObjectID, ObjectRef, SuiAddress},
    gas_coin::GasCoin,
    messages::{CallArg, CertifiedTransaction, ExecutionStatus, Transaction, TransactionEffects},
    object::{Object, ObjectRead, ObjectRead::Exists},
    SUI_FRAMEWORK_ADDRESS,
};
use tracing::info;

const EXAMPLE_NFT_NAME: &str = "Example NFT";
const EXAMPLE_NFT_DESCRIPTION: &str = "An NFT created by the wallet Command Line Tool";
const EXAMPLE_NFT_URL: &str = "ipfs://bafkreibngqhl3gaa7daob4i2vccziay2jjlp435cf66vhono7nrvww53ty";

#[derive(Parser)]
#[clap(name = "", rename_all = "kebab-case", no_binary_name = true)]
pub struct WalletOpts {
    #[clap(subcommand)]
    pub command: WalletCommands,
    /// Returns command outputs in JSON format.
    #[clap(long, global = true)]
    pub json: bool,
}

#[derive(StructOpt, Debug)]
#[clap(rename_all = "kebab-case", no_binary_name = true)]
pub enum WalletCommands {
    /// Switch active address and network(e.g., devnet, local rpc server)
    #[clap(name = "switch")]
    Switch {
        /// An Sui address to be used as the active address for subsequent
        /// commands.
        #[clap(long, parse(try_from_str = decode_bytes_hex))]
        address: Option<SuiAddress>,
        /// The gateway URL (e.g., local rpc server, devnet rpc server, etc) to be
        /// used for subsequent commands.
        #[clap(long, value_hint = ValueHint::Url)]
        gateway: Option<String>,
    },

    /// Default address used for commands when none specified
    #[clap(name = "active-address")]
    ActiveAddress {},

    /// Get obj info
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
        #[clap(long)]
        path: String,

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
        module: Identifier,
        /// Function name in module
        #[clap(long)]
        function: Identifier,
        /// Function name in module
        #[clap(
            long,
            parse(try_from_str = parse_type_tag),
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

    /// Transfer an object
    #[clap(name = "transfer")]
    Transfer {
        /// Recipient address
        #[clap(long, parse(try_from_str = decode_bytes_hex))]
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
    /// Synchronize client state with authorities.
    #[clap(name = "sync")]
    SyncClientState {
        #[clap(long, parse(try_from_str = decode_bytes_hex))]
        address: Option<SuiAddress>,
    },

    /// Obtain the Addresses managed by the wallet.
    #[clap(name = "addresses")]
    Addresses,

    /// Generate new address and keypair.
    #[clap(name = "new-address")]
    NewAddress,

    /// Obtain all objects owned by the address.
    #[clap(name = "objects")]
    Objects {
        /// Address owning the objects
        #[clap(long, parse(try_from_str = decode_bytes_hex))]
        address: Option<SuiAddress>,
    },

    /// Obtain all gas objects owned by the address.
    #[clap(name = "gas")]
    Gas {
        /// Address owning the objects
        #[clap(long, parse(try_from_str = decode_bytes_hex))]
        address: Option<SuiAddress>,
    },

    /// Split a coin object into multiple coins.
    SplitCoin {
        /// Coin to Split, in 20 bytes Hex string
        #[clap(long)]
        coin_id: ObjectID,
        /// Amount to split out from the coin
        #[clap(long, multiple_occurrences = false, multiple_values = true)]
        amounts: Vec<u64>,
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
}

pub struct SimpleTransactionSigner {
    pub keystore: Arc<RwLock<Box<dyn Keystore>>>,
}

impl WalletCommands {
    pub async fn execute(
        &mut self,
        context: &mut WalletContext,
    ) -> Result<WalletCommandResult, anyhow::Error> {
        let ret = Ok(match self {
            WalletCommands::Publish {
                path,
                gas,
                gas_budget,
            } => {
                let gas_object = context
                    .choose_gas_for_wallet(*gas, *gas_budget, BTreeSet::new())
                    .await?;
                let sender = gas_object.owner.get_owner_address()?;
                let gas_obj_ref = gas_object.compute_object_reference();

                let compiled_modules = build_move_package_to_bytes(Path::new(path), false)?;
                let data = context
                    .gateway
                    .publish(sender, compiled_modules, gas_obj_ref, *gas_budget)
                    .await?;
                let signature = context
                    .keystore
                    .read()
                    .unwrap()
                    .sign(&sender, &data.to_bytes())?;
                let response = context
                    .gateway
                    .execute_transaction(Transaction::new(data, signature))
                    .await?
                    .to_publish_response()?;

                WalletCommandResult::Publish(response)
            }

            WalletCommands::Object { id } => {
                // Fetch the object ref
                let object_read = context.gateway.get_object_info(*id).await?;
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
                let (cert, effects) = call_move(
                    package, module, function, type_args, gas, gas_budget, args, context,
                )
                .await?;
                WalletCommandResult::Call(cert, effects)
            }

            WalletCommands::Transfer {
                to,
                object_id,
                gas,
                gas_budget,
            } => {
                let obj = context
                    .gateway
                    .get_object_info(*object_id)
                    .await?
                    .object()?
                    .clone();
                let forbidden_gas_objects = BTreeSet::from([*object_id]);

                // If this isnt the active account, and no gas is specified, derive sender and gas from object to be sent
                let gas_object = if context.active_address()? != obj.owner.get_owner_address()?
                    && gas.is_none()
                {
                    context
                        .gas_for_owner_budget(
                            obj.owner.get_owner_address()?,
                            *gas_budget,
                            forbidden_gas_objects,
                        )
                        .await?
                        .1
                } else {
                    context
                        .choose_gas_for_wallet(*gas, *gas_budget, forbidden_gas_objects)
                        .await?
                };
                let from = gas_object.owner.get_owner_address()?;

                let time_start = Instant::now();

                let data = context
                    .gateway
                    .transfer_coin(from, *object_id, gas_object.id(), *gas_budget, *to)
                    .await?;
                let signature = context
                    .keystore
                    .read()
                    .unwrap()
                    .sign(&from, &data.to_bytes())?;
                let (cert, effects) = context
                    .gateway
                    .execute_transaction(Transaction::new(data, signature))
                    .await?
                    .to_effect_response()?;

                let time_total = time_start.elapsed().as_micros();

                if matches!(effects.status, ExecutionStatus::Failure { .. }) {
                    return Err(anyhow!("Error transferring object: {:#?}", effects.status));
                }
                WalletCommandResult::Transfer(time_total, cert, effects)
            }

            WalletCommands::Addresses => {
                WalletCommandResult::Addresses(context.config.accounts.clone())
            }

            WalletCommands::Objects { address } => {
                let address = match address {
                    Some(a) => *a,
                    None => context.active_address()?,
                };
                WalletCommandResult::Objects(context.gateway.get_owned_objects(address).await?)
            }

            WalletCommands::SyncClientState { address } => {
                let address = match address {
                    Some(a) => *a,
                    None => context.active_address()?,
                };
                context.gateway.sync_account_state(address).await?;
                WalletCommandResult::SyncClientState
            }
            WalletCommands::NewAddress => {
                let address = context.keystore.write().unwrap().add_random_key()?;
                context.config.accounts.push(address);
                context.config.save()?;
                WalletCommandResult::NewAddress(address)
            }
            WalletCommands::Gas { address } => {
                let address = match address {
                    Some(a) => *a,
                    None => context.active_address()?,
                };
                let coins = context
                    .gas_objects(address)
                    .await?
                    .iter()
                    // Ok to unwrap() since `get_gas_objects` guarantees gas
                    .map(|q| GasCoin::try_from(&q.1).unwrap())
                    .collect();
                WalletCommandResult::Gas(coins)
            }
            WalletCommands::SplitCoin {
                coin_id,
                amounts,
                gas,
                gas_budget,
            } => {
                let forbidden_gas_objects = BTreeSet::from([*coin_id]);
                let gas_object = context
                    .choose_gas_for_wallet(*gas, *gas_budget, forbidden_gas_objects)
                    .await?;
                let signer = gas_object.owner.get_owner_address()?;
                let data = context
                    .gateway
                    .split_coin(
                        signer,
                        *coin_id,
                        amounts.clone(),
                        gas_object.id(),
                        *gas_budget,
                    )
                    .await?;
                let signature = context
                    .keystore
                    .read()
                    .unwrap()
                    .sign(&signer, &data.to_bytes())?;
                let response = context
                    .gateway
                    .execute_transaction(Transaction::new(data, signature))
                    .await?
                    .to_split_coin_response()?;
                WalletCommandResult::SplitCoin(response)
            }
            WalletCommands::MergeCoin {
                primary_coin,
                coin_to_merge,
                gas,
                gas_budget,
            } => {
                let forbidden_gas_objects = BTreeSet::from([*primary_coin, *coin_to_merge]);
                let gas_object = context
                    .choose_gas_for_wallet(*gas, *gas_budget, forbidden_gas_objects)
                    .await?;

                let signer = gas_object.owner.get_owner_address()?;
                let data = context
                    .gateway
                    .merge_coins(
                        signer,
                        *primary_coin,
                        *coin_to_merge,
                        gas_object.id(),
                        *gas_budget,
                    )
                    .await?;
                let signature = context
                    .keystore
                    .read()
                    .unwrap()
                    .sign(&signer, &data.to_bytes())?;
                let response = context
                    .gateway
                    .execute_transaction(Transaction::new(data, signature))
                    .await?
                    .to_merge_coin_response()?;

                WalletCommandResult::MergeCoin(response)
            }
            WalletCommands::Switch { address, gateway } => {
                if let Some(addr) = address {
                    if !context.config.accounts.contains(addr) {
                        return Err(anyhow!("Address {} not managed by wallet", addr));
                    }
                    context.config.active_address = Some(*addr);
                    context.config.save()?;
                }

                if let Some(gateway) = gateway {
                    // TODO: handle embedded gateway
                    context.config.gateway = GatewayType::RPC(gateway.clone());
                    context.config.save()?;
                }

                if Option::is_none(address) && Option::is_none(gateway) {
                    return Err(anyhow!(
                        "No address or gateway specified. Please Specify one."
                    ));
                }

                WalletCommandResult::Switch(SwitchResponse {
                    address: *address,
                    gateway: gateway.clone(),
                })
            }
            WalletCommands::ActiveAddress {} => {
                WalletCommandResult::ActiveAddress(context.active_address().ok())
            }
            WalletCommands::CreateExampleNFT {
                name,
                description,
                url,
                gas,
                gas_budget,
            } => {
                let args_json = json!([
                    unwrap_or(name, EXAMPLE_NFT_NAME),
                    unwrap_or(description, EXAMPLE_NFT_DESCRIPTION),
                    unwrap_or(url, EXAMPLE_NFT_URL)
                ]);
                let mut args = vec![];
                for a in args_json.as_array().unwrap() {
                    args.push(SuiJsonValue::new(a.clone()).unwrap());
                }
                let (_, effects) = call_move(
                    &ObjectID::from(SUI_FRAMEWORK_ADDRESS),
                    &Identifier::new("DevNetNFT").unwrap(),
                    &Identifier::new("mint").unwrap(),
                    &[],
                    gas,
                    &gas_budget.unwrap_or(3000),
                    &args,
                    context,
                )
                .await?;
                let ((nft_id, _, _), _) = effects
                    .created
                    .first()
                    .ok_or_else(|| anyhow!("Failed to create NFT"))?;
                let object_read = context.gateway.get_object_info(*nft_id).await?;
                WalletCommandResult::CreateExampleNFT(object_read)
            }
        });
        ret
    }
}

pub struct WalletContext {
    pub config: PersistedConfig<WalletConfig>,
    pub keystore: Arc<RwLock<Box<dyn Keystore>>>,
    pub gateway: GatewayClient,
}

impl WalletContext {
    pub fn new(config_path: &Path) -> Result<Self, anyhow::Error> {
        let config: WalletConfig = PersistedConfig::read(config_path).map_err(|err| {
            err.context(format!(
                "Cannot open wallet config file at {:?}",
                config_path
            ))
        })?;
        let config = config.persisted(config_path);
        let keystore = Arc::new(RwLock::new(config.keystore.init()?));
        let gateway = config.gateway.init()?;
        let context = Self {
            config,
            keystore,
            gateway,
        };
        Ok(context)
    }
    pub fn active_address(&mut self) -> Result<SuiAddress, anyhow::Error> {
        if self.config.accounts.is_empty() {
            return Err(anyhow!(
                "No managed addresses. Create new address with `new-address` command."
            ));
        }

        // Ok to unwrap because we checked that config addresses not empty
        // Set it if not exists
        self.config.active_address = Some(
            self.config
                .active_address
                .unwrap_or(*self.config.accounts.get(0).unwrap()),
        );

        Ok(self.config.active_address.unwrap())
    }

    /// Get all the gas objects (and conveniently, gas amounts) for the address
    pub async fn gas_objects(
        &self,
        address: SuiAddress,
    ) -> Result<Vec<(u64, Object)>, anyhow::Error> {
        let object_refs = self.gateway.get_owned_objects(address).await?;

        // TODO: We should ideally fetch the objects from local cache
        let mut values_objects = Vec::new();
        for (id, _, _) in object_refs {
            match self.gateway.get_object_info(id).await? {
                Exists(_, o, _) => {
                    if matches!( o.type_(), Some(v)  if *v == GasCoin::type_()) {
                        // Okay to unwrap() since we already checked type
                        let gas_coin = GasCoin::try_from(o.data.try_as_move().unwrap())?;
                        values_objects.push((gas_coin.value(), o));
                    }
                }
                _ => continue,
            }
        }

        Ok(values_objects)
    }

    /// Choose ideal gas object based on the budget and provided gas if any
    async fn choose_gas_for_wallet(
        &mut self,
        specified_gas: Option<ObjectID>,
        budget: u64,
        forbidden_gas_objects: BTreeSet<ObjectID>,
    ) -> Result<Object, anyhow::Error> {
        Ok(match specified_gas {
            None => {
                let addr = self.active_address()?;
                self.gas_for_owner_budget(addr, budget, forbidden_gas_objects)
                    .await?
                    .1
            }
            Some(g) => {
                if forbidden_gas_objects.contains(&g) {
                    return Err(anyhow!(
                        "Gas {} cannot be used as payment and in transaction input",
                        g
                    ));
                }

                let gas_object_read = self.gateway.get_object_info(g).await?;
                // You could technically try to pay with a gas not owned by user.
                // Especially if one forgets to switch account
                // Allow it still
                gas_object_read.object()?.clone()
            }
        })
    }

    /// Find a gas object which fits the budget
    pub async fn gas_for_owner_budget(
        &self,
        address: SuiAddress,
        budget: u64,
        forbidden_gas_objects: BTreeSet<ObjectID>,
    ) -> Result<(u64, Object), anyhow::Error> {
        for o in self.gas_objects(address).await.unwrap() {
            if o.0 >= budget && !forbidden_gas_objects.contains(&o.1.id()) {
                return Ok(o);
            }
        }
        return Err(anyhow!(
            "No non-argument gas objects found with value >= budget {}",
            budget
        ));
    }
}

impl Display for WalletCommandResult {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut writer = String::new();
        match self {
            WalletCommandResult::Publish(response) => {
                write!(writer, "{}", response)?;
            }
            WalletCommandResult::Object(object_read) => {
                let object = unwrap_err_to_string(|| Ok(object_read.object()?));
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
            WalletCommandResult::SplitCoin(response) => {
                write!(writer, "{}", response)?;
            }
            WalletCommandResult::MergeCoin(response) => {
                write!(writer, "{}", response)?;
            }
            WalletCommandResult::Switch(response) => {
                write!(writer, "{}", response)?;
            }
            WalletCommandResult::ActiveAddress(response) => {
                match response {
                    Some(r) => write!(writer, "{}", r)?,
                    None => write!(writer, "None")?,
                };
            }
            WalletCommandResult::CreateExampleNFT(object_read) => {
                // TODO: display the content of the object
                let object = unwrap_err_to_string(|| Ok(object_read.object()?));
                writeln!(writer, "{}\n", "Successfully created an ExampleNFT:".bold())?;
                writeln!(writer, "{}", object)?;
            }
        }
        write!(f, "{}", writer)
    }
}

async fn call_move(
    package: &ObjectID,
    module: &Identifier,
    function: &Identifier,
    type_args: &[TypeTag],
    gas: &Option<ObjectID>,
    gas_budget: &u64,
    args: &[SuiJsonValue],
    context: &mut WalletContext,
) -> Result<(CertifiedTransaction, TransactionEffects), anyhow::Error> {
    let package_obj_info = context.gateway.get_object_info(*package).await?;
    let package_obj = package_obj_info.object().clone()?;
    let package_obj_ref = package_obj_info.reference().unwrap();

    // These steps can potentially be condensed and moved into the client/manager level
    // Extract the input args
    let json_args =
        resolve_move_function_args(package_obj, module.clone(), function.clone(), args.to_vec())?;

    // Fetch all the objects needed for this call
    let mut objects = BTreeMap::new();
    let mut args = Vec::with_capacity(json_args.len());
    for json_arg in json_args {
        args.push(match json_arg {
            SuiJsonCallArg::Object(id) => {
                let obj = context.gateway.get_object_info(id).await?.into_object()?;
                let arg = if obj.is_shared() {
                    CallArg::SharedObject(id)
                } else {
                    CallArg::ImmOrOwnedObject(obj.compute_object_reference())
                };
                objects.insert(id, obj);
                arg
            }
            SuiJsonCallArg::Pure(bytes) => CallArg::Pure(bytes),
        })
    }
    let forbidden_gas_objects = objects.keys().copied().collect();
    let gas_object = context
        .choose_gas_for_wallet(*gas, *gas_budget, forbidden_gas_objects)
        .await?;
    let sender = gas_object.owner.get_owner_address()?;

    // Pass in the objects for a deeper check
    let compiled_module = package_obj
        .data
        .try_as_package()
        .ok_or_else(|| anyhow!("Cannot get package from object"))?
        .deserialize_module(module)?;
    resolve_and_type_check(
        &objects,
        &compiled_module,
        function,
        type_args,
        args.clone(),
    )?;

    // Fetch the object info for the gas obj
    let gas_obj_ref = gas_object.compute_object_reference();

    let data = context
        .gateway
        .move_call(
            sender,
            package_obj_ref,
            module.to_owned(),
            function.to_owned(),
            type_args.to_owned(),
            gas_obj_ref,
            args,
            *gas_budget,
        )
        .await?;
    let signature = context
        .keystore
        .read()
        .unwrap()
        .sign(&sender, &data.to_bytes())?;
    let (cert, effects) = context
        .gateway
        .execute_transaction(Transaction::new(data, signature))
        .await?
        .to_effect_response()?;

    if matches!(effects.status, ExecutionStatus::Failure { .. }) {
        return Err(anyhow!("Error calling module: {:#?}", effects.status));
    }
    Ok((cert, effects))
}

fn unwrap_or<'a>(val: &'a mut Option<String>, default: &'a str) -> &'a str {
    match val {
        Some(v) => v,
        None => default,
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
        let s = unwrap_err_to_string(|| match self {
            WalletCommandResult::Object(object_read) => {
                let object = object_read.object()?;
                let layout = object_read.layout()?;
                Ok(object.to_json(layout)?.to_string())
            }
            _ => Ok(serde_json::to_string(self)?),
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

impl WalletCommandResult {
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
pub enum WalletCommandResult {
    Publish(PublishResponse),
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
    SplitCoin(SplitCoinResponse),
    MergeCoin(MergeCoinResponse),
    Switch(SwitchResponse),
    ActiveAddress(Option<SuiAddress>),
    CreateExampleNFT(ObjectRead),
}
