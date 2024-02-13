// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#![allow(clippy::inconsistent_digit_grouping)]

use crate::{
    crypto::BridgeAuthorityPublicKeyBytes,
    error::BridgeError,
    server::handler::{BridgeRequestHandler, BridgeRequestHandlerTrait},
    types::{
        AssetPriceUpdateAction, BlocklistCommitteeAction, BlocklistType, BridgeAction,
        BridgeChainId, EmergencyAction, EmergencyActionType, EvmContractUpgradeAction,
        LimitUpdateAction, SignedBridgeAction, TokenId,
    },
};
use axum::{
    extract::{Path, State},
    Json,
};
use axum::{http::StatusCode, routing::get, Router};
use ethers::types::Address as EthAddress;
use fastcrypto::{
    encoding::{Encoding, Hex},
    traits::ToFromBytes,
};
use std::net::SocketAddr;
use std::sync::Arc;

pub mod governance_verifier;
pub mod handler;

#[cfg(test)]
pub(crate) mod mock_handler;

pub const APPLICATION_JSON: &str = "application/json";

// Important: the paths need to match the ones in bridge_client.rs
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

pub async fn run_server(socket_address: &SocketAddr, handler: BridgeRequestHandler) {
    axum::Server::bind(socket_address)
        .serve(make_router(Arc::new(handler)).into_make_service())
        .await
        .unwrap();
}

pub(crate) fn make_router(
    handler: Arc<impl BridgeRequestHandlerTrait + Sync + Send + 'static>,
) -> Router {
    Router::new()
        .route("/", get(health_check))
        .route(ETH_TO_SUI_TX_PATH, get(handle_eth_tx_hash))
        .route(SUI_TO_ETH_TX_PATH, get(handle_sui_tx_digest))
        .route(
            COMMITTEE_BLOCKLIST_UPDATE_PATH,
            get(handle_update_committee_blocklist_action),
        )
        .route(EMERGENCY_BUTTON_PATH, get(handle_emergecny_action))
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
        .with_state(handler)
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

async fn handle_eth_tx_hash(
    Path((tx_hash_hex, event_idx)): Path<(String, u16)>,
    State(handler): State<Arc<impl BridgeRequestHandlerTrait + Sync + Send>>,
) -> Result<Json<SignedBridgeAction>, BridgeError> {
    let sig = handler.handle_eth_tx_hash(tx_hash_hex, event_idx).await?;
    Ok(sig)
}

async fn handle_sui_tx_digest(
    Path((tx_digest_base58, event_idx)): Path<(String, u16)>,
    State(handler): State<Arc<impl BridgeRequestHandlerTrait + Sync + Send>>,
) -> Result<Json<SignedBridgeAction>, BridgeError> {
    let sig: Json<SignedBridgeAction> = handler
        .handle_sui_tx_digest(tx_digest_base58, event_idx)
        .await?;
    Ok(sig)
}

async fn handle_update_committee_blocklist_action(
    Path((chain_id, nonce, blocklist_type, keys)): Path<(u8, u64, u8, String)>,
    State(handler): State<Arc<impl BridgeRequestHandlerTrait + Sync + Send>>,
) -> Result<Json<SignedBridgeAction>, BridgeError> {
    let chain_id = BridgeChainId::try_from(chain_id).map_err(|err| {
        BridgeError::InvalidBridgeClientRequest(format!("Invalid chain id: {:?}", err))
    })?;
    let blocklist_type = BlocklistType::try_from(blocklist_type).map_err(|err| {
        BridgeError::InvalidBridgeClientRequest(format!("Invalid blocklist action type: {:?}", err))
    })?;
    let blocklisted_members = keys
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
        blocklisted_members,
    });

    let sig: Json<SignedBridgeAction> = handler.handle_governance_action(action).await?;
    Ok(sig)
}

async fn handle_emergecny_action(
    Path((chain_id, nonce, action_type)): Path<(u8, u64, u8)>,
    State(handler): State<Arc<impl BridgeRequestHandlerTrait + Sync + Send>>,
) -> Result<Json<SignedBridgeAction>, BridgeError> {
    let chain_id = BridgeChainId::try_from(chain_id).map_err(|err| {
        BridgeError::InvalidBridgeClientRequest(format!("Invalid chain id: {:?}", err))
    })?;
    let action_type = EmergencyActionType::try_from(action_type).map_err(|err| {
        BridgeError::InvalidBridgeClientRequest(format!("Invalid emergency action type: {:?}", err))
    })?;
    let action = BridgeAction::EmergencyAction(EmergencyAction {
        chain_id,
        nonce,
        action_type,
    });
    let sig: Json<SignedBridgeAction> = handler.handle_governance_action(action).await?;
    Ok(sig)
}

async fn handle_limit_update_action(
    Path((chain_id, nonce, sending_chain_id, new_usd_limit)): Path<(u8, u64, u8, u64)>,
    State(handler): State<Arc<impl BridgeRequestHandlerTrait + Sync + Send>>,
) -> Result<Json<SignedBridgeAction>, BridgeError> {
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
}

async fn handle_asset_price_update_action(
    Path((chain_id, nonce, token_id, new_usd_price)): Path<(u8, u64, u8, u64)>,
    State(handler): State<Arc<impl BridgeRequestHandlerTrait + Sync + Send>>,
) -> Result<Json<SignedBridgeAction>, BridgeError> {
    let chain_id = BridgeChainId::try_from(chain_id).map_err(|err| {
        BridgeError::InvalidBridgeClientRequest(format!("Invalid chain id: {:?}", err))
    })?;
    let token_id = TokenId::try_from(token_id).map_err(|err| {
        BridgeError::InvalidBridgeClientRequest(format!("Invalid token id: {:?}", err))
    })?;
    let action = BridgeAction::AssetPriceUpdateAction(AssetPriceUpdateAction {
        chain_id,
        nonce,
        token_id,
        new_usd_price,
    });
    let sig: Json<SignedBridgeAction> = handler.handle_governance_action(action).await?;
    Ok(sig)
}

async fn handle_evm_contract_upgrade_with_calldata(
    Path((chain_id, nonce, proxy_address, new_impl_address, calldata)): Path<(
        u8,
        u64,
        EthAddress,
        EthAddress,
        String,
    )>,
    State(handler): State<Arc<impl BridgeRequestHandlerTrait + Sync + Send>>,
) -> Result<Json<SignedBridgeAction>, BridgeError> {
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
}

async fn handle_evm_contract_upgrade(
    Path((chain_id, nonce, proxy_address, new_impl_address)): Path<(
        u8,
        u64,
        EthAddress,
        EthAddress,
    )>,
    State(handler): State<Arc<impl BridgeRequestHandlerTrait + Sync + Send>>,
) -> Result<Json<SignedBridgeAction>, BridgeError> {
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
}

#[cfg(test)]
mod tests {
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
            chain_id: BridgeChainId::SuiLocalTest,
            blocklist_type: BlocklistType::Blocklist,
            blocklisted_members: vec![pub_key_bytes.clone()],
        });
        client.request_sign_bridge_action(action).await.unwrap();
    }

    #[tokio::test]
    async fn test_bridge_server_handle_emergency_action_path() {
        let client = setup();

        let action = BridgeAction::EmergencyAction(EmergencyAction {
            nonce: 55,
            chain_id: BridgeChainId::SuiLocalTest,
            action_type: EmergencyActionType::Pause,
        });
        client.request_sign_bridge_action(action).await.unwrap();
    }

    #[tokio::test]
    async fn test_bridge_server_handle_limit_update_action_path() {
        let client = setup();

        let action = BridgeAction::LimitUpdateAction(LimitUpdateAction {
            nonce: 15,
            chain_id: BridgeChainId::SuiLocalTest,
            sending_chain_id: BridgeChainId::EthLocalTest,
            new_usd_limit: 1_000_000_0000, // $1M USD
        });
        client.request_sign_bridge_action(action).await.unwrap();
    }

    #[tokio::test]
    async fn test_bridge_server_handle_asset_price_update_action_path() {
        let client = setup();

        let action = BridgeAction::AssetPriceUpdateAction(AssetPriceUpdateAction {
            nonce: 266,
            chain_id: BridgeChainId::SuiLocalTest,
            token_id: TokenId::BTC,
            new_usd_price: 100_000_0000, // $100k USD
        });
        client.request_sign_bridge_action(action).await.unwrap();
    }

    #[tokio::test]
    async fn test_bridge_server_handle_evm_contract_upgrade_action_path() {
        let client = setup();

        let action = BridgeAction::EvmContractUpgradeAction(EvmContractUpgradeAction {
            nonce: 123,
            chain_id: BridgeChainId::EthLocalTest,
            proxy_address: EthAddress::repeat_byte(6),
            new_impl_address: EthAddress::repeat_byte(9),
            call_data: vec![],
        });
        client.request_sign_bridge_action(action).await.unwrap();

        let action = BridgeAction::EvmContractUpgradeAction(EvmContractUpgradeAction {
            nonce: 123,
            chain_id: BridgeChainId::EthLocalTest,
            proxy_address: EthAddress::repeat_byte(6),
            new_impl_address: EthAddress::repeat_byte(9),
            call_data: vec![12, 34, 56],
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
