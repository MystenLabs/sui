// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use jsonrpsee_http_client::{HttpClient, HttpClientBuilder};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::num::NonZeroUsize;
use std::path::Path;
use sui::{
    client_commands::{SuiClientCommands, WalletContext},
    config::SuiClientConfig,
};
use sui_config::gateway::GatewayConfig;
use sui_config::genesis_config::GenesisConfig;
use sui_config::{Config, SUI_CLIENT_CONFIG, SUI_GATEWAY_CONFIG, SUI_NETWORK_CONFIG};
use sui_config::{PersistedConfig, SUI_KEYSTORE_FILENAME};
use sui_core::gateway_state::GatewayState;

use sui_json_rpc::bcs_api::BcsApiImpl;
use sui_json_rpc::gateway_api::{
    GatewayReadApiImpl, GatewayWalletSyncApiImpl, RpcGatewayImpl, TransactionBuilderImpl,
};

use sui_json_rpc::{JsonRpcServerBuilder, ServerHandle};
use sui_sdk::crypto::KeystoreType;
use sui_sdk::{ClientType, SuiClient};
use sui_swarm::memory::{Swarm, SwarmBuilder};
use sui_types::base_types::SuiAddress;
use sui_types::crypto::KeypairTraits;
use sui_types::crypto::SuiKeyPair::Ed25519SuiKeyPair;
use sui_types::intent::ChainId;

const NUM_VALIDAOTR: usize = 4;

pub async fn start_test_network(
    genesis_config: Option<GenesisConfig>,
) -> Result<Swarm, anyhow::Error> {
    start_test_network_with_fullnodes(genesis_config, 0, None, None).await
}

pub async fn start_test_network_with_fullnodes(
    genesis_config: Option<GenesisConfig>,
    fullnode_count: usize,
    fullnode_port: Option<u16>,
    websocket_port: Option<u16>,
) -> Result<Swarm, anyhow::Error> {
    let chain_id: ChainId = genesis_config
        .as_ref()
        .map_or_else(|| ChainId::Testing, |config| config.chain_id);
    let mut builder: SwarmBuilder = Swarm::builder()
        .committee_size(NonZeroUsize::new(NUM_VALIDAOTR).unwrap())
        .with_fullnode_count(fullnode_count);
    if let Some(fullnode_port) = fullnode_port {
        builder =
            builder.with_fullnode_rpc_addr(format!("127.0.0.1:{}", fullnode_port).parse().unwrap());
    }
    if let Some(websocket_port) = websocket_port {
        builder = builder
            .with_websocket_rpc_addr(format!("127.0.0.1:{}", websocket_port).parse().unwrap())
    }
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
    let mut keystore = KeystoreType::File(keystore_path.clone()).init(&chain_id)?;
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
        keystore: KeystoreType::File(keystore_path),
        client_type: ClientType::Embedded(GatewayConfig {
            db_folder_path,
            validator_set: validators,
            ..Default::default()
        }),
        active_address,
        chain_id,
    }
    .save(&wallet_path)?;

    // Return network handle
    Ok(swarm)
}

// TODO make a buidler for this...
pub async fn setup_network_and_wallet() -> Result<(Swarm, WalletContext, SuiAddress), anyhow::Error>
{
    let swarm = start_test_network(None).await?;

    // Create Wallet context.
    let wallet_conf = swarm.dir().join(SUI_CLIENT_CONFIG);
    let mut context = WalletContext::new(&wallet_conf).await?;
    let address = context.keystore.addresses().first().cloned().unwrap();

    // Sync client to retrieve objects from the network.
    SuiClientCommands::SyncClientState {
        address: Some(address),
    }
    .execute(&mut context)
    .await?;
    Ok((swarm, context, address))
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

pub async fn start_rpc_test_network(
    genesis_config: Option<GenesisConfig>,
) -> Result<TestNetwork, anyhow::Error> {
    start_rpc_test_network_with_fullnode(genesis_config, 0, None, None, None).await
}

pub async fn start_rpc_test_network_with_fullnode(
    genesis_config: Option<GenesisConfig>,
    fullnode_count: usize,
    gateway_port: Option<u16>,
    fullnode_port: Option<u16>,
    websocket_port: Option<u16>,
) -> Result<TestNetwork, anyhow::Error> {
    let network = start_test_network_with_fullnodes(
        genesis_config,
        fullnode_count,
        fullnode_port,
        websocket_port,
    )
    .await?;
    let working_dir = network.dir();
    let rpc_server_handle =
        start_rpc_gateway(&working_dir.join(SUI_GATEWAY_CONFIG), gateway_port).await?;
    let mut wallet_conf: SuiClientConfig =
        PersistedConfig::read(&working_dir.join(SUI_CLIENT_CONFIG))?;
    let rpc_url = format!("http://{}", rpc_server_handle.local_addr());
    let accounts = wallet_conf.keystore.init(&ChainId::Testing)?.addresses();
    wallet_conf.client_type = ClientType::RPC(rpc_url.clone(), None);
    wallet_conf
        .persisted(&working_dir.join(SUI_CLIENT_CONFIG))
        .save()?;

    let http_client = HttpClientBuilder::default().build(rpc_url.clone())?;
    let gateway_client = SuiClient::new_rpc_client(&rpc_url, None).await?;
    Ok(TestNetwork {
        network,
        _rpc_server: rpc_server_handle,
        accounts,
        http_client,
        gateway_client,
        rpc_url,
    })
}

pub struct TestNetwork {
    pub network: Swarm,
    _rpc_server: ServerHandle,
    pub accounts: Vec<SuiAddress>,
    pub http_client: HttpClient,
    pub gateway_client: SuiClient,
    pub rpc_url: String,
}
