// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0
use crate::config::{AccountInfoConfig, AuthorityInfo, KeyPairConfig, WalletConfig};
use fastpay_core::authority_client::AuthorityClient;
use fastpay_core::client::{Client, ClientState};
use fastx_network::network::NetworkClient;
use fastx_types::base_types::{
    decode_address_hex, encode_address_hex, AuthorityName, FastPayAddress, ObjectID, PublicKeyBytes,
};
use fastx_types::committee::Committee;
use fastx_types::messages::{ExecutionStatus, OrderEffects};

use crate::utils::Config;
use fastx_types::error::FastPayError;
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::TypeTag;
use move_core_types::parser::{parse_transaction_argument, parse_type_tag};
use move_core_types::transaction_argument::{convert_txn_args, TransactionArgument};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use structopt::clap::AppSettings;
use structopt::StructOpt;
use tracing::*;

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
        /// Path to directory containing a Move package
        #[structopt(long)]
        path: String,

        /// ID of the gas object for gas payment, in 20 bytes Hex string
        #[structopt(long)]
        gas: ObjectID,
    },

    /// Call Move
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

    /// Transfer funds
    #[structopt(name = "transfer")]
    Transfer {
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
            WalletCommands::Publish { path, gas } => {
                // Find owner of gas object
                let (owner, owner_kf_path) = context.find_owner_and_key_file_path(gas)?;
                let client_state = context.get_or_create_client_state(&owner)?;
                let gas_obj_ref = *client_state
                    .object_refs()
                    .get(gas)
                    .expect("Gas object not found");

                let publish_order = client_state
                    .create_publish_order(path.to_string(), gas_obj_ref)
                    .unwrap();
                let signed_order = KeyPairConfig::sign_order(&owner_kf_path, publish_order)
                    .expect("Could not sign publish order");
               let (_, effects) = client_state.execute_signed_order(signed_order).await?;
                if effects.status != ExecutionStatus::Success {
                    error!("Error publishing module: {:#?}", effects.status);
                }
                show_object_effects(effects);
            }

            WalletCommands::Object { id, deep } => {
                // Pick the first (or any) account for use in finding obj info
                let (account, _) = context.find_owner_and_key_file_path(id)?;
                // Fetch the object ref
                let client_state = context.get_or_create_client_state(&account)?;
                let object_read = client_state.get_object_info(*id).await?;
                let object = object_read.object()?;
                println!("{}", object);
                if *deep {
                    println!("Full Info: {:#?}", object);
                }
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
                // Find owner of gas object
                let (sender, sender_kf_path) = context.find_owner_and_key_file_path(gas)?;
                let client_state = context.get_or_create_client_state(&sender)?;

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

                // Make the order
                let call_order = client_state
                    .create_call_order(
                        package_obj_ref,
                        module.to_owned(),
                        function.to_owned(),
                        type_args.clone(),
                        gas_obj_ref,
                        object_args_refs,
                        convert_txn_args(pure_args),
                        *gas_budget,
                    )
                    .unwrap();
                let signed_order = KeyPairConfig::sign_order(&sender_kf_path, call_order)
                    .expect("Could not sign call order");
                let (cert, effects) = client_state.execute_signed_order(signed_order).await?;

                println!("Cert: {:?}", cert);
                show_object_effects(effects);
            }

            WalletCommands::Transfer { to, object_id, gas } => {
                let (owner, owner_kf_path) = context.find_owner_and_key_file_path(gas)?;

                let client_state = context.get_or_create_client_state(&owner)?;
                info!("Starting transfer");
                let time_start = Instant::now();

                let transfer_order = client_state
                    .create_transfer_order(*object_id, *gas, *to)
                    .unwrap();
                let signed_order = KeyPairConfig::sign_order(&owner_kf_path, transfer_order)
                    .expect("Could not sign transfer order");

                let (cert, _) = client_state.execute_signed_order(signed_order).await?;

                let time_total = time_start.elapsed().as_micros();
                info!("Transfer confirmed after {} us", time_total);
                println!("{:?}", cert);
            }

            WalletCommands::Addresses => {
                let addr_strings: Vec<_> = context
                    .config
                    .accounts
                    .iter()
                    .map(|account| encode_address_hex(&account.address))
                    .collect();
                let addr_text = addr_strings.join("\n");
                println!("Showing {} results.", context.config.accounts.len());
                println!("{}", addr_text);
            }

            WalletCommands::Objects { address } => {
                let client_state = context.get_or_create_client_state(address)?;
                let object_refs = client_state.object_refs();
                println!("Showing {} results.", object_refs.len());
                for (obj_id, object_ref) in object_refs {
                    println!("{}: {:?}", obj_id, object_ref);
                }
            }

            WalletCommands::SyncClientState { address } => {
                let client_state = context.get_or_create_client_state(address)?;
                client_state.sync_client_state().await?;
            }
            WalletCommands::NewAddress => {
                let (path, address) = KeyPairConfig::create_and_get_public_key();
                context.config.accounts.push(AccountInfoConfig {
                    address,
                    key_file_path: path,
                });
                context.config.save()?;
                println!(
                    "Created new keypair for address : {}",
                    encode_address_hex(&address)
                );
            }
        }
        Ok(())
    }
}

fn show_object_effects(order_effects: OrderEffects) {
    if !order_effects.created.is_empty() {
        println!("Created Objects:");
        for (obj, _) in order_effects.created {
            println!("{:?} {:?} {:?}", obj.0, obj.1, obj.2);
        }
    }
    if !order_effects.mutated.is_empty() {
        println!("Mutated Objects:");
        for (obj, _) in order_effects.mutated {
            println!("{:?} {:?} {:?}", obj.0, obj.1, obj.2);
        }
    }
    if !order_effects.deleted.is_empty() {
        println!("Deleted Objects:");
        for obj in order_effects.deleted {
            println!("{:?} {:?} {:?}", obj.0, obj.1, obj.2);
        }
    }
}

fn make_authority_clients(
    authorities: &[AuthorityInfo],
    buffer_size: usize,
    send_timeout: Duration,
    recv_timeout: Duration,
) -> BTreeMap<AuthorityName, AuthorityClient> {
    let mut authority_clients = BTreeMap::new();
    for authority in authorities {
        let client = AuthorityClient::new(NetworkClient::new(
            authority.host.clone(),
            authority.base_port,
            buffer_size,
            send_timeout,
            recv_timeout,
        ));
        authority_clients.insert(authority.address, client);
    }
    authority_clients
}

pub struct WalletContext {
    pub config: WalletConfig,
    pub client_states: BTreeMap<FastPayAddress, ClientState<AuthorityClient>>,
}

impl WalletContext {
    pub fn new(config: WalletConfig) -> Self {
        Self {
            config,
            client_states: Default::default(),
        }
    }

    pub fn find_owner_and_key_file_path(
        &self,
        object_id: &ObjectID,
    ) -> Result<(FastPayAddress, String), FastPayError> {
        let addr = self
            .client_states
            .iter()
            .find_map(|(owner, client_state)| {
                if client_state.get_owned_objects().contains(object_id) {
                    Some(owner)
                } else {
                    None
                }
            })
            .copied()
            .ok_or(FastPayError::ObjectNotFound {
                object_id: *object_id,
            })?;
        Ok((
            addr,
            self.config
                .accounts
                .iter()
                .find(|a| a.address == addr)
                .expect("No account config found")
                .key_file_path
                .clone(),
        ))
    }

    pub fn get_or_create_client_state(
        &mut self,
        owner: &FastPayAddress,
    ) -> Result<&mut ClientState<AuthorityClient>, anyhow::Error> {
        Ok(if !self.client_states.contains_key(owner) {
            let new_client = self.create_client_state(owner)?;
            self.client_states.entry(*owner).or_insert(new_client)
        } else {
            self.client_states.get_mut(owner).unwrap()
        })
    }

    fn create_client_state(
        &self,
        owner: &FastPayAddress,
    ) -> Result<ClientState<AuthorityClient>, FastPayError> {
        let client_info = self.get_account_info(owner)?;

        let voting_rights = self
            .config
            .authorities
            .iter()
            .map(|authority| (authority.address, 1))
            .collect();
        let committee = Committee::new(voting_rights);
        let authority_clients = make_authority_clients(
            &self.config.authorities,
            self.config.buffer_size,
            self.config.send_timeout,
            self.config.recv_timeout,
        );
        let path = PathBuf::from(format!("{}/{:?}", self.config.db_folder_path, owner));
        ClientState::new(
            path,
            client_info.address,
            committee,
            authority_clients,
            BTreeMap::new(),
            BTreeMap::new(),
        )
    }

    pub fn get_account_info(
        &self,
        address: &FastPayAddress,
    ) -> Result<&AccountInfoConfig, FastPayError> {
        self.config
            .accounts
            .iter()
            .find(|info| &info.address == address)
            .ok_or(FastPayError::AccountNotFound)
    }
}
