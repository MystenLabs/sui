// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use jsonrpsee::ws_client::WsClientBuilder;
use jsonrpsee_http_client::{HttpClient, HttpClientBuilder};
use prometheus::Registry;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::num::NonZeroUsize;
use std::path::Path;
use sui::{
    client_commands::{SuiClientCommands, WalletContext},
    config::SuiClientConfig,
};
use sui_config::gateway::GatewayConfig;
use sui_config::genesis_config::GenesisConfig;
use sui_config::utils::get_available_port;
use sui_config::{Config, SUI_CLIENT_CONFIG, SUI_GATEWAY_CONFIG, SUI_NETWORK_CONFIG};
use sui_config::{PersistedConfig, SUI_KEYSTORE_FILENAME};
use sui_core::gateway_state::GatewayState;
use sui_node::SuiNode;

use jsonrpsee::ws_client::WsClient;
use sui_json_rpc::bcs_api::BcsApiImpl;
use sui_json_rpc::gateway_api::{
    GatewayReadApiImpl, GatewayWalletSyncApiImpl, RpcGatewayImpl, TransactionBuilderImpl,
};
use sui_json_rpc::{JsonRpcServerBuilder, ServerHandle};
use sui_keys::keystore::{AccountKeystore, FileBasedKeystore, Keystore};
use sui_sdk::{ClientType, SuiClient};
use sui_swarm::memory::{Swarm, SwarmBuilder};
use sui_types::base_types::SuiAddress;
use sui_types::crypto::KeypairTraits;
use sui_types::crypto::SuiKeyPair::Ed25519SuiKeyPair;
const NUM_VALIDAOTR: usize = 4;

pub struct FullNodeHandle {
    pub sui_node: SuiNode,
    pub sui_client: SuiClient,
    pub rpc_client: HttpClient,
    pub rpc_url: String,
    pub ws_client: Option<WsClient>,
    pub ws_url: Option<String>,
}

pub struct GatewayHandle {
    pub handle: ServerHandle,
    pub http_client: HttpClient,
    pub url: String,
}

pub struct TestCluster {
    pub swarm: Swarm,
    pub gateway_handle: Option<GatewayHandle>,
    pub fullnode_handle: Option<FullNodeHandle>,
    pub accounts: Vec<SuiAddress>,
    pub wallet: WalletContext,
}

impl TestCluster {
    pub fn rpc_client(&self) -> Option<&HttpClient> {
        if let Some(gateway_handle) = &self.gateway_handle {
            Some(&gateway_handle.http_client)
        } else if let Some(fullnode_handle) = &self.fullnode_handle {
            Some(&fullnode_handle.rpc_client)
        } else {
            None
        }
    }

    pub fn rpc_url(&self) -> Option<&str> {
        if let Some(gateway_handle) = &self.gateway_handle {
            Some(&gateway_handle.url)
        } else if let Some(fullnode_handle) = &self.fullnode_handle {
            Some(&fullnode_handle.rpc_url)
        } else {
            None
        }
    }

    pub fn wallet_mut(&mut self) -> &mut WalletContext {
        &mut self.wallet
    }

    // Helper function to get the 0th address in WalletContext
    pub fn get_address_0(&self) -> SuiAddress {
        self.wallet
            .config
            .keystore
            .addresses()
            .get(0)
            .cloned()
            .unwrap()
    }

    // Helper function to get the 1st address in WalletContext
    pub fn get_address_1(&self) -> SuiAddress {
        self.wallet
            .config
            .keystore
            .addresses()
            .get(1)
            .cloned()
            .unwrap()
    }
}

pub struct TestClusterBuilder {
    genesis_config: Option<GenesisConfig>,
    use_embedded_gateway: bool,
    fullnode_rpc_port: Option<u16>,
    fullnode_ws_port: Option<u16>,
    gateway_rpc_port: Option<u16>,
    do_not_build_fullnode: bool,
}

impl TestClusterBuilder {
    pub fn new() -> Self {
        TestClusterBuilder {
            genesis_config: None,
            use_embedded_gateway: false,
            fullnode_rpc_port: None,
            fullnode_ws_port: None,
            gateway_rpc_port: None,
            do_not_build_fullnode: false,
        }
    }

    pub fn set_fullnode_rpc_port(mut self, rpc_port: u16) -> Self {
        self.fullnode_rpc_port = Some(rpc_port);
        self
    }

    pub fn set_fullnode_ws_port(mut self, ws_port: u16) -> Self {
        self.fullnode_ws_port = Some(ws_port);
        self
    }

    // Only start a Gateway Server when this field is set.
    // This Gateway Server exposes RPC endpoints.
    pub fn set_gateway_rpc_port(mut self, rpc_port: u16) -> Self {
        self.gateway_rpc_port = Some(rpc_port);
        self
    }

    pub fn set_genesis_config(mut self, genesis_config: GenesisConfig) -> Self {
        self.genesis_config = Some(genesis_config);
        self
    }

    pub fn do_not_build_fullnode(mut self) -> Self {
        self.do_not_build_fullnode = true;
        self
    }

    // Let WalltContext to use an embedded Gateway
    // If set to false (default), WalletContext connects to an RPC endpoint,
    // which will be Gateway if `set_gateway_rpc_port` is set, otherwise
    // FullNode.
    pub fn use_embedded_gateway(mut self) -> Self {
        self.use_embedded_gateway = true;
        self
    }

    pub async fn build(self) -> anyhow::Result<TestCluster> {
        let use_embedded_gateway = self.use_embedded_gateway;
        let mut cluster = self.start_test_network_with_customized_ports().await?;

        if use_embedded_gateway {
            SuiClientCommands::SyncClientState {
                address: Some(cluster.get_address_0()),
            }
            .execute(cluster.wallet_mut())
            .await?;
        };

        Ok(cluster)
    }

    async fn start_test_network_with_customized_ports(self) -> Result<TestCluster, anyhow::Error> {
        // Where does wallet client connect to?
        // 1. `start_test_swarm_with_fullnodes` init the wallet to use an embedded
        //  Gateway. If `use_embedded_gateway` is true, the config remains intact.
        // 2. If `use_embedded_gateway` is false, and `gateway_rpc_port` is set,
        //  wallet connects to the Gateway rpc server.
        // 3. Otherwise, the wallet connects to Fullnode rpc server, unless
        //   `do_not_build_fullnode` is false, in which case the wallet is connected
        //  with the initial embedded Gateway.
        let swarm = Self::start_test_swarm_with_fullnodes(self.genesis_config).await?;
        let working_dir = swarm.dir();

        let mut wallet_conf: SuiClientConfig =
            PersistedConfig::read(&working_dir.join(SUI_CLIENT_CONFIG))?;

        // Before simtest support jsonrpc/websocket, keep the fullnode handle optional
        let fullnode_handle = if self.do_not_build_fullnode {
            None
        } else {
            let handle = start_a_fullnode_with_handle(
                &swarm,
                self.fullnode_rpc_port,
                self.fullnode_ws_port,
                false,
            )
            .await?;
            if !self.use_embedded_gateway {
                wallet_conf.client_type =
                    ClientType::RPC(handle.rpc_url.clone(), handle.ws_url.clone());
            }
            Some(handle)
        };

        let gateway_handle = if let Some(gateway_port) = self.gateway_rpc_port {
            let handle =
                Self::start_rpc_gateway(&working_dir.join(SUI_GATEWAY_CONFIG), Some(gateway_port))
                    .await?;
            let url = format!("http://{}", handle.local_addr());
            let http_client = HttpClientBuilder::default().build(url.clone())?;
            if !self.use_embedded_gateway {
                wallet_conf.client_type = ClientType::RPC(url.clone(), None);
            }
            Some(GatewayHandle {
                handle,
                http_client,
                url,
            })
        } else {
            None
        };

        let accounts = wallet_conf.keystore.addresses();

        wallet_conf
            .persisted(&working_dir.join(SUI_CLIENT_CONFIG))
            .save()?;

        let wallet_conf = swarm.dir().join(SUI_CLIENT_CONFIG);
        let wallet = WalletContext::new(&wallet_conf).await?;

        Ok(TestCluster {
            swarm,
            gateway_handle,
            fullnode_handle,
            accounts,
            wallet,
        })
    }

    /// Start a Swarm and set up WalletConfig with an embedded Gateway
    async fn start_test_swarm_with_fullnodes(
        genesis_config: Option<GenesisConfig>,
    ) -> Result<Swarm, anyhow::Error> {
        let mut builder: SwarmBuilder =
            Swarm::builder().committee_size(NonZeroUsize::new(NUM_VALIDAOTR).unwrap());

        if let Some(genesis_config) = genesis_config {
            builder = builder.initial_accounts_config(genesis_config);
        }

        let mut swarm = builder.build();
        swarm.launch().await?;

        let dir = swarm.dir();

        let network_path = dir.join(SUI_NETWORK_CONFIG);
        let wallet_path = dir.join(SUI_CLIENT_CONFIG);
        let keystore_path = dir.join(SUI_KEYSTORE_FILENAME);
        let db_folder_path = dir.join("client_db");
        let gateway_path = dir.join(SUI_GATEWAY_CONFIG);

        swarm.config().save(&network_path)?;
        let mut keystore = Keystore::from(FileBasedKeystore::new(&keystore_path)?);
        for key in &swarm.config().account_keys {
            keystore.add_key(Ed25519SuiKeyPair(key.copy()))?;
        }

        let validators = swarm.config().validator_set().to_owned();
        let active_address = keystore.addresses().first().cloned();

        GatewayConfig {
            db_folder_path: db_folder_path.clone(),
            validator_set: validators.clone(),
            ..Default::default()
        }
        .save(gateway_path)?;

        // Create wallet config with stated authorities port
        SuiClientConfig {
            keystore: Keystore::from(FileBasedKeystore::new(&keystore_path)?),
            client_type: ClientType::Embedded(GatewayConfig {
                db_folder_path,
                validator_set: validators,
                ..Default::default()
            }),
            active_address,
        }
        .save(&wallet_path)?;

        // Return network handle
        Ok(swarm)
    }

    async fn start_rpc_gateway(
        config_path: &Path,
        port: Option<u16>,
    ) -> Result<ServerHandle, anyhow::Error> {
        let server_addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port.unwrap_or(0));
        let mut server = JsonRpcServerBuilder::new_without_metrics_for_testing(false)?;

        let config = PersistedConfig::read(config_path)?;
        let client = GatewayState::create_client(&config, None)?;
        server.register_module(RpcGatewayImpl::new(client.clone()))?;
        server.register_module(GatewayReadApiImpl::new(client.clone()))?;
        server.register_module(TransactionBuilderImpl::new(client.clone()))?;
        server.register_module(GatewayWalletSyncApiImpl::new(client.clone()))?;
        server.register_module(BcsApiImpl::new_with_gateway(client.clone()))?;

        server.start(server_addr).await
    }
}

impl Default for TestClusterBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Note: the initial purpose of thi function is to make tests compatible with
/// simtest before it supports jsonrpc/ws. We should use `start_a_fullnode_with_handle`
/// once the support is added.
/// Start a fullnode for an existing Swarm and return FullNodeHandle
pub async fn start_a_fullnode(swarm: &Swarm, disable_ws: bool) -> Result<SuiNode, anyhow::Error> {
    let jsonrpc_server_url = format!("127.0.0.1:{}", get_available_port());
    let jsonrpc_addr: SocketAddr = jsonrpc_server_url.parse().unwrap();

    let mut config = swarm
        .config()
        .generate_fullnode_config_with_random_dir_name(true, false);
    config.json_rpc_address = jsonrpc_addr;

    if !disable_ws {
        let ws_server_url = format!("127.0.0.1:{}", get_available_port());
        let ws_addr: SocketAddr = ws_server_url.parse().unwrap();
        config.websocket_address = Some(ws_addr);
    };

    SuiNode::start(&config, Registry::new()).await
}

/// Note: before simtest supports jsonrpc/ws, use `start_a_fullnode` instead.
/// Start a fullnode for an existing Swarm and return FullNodeHandle
pub async fn start_a_fullnode_with_handle(
    swarm: &Swarm,
    rpc_port: Option<u16>,
    ws_port: Option<u16>,
    disable_ws: bool,
) -> Result<FullNodeHandle, anyhow::Error> {
    let jsonrpc_server_url = format!("127.0.0.1:{}", rpc_port.unwrap_or_else(get_available_port));
    let jsonrpc_addr: SocketAddr = jsonrpc_server_url.parse().unwrap();

    let mut config = swarm
        .config()
        .generate_fullnode_config_with_random_dir_name(true, false);
    config.json_rpc_address = jsonrpc_addr;

    let ws_url = if !disable_ws {
        let ws_server_url = format!("127.0.0.1:{}", ws_port.unwrap_or_else(get_available_port));
        let ws_addr: SocketAddr = ws_server_url.parse().unwrap();
        config.websocket_address = Some(ws_addr);
        Some(format!("ws://{}", ws_server_url))
    } else {
        None
    };

    let sui_node = SuiNode::start(&config, Registry::new()).await?;

    let rpc_url = format!("http://{}", jsonrpc_server_url);
    let rpc_client = HttpClientBuilder::default().build(&rpc_url)?;
    let sui_client = ClientType::RPC(rpc_url.clone(), ws_url.clone());

    let ws_client = if let Some(ws_url) = &ws_url {
        Some(WsClientBuilder::default().build(ws_url).await?)
    } else {
        None
    };

    // Check url is valid
    let sui_client = sui_client.init().await?;
    Ok(FullNodeHandle {
        sui_node,
        sui_client,
        rpc_client,
        rpc_url,
        ws_client,
        ws_url,
    })
}

/// A helper function to init a TestClusterBuilder depending on how the
/// test runs. Before simtest supports jsonrpc/ws, we use an embedded
/// Gateway.
pub fn init_cluster_builder_env_aware() -> TestClusterBuilder {
    let mut builder = TestClusterBuilder::new();
    if cfg!(msim) {
        builder = builder.use_embedded_gateway().do_not_build_fullnode();
    }
    builder
}
