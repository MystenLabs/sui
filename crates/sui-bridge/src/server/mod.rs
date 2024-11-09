// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#![allow(clippy::inconsistent_digit_grouping)]
use crate::with_metrics;
use crate::{
    crypto::BridgeAuthorityPublicKeyBytes,
    error::BridgeError,
    metrics::BridgeMetrics,
    server::handler::{BridgeRequestHandler, BridgeRequestHandlerTrait},
    types::{
        AddTokensOnEvmAction, AddTokensOnSuiAction, AssetPriceUpdateAction,
        BlocklistCommitteeAction, BlocklistType, BridgeAction, EmergencyAction,
        EmergencyActionType, EvmContractUpgradeAction, LimitUpdateAction, SignedBridgeAction,
    },
};
use axum::{
    extract::{Path, State},
    Json,
};
use axum::{http::StatusCode, routing::get, Router};
use ethers::types::Address as EthAddress;
use fastcrypto::ed25519::Ed25519PublicKey;
use fastcrypto::{
    encoding::{Encoding, Hex},
    traits::ToFromBytes,
};
use std::sync::Arc;
use std::{net::SocketAddr, str::FromStr};
use sui_types::{bridge::BridgeChainId, TypeTag};
use tracing::{info, instrument};

pub mod governance_verifier;
pub mod handler;

#[cfg(any(feature = "test-utils", test))]
pub(crate) mod mock_handler;

pub const APPLICATION_JSON: &str = "application/json";

pub const PING_PATH: &str = "/ping";
pub const METRICS_KEY_PATH: &str = "/metrics_pub_key";

// Important: for BridgeActions, the paths need to match the ones in bridge_client.rs
pub const ETH_TO_SUI_TX_PATH: &str = "/sign/bridge_tx/eth/sui/:tx_hash/:event_index";
pub const SUI_TO_ETH_TX_PATH: &str = "/sign/bridge_tx/sui/eth/:tx_digest/:event_index";
pub const COMMITTEE_BLOCKLIST_UPDATE_PATH: &str =
    "/sign/update_committee_blocklist/:chain_id/:nonce/:type/:keys";
pub const EMERGENCY_BUTTON_PATH: &str = "/sign/emergency_button/:chain_id/:nonce/:type";
pub const LIMIT_UPDATE_PATH: &str =
    "/sign/update_limit/:chain_id/:nonce/:sending_chain_id/:new_usd_limit";
pub const ASSET_PRICE_UPDATE_PATH: &str =
    "/sign/update_asset_price/:chain_id/:nonce/:token_id/:new_usd_price";
pub const EVM_CONTRACT_UPGRADE_PATH_WITH_CALLDATA: &str =
    "/sign/upgrade_evm_contract/:chain_id/:nonce/:proxy_address/:new_impl_address/:calldata";
pub const EVM_CONTRACT_UPGRADE_PATH: &str =
    "/sign/upgrade_evm_contract/:chain_id/:nonce/:proxy_address/:new_impl_address";
pub const ADD_TOKENS_ON_SUI_PATH: &str =
    "/sign/add_tokens_on_sui/:chain_id/:nonce/:native/:token_ids/:token_type_names/:token_prices";
pub const ADD_TOKENS_ON_EVM_PATH: &str =
    "/sign/add_tokens_on_evm/:chain_id/:nonce/:native/:token_ids/:token_addresses/:token_sui_decimals/:token_prices";

// BridgeNode's public metadata that is accessible via the `/ping` endpoint.
// Be careful with what to put here, as it is public.
#[derive(serde::Serialize)]
pub struct BridgeNodePublicMetadata {
    pub version: &'static str,
    pub metrics_pubkey: Option<Arc<Ed25519PublicKey>>,
}

impl BridgeNodePublicMetadata {
    pub fn new(version: &'static str, metrics_pubkey: Ed25519PublicKey) -> Self {
        Self {
            version,
            metrics_pubkey: Some(metrics_pubkey.into()),
        }
    }

    pub fn empty_for_testing() -> Self {
        Self {
            version: "testing",
            metrics_pubkey: None,
        }
    }
}

pub fn run_server(
    socket_address: &SocketAddr,
    handler: BridgeRequestHandler,
    metrics: Arc<BridgeMetrics>,
    metadata: Arc<BridgeNodePublicMetadata>,
) -> tokio::task::JoinHandle<()> {
    let socket_address = *socket_address;
    tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(socket_address).await.unwrap();
        axum::serve(
            listener,
            make_router(Arc::new(handler), metrics, metadata).into_make_service(),
        )
        .await
        .unwrap();
    })
}

pub(crate) fn make_router(
    handler: Arc<impl BridgeRequestHandlerTrait + Sync + Send + 'static>,
    metrics: Arc<BridgeMetrics>,
    metadata: Arc<BridgeNodePublicMetadata>,
) -> Router {
    Router::new()
        .route("/", get(health_check))
        .route(PING_PATH, get(ping))
        .route(METRICS_KEY_PATH, get(metrics_key_fetch))
        .route(ETH_TO_SUI_TX_PATH, get(handle_eth_tx_hash))
        .route(SUI_TO_ETH_TX_PATH, get(handle_sui_tx_digest))
        .route(
            COMMITTEE_BLOCKLIST_UPDATE_PATH,
            get(handle_update_committee_blocklist_action),
        )
        .route(EMERGENCY_BUTTON_PATH, get(handle_emergency_action))
        .route(LIMIT_UPDATE_PATH, get(handle_limit_update_action))
        .route(
            ASSET_PRICE_UPDATE_PATH,
            get(handle_asset_price_update_action),
        )
        .route(EVM_CONTRACT_UPGRADE_PATH, get(handle_evm_contract_upgrade))
        .route(
            EVM_CONTRACT_UPGRADE_PATH_WITH_CALLDATA,
            get(handle_evm_contract_upgrade_with_calldata),
        )
        .route(ADD_TOKENS_ON_SUI_PATH, get(handle_add_tokens_on_sui))
        .route(ADD_TOKENS_ON_EVM_PATH, get(handle_add_tokens_on_evm))
        .with_state((handler, metrics, metadata))
}

impl axum::response::IntoResponse for BridgeError {
    // TODO: distinguish client error.
    fn into_response(self) -> axum::response::Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Something went wrong: {:?}", self),
        )
            .into_response()
    }
}

impl<E> From<E> for BridgeError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self::Generic(err.into().to_string())
    }
}

async fn health_check() -> StatusCode {
    StatusCode::OK
}

async fn ping(
    State((_handler, _metrics, metadata)): State<(
        Arc<impl BridgeRequestHandlerTrait + Sync + Send>,
        Arc<BridgeMetrics>,
        Arc<BridgeNodePublicMetadata>,
    )>,
) -> Result<Json<Arc<BridgeNodePublicMetadata>>, BridgeError> {
    Ok(Json(metadata))
}

async fn metrics_key_fetch(
    State((_handler, _metrics, metadata)): State<(
        Arc<impl BridgeRequestHandlerTrait + Sync + Send>,
        Arc<BridgeMetrics>,
        Arc<BridgeNodePublicMetadata>,
    )>,
) -> Result<Json<Option<Arc<Ed25519PublicKey>>>, BridgeError> {
    Ok(Json(metadata.metrics_pubkey.clone()))
}

#[instrument(level = "error", skip_all, fields(tx_hash_hex=tx_hash_hex, event_idx=event_idx))]
async fn handle_eth_tx_hash(
    Path((tx_hash_hex, event_idx)): Path<(String, u16)>,
    State((handler, metrics, _metadata)): State<(
        Arc<impl BridgeRequestHandlerTrait + Sync + Send>,
        Arc<BridgeMetrics>,
        Arc<BridgeNodePublicMetadata>,
    )>,
) -> Result<Json<SignedBridgeAction>, BridgeError> {
    let future = async {
        let sig = handler.handle_eth_tx_hash(tx_hash_hex, event_idx).await?;
        Ok(sig)
    };
    with_metrics!(metrics.clone(), "handle_eth_tx_hash", future).await
}

#[instrument(level = "error", skip_all, fields(tx_digest_base58=tx_digest_base58, event_idx=event_idx))]
async fn handle_sui_tx_digest(
    Path((tx_digest_base58, event_idx)): Path<(String, u16)>,
    State((handler, metrics, _metadata)): State<(
        Arc<impl BridgeRequestHandlerTrait + Sync + Send>,
        Arc<BridgeMetrics>,
        Arc<BridgeNodePublicMetadata>,
    )>,
) -> Result<Json<SignedBridgeAction>, BridgeError> {
    let future = async {
        let sig: Json<SignedBridgeAction> = handler
            .handle_sui_tx_digest(tx_digest_base58, event_idx)
            .await?;
        Ok(sig)
    };
    with_metrics!(metrics.clone(), "handle_sui_tx_digest", future).await
}

#[instrument(level = "error", skip_all, fields(chain_id=chain_id, nonce=nonce, blocklist_type=blocklist_type, keys=keys))]
async fn handle_update_committee_blocklist_action(
    Path((chain_id, nonce, blocklist_type, keys)): Path<(u8, u64, u8, String)>,
    State((handler, metrics, _metadata)): State<(
        Arc<impl BridgeRequestHandlerTrait + Sync + Send>,
        Arc<BridgeMetrics>,
        Arc<BridgeNodePublicMetadata>,
    )>,
) -> Result<Json<SignedBridgeAction>, BridgeError> {
    let future = async {
        let chain_id = BridgeChainId::try_from(chain_id).map_err(|err| {
            BridgeError::InvalidBridgeClientRequest(format!("Invalid chain id: {:?}", err))
        })?;
        let blocklist_type = BlocklistType::try_from(blocklist_type).map_err(|err| {
            BridgeError::InvalidBridgeClientRequest(format!(
                "Invalid blocklist action type: {:?}",
                err
            ))
        })?;
        let members_to_update = keys
            .split(',')
            .map(|s| {
                let bytes = Hex::decode(s).map_err(|e| anyhow::anyhow!("{:?}", e))?;
                BridgeAuthorityPublicKeyBytes::from_bytes(&bytes)
                    .map_err(|e| anyhow::anyhow!("{:?}", e))
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| BridgeError::InvalidBridgeClientRequest(format!("{:?}", e)))?;
        let action = BridgeAction::BlocklistCommitteeAction(BlocklistCommitteeAction {
            chain_id,
            nonce,
            blocklist_type,
            members_to_update,
        });

        let sig: Json<SignedBridgeAction> = handler.handle_governance_action(action).await?;
        Ok(sig)
    };
    with_metrics!(
        metrics.clone(),
        "handle_update_committee_blocklist_action",
        future
    )
    .await
}

#[instrument(level = "error", skip_all, fields(chain_id=chain_id, nonce=nonce, action_type=action_type))]
async fn handle_emergency_action(
    Path((chain_id, nonce, action_type)): Path<(u8, u64, u8)>,
    State((handler, metrics, _metadata)): State<(
        Arc<impl BridgeRequestHandlerTrait + Sync + Send>,
        Arc<BridgeMetrics>,
        Arc<BridgeNodePublicMetadata>,
    )>,
) -> Result<Json<SignedBridgeAction>, BridgeError> {
    let future = async {
        let chain_id = BridgeChainId::try_from(chain_id).map_err(|err| {
            BridgeError::InvalidBridgeClientRequest(format!("Invalid chain id: {:?}", err))
        })?;
        let action_type = EmergencyActionType::try_from(action_type).map_err(|err| {
            BridgeError::InvalidBridgeClientRequest(format!(
                "Invalid emergency action type: {:?}",
                err
            ))
        })?;
        let action = BridgeAction::EmergencyAction(EmergencyAction {
            chain_id,
            nonce,
            action_type,
        });
        let sig: Json<SignedBridgeAction> = handler.handle_governance_action(action).await?;
        Ok(sig)
    };
    with_metrics!(metrics.clone(), "handle_emergency_action", future).await
}

#[instrument(level = "error", skip_all, fields(chain_id=chain_id, nonce=nonce, sending_chain_id=sending_chain_id, new_usd_limit=new_usd_limit))]
async fn handle_limit_update_action(
    Path((chain_id, nonce, sending_chain_id, new_usd_limit)): Path<(u8, u64, u8, u64)>,
    State((handler, metrics, _metadata)): State<(
        Arc<impl BridgeRequestHandlerTrait + Sync + Send>,
        Arc<BridgeMetrics>,
        Arc<BridgeNodePublicMetadata>,
    )>,
) -> Result<Json<SignedBridgeAction>, BridgeError> {
    let future = async {
        let chain_id = BridgeChainId::try_from(chain_id).map_err(|err| {
            BridgeError::InvalidBridgeClientRequest(format!("Invalid chain id: {:?}", err))
        })?;
        let sending_chain_id = BridgeChainId::try_from(sending_chain_id).map_err(|err| {
            BridgeError::InvalidBridgeClientRequest(format!("Invalid chain id: {:?}", err))
        })?;
        let action = BridgeAction::LimitUpdateAction(LimitUpdateAction {
            chain_id,
            nonce,
            sending_chain_id,
            new_usd_limit,
        });
        let sig: Json<SignedBridgeAction> = handler.handle_governance_action(action).await?;
        Ok(sig)
    };
    with_metrics!(metrics.clone(), "handle_limit_update_action", future).await
}

#[instrument(level = "error", skip_all, fields(chain_id=chain_id, nonce=nonce, token_id=token_id, new_usd_price=new_usd_price))]
async fn handle_asset_price_update_action(
    Path((chain_id, nonce, token_id, new_usd_price)): Path<(u8, u64, u8, u64)>,
    State((handler, metrics, _metadata)): State<(
        Arc<impl BridgeRequestHandlerTrait + Sync + Send>,
        Arc<BridgeMetrics>,
        Arc<BridgeNodePublicMetadata>,
    )>,
) -> Result<Json<SignedBridgeAction>, BridgeError> {
    let future = async {
        let chain_id = BridgeChainId::try_from(chain_id).map_err(|err| {
            BridgeError::InvalidBridgeClientRequest(format!("Invalid chain id: {:?}", err))
        })?;
        let action = BridgeAction::AssetPriceUpdateAction(AssetPriceUpdateAction {
            chain_id,
            nonce,
            token_id,
            new_usd_price,
        });
        let sig: Json<SignedBridgeAction> = handler.handle_governance_action(action).await?;
        Ok(sig)
    };
    with_metrics!(metrics.clone(), "handle_asset_price_update_action", future).await
}

#[instrument(level = "error", skip_all, fields(chain_id=chain_id, nonce=nonce, proxy_address=format!("{:x}", proxy_address), new_impl_address=format!("{:x}", new_impl_address)))]
async fn handle_evm_contract_upgrade_with_calldata(
    Path((chain_id, nonce, proxy_address, new_impl_address, calldata)): Path<(
        u8,
        u64,
        EthAddress,
        EthAddress,
        String,
    )>,
    State((handler, metrics, _metadata)): State<(
        Arc<impl BridgeRequestHandlerTrait + Sync + Send>,
        Arc<BridgeMetrics>,
        Arc<BridgeNodePublicMetadata>,
    )>,
) -> Result<Json<SignedBridgeAction>, BridgeError> {
    let future = async {
        let chain_id = BridgeChainId::try_from(chain_id).map_err(|err| {
            BridgeError::InvalidBridgeClientRequest(format!("Invalid chain id: {:?}", err))
        })?;
        let call_data = Hex::decode(&calldata).map_err(|e| {
            BridgeError::InvalidBridgeClientRequest(format!("Invalid call data: {:?}", e))
        })?;
        let action = BridgeAction::EvmContractUpgradeAction(EvmContractUpgradeAction {
            chain_id,
            nonce,
            proxy_address,
            new_impl_address,
            call_data,
        });
        let sig: Json<SignedBridgeAction> = handler.handle_governance_action(action).await?;
        Ok(sig)
    };
    with_metrics!(
        metrics.clone(),
        "handle_evm_contract_upgrade_with_calldata",
        future
    )
    .await
}

#[instrument(
    level = "error",
    skip_all,
    fields(chain_id, nonce, proxy_address, new_impl_address)
)]
async fn handle_evm_contract_upgrade(
    Path((chain_id, nonce, proxy_address, new_impl_address)): Path<(
        u8,
        u64,
        EthAddress,
        EthAddress,
    )>,
    State((handler, metrics, _metadata)): State<(
        Arc<impl BridgeRequestHandlerTrait + Sync + Send>,
        Arc<BridgeMetrics>,
        Arc<BridgeNodePublicMetadata>,
    )>,
) -> Result<Json<SignedBridgeAction>, BridgeError> {
    let future = async {
        let chain_id = BridgeChainId::try_from(chain_id).map_err(|err| {
            BridgeError::InvalidBridgeClientRequest(format!("Invalid chain id: {:?}", err))
        })?;
        let action = BridgeAction::EvmContractUpgradeAction(EvmContractUpgradeAction {
            chain_id,
            nonce,
            proxy_address,
            new_impl_address,
            call_data: vec![],
        });
        let sig: Json<SignedBridgeAction> = handler.handle_governance_action(action).await?;

        Ok(sig)
    };
    with_metrics!(metrics.clone(), "handle_evm_contract_upgrade", future).await
}

#[instrument(level = "error", skip_all, fields(chain_id=chain_id, nonce=nonce, native=native, token_ids=token_ids, token_type_names=token_type_names, token_prices=token_prices))]
async fn handle_add_tokens_on_sui(
    Path((chain_id, nonce, native, token_ids, token_type_names, token_prices)): Path<(
        u8,
        u64,
        u8,
        String,
        String,
        String,
    )>,
    State((handler, metrics, _metadata)): State<(
        Arc<impl BridgeRequestHandlerTrait + Sync + Send>,
        Arc<BridgeMetrics>,
        Arc<BridgeNodePublicMetadata>,
    )>,
) -> Result<Json<SignedBridgeAction>, BridgeError> {
    let future = async {
        let chain_id = BridgeChainId::try_from(chain_id).map_err(|err| {
            BridgeError::InvalidBridgeClientRequest(format!("Invalid chain id: {:?}", err))
        })?;

        if !chain_id.is_sui_chain() {
            return Err(BridgeError::InvalidBridgeClientRequest(
                "handle_add_tokens_on_sui only expects Sui chain id".to_string(),
            ));
        }

        let native = match native {
            1 => true,
            0 => false,
            _ => {
                return Err(BridgeError::InvalidBridgeClientRequest(format!(
                    "Invalid native flag: {}",
                    native
                )))
            }
        };
        let token_ids = token_ids
            .split(',')
            .map(|s| {
                s.parse::<u8>().map_err(|err| {
                    BridgeError::InvalidBridgeClientRequest(format!("Invalid token id: {:?}", err))
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let token_type_names = token_type_names
            .split(',')
            .map(|s| {
                TypeTag::from_str(s).map_err(|err| {
                    BridgeError::InvalidBridgeClientRequest(format!(
                        "Invalid token type name: {:?}",
                        err
                    ))
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let token_prices = token_prices
            .split(',')
            .map(|s| {
                s.parse::<u64>().map_err(|err| {
                    BridgeError::InvalidBridgeClientRequest(format!(
                        "Invalid token price: {:?}",
                        err
                    ))
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let action = BridgeAction::AddTokensOnSuiAction(AddTokensOnSuiAction {
            chain_id,
            nonce,
            native,
            token_ids,
            token_type_names,
            token_prices,
        });
        let sig: Json<SignedBridgeAction> = handler.handle_governance_action(action).await?;
        Ok(sig)
    };
    with_metrics!(metrics.clone(), "handle_add_tokens_on_sui", future).await
}

#[instrument(level = "error", skip_all, fields(chain_id=chain_id, nonce=nonce, native=native, token_ids=token_ids, token_addresses=token_addresses, token_sui_decimals=token_sui_decimals, token_prices=token_prices))]
async fn handle_add_tokens_on_evm(
    Path((chain_id, nonce, native, token_ids, token_addresses, token_sui_decimals, token_prices)): Path<(
        u8,
        u64,
        u8,
        String,
        String,
        String,
        String,
    )>,
    State((handler, metrics, _metadata)): State<(
        Arc<impl BridgeRequestHandlerTrait + Sync + Send>,
        Arc<BridgeMetrics>,
        Arc<BridgeNodePublicMetadata>,
    )>,
) -> Result<Json<SignedBridgeAction>, BridgeError> {
    let future = async {
        let chain_id = BridgeChainId::try_from(chain_id).map_err(|err| {
            BridgeError::InvalidBridgeClientRequest(format!("Invalid chain id: {:?}", err))
        })?;
        if chain_id.is_sui_chain() {
            return Err(BridgeError::InvalidBridgeClientRequest(
                "handle_add_tokens_on_evm does not expect Sui chain id".to_string(),
            ));
        }

        let native = match native {
            1 => true,
            0 => false,
            _ => {
                return Err(BridgeError::InvalidBridgeClientRequest(format!(
                    "Invalid native flag: {}",
                    native
                )))
            }
        };
        let token_ids = token_ids
            .split(',')
            .map(|s| {
                s.parse::<u8>().map_err(|err| {
                    BridgeError::InvalidBridgeClientRequest(format!("Invalid token id: {:?}", err))
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let token_addresses = token_addresses
            .split(',')
            .map(|s| {
                EthAddress::from_str(s).map_err(|err| {
                    BridgeError::InvalidBridgeClientRequest(format!(
                        "Invalid token address: {:?}",
                        err
                    ))
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let token_sui_decimals = token_sui_decimals
            .split(',')
            .map(|s| {
                s.parse::<u8>().map_err(|err| {
                    BridgeError::InvalidBridgeClientRequest(format!(
                        "Invalid token sui decimals: {:?}",
                        err
                    ))
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let token_prices = token_prices
            .split(',')
            .map(|s| {
                s.parse::<u64>().map_err(|err| {
                    BridgeError::InvalidBridgeClientRequest(format!(
                        "Invalid token price: {:?}",
                        err
                    ))
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let action = BridgeAction::AddTokensOnEvmAction(AddTokensOnEvmAction {
            chain_id,
            nonce,
            native,
            token_ids,
            token_addresses,
            token_sui_decimals,
            token_prices,
        });
        let sig: Json<SignedBridgeAction> = handler.handle_governance_action(action).await?;
        Ok(sig)
    };
    with_metrics!(metrics.clone(), "handle_add_tokens_on_evm", future).await
}

#[macro_export]
macro_rules! with_metrics {
    ($metrics:expr, $type_:expr, $func:expr) => {
        async move {
            info!("Received {} request", $type_);
            $metrics
                .requests_received
                .with_label_values(&[$type_])
                .inc();
            $metrics
                .requests_inflight
                .with_label_values(&[$type_])
                .inc();

            let result = $func.await;

            match &result {
                Ok(_) => {
                    info!("{} request succeeded", $type_);
                    $metrics.requests_ok.with_label_values(&[$type_]).inc();
                }
                Err(e) => {
                    info!("{} request failed: {:?}", $type_, e);
                    $metrics.err_requests.with_label_values(&[$type_]).inc();
                }
            }

            $metrics
                .requests_inflight
                .with_label_values(&[$type_])
                .dec();
            result
        }
    };
}

#[cfg(test)]
mod tests {
    use sui_types::bridge::TOKEN_ID_BTC;

    use super::*;
    use crate::client::bridge_client::BridgeClient;
    use crate::server::mock_handler::BridgeRequestMockHandler;
    use crate::test_utils::get_test_authorities_and_run_mock_bridge_server;
    use crate::types::BridgeCommittee;

    #[tokio::test]
    async fn test_bridge_server_handle_blocklist_update_action_path() {
        let client = setup();

        let pub_key_bytes = BridgeAuthorityPublicKeyBytes::from_bytes(
            &Hex::decode("02321ede33d2c2d7a8a152f275a1484edef2098f034121a602cb7d767d38680aa4")
                .unwrap(),
        )
        .unwrap();
        let action = BridgeAction::BlocklistCommitteeAction(BlocklistCommitteeAction {
            nonce: 129,
            chain_id: BridgeChainId::SuiCustom,
            blocklist_type: BlocklistType::Blocklist,
            members_to_update: vec![pub_key_bytes.clone()],
        });
        client.request_sign_bridge_action(action).await.unwrap();
    }

    #[tokio::test]
    async fn test_bridge_server_handle_emergency_action_path() {
        let client = setup();

        let action = BridgeAction::EmergencyAction(EmergencyAction {
            nonce: 55,
            chain_id: BridgeChainId::SuiCustom,
            action_type: EmergencyActionType::Pause,
        });
        client.request_sign_bridge_action(action).await.unwrap();
    }

    #[tokio::test]
    async fn test_bridge_server_handle_limit_update_action_path() {
        let client = setup();

        let action = BridgeAction::LimitUpdateAction(LimitUpdateAction {
            nonce: 15,
            chain_id: BridgeChainId::SuiCustom,
            sending_chain_id: BridgeChainId::EthCustom,
            new_usd_limit: 1_000_000_0000, // $1M USD
        });
        client.request_sign_bridge_action(action).await.unwrap();
    }

    #[tokio::test]
    async fn test_bridge_server_handle_asset_price_update_action_path() {
        let client = setup();

        let action = BridgeAction::AssetPriceUpdateAction(AssetPriceUpdateAction {
            nonce: 266,
            chain_id: BridgeChainId::SuiCustom,
            token_id: TOKEN_ID_BTC,
            new_usd_price: 100_000_0000, // $100k USD
        });
        client.request_sign_bridge_action(action).await.unwrap();
    }

    #[tokio::test]
    async fn test_bridge_server_handle_evm_contract_upgrade_action_path() {
        let client = setup();

        let action = BridgeAction::EvmContractUpgradeAction(EvmContractUpgradeAction {
            nonce: 123,
            chain_id: BridgeChainId::EthCustom,
            proxy_address: EthAddress::repeat_byte(6),
            new_impl_address: EthAddress::repeat_byte(9),
            call_data: vec![],
        });
        client.request_sign_bridge_action(action).await.unwrap();

        let action = BridgeAction::EvmContractUpgradeAction(EvmContractUpgradeAction {
            nonce: 123,
            chain_id: BridgeChainId::EthCustom,
            proxy_address: EthAddress::repeat_byte(6),
            new_impl_address: EthAddress::repeat_byte(9),
            call_data: vec![12, 34, 56],
        });
        client.request_sign_bridge_action(action).await.unwrap();
    }

    #[tokio::test]
    async fn test_bridge_server_handle_add_tokens_on_sui_action_path() {
        let client = setup();

        let action = BridgeAction::AddTokensOnSuiAction(AddTokensOnSuiAction {
            nonce: 266,
            chain_id: BridgeChainId::SuiCustom,
            native: false,
            token_ids: vec![100, 101, 102],
            token_type_names: vec![
                TypeTag::from_str("0x0000000000000000000000000000000000000000000000000000000000000abc::my_coin::MyCoin1").unwrap(),
                TypeTag::from_str("0x0000000000000000000000000000000000000000000000000000000000000abc::my_coin::MyCoin2").unwrap(),
                TypeTag::from_str("0x0000000000000000000000000000000000000000000000000000000000000abc::my_coin::MyCoin3").unwrap(),
            ],
            token_prices: vec![100_000_0000, 200_000_0000, 300_000_0000],
        });
        client.request_sign_bridge_action(action).await.unwrap();
    }

    #[tokio::test]
    async fn test_bridge_server_handle_add_tokens_on_evm_action_path() {
        let client = setup();

        let action = BridgeAction::AddTokensOnEvmAction(crate::types::AddTokensOnEvmAction {
            nonce: 0,
            chain_id: BridgeChainId::EthCustom,
            native: false,
            token_ids: vec![99, 100, 101],
            token_addresses: vec![
                EthAddress::repeat_byte(1),
                EthAddress::repeat_byte(2),
                EthAddress::repeat_byte(3),
            ],
            token_sui_decimals: vec![5, 6, 7],
            token_prices: vec![1_000_000_000, 2_000_000_000, 3_000_000_000],
        });
        client.request_sign_bridge_action(action).await.unwrap();
    }

    fn setup() -> BridgeClient {
        let mock = BridgeRequestMockHandler::new();
        let (_handles, authorities, mut secrets) =
            get_test_authorities_and_run_mock_bridge_server(vec![10000], vec![mock.clone()]);
        mock.set_signer(secrets.swap_remove(0));
        let committee = BridgeCommittee::new(authorities).unwrap();
        let pub_key = committee.members().keys().next().unwrap();
        BridgeClient::new(pub_key.clone(), Arc::new(committee)).unwrap()
    }
}
