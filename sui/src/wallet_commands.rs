// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0
use crate::config::{AccountInfo, WalletConfig};
use sui_core::authority_client::AuthorityClient;
use sui_core::client::{Client, ClientAddressManager, ClientState};
use sui_network::network::NetworkClient;
use sui_types::base_types::{
    decode_address_hex, encode_address_hex, get_key_pair, AuthorityName, ObjectID, PublicKeyBytes,
    SuiAddress,
};
use sui_types::committee::Committee;
use sui_types::messages::ExecutionStatus;

use crate::utils::Config;
use anyhow::anyhow;
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::TypeTag;
use move_core_types::parser::{parse_transaction_argument, parse_type_tag};
use move_core_types::transaction_argument::{convert_txn_args, TransactionArgument};
use std::collections::BTreeMap;
use std::time::Instant;
use structopt::clap::AppSettings;
use structopt::StructOpt;
use sui_types::error::SuiError;
use tracing::info;

#[derive(StructOpt)]
#[structopt(
    name = "",
    about = "A Byzantine fault tolerant payments chain with low-latency finality and high throughput",
    rename_all = "kebab-case"
)]
#[structopt(setting(AppSettings::NoBinaryName))]
pub enum WalletCommands {
    /// Get obj info
    #[structopt(name = "object")]
    Object {
        /// Owner address
        #[structopt(long, parse(try_from_str = decode_address_hex))]
        owner: PublicKeyBytes,

        /// Object ID of the object to fetch
        #[structopt(long)]
        id: ObjectID,

        /// Deep inspection of object
        #[structopt(long)]
        deep: bool,
    },

    /// Publish Move modules
    #[structopt(name = "publish")]
    Publish {
        /// Sender address
        #[structopt(long, parse(try_from_str = decode_address_hex))]
        sender: PublicKeyBytes,

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

    /// Call Move
    #[structopt(name = "call")]
    Call {
        /// Sender address
        #[structopt(long, parse(try_from_str = decode_address_hex))]
        sender: PublicKeyBytes,
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

    /// Transfer funds
    #[structopt(name = "transfer")]
    Transfer {
        /// Sender address
        #[structopt(long, parse(try_from_str = decode_address_hex))]
        from: PublicKeyBytes,

        /// Recipient address
        #[structopt(long, parse(try_from_str = decode_address_hex))]
        to: PublicKeyBytes,

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
        #[structopt(long, parse(try_from_str = decode_address_hex))]
        address: PublicKeyBytes,
    },

    /// Obtain the Account Addresses managed by the wallet.
    #[structopt(name = "addresses")]
    Addresses,

    /// Generate new address and keypair.
    #[structopt(name = "new-address")]
    NewAddress,

    /// Obtain all objects owned by the account address.
    #[structopt(name = "objects")]
    Objects {
        /// Address of the account
        #[structopt(long, parse(try_from_str = decode_address_hex))]
        address: PublicKeyBytes,
    },
}

impl WalletCommands {
    pub async fn execute(&mut self, context: &mut WalletContext) -> Result<(), anyhow::Error> {
        match self {
            WalletCommands::Publish {
                sender,
                path,
                gas,
                gas_budget,
            } => {
                // Find owner of gas object
                let client_state = context.get_or_create_client_state(sender)?;
                let gas_obj_ref = *client_state
                    .object_refs()
                    .get(gas)
                    .ok_or(anyhow!("Gas object not found"))?;

                let (_, effects) = client_state
                    .publish(path.clone(), gas_obj_ref, *gas_budget)
                    .await?;

                if !matches!(effects.status, ExecutionStatus::Failure { .. }) {
                    return Err(anyhow!("Error publishing module: {:#?}", effects.status));
                }
                info!("{}", effects);
            }

            WalletCommands::Object { id, deep, owner } => {
                // Fetch the object ref
                let client_state = context.get_or_create_client_state(owner)?;
                let object_read = client_state.get_object_info(*id).await?;
                let object = object_read.object()?;
                if *deep {
                    let layout = object_read.layout()?;
                    info!("{}", object.to_json(layout)?);
                } else {
                    info!("{}", object);
                }
            }
            WalletCommands::Call {
                sender,
                package,
                module,
                function,
                type_args,
                object_args,
                pure_args,
                gas,
                gas_budget,
            } => {
                let client_state = context.get_or_create_client_state(sender)?;

                let package_obj_info = client_state.get_object_info(*package).await?;
                let package_obj_ref = package_obj_info.object().unwrap().to_object_reference();

                // Fetch the object info for the gas obj
                let gas_obj_ref = *client_state
                    .object_refs()
                    .get(gas)
                    .expect("Gas object not found");

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
                info!("Cert: {:?}", cert);
                info!("{}", effects);
            }

            WalletCommands::Transfer {
                to,
                object_id,
                gas,
                from,
            } => {
                let client_state = context.get_or_create_client_state(from)?;
                info!("Starting transfer");
                let time_start = Instant::now();
                let cert = client_state
                    .transfer_object(*object_id, *gas, *to)
                    .await
                    .unwrap();
                let time_total = time_start.elapsed().as_micros();
                info!("Transfer confirmed after {} us", time_total);
                info!("{:?}", cert);
            }

            WalletCommands::Addresses => {
                info!(
                    "Showing {} results.",
                    context.address_manager.get_managed_address_states().len()
                );
                for address in context.address_manager.get_managed_address_states().keys() {
                    info!("{}", encode_address_hex(address));
                }
            }

            WalletCommands::Objects { address } => {
                let client_state = context.get_or_create_client_state(address)?;
                let object_refs = client_state.object_refs();
                info!("Showing {} results.", object_refs.len());
                for (obj_id, object_ref) in object_refs {
                    info!("{}: {:?}", obj_id, object_ref);
                }
            }

            WalletCommands::SyncClientState { address } => {
                let client_state = context.get_or_create_client_state(address)?;
                client_state.sync_client_state().await?;
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
                info!(
                    "Created new keypair for address : {}",
                    encode_address_hex(&address)
                );
            }
        }
        Ok(())
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
        let mut context = Self {
            config,
            address_manager: ClientAddressManager::new(path)?,
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
        let kp = Box::pin(self.get_account_cfg_info(owner)?.key_pair.copy());
        let voting_rights = self
            .config
            .authorities
            .iter()
            .map(|authority| (authority.address, 1))
            .collect();
        let committee = Committee::new(voting_rights);
        let authority_clients = self.make_authority_clients();
        self.address_manager
            .get_or_create_state_mut(*owner, kp, committee, authority_clients)
    }

    fn make_authority_clients(&self) -> BTreeMap<AuthorityName, AuthorityClient> {
        let mut authority_clients = BTreeMap::new();
        for authority in &self.config.authorities {
            let client = AuthorityClient::new(NetworkClient::new(
                authority.host.clone(),
                authority.base_port,
                self.config.buffer_size,
                self.config.send_timeout,
                self.config.recv_timeout,
            ));
            authority_clients.insert(authority.address, client);
        }
        authority_clients
    }

    pub fn get_account_cfg_info(&self, address: &SuiAddress) -> Result<&AccountInfo, SuiError> {
        self.config
            .accounts
            .iter()
            .find(|info| &info.address == address)
            .ok_or(SuiError::AccountNotFound)
    }
}
