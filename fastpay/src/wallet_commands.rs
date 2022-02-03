use crate::config::{AccountInfo, AuthorityInfo, MoveCallConfig, WalletConfig};
use fastpay_core::authority_client::AuthorityClient;
use fastpay_core::client::{Client, ClientState};
use fastx_network::network::NetworkClient;
use fastx_types::base_types::{
    decode_address_hex, encode_address_hex, get_key_pair, AuthorityName, FastPayAddress, ObjectID,
    PublicKeyBytes,
};
use fastx_types::committee::Committee;
use fastx_types::messages::{ExecutionStatus, ObjectInfoRequest, OrderEffects};

use fastx_types::error::FastPayError;
use move_core_types::transaction_argument::convert_txn_args;
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
pub enum ClientCommands {
    /// Get obj info
    #[structopt(name = "object")]
    Object {
        /// Object ID of the object to fetch
        obj_id: ObjectID,

        /// Deep inspection of object
        #[structopt(long)]
        deep: bool,
    },

    /// Publish Move modules
    #[structopt(name = "publish")]
    Publish {
        /// Path to directory containing a Move package
        path: String,

        /// ID of the gas object for gas payment, in 20 bytes Hex string
        gas_object_id: ObjectID,
    },

    /// Call Move
    #[structopt(name = "call")]
    Call { path: String },

    /// Transfer funds
    #[structopt(name = "transfer")]
    Transfer {
        /// Recipient address
        #[structopt(long, parse(try_from_str = decode_address_hex))]
        to: PublicKeyBytes,

        /// Object to transfer, in 20 bytes Hex string
        object_id: ObjectID,

        /// ID of the gas object for gas payment, in 20 bytes Hex string
        gas_object_id: ObjectID,
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

impl ClientCommands {
    pub async fn execute(&mut self, context: &mut WalletContext) -> Result<(), anyhow::Error> {
        match self {
            ClientCommands::Publish {
                path,
                gas_object_id,
            } => {
                // Find owner of gas object
                let owner = context.find_owner(gas_object_id)?;
                let client_state = context.get_or_create_client_state(&owner)?;
                publish(client_state, path.clone(), *gas_object_id).await;
            }

            ClientCommands::Object { obj_id, deep } => {
                // Pick the first (or any) account for use in finding obj info
                let account = context.find_owner(obj_id)?;
                // Fetch the object ref
                let client_state = context.get_or_create_client_state(&account)?;
                get_object_info(client_state, *obj_id, *deep).await;
            }

            ClientCommands::Call { path } => {
                let call_config = MoveCallConfig::read(path).unwrap();
                // Find owner of gas object
                let owner = context.find_owner(&call_config.gas_object_id)?;

                let client_state = context.get_or_create_client_state(&owner)?;

                // Fetch the object info for the package
                let package_obj_info_req = ObjectInfoRequest {
                    object_id: call_config.package_obj_id,
                    request_sequence_number: None,
                };
                let package_obj_info = client_state.get_object_info(package_obj_info_req).await?;
                let package_obj_ref = package_obj_info.object().unwrap().to_object_reference();

                // Fetch the object info for the gas obj
                let gas_obj_ref = *client_state
                    .object_refs()
                    .get(&call_config.gas_object_id)
                    .expect("Gas object not found");

                // Fetch the objects for the object args
                let mut object_args_refs = Vec::new();
                for obj_id in call_config.object_args_ids {
                    // Fetch the obj ref
                    let obj_info_req = ObjectInfoRequest {
                        object_id: obj_id,
                        request_sequence_number: None,
                    };

                    let obj_info = client_state.get_object_info(obj_info_req).await?;
                    object_args_refs.push(
                        obj_info
                            .object()
                            .unwrap_or_else(|| panic!("Could not find object {:?}", obj_id))
                            .to_object_reference(),
                    );
                }

                let pure_args = convert_txn_args(&call_config.pure_args);

                let call_ret = client_state
                    .move_call(
                        package_obj_ref,
                        call_config.module,
                        call_config.function,
                        call_config.type_args,
                        gas_obj_ref,
                        object_args_refs,
                        pure_args,
                        call_config.gas_budget,
                    )
                    .await
                    .unwrap();
                println!("Cert: {:?}", call_ret.0);
                show_object_effects(call_ret.1);
            }

            ClientCommands::Transfer {
                to,
                object_id,
                gas_object_id,
            } => {
                let owner = context.find_owner(gas_object_id)?;

                let client_state = context.get_or_create_client_state(&owner)?;
                info!("Starting transfer");
                let time_start = Instant::now();
                let cert = client_state
                    .transfer_object(*object_id, *gas_object_id, *to)
                    .await
                    .unwrap();
                let time_total = time_start.elapsed().as_micros();
                info!("Transfer confirmed after {} us", time_total);
                println!("{:?}", cert);
            }

            ClientCommands::Addresses => {
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

            ClientCommands::Objects { address } => {
                let client_state = context.get_or_create_client_state(address)?;
                let object_refs = client_state.object_refs();
                println!("Showing {} results.", object_refs.len());
                for (obj_id, object_ref) in object_refs {
                    println!("{}: {:?}", obj_id, object_ref);
                }
            }

            ClientCommands::SyncClientState { address } => {
                let client_state = context.get_or_create_client_state(address)?;
                client_state
                    .sync_client_state_with_random_authority()
                    .await?;
            }
            ClientCommands::NewAddress => {
                let (address, key) = get_key_pair();
                context.config.accounts.push(AccountInfo {
                    address,
                    key_pair: key,
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

async fn get_object_info(
    client_state: &mut ClientState<AuthorityClient>,
    obj_id: ObjectID,
    deep: bool,
) {
    // Fetch the object info for the object
    let obj_info_req = ObjectInfoRequest {
        object_id: obj_id,
        request_sequence_number: None,
    };
    if let Some(object) = client_state
        .get_object_info(obj_info_req)
        .await
        .unwrap()
        .object()
    {
        println!("Owner: {:#?}", object.owner);
        println!("Version: {:#?}", object.version().value());
        println!("ID: {:#?}", object.id());
        println!("Readonly: {:#?}", object.is_read_only());
        println!(
            "Type: {:#?}",
            object
                .data
                .type_()
                .map_or("Type Unwrap Failed".to_owned(), |type_| type_
                    .module
                    .as_ident_str()
                    .to_string())
        );
        if deep {
            println!("Full Info: {:#?}", object);
        }
    } else {
        panic!("Object with id {:?} not found", obj_id);
    }
}

async fn publish(
    client_state: &mut ClientState<AuthorityClient>,
    path: String,
    gas_object_id: ObjectID,
) {
    let gas_obj_ref = *client_state
        .object_refs()
        .get(&gas_object_id)
        .expect("Gas object not found");

    let pub_resp = client_state.publish(path, gas_obj_ref).await;

    match pub_resp {
        Ok(resp) => {
            if resp.1.status != ExecutionStatus::Success {
                error!("Error publishing module: {:#?}", resp.1.status);
            }
            let (_, effects) = resp;
            show_object_effects(effects);
        }
        Err(err) => error!("{:#?}", err),
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

    pub fn find_owner(&self, object_id: &ObjectID) -> Result<FastPayAddress, FastPayError> {
        self.client_states
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
            })
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
            client_info.key_pair.copy(),
            committee,
            authority_clients,
            BTreeMap::new(),
            BTreeMap::new(),
        )
    }

    pub fn get_account_info(&self, address: &FastPayAddress) -> Result<&AccountInfo, FastPayError> {
        self.config
            .accounts
            .iter()
            .find(|info| &info.address == address)
            .ok_or(FastPayError::AccountNotFound)
    }
}
