use fastpay::config::{AccountInfo, AuthorityInfo, AuthorityPrivateInfo, WalletConfig};
use fastpay_core::authority::{AuthorityState, AuthorityStore};
use fastpay_core::authority_server::AuthorityServer;
use fastx_network::transport;
use fastx_types::base_types::{get_key_pair, ObjectID, SequenceNumber};
use fastx_types::committee::Committee;
use fastx_types::object::Object;
use futures::future::join_all;
use portpicker::pick_unused_port;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::fs::read_to_string;
use std::path::PathBuf;
use std::process::exit;
use std::sync::Arc;
use std::time::Duration;
use structopt::StructOpt;
use tracing::error;
use tracing::subscriber::set_global_default;
use tracing_subscriber::EnvFilter;

#[derive(StructOpt)]
#[structopt(
    name = "FastX",
    about = "A Byzantine fault tolerant payments chain with low-latency finality and high throughput",
    rename_all = "kebab-case"
)]
struct FastXOpt {
    #[structopt(subcommand)]
    command: FastXCommand,
    #[structopt(long, default_value = "./network.conf")]
    config: String,
}

#[derive(StructOpt)]
#[structopt(rename_all = "kebab-case")]
pub enum FastXCommand {
    /// Start fastx network.
    #[structopt(name = "start")]
    Start,
    #[structopt(name = "genesis")]
    Genesis,
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let subscriber_builder =
        tracing_subscriber::fmt::Subscriber::builder().with_env_filter(env_filter);
    let subscriber = subscriber_builder.with_writer(std::io::stderr).finish();
    set_global_default(subscriber).expect("Failed to set subscriber");

    let options: FastXOpt = FastXOpt::from_args();
    let network_conf_path = options.config;
    let mut config =
        NetworkConfig::read_or_create(&network_conf_path).expect("Unable to read user accounts");

    match options.command {
        FastXCommand::Start => {
            if config.authorities.is_empty() {
                println!("No authority configured for the network, please run genesis.");
                exit(1);
            }
            println!(
                "Starting network with {} authorities",
                config.authorities.len()
            );
            let mut handles = Vec::new();

            let committee = Committee::new(
                config
                    .authorities
                    .iter()
                    .map(|info| (info.address, 1))
                    .collect(),
            );

            for authority in config.authorities {
                let server = make_server(&authority, &committee, &[], config.buffer_size).await;
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
            println!("Started {} authorities", handles.len());
            join_all(handles).await;
            println!("All server stopped.");
        }
        FastXCommand::Genesis => {
            if !config.authorities.is_empty() {
                println!("Cannot run genesis on a existing network, please delete network config file and try again.");
                exit(1);
            }

            let mut authorities = BTreeMap::new();
            let mut authority_info = Vec::new();

            println!("Creating new addresses...");
            for _ in 0..4 {
                let (address, key_pair) = get_key_pair();
                let info = AuthorityPrivateInfo {
                    address,
                    key_pair,
                    host: "127.0.0.1".to_string(),
                    port: pick_unused_port().expect("No free ports"),
                    db_path: format!("./authorities_db/{:?}", address),
                };
                authority_info.push(AuthorityInfo {
                    address,
                    host: info.host.clone(),
                    base_port: info.port,
                });
                authorities.insert(info.address, 1);
                config.authorities.push(info);
            }

            config.save()?;

            let mut new_addresses = Vec::new();
            let mut preload_objects = Vec::new();

            println!("Creating test objects...");
            for _ in 0..5 {
                let (address, key_pair) = get_key_pair();
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
            for authority in config.authorities {
                make_server(&authority, &committee, &preload_objects, config.buffer_size).await;
            }

            let wallet_config = WalletConfig {
                accounts: new_addresses,
                authorities: authority_info,
                send_timeout: Duration::from_micros(4000000),
                recv_timeout: Duration::from_micros(4000000),
                buffer_size: config.buffer_size,
                db_folder_path: "./client_db".to_string(),
                config_path: "./wallet.conf".to_string(),
            };
            wallet_config.save()?;

            println!("Network genesis completed.");
            println!("Network config file is stored in {}.", config.config_path);
            println!(
                "Wallet config file is stored in {}.",
                wallet_config.config_path
            );
        }
    };

    Ok(())
}

#[derive(Serialize, Deserialize)]
struct NetworkConfig {
    authorities: Vec<AuthorityPrivateInfo>,
    buffer_size: usize,
    #[serde(skip)]
    config_path: String,
}

impl NetworkConfig {
    pub fn read_or_create(path: &str) -> Result<Self, anyhow::Error> {
        let path_buf = PathBuf::from(path);
        Ok(if path_buf.exists() {
            let raw_data: String = read_to_string(path_buf)?.parse()?;
            let mut config: NetworkConfig = serde_json::from_str(&raw_data)?;
            config.config_path = path.to_string();
            config
        } else {
            let new_config = NetworkConfig {
                authorities: Vec::new(),
                buffer_size: transport::DEFAULT_MAX_DATAGRAM_SIZE.to_string().parse()?,
                config_path: path.to_string(),
            };
            new_config.write(path)?;
            new_config
        })
    }

    pub fn write(&self, path: &str) -> Result<(), anyhow::Error> {
        let config = serde_json::to_string_pretty(self).unwrap();
        fs::write(path, config).expect("Unable to write to wallet config file");
        Ok(())
    }

    pub fn save(&self) -> Result<(), anyhow::Error> {
        self.write(&*self.config_path)
    }
}

async fn make_server(
    authority: &AuthorityPrivateInfo,
    committee: &Committee,
    pre_load_objects: &[Object],
    buffer_size: usize,
) -> AuthorityServer {
    let path = PathBuf::from(authority.db_path.clone());
    let store = Arc::new(AuthorityStore::open(path, None));

    let state = AuthorityState::new_with_genesis_modules(
        committee.clone(),
        authority.address,
        authority.key_pair.copy(),
        store,
    )
    .await;

    for object in pre_load_objects {
        state.init_order_lock(object.to_object_reference()).await;
        state.insert_object(object.clone()).await;
    }

    AuthorityServer::new(authority.host.clone(), authority.port, buffer_size, state)
}
