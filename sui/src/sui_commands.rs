use crate::config::{
    AccountInfo, AuthorityInfo, AuthorityPrivateInfo, NetworkConfig, WalletConfig,
};
use crate::utils::Config;
use anyhow::anyhow;
use futures::future::join_all;
use std::collections::BTreeMap;
use std::net::TcpListener;
use std::sync::Arc;
use structopt::StructOpt;
use sui_core::authority::{AuthorityState, AuthorityStore};
use sui_core::authority_server::AuthorityServer;
use sui_types::base_types::{encode_bytes_hex, get_key_pair, ObjectID, SequenceNumber};
use sui_types::committee::Committee;
use sui_types::object::Object;
use tracing::{error, info};

const DEFAULT_WEIGHT: usize = 1;

#[derive(StructOpt)]
#[structopt(rename_all = "kebab-case")]
pub enum SuiCommand {
    /// Start sui network.
    #[structopt(name = "start")]
    Start,
    #[structopt(name = "genesis")]
    Genesis,
}

impl SuiCommand {
    pub async fn execute(&self, config: &mut NetworkConfig) -> Result<(), anyhow::Error> {
        match self {
            SuiCommand::Start => start_network(config).await,
            SuiCommand::Genesis => genesis(config).await,
        }
    }
}

async fn start_network(config: &NetworkConfig) -> Result<(), anyhow::Error> {
    if config.authorities.is_empty() {
        return Err(anyhow!(
            "No authority configured for the network, please run genesis."
        ));
    }
    info!(
        "Starting network with {} authorities",
        config.authorities.len()
    );
    let mut handles = Vec::new();

    let committee = Committee::new(
        config
            .authorities
            .iter()
            .map(|info| (info.key_pair.public(), DEFAULT_WEIGHT))
            .collect(),
    );

    for authority in &config.authorities {
        let server = make_server(authority, &committee, &[], config.buffer_size).await;
        handles.push(async move {
            let spawned_server = match server.spawn().await {
                Ok(server) => server,
                Err(err) => {
                    error!("Failed to start server: {}", err);
                    return;
                }
            };
            if let Err(err) = spawned_server.join().await {
                error!("Server ended with an error: {}", err);
            }
        });
    }
    info!("Started {} authorities", handles.len());
    join_all(handles).await;
    info!("All server stopped.");
    Ok(())
}

async fn genesis(config: &mut NetworkConfig) -> Result<(), anyhow::Error> {
    // We have created the config file, safe to unwrap the path here.
    let working_dir = &config.config_path().parent().unwrap().to_path_buf();
    if !config.authorities.is_empty() {
        return Err(anyhow!("Cannot run genesis on a existing network, please delete network config file and try again."));
    }

    let mut authorities = BTreeMap::new();
    let mut authority_info = Vec::new();
    let mut port_allocator = PortAllocator::new(10000);

    info!("Creating new authorities...");
    let authorities_db_path = working_dir.join("authorities_db");
    for _ in 0..4 {
        let (pub_key, key_pair) = get_key_pair();
        let info = AuthorityPrivateInfo {
            key_pair,
            host: "127.0.0.1".to_string(),
            port: port_allocator.next_port().expect("No free ports"),
            db_path: authorities_db_path.join(encode_bytes_hex(&pub_key)),
        };
        authority_info.push(AuthorityInfo {
            name: pub_key,
            host: info.host.clone(),
            base_port: info.port,
        });
        authorities.insert(pub_key, 1);
        config.authorities.push(info);
    }

    config.save()?;

    let mut new_addresses = Vec::new();
    let mut preload_objects = Vec::new();

    info!("Creating test objects...");
    for _ in 0..5 {
        let (pub_key, key_pair) = get_key_pair();
        let address = pub_key.into();
        new_addresses.push(AccountInfo { address, key_pair });
        for _ in 0..5 {
            let new_object = Object::with_id_owner_gas_coin_object_for_testing(
                ObjectID::random(),
                SequenceNumber::new(),
                address,
                1000,
            );
            preload_objects.push(new_object);
        }
    }
    let committee = Committee::new(authorities);

    // Make server state to persist the objects.
    let config_path = config.config_path();
    for authority in &config.authorities {
        make_server(authority, &committee, &preload_objects, config.buffer_size).await;
    }

    let wallet_path = working_dir.join("wallet.conf");
    let mut wallet_config = WalletConfig::create(&wallet_path)?;
    wallet_config.authorities = authority_info;
    wallet_config.accounts = new_addresses;
    wallet_config.db_folder_path = working_dir.join("client_db");
    wallet_config.save()?;

    info!("Network genesis completed.");
    info!("Network config file is stored in {:?}.", config_path);
    info!(
        "Wallet config file is stored in {:?}.",
        wallet_config.config_path()
    );

    Ok(())
}

async fn make_server(
    authority: &AuthorityPrivateInfo,
    committee: &Committee,
    pre_load_objects: &[Object],
    buffer_size: usize,
) -> AuthorityServer {
    let store = Arc::new(AuthorityStore::open(&authority.db_path, None));

    let state = AuthorityState::new_with_genesis_modules(
        committee.clone(),
        authority.key_pair.public(),
        Box::pin(authority.key_pair.copy()),
        store,
    )
    .await;

    for object in pre_load_objects {
        state.init_order_lock(object.to_object_reference()).await;
        state.insert_object(object.clone()).await;
    }

    AuthorityServer::new(authority.host.clone(), authority.port, buffer_size, state)
}

struct PortAllocator {
    next_port: u16,
}

impl PortAllocator {
    pub fn new(starting_port: u16) -> Self {
        Self {
            next_port: starting_port,
        }
    }
    fn next_port(&mut self) -> Option<u16> {
        for port in self.next_port..65535 {
            if TcpListener::bind(("127.0.0.1", port)).is_ok() {
                self.next_port = port + 1;
                return Some(port);
            }
        }
        None
    }
}
