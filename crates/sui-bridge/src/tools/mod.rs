// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::abi::EthSuiBridge;
use crate::config::read_key;
use crate::crypto::BridgeAuthorityPublicKeyBytes;
use crate::events::EmittedSuiToEthTokenBridgeV1;
use crate::sui_client::{self, SuiBridgeClient};
use crate::types::{
    AssetPriceUpdateAction, BlocklistCommitteeAction, BlocklistType, EmergencyAction,
    EmergencyActionType, EvmContractUpgradeAction, LimitUpdateAction, SuiToEthBridgeAction,
};
use crate::utils::{get_eth_signer_client, EthSigner};
use anyhow::anyhow;
use clap::*;
use ethers::providers::Middleware;
use ethers::types::Address as EthAddress;
use fastcrypto::encoding::Encoding;
use fastcrypto::encoding::Hex;
use fastcrypto::hash::{HashFunction, Keccak256};
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use sui_types::digests::TransactionDigest;
use std::path::PathBuf;
use std::sync::Arc;
use sui_config::Config;
use sui_sdk::{SuiClient, SuiClientBuilder};
use sui_types::base_types::ObjectRef;
use sui_types::base_types::SuiAddress;
use sui_types::bridge::BridgeChainId;
use sui_types::crypto::SuiKeyPair;

use crate::types::BridgeAction;

#[derive(Parser)]
#[clap(rename_all = "kebab-case")]
pub struct Args {
    #[clap(subcommand)]
    pub command: BridgeValidatorCommand,
}

#[derive(Parser)]
#[clap(rename_all = "kebab-case")]
pub enum BridgeValidatorCommand {
    #[clap(name = "create-bridge-validator-key")]
    CreateBridgeValidatorKey { path: PathBuf },
    #[clap(name = "create-bridge-client-key")]
    CreateBridgeClientKey {
        path: PathBuf,
        #[clap(name = "use-ecdsa", long)]
        use_ecdsa: bool,
    },
    #[clap(name = "create-bridge-node-config-template")]
    CreateBridgeNodeConfigTemplate {
        path: PathBuf,
        #[clap(name = "run-client", long)]
        run_client: bool,
    },
    /// Governance client to facilitate and execute Bridge governance actions
    #[clap(name = "governance")]
    Governance {
        /// Path of BridgeCliConfig
        #[clap(long = "config-path")]
        config_path: PathBuf,
        #[clap(long = "chain-id")]
        chain_id: u8,
        #[clap(subcommand)]
        cmd: GovernanceClientCommands,
    },
    /// Client to facilitate and execute Bridge actions
    #[clap(name = "client")]
    Client {
        /// Path of BridgeCliConfig
        #[clap(long = "config-path")]
        config_path: PathBuf,
        #[clap(subcommand)]
        cmd: BridgeClientCommands,
    },
}

#[derive(Parser)]
#[clap(rename_all = "kebab-case")]
pub enum GovernanceClientCommands {
    #[clap(name = "emergency-button")]
    EmergencyButton {
        #[clap(name = "nonce", long)]
        nonce: u64,
        #[clap(name = "action-type", long)]
        action_type: EmergencyActionType,
    },
    #[clap(name = "update-committee-blocklist")]
    UpdateCommitteeBlocklist {
        #[clap(name = "nonce", long)]
        nonce: u64,
        #[clap(name = "blocklist-type", long)]
        blocklist_type: BlocklistType,
        #[clap(name = "pubkey-hex", long)]
        pubkeys_hex: Vec<BridgeAuthorityPublicKeyBytes>,
    },
    #[clap(name = "update-limit")]
    UpdateLimit {
        #[clap(name = "nonce", long)]
        nonce: u64,
        #[clap(name = "sending-chain", long)]
        sending_chain: u8,
        #[clap(name = "new-usd-limit", long)]
        new_usd_limit: u64,
    },
    #[clap(name = "update-asset-price")]
    UpdateAssetPrice {
        #[clap(name = "nonce", long)]
        nonce: u64,
        #[clap(name = "token-id", long)]
        token_id: u8,
        #[clap(name = "new-usd-price", long)]
        new_usd_price: u64,
    },
    #[clap(name = "upgrade-evm-contract")]
    UpgradeEVMContract {
        #[clap(name = "nonce", long)]
        nonce: u64,
        #[clap(name = "proxy-address", long)]
        proxy_address: EthAddress,
        /// The address of the new implementation contract
        #[clap(name = "implementation-address", long)]
        implementation_address: EthAddress,
        /// Function selector with params types, e.g. `foo(uint256,bool,string)`
        #[clap(name = "function-selector", long)]
        function_selector: String,
        /// Params to be passed to the function, e.g. `420,false,hello`
        #[clap(name = "params", long)]
        params: Vec<String>,
    },
}

pub fn make_action(chain_id: BridgeChainId, cmd: &GovernanceClientCommands) -> BridgeAction {
    match cmd {
        GovernanceClientCommands::EmergencyButton { nonce, action_type } => {
            BridgeAction::EmergencyAction(EmergencyAction {
                nonce: *nonce,
                chain_id,
                action_type: *action_type,
            })
        }
        GovernanceClientCommands::UpdateCommitteeBlocklist {
            nonce,
            blocklist_type,
            pubkeys_hex,
        } => BridgeAction::BlocklistCommitteeAction(BlocklistCommitteeAction {
            nonce: *nonce,
            chain_id,
            blocklist_type: *blocklist_type,
            blocklisted_members: pubkeys_hex.clone(),
        }),
        GovernanceClientCommands::UpdateLimit {
            nonce,
            sending_chain,
            new_usd_limit,
        } => {
            let sending_chain_id =
                BridgeChainId::try_from(*sending_chain).expect("Invalid sending chain id");
            BridgeAction::LimitUpdateAction(LimitUpdateAction {
                nonce: *nonce,
                chain_id,
                sending_chain_id,
                new_usd_limit: *new_usd_limit,
            })
        }
        GovernanceClientCommands::UpdateAssetPrice {
            nonce,
            token_id,
            new_usd_price,
        } => BridgeAction::AssetPriceUpdateAction(AssetPriceUpdateAction {
            nonce: *nonce,
            chain_id,
            token_id: *token_id,
            new_usd_price: *new_usd_price,
        }),
        GovernanceClientCommands::UpgradeEVMContract {
            nonce,
            proxy_address,
            implementation_address,
            function_selector,
            params,
        } => BridgeAction::EvmContractUpgradeAction(EvmContractUpgradeAction {
            nonce: *nonce,
            chain_id,
            proxy_address: *proxy_address,
            new_impl_address: *implementation_address,
            call_data: encode_call_data(function_selector, params),
        }),
    }
}

fn encode_call_data(function_selector: &str, params: &Vec<String>) -> Vec<u8> {
    let left = function_selector
        .find('(')
        .expect("Invalid function selector, no left parentheses");
    let right = function_selector
        .find(')')
        .expect("Invalid function selector, no right parentheses");
    let param_types = function_selector[left + 1..right]
        .split(',')
        .map(|x| x.trim())
        .collect::<Vec<&str>>();

    assert_eq!(param_types.len(), params.len(), "Invalid number of params");

    let mut call_data = Keccak256::digest(function_selector).digest[0..4].to_vec();
    let mut tokens = vec![];
    for (param, param_type) in params.iter().zip(param_types.iter()) {
        match param_type.to_lowercase().as_str() {
            "uint256" => {
                tokens.push(ethers::abi::Token::Uint(
                    ethers::types::U256::from_dec_str(param).expect("Invalid U256"),
                ));
            }
            "bool" => {
                tokens.push(ethers::abi::Token::Bool(match param.as_str() {
                    "true" => true,
                    "false" => false,
                    _ => panic!("Invalid bool in params"),
                }));
            }
            "string" => {
                tokens.push(ethers::abi::Token::String(param.clone()));
            }
            // TODO: need to support more types if needed
            _ => panic!("Invalid param type"),
        }
    }
    if !tokens.is_empty() {
        call_data.extend(ethers::abi::encode(&tokens));
    }
    call_data
}

pub fn select_contract_address(
    config: &LoadedBridgeCliConfig,
    cmd: &GovernanceClientCommands,
) -> EthAddress {
    match cmd {
        GovernanceClientCommands::EmergencyButton { .. } => config.eth_bridge_proxy_address,
        GovernanceClientCommands::UpdateCommitteeBlocklist { .. } => {
            config.eth_bridge_committee_proxy_address
        }
        GovernanceClientCommands::UpdateLimit { .. } => config.eth_bridge_limiter_proxy_address,
        GovernanceClientCommands::UpdateAssetPrice { .. } => {
            config.eth_bridge_limiter_proxy_address
        }
        GovernanceClientCommands::UpgradeEVMContract { proxy_address, .. } => *proxy_address,
    }
}

#[serde_as]
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct BridgeCliConfig {
    /// Rpc url for Sui fullnode, used for query stuff and submit transactions.
    pub sui_rpc_url: String,
    /// Rpc url for Eth fullnode, used for query stuff.
    pub eth_rpc_url: String,
    /// Proxy address for SuiBridge deployed on Eth
    pub eth_bridge_proxy_address: EthAddress,
    /// Path of the file where private key is stored. The content could be any of the following:
    /// - Base64 encoded `flag || privkey` for ECDSA key
    /// - Base64 encoded `privkey` for Raw key
    /// - Hex encoded `privkey` for Raw key
    /// At leaset one of `sui_key_path` or `eth_key_path` must be provided.
    /// If only one is provided, it will be used for both Sui and Eth.
    pub sui_key_path: Option<PathBuf>,
    /// See `sui_key_path`. Must be Secp256k1 key.
    pub eth_key_path: Option<PathBuf>,
}

impl Config for BridgeCliConfig {}

pub struct LoadedBridgeCliConfig {
    /// Rpc url for Sui fullnode, used for query stuff and submit transactions.
    pub sui_rpc_url: String,
    /// Rpc url for Eth fullnode, used for query stuff.
    pub eth_rpc_url: String,
    /// Proxy address for SuiBridge deployed on Eth
    pub eth_bridge_proxy_address: EthAddress,
    /// Proxy address for BridgeCommittee deployed on Eth
    pub eth_bridge_committee_proxy_address: EthAddress,
    /// Proxy address for BridgeLimiter deployed on Eth
    pub eth_bridge_limiter_proxy_address: EthAddress,
    /// Key pair for Sui operations
    sui_key: SuiKeyPair,
    /// Key pair for Eth operations, must be Secp256k1 key
    // pub eth_key: SuiKeyPair,
    eth_signer: EthSigner,
}

impl LoadedBridgeCliConfig {
    pub async fn load(cli_config: BridgeCliConfig) -> anyhow::Result<Self> {
        if cli_config.eth_key_path.is_none() && cli_config.sui_key_path.is_none() {
            return Err(anyhow!(
                "At least one of `sui_key_path` or `eth_key_path` must be provided"
            ));
        }
        let sui_key = if let Some(sui_key_path) = &cli_config.sui_key_path {
            Some(read_key(sui_key_path, false)?)
        } else {
            None
        };
        let eth_key = if let Some(eth_key_path) = &cli_config.eth_key_path {
            let eth_key = read_key(eth_key_path, true)?;
            Some(eth_key)
        } else {
            None
        };
        let (eth_key, sui_key) = {
            if eth_key.is_none() {
                let sui_key = sui_key.unwrap();
                if !matches!(sui_key, SuiKeyPair::Secp256k1(_)) {
                    return Err(anyhow!("Eth key must be an ECDSA key"));
                }
                (sui_key.copy(), sui_key)
            } else if sui_key.is_none() {
                let eth_key = eth_key.unwrap();
                (eth_key.copy(), eth_key)
            } else {
                (eth_key.unwrap(), sui_key.unwrap())
            }
        };

        let provider = Arc::new(
            ethers::prelude::Provider::<ethers::providers::Http>::try_from(&cli_config.eth_rpc_url)
                .unwrap()
                .interval(std::time::Duration::from_millis(2000)),
        );
        let private_key = Hex::encode(eth_key.to_bytes_no_flag());
        let eth_signer = get_eth_signer_client(&cli_config.eth_rpc_url, &private_key).await?;
        let sui_bridge = EthSuiBridge::new(cli_config.eth_bridge_proxy_address, provider.clone());
        let eth_bridge_committee_proxy_address: EthAddress = sui_bridge.committee().call().await?;
        let eth_bridge_limiter_proxy_address: EthAddress = sui_bridge.limiter().call().await?;

        let eth_address = eth_signer.address();
        let eth_chain_id = provider.get_chainid().await?;
        let sui_address = SuiAddress::from(&sui_key.public());
        println!("Using Sui address: {:?}", sui_address);
        println!("Using Eth address: {:?}", eth_address);
        println!("Using Eth chain: {:?}", eth_chain_id);

        Ok(Self {
            sui_rpc_url: cli_config.sui_rpc_url,
            eth_rpc_url: cli_config.eth_rpc_url,
            eth_bridge_proxy_address: cli_config.eth_bridge_proxy_address,
            eth_bridge_committee_proxy_address,
            eth_bridge_limiter_proxy_address,
            sui_key,
            eth_signer,
        })
    }
}

impl LoadedBridgeCliConfig {
    pub fn eth_signer(self: &LoadedBridgeCliConfig) -> &EthSigner {
        &self.eth_signer
    }

    pub async fn get_sui_account_info(
        self: &LoadedBridgeCliConfig,
    ) -> anyhow::Result<(SuiKeyPair, SuiAddress, ObjectRef)> {
        let pubkey = self.sui_key.public();
        let sui_client_address = SuiAddress::from(&pubkey);
        let sui_sdk_client = SuiClientBuilder::default()
            .build(self.sui_rpc_url.clone())
            .await?;
        let gases = sui_sdk_client
            .coin_read_api()
            .get_coins(sui_client_address, None, None, None)
            .await?
            .data;
        // TODO: is 5 Sui a good number?
        let gas = gases
            .into_iter()
            .find(|coin| coin.balance >= 5_000_000_000)
            .ok_or(anyhow!("Did not find gas object with enough balance"))?;
        println!("Using Gas object: {}", gas.coin_object_id);
        Ok((self.sui_key.copy(), sui_client_address, gas.object_ref()))
    }
}

#[derive(Parser)]
#[clap(rename_all = "kebab-case")]
pub enum BridgeClientCommands {
    /// Default address used for commands when none specified
    // #[clap(name = "active-address")]
    // ClaimOnEth
    // /// Default environment used for commands when none specified
    // #[clap(name = "active-env")]
    // ActiveEnv,
    // /// Obtain the Addresses managed by the client.
    #[clap(name = "addresses")]
    ClaimOnEth {
        #[clap(long)]
        seq_num: u64,
    },

}

impl BridgeClientCommands {
    pub async fn handle_command(
        self,
        config: &LoadedBridgeCliConfig,
        // sui_client: SuiClient,
        sui_bridge_client: SuiBridgeClient,
    ) -> anyhow::Result<()> {
        match self {
            BridgeClientCommands::ClaimOnEth { seq_num } => {
                // let sui_chain_id = sui_bridge_client.get_bridge_summary().await?.chain_id;
                // let sigs = sui_bridge_client.get_token_transfer_action_onchain_signatures_until_success(sui_chain_id, seq_num).await;
                // if sigs.is_none() {
                //     println!("No signatures found for seq_num: {}, chain id: {}", seq_num);
                //     return Ok(());
                // }
                // let signatures = sigs.unwrap().into_iter()
                //     .map(|sig: Vec<u8>| ethers::types::Bytes::from(sig))
                //     .collect::<Vec<_>>();

                // let eth_sui_bridge = EthSuiBridge::new(
                //     config.eth_bridge_proxy_address,
                //     Arc::new(config.eth_signer().clone()),
                // );
                // let action = SuiToEthBridgeAction {
                //     sui_tx_digest: TransactionDigest::random(), // dummy, this is not used in conversion
                //     sui_tx_event_index: 0,  // dummy, this is not used in conversion
                //     sui_bridge_event: EmittedSuiToEthTokenBridgeV1 {
                //         nonce: u64,
                //         sui_chain_id: BridgeChainId,
                //         eth_chain_id: BridgeChainId,
                //         sui_address: SuiAddress,
                //         eth_address: EthAddress,
                //         token_id: u8,
                //         amount_sui_adjusted: u64,

                //     } 
                // };
                // let message = eth_sui_bridge::Message::from(sui_to_eth_bridge_action);
                // let tx = eth_sui_bridge.transfer_bridged_tokens_with_signatures(signatures, message);
                // let _eth_claim_tx_receipt = tx.send().await.unwrap().await.unwrap().unwrap();
                // info!("Sui to Eth bridge transfer claimed");
                Ok(())
            }
        }
    }
}


#[cfg(test)]
mod tests {
    use ethers::abi::FunctionExt;

    use super::*;

    #[tokio::test]
    async fn test_encode_call_data() {
        let abi_json = std::fs::read_to_string("abi/tests/mock_sui_bridge_v2.json").unwrap();
        let abi: ethers::abi::Abi = serde_json::from_str(&abi_json).unwrap();

        let function_selector = "initializeV2Params(uint256,bool,string)";
        let params = vec!["420".to_string(), "false".to_string(), "hello".to_string()];
        let call_data = encode_call_data(function_selector, &params);

        let function = abi
            .functions()
            .find(|f| {
                let selector = f.selector();
                call_data.starts_with(selector.as_ref())
            })
            .expect("Function not found");

        // Decode the data excluding the selector
        let tokens = function.decode_input(&call_data[4..]).unwrap();
        assert_eq!(
            tokens,
            vec![
                ethers::abi::Token::Uint(ethers::types::U256::from_dec_str("420").unwrap()),
                ethers::abi::Token::Bool(false),
                ethers::abi::Token::String("hello".to_string())
            ]
        )
    }
}
