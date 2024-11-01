// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `BridgeClient` talks to BridgeNode.

use crate::crypto::{verify_signed_bridge_action, BridgeAuthorityPublicKeyBytes};
use crate::error::{BridgeError, BridgeResult};
use crate::server::APPLICATION_JSON;
use crate::types::{BridgeAction, BridgeCommittee, VerifiedSignedBridgeAction};
use fastcrypto::encoding::{Encoding, Hex};
use fastcrypto::traits::ToFromBytes;
use std::str::FromStr;
use std::sync::Arc;
use url::Url;

// Note: `base_url` is `Option<Url>` because `quorum_map_then_reduce_with_timeout_and_prefs`
// uses `[]` to get Client based on key. Therefore even when the URL is invalid we need to
// create a Client instance.
// TODO: In the future we can consider change `quorum_map_then_reduce_with_timeout_and_prefs`
// and its callsites to use `get` instead of `[]`.
#[derive(Clone, Debug)]
pub struct BridgeClient {
    inner: reqwest::Client,
    authority: BridgeAuthorityPublicKeyBytes,
    committee: Arc<BridgeCommittee>,
    base_url: Option<Url>,
}

impl BridgeClient {
    pub fn new(
        authority_name: BridgeAuthorityPublicKeyBytes,
        committee: Arc<BridgeCommittee>,
    ) -> BridgeResult<Self> {
        if !committee.is_active_member(&authority_name) {
            return Err(BridgeError::InvalidBridgeAuthority(authority_name));
        }
        // Unwrap safe: we passed the `is_active_member` check above
        let member = committee.member(&authority_name).unwrap();
        Ok(Self {
            inner: reqwest::Client::new(),
            authority: authority_name.clone(),
            base_url: Url::from_str(&member.base_url).ok(),
            committee,
        })
    }

    #[cfg(test)]
    pub fn update_committee(&mut self, committee: Arc<BridgeCommittee>) {
        self.committee = committee;
    }

    // Important: the paths need to match the ones in server/mod.rs
    fn bridge_action_to_path(event: &BridgeAction) -> String {
        match event {
            BridgeAction::SuiToEthBridgeAction(e) => format!(
                "sign/bridge_tx/sui/eth/{}/{}",
                e.sui_tx_digest, e.sui_tx_event_index
            ),
            BridgeAction::EthToSuiBridgeAction(e) => format!(
                "sign/bridge_tx/eth/sui/{}/{}",
                Hex::encode(e.eth_tx_hash.0),
                e.eth_event_index
            ),
            BridgeAction::BlocklistCommitteeAction(a) => {
                let chain_id = (a.chain_id as u8).to_string();
                let nonce = a.nonce.to_string();
                let type_ = (a.blocklist_type as u8).to_string();
                let keys = a
                    .members_to_update
                    .iter()
                    .map(|k| Hex::encode(k.as_bytes()))
                    .collect::<Vec<_>>()
                    .join(",");
                format!("sign/update_committee_blocklist/{chain_id}/{nonce}/{type_}/{keys}")
            }
            BridgeAction::EmergencyAction(a) => {
                let chain_id = (a.chain_id as u8).to_string();
                let nonce = a.nonce.to_string();
                let type_ = (a.action_type as u8).to_string();
                format!("sign/emergency_button/{chain_id}/{nonce}/{type_}")
            }
            BridgeAction::LimitUpdateAction(a) => {
                let chain_id = (a.chain_id as u8).to_string();
                let nonce = a.nonce.to_string();
                let sending_chain_id = (a.sending_chain_id as u8).to_string();
                let new_usd_limit = a.new_usd_limit.to_string();
                format!("sign/update_limit/{chain_id}/{nonce}/{sending_chain_id}/{new_usd_limit}")
            }
            BridgeAction::AssetPriceUpdateAction(a) => {
                let chain_id = (a.chain_id as u8).to_string();
                let nonce = a.nonce.to_string();
                let token_id = a.token_id.to_string();
                let new_usd_price = a.new_usd_price.to_string();
                format!("sign/update_asset_price/{chain_id}/{nonce}/{token_id}/{new_usd_price}")
            }
            BridgeAction::EvmContractUpgradeAction(a) => {
                let chain_id = (a.chain_id as u8).to_string();
                let nonce = a.nonce.to_string();
                let proxy_address = Hex::encode(a.proxy_address.as_bytes());
                let new_impl_address = Hex::encode(a.new_impl_address.as_bytes());
                let path = format!(
                    "sign/upgrade_evm_contract/{chain_id}/{nonce}/{proxy_address}/{new_impl_address}"
                );
                if a.call_data.is_empty() {
                    path
                } else {
                    let call_data = Hex::encode(a.call_data.clone());
                    format!("{}/{}", path, call_data)
                }
            }
            BridgeAction::AddTokensOnSuiAction(a) => {
                let chain_id = (a.chain_id as u8).to_string();
                let nonce = a.nonce.to_string();
                let native = if a.native { "1" } else { "0" };
                let token_ids = a
                    .token_ids
                    .iter()
                    .map(|id| id.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                let token_type_names = a
                    .token_type_names
                    .iter()
                    .map(|name| name.to_canonical_string(true))
                    .collect::<Vec<_>>()
                    .join(",");
                let token_prices = a
                    .token_prices
                    .iter()
                    .map(|price| price.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                format!(
                    "sign/add_tokens_on_sui/{chain_id}/{nonce}/{native}/{token_ids}/{token_type_names}/{token_prices}"
                )
            }
            BridgeAction::AddTokensOnEvmAction(a) => {
                let chain_id = (a.chain_id as u8).to_string();
                let nonce = a.nonce.to_string();
                let native = if a.native { "1" } else { "0" };
                let token_ids = a
                    .token_ids
                    .iter()
                    .map(|id| id.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                let token_addresses = a
                    .token_addresses
                    .iter()
                    .map(|name| format!("{:?}", name))
                    .collect::<Vec<_>>()
                    .join(",");
                let token_sui_decimals = a
                    .token_sui_decimals
                    .iter()
                    .map(|id| id.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                let token_prices = a
                    .token_prices
                    .iter()
                    .map(|price| price.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                format!(
                    "sign/add_tokens_on_evm/{chain_id}/{nonce}/{native}/{token_ids}/{token_addresses}/{token_sui_decimals}/{token_prices}"
                )
            }
        }
    }

    // Returns Ok(true) if the server is up and running
    pub async fn ping(&self) -> BridgeResult<bool> {
        if self.base_url.is_none() {
            return Err(BridgeError::InvalidAuthorityUrl(self.authority.clone()));
        }
        // Unwrap safe: checked `self.base_url.is_none()` above
        let url = self.base_url.clone().unwrap();
        Ok(self
            .inner
            .get(url)
            .header(reqwest::header::ACCEPT, APPLICATION_JSON)
            .send()
            .await?
            .error_for_status()
            .is_ok())
    }

    pub async fn request_sign_bridge_action(
        &self,
        action: BridgeAction,
    ) -> BridgeResult<VerifiedSignedBridgeAction> {
        if self.base_url.is_none() {
            return Err(BridgeError::InvalidAuthorityUrl(self.authority.clone()));
        }
        // Unwrap safe: checked `self.base_url.is_none()` above
        let url = self
            .base_url
            .clone()
            .unwrap()
            .join(&Self::bridge_action_to_path(&action))?;
        let resp = self
            .inner
            .get(url)
            .header(reqwest::header::ACCEPT, APPLICATION_JSON)
            .send()
            .await?;
        if !resp.status().is_success() {
            let error_status = format!("{:?}", resp.error_for_status_ref());
            let resp_text = resp.text().await?;
            return match resp_text {
                text if text.contains(&format!("{:?}", BridgeError::TxNotFinalized)) => {
                    Err(BridgeError::TxNotFinalized)
                }
                _ => Err(BridgeError::RestAPIError(format!(
                    "request_sign_bridge_action failed with status {:?}: {:?}",
                    error_status, resp_text
                ))),
            };
        }
        let signed_bridge_action = resp.json().await?;
        verify_signed_bridge_action(
            &action,
            signed_bridge_action,
            &self.authority,
            &self.committee,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::run_mock_bridge_server;
    use crate::{
        abi::EthToSuiTokenBridgeV1,
        crypto::BridgeAuthoritySignInfo,
        events::EmittedSuiToEthTokenBridgeV1,
        server::mock_handler::BridgeRequestMockHandler,
        test_utils::{get_test_authority_and_key, get_test_sui_to_eth_bridge_action},
        types::SignedBridgeAction,
    };
    use ethers::types::Address as EthAddress;
    use ethers::types::TxHash;
    use fastcrypto::hash::{HashFunction, Keccak256};
    use fastcrypto::traits::KeyPair;
    use prometheus::Registry;
    use sui_types::bridge::{BridgeChainId, TOKEN_ID_BTC, TOKEN_ID_USDT};
    use sui_types::TypeTag;
    use sui_types::{base_types::SuiAddress, crypto::get_key_pair, digests::TransactionDigest};

    #[tokio::test]
    async fn test_bridge_client() {
        telemetry_subscribers::init_for_testing();

        let (mut authority, pubkey, _) = get_test_authority_and_key(10000, 12345);

        let pubkey_bytes = BridgeAuthorityPublicKeyBytes::from(&pubkey);
        let committee = Arc::new(BridgeCommittee::new(vec![authority.clone()]).unwrap());
        let action =
            get_test_sui_to_eth_bridge_action(None, Some(1), Some(1), Some(100), None, None, None);

        // Ok
        let client = BridgeClient::new(pubkey_bytes.clone(), committee).unwrap();
        assert!(client.base_url.is_some());

        // Ok
        authority.base_url = "https://foo.suibridge.io".to_string();
        let committee = Arc::new(BridgeCommittee::new(vec![authority.clone()]).unwrap());
        let client = BridgeClient::new(pubkey_bytes.clone(), committee.clone()).unwrap();
        assert!(client.base_url.is_some());

        // Err, not in committee
        let (_, kp2): (_, fastcrypto::secp256k1::Secp256k1KeyPair) = get_key_pair();
        let pubkey2_bytes = BridgeAuthorityPublicKeyBytes::from(kp2.public());
        let err = BridgeClient::new(pubkey2_bytes, committee.clone()).unwrap_err();
        assert!(matches!(err, BridgeError::InvalidBridgeAuthority(_)));

        // invalid base url
        authority.base_url = "127.0.0.1:12345".to_string(); // <-- bad, missing http://
        let committee = Arc::new(BridgeCommittee::new(vec![authority.clone()]).unwrap());
        let client = BridgeClient::new(pubkey_bytes.clone(), committee.clone()).unwrap();
        assert!(client.base_url.is_none());
        assert!(matches!(
            client.ping().await.unwrap_err(),
            BridgeError::InvalidAuthorityUrl(_)
        ));
        assert!(matches!(
            client
                .request_sign_bridge_action(action.clone())
                .await
                .unwrap_err(),
            BridgeError::InvalidAuthorityUrl(_)
        ));

        // invalid base url
        authority.base_url = "http://127.256.0.1:12345".to_string(); // <-- bad, invalid ipv4 address
        let committee = Arc::new(BridgeCommittee::new(vec![authority.clone()]).unwrap());
        let client = BridgeClient::new(pubkey_bytes, committee.clone()).unwrap();
        assert!(client.base_url.is_none());
        assert!(matches!(
            client.ping().await.unwrap_err(),
            BridgeError::InvalidAuthorityUrl(_)
        ));
        assert!(matches!(
            client
                .request_sign_bridge_action(action.clone())
                .await
                .unwrap_err(),
            BridgeError::InvalidAuthorityUrl(_)
        ));
    }

    #[tokio::test]
    async fn test_bridge_client_request_sign_action() {
        telemetry_subscribers::init_for_testing();
        let registry = Registry::new();
        mysten_metrics::init_metrics(&registry);

        let mock_handler = BridgeRequestMockHandler::new();

        // start server
        let (_handles, ports) = run_mock_bridge_server(vec![mock_handler.clone()]);

        let port = ports[0];

        let (authority, _pubkey, secret) = get_test_authority_and_key(5000, port);
        let (authority2, _pubkey2, secret2) = get_test_authority_and_key(5000, port - 1);

        let committee = BridgeCommittee::new(vec![authority.clone(), authority2.clone()]).unwrap();

        let mut client =
            BridgeClient::new(authority.pubkey_bytes(), Arc::new(committee.clone())).unwrap();

        let tx_digest = TransactionDigest::random();
        let event_idx = 4;

        let action = get_test_sui_to_eth_bridge_action(
            Some(tx_digest),
            Some(event_idx),
            Some(1),
            Some(100),
            None,
            None,
            None,
        );
        let sig = BridgeAuthoritySignInfo::new(&action, &secret);
        let signed_event = SignedBridgeAction::new_from_data_and_sig(action.clone(), sig.clone());
        mock_handler.add_sui_event_response(tx_digest, event_idx, Ok(signed_event.clone()), None);

        // success
        client
            .request_sign_bridge_action(action.clone())
            .await
            .unwrap();

        // mismatched action would fail, this could happen when the authority fetched the wrong event
        let action2 = get_test_sui_to_eth_bridge_action(
            Some(tx_digest),
            Some(event_idx),
            Some(2),
            Some(200),
            None,
            None,
            None,
        );
        let wrong_sig = BridgeAuthoritySignInfo::new(&action2, &secret);
        let wrong_signed_action =
            SignedBridgeAction::new_from_data_and_sig(action2.clone(), wrong_sig.clone());
        mock_handler.add_sui_event_response(tx_digest, event_idx, Ok(wrong_signed_action), None);
        let err = client
            .request_sign_bridge_action(action.clone())
            .await
            .unwrap_err();
        assert!(matches!(err, BridgeError::MismatchedAction));

        // The action matches but the signature is wrong, fail
        let wrong_signed_action =
            SignedBridgeAction::new_from_data_and_sig(action.clone(), wrong_sig);
        mock_handler.add_sui_event_response(tx_digest, event_idx, Ok(wrong_signed_action), None);
        let err = client
            .request_sign_bridge_action(action.clone())
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            BridgeError::InvalidBridgeAuthoritySignature(..)
        ));

        // sig from blocklisted authority would fail
        let mut authority_blocklisted = authority.clone();
        authority_blocklisted.is_blocklisted = true;
        let committee2 = Arc::new(
            BridgeCommittee::new(vec![authority_blocklisted.clone(), authority2.clone()]).unwrap(),
        );
        client.update_committee(committee2);
        mock_handler.add_sui_event_response(tx_digest, event_idx, Ok(signed_event), None);

        let err = client
            .request_sign_bridge_action(action.clone())
            .await
            .unwrap_err();
        assert!(
            matches!(err, BridgeError::InvalidBridgeAuthority(pk) if pk == authority_blocklisted.pubkey_bytes()),
        );

        client.update_committee(committee.into());

        // signed by a different authority in committee would fail
        let sig2 = BridgeAuthoritySignInfo::new(&action, &secret2);
        let signed_event2 = SignedBridgeAction::new_from_data_and_sig(action.clone(), sig2.clone());
        mock_handler.add_sui_event_response(tx_digest, event_idx, Ok(signed_event2), None);
        let err = client
            .request_sign_bridge_action(action.clone())
            .await
            .unwrap_err();
        assert!(matches!(err, BridgeError::MismatchedAuthoritySigner));

        // signed by a different key, not in committee, would fail
        let (_, kp3): (_, fastcrypto::secp256k1::Secp256k1KeyPair) = get_key_pair();
        let secret3 = Arc::pin(kp3);
        let sig3 = BridgeAuthoritySignInfo::new(&action, &secret3);
        let signed_event3 = SignedBridgeAction::new_from_data_and_sig(action.clone(), sig3);
        mock_handler.add_sui_event_response(tx_digest, event_idx, Ok(signed_event3), None);
        let err = client
            .request_sign_bridge_action(action.clone())
            .await
            .unwrap_err();
        assert!(matches!(err, BridgeError::MismatchedAuthoritySigner));
    }

    #[test]
    fn test_bridge_action_path_regression_tests() {
        let sui_tx_digest = TransactionDigest::random();
        let sui_tx_event_index = 5;
        let action = BridgeAction::SuiToEthBridgeAction(crate::types::SuiToEthBridgeAction {
            sui_tx_digest,
            sui_tx_event_index,
            sui_bridge_event: EmittedSuiToEthTokenBridgeV1 {
                sui_chain_id: BridgeChainId::SuiCustom,
                nonce: 1,
                sui_address: SuiAddress::random_for_testing_only(),
                eth_chain_id: BridgeChainId::EthSepolia,
                eth_address: EthAddress::random(),
                token_id: TOKEN_ID_USDT,
                amount_sui_adjusted: 1,
            },
        });
        assert_eq!(
            BridgeClient::bridge_action_to_path(&action),
            format!(
                "sign/bridge_tx/sui/eth/{}/{}",
                sui_tx_digest, sui_tx_event_index
            )
        );

        let eth_tx_hash = TxHash::random();
        let eth_event_index = 6;
        let action = BridgeAction::EthToSuiBridgeAction(crate::types::EthToSuiBridgeAction {
            eth_tx_hash,
            eth_event_index,
            eth_bridge_event: EthToSuiTokenBridgeV1 {
                eth_chain_id: BridgeChainId::EthSepolia,
                nonce: 1,
                eth_address: EthAddress::random(),
                sui_chain_id: BridgeChainId::SuiCustom,
                sui_address: SuiAddress::random_for_testing_only(),
                token_id: TOKEN_ID_USDT,
                sui_adjusted_amount: 1,
            },
        });

        assert_eq!(
            BridgeClient::bridge_action_to_path(&action),
            format!(
                "sign/bridge_tx/eth/sui/{}/{}",
                Hex::encode(eth_tx_hash.0),
                eth_event_index
            )
        );

        let pub_key_bytes = BridgeAuthorityPublicKeyBytes::from_bytes(
            &Hex::decode("027f1178ff417fc9f5b8290bd8876f0a157a505a6c52db100a8492203ddd1d4279")
                .unwrap(),
        )
        .unwrap();

        let action =
            BridgeAction::BlocklistCommitteeAction(crate::types::BlocklistCommitteeAction {
                chain_id: BridgeChainId::EthSepolia,
                nonce: 1,
                blocklist_type: crate::types::BlocklistType::Blocklist,
                members_to_update: vec![pub_key_bytes.clone()],
            });
        assert_eq!(
            BridgeClient::bridge_action_to_path(&action),
            "sign/update_committee_blocklist/11/1/0/027f1178ff417fc9f5b8290bd8876f0a157a505a6c52db100a8492203ddd1d4279",
        );
        let pub_key_bytes2 = BridgeAuthorityPublicKeyBytes::from_bytes(
            &Hex::decode("02321ede33d2c2d7a8a152f275a1484edef2098f034121a602cb7d767d38680aa4")
                .unwrap(),
        )
        .unwrap();
        let action =
            BridgeAction::BlocklistCommitteeAction(crate::types::BlocklistCommitteeAction {
                chain_id: BridgeChainId::EthSepolia,
                nonce: 1,
                blocklist_type: crate::types::BlocklistType::Blocklist,
                members_to_update: vec![pub_key_bytes.clone(), pub_key_bytes2.clone()],
            });
        assert_eq!(
            BridgeClient::bridge_action_to_path(&action),
            "sign/update_committee_blocklist/11/1/0/027f1178ff417fc9f5b8290bd8876f0a157a505a6c52db100a8492203ddd1d4279,02321ede33d2c2d7a8a152f275a1484edef2098f034121a602cb7d767d38680aa4",
        );

        let action = BridgeAction::EmergencyAction(crate::types::EmergencyAction {
            chain_id: BridgeChainId::SuiCustom,
            nonce: 5,
            action_type: crate::types::EmergencyActionType::Pause,
        });
        assert_eq!(
            BridgeClient::bridge_action_to_path(&action),
            "sign/emergency_button/2/5/0",
        );

        let action = BridgeAction::LimitUpdateAction(crate::types::LimitUpdateAction {
            chain_id: BridgeChainId::SuiCustom,
            nonce: 10,
            sending_chain_id: BridgeChainId::EthCustom,
            new_usd_limit: 100,
        });
        assert_eq!(
            BridgeClient::bridge_action_to_path(&action),
            "sign/update_limit/2/10/12/100",
        );

        let action = BridgeAction::AssetPriceUpdateAction(crate::types::AssetPriceUpdateAction {
            chain_id: BridgeChainId::SuiCustom,
            nonce: 8,
            token_id: TOKEN_ID_BTC,
            new_usd_price: 100_000_000,
        });
        assert_eq!(
            BridgeClient::bridge_action_to_path(&action),
            "sign/update_asset_price/2/8/1/100000000",
        );

        let action =
            BridgeAction::EvmContractUpgradeAction(crate::types::EvmContractUpgradeAction {
                nonce: 123,
                chain_id: BridgeChainId::EthCustom,
                proxy_address: EthAddress::repeat_byte(6),
                new_impl_address: EthAddress::repeat_byte(9),
                call_data: vec![],
            });
        assert_eq!(
            BridgeClient::bridge_action_to_path(&action),
            "sign/upgrade_evm_contract/12/123/0606060606060606060606060606060606060606/0909090909090909090909090909090909090909",
        );

        let function_signature = "initializeV2()";
        let selector = &Keccak256::digest(function_signature).digest[0..4];
        let mut call_data = selector.to_vec();
        let action =
            BridgeAction::EvmContractUpgradeAction(crate::types::EvmContractUpgradeAction {
                nonce: 123,
                chain_id: BridgeChainId::EthCustom,
                proxy_address: EthAddress::repeat_byte(6),
                new_impl_address: EthAddress::repeat_byte(9),
                call_data: call_data.clone(),
            });
        assert_eq!(
            BridgeClient::bridge_action_to_path(&action),
            "sign/upgrade_evm_contract/12/123/0606060606060606060606060606060606060606/0909090909090909090909090909090909090909/5cd8a76b",
        );

        call_data.extend(ethers::abi::encode(&[ethers::abi::Token::Uint(42.into())]));
        let action =
            BridgeAction::EvmContractUpgradeAction(crate::types::EvmContractUpgradeAction {
                nonce: 123,
                chain_id: BridgeChainId::EthCustom,
                proxy_address: EthAddress::repeat_byte(6),
                new_impl_address: EthAddress::repeat_byte(9),
                call_data,
            });
        assert_eq!(
            BridgeClient::bridge_action_to_path(&action),
            "sign/upgrade_evm_contract/12/123/0606060606060606060606060606060606060606/0909090909090909090909090909090909090909/5cd8a76b000000000000000000000000000000000000000000000000000000000000002a",
        );

        let action = BridgeAction::AddTokensOnSuiAction(crate::types::AddTokensOnSuiAction {
            nonce: 3,
            chain_id: BridgeChainId::SuiCustom,
            native: false,
            token_ids: vec![99, 100, 101],
            token_type_names: vec![
                TypeTag::from_str("0x0000000000000000000000000000000000000000000000000000000000000abc::my_coin::MyCoin1").unwrap(),
                TypeTag::from_str("0x0000000000000000000000000000000000000000000000000000000000000abc::my_coin::MyCoin2").unwrap(),
                TypeTag::from_str("0x0000000000000000000000000000000000000000000000000000000000000abc::my_coin::MyCoin3").unwrap(),
            ],
            token_prices: vec![1_000_000_000, 2_000_000_000, 3_000_000_000],
        });
        assert_eq!(
            BridgeClient::bridge_action_to_path(&action),
            "sign/add_tokens_on_sui/2/3/0/99,100,101/0x0000000000000000000000000000000000000000000000000000000000000abc::my_coin::MyCoin1,0x0000000000000000000000000000000000000000000000000000000000000abc::my_coin::MyCoin2,0x0000000000000000000000000000000000000000000000000000000000000abc::my_coin::MyCoin3/1000000000,2000000000,3000000000",
        );

        let action = BridgeAction::AddTokensOnEvmAction(crate::types::AddTokensOnEvmAction {
            nonce: 0,
            chain_id: BridgeChainId::EthCustom,
            native: true,
            token_ids: vec![99, 100, 101],
            token_addresses: vec![
                EthAddress::repeat_byte(1),
                EthAddress::repeat_byte(2),
                EthAddress::repeat_byte(3),
            ],
            token_sui_decimals: vec![5, 6, 7],
            token_prices: vec![1_000_000_000, 2_000_000_000, 3_000_000_000],
        });
        assert_eq!(
            BridgeClient::bridge_action_to_path(&action),
            "sign/add_tokens_on_evm/12/0/1/99,100,101/0x0101010101010101010101010101010101010101,0x0202020202020202020202020202020202020202,0x0303030303030303030303030303030303030303/5,6,7/1000000000,2000000000,3000000000",
        );
    }
}
