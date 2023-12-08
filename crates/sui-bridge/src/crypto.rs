// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    error::{BridgeError, BridgeResult},
    types::{BridgeAction, BridgeCommittee, SignedBridgeAction, VerifiedSignedBridgeAction},
};
use fastcrypto::secp256k1::{Secp256k1PublicKey, Secp256k1PublicKeyAsBytes, Secp256k1Signature};
use fastcrypto::{hash::Keccak256, traits::KeyPair};
use serde::{Deserialize, Serialize};
use std::{pin::Pin, sync::Arc};
use sui_types::{crypto::Signer, message_envelope::VerifiedEnvelope};
use tap::TapFallible;

pub type BridgeAuthorityPublicKey = Secp256k1PublicKey;
pub type BridgeAuthorityPublicKeyBytes = Secp256k1PublicKeyAsBytes;
pub type BridgeAuthoritySignature = Secp256k1Signature;

/// See `StableSyncAuthoritySigner`
pub type StableSyncBridgeAuthoritySigner =
    Pin<Arc<dyn Signer<BridgeAuthoritySignature> + Send + Sync>>;

// TODO: include epoch ID here to reduce race conditions?
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BridgeAuthoritySignInfo {
    pub authority_pub_key: BridgeAuthorityPublicKey,
    pub signature: BridgeAuthoritySignature,
}

impl BridgeAuthoritySignInfo {
    pub fn new(msg: &BridgeAction, secret: &fastcrypto::secp256k1::Secp256k1KeyPair) -> Self {
        let msg_bytes = msg.to_bytes();

        Self {
            authority_pub_key: secret.public().clone(),
            signature: secret.sign_with_hash::<Keccak256>(&msg_bytes),
        }
    }

    pub fn verify(&self, msg: &BridgeAction, committee: &BridgeCommittee) -> BridgeResult<()> {
        // 1. verify committee member is in the committee and not blocklisted
        if !committee.is_active_member(&self.authority_pub_key_bytes()) {
            return Err(BridgeError::InvalidBridgeAuthority(
                self.authority_pub_key.clone(),
            ));
        }

        // 2. verify signature
        let msg_bytes = msg.to_bytes();

        self.authority_pub_key
            .verify_with_hash::<Keccak256>(&msg_bytes, &self.signature)
            .map_err(|e| {
                BridgeError::InvalidBridgeAuthoritySignature((
                    self.authority_pub_key.clone(),
                    e.to_string(),
                ))
            })
    }

    pub fn authority_pub_key_bytes(&self) -> BridgeAuthorityPublicKeyBytes {
        BridgeAuthorityPublicKeyBytes::from(&self.authority_pub_key)
    }
}

pub fn verify_signed_bridge_event(
    e: SignedBridgeAction,
    committee: &BridgeCommittee,
) -> BridgeResult<VerifiedSignedBridgeAction> {
    e.auth_sig()
        .verify(e.data(), committee)
        .tap_err(|e| tracing::error!("Failed to verify SignedBridgeEvent. Error {:?}", e))?;
    Ok(VerifiedEnvelope::new_from_verified(e))
}

#[cfg(test)]
mod tests {
    use crate::events::EmittedSuiToEthTokenBridgeV1;
    use crate::types::{BridgeAction, BridgeAuthority, BridgeChainId, SuiToEthBridgeAction};
    use crate::types::{SignedBridgeAction, TokenId};
    use ethers::types::Address as EthAddress;
    use fastcrypto::traits::KeyPair;
    use prometheus::Registry;
    use sui_types::base_types::{SuiAddress, TransactionDigest};
    use sui_types::crypto::get_key_pair;
    use sui_types::multiaddr::Multiaddr;

    use super::*;

    #[test]
    fn test_sign_and_verify_bridge_event_basic() -> anyhow::Result<()> {
        telemetry_subscribers::init_for_testing();
        let registry = Registry::new();
        mysten_metrics::init_metrics(&registry);
        let (_, kp): (_, fastcrypto::secp256k1::Secp256k1KeyPair) = get_key_pair();
        let pubkey = kp.public().clone();
        let secret = Arc::pin(kp);
        let mut authority = BridgeAuthority {
            pubkey: pubkey.clone(),
            voting_power: 10000,
            bridge_network_address: Multiaddr::try_from("/ip4/127.0.0.1/tcp/9999/http".to_string())
                .unwrap(),
            is_blocklisted: false,
        };
        let committee = BridgeCommittee::new(vec![authority.clone()]).unwrap();

        let event = BridgeAction::SuiToEthBridgeAction(SuiToEthBridgeAction {
            sui_tx_digest: TransactionDigest::random(),
            sui_tx_event_index: 1,
            sui_bridge_event: EmittedSuiToEthTokenBridgeV1 {
                nonce: 1,
                sui_chain_id: BridgeChainId::SuiTestnet,
                sui_address: SuiAddress::random_for_testing_only(),
                eth_chain_id: BridgeChainId::EthSepolia,
                eth_address: EthAddress::random(),
                token_id: TokenId::Sui,
                amount: 100,
            },
        });

        let sig = BridgeAuthoritySignInfo::new(&event, &secret);

        let signed_event = SignedBridgeAction::new_from_data_and_sig(event.clone(), sig.clone());

        // Verification should succeed
        let _ = verify_signed_bridge_event(signed_event, &committee).unwrap();

        // Signature is invalid (signed over different message), verification should fail
        let event2 = BridgeAction::SuiToEthBridgeAction(SuiToEthBridgeAction {
            sui_tx_digest: TransactionDigest::random(),
            sui_tx_event_index: 2, // <--------------------- this is different
            sui_bridge_event: EmittedSuiToEthTokenBridgeV1 {
                nonce: 1,
                sui_chain_id: BridgeChainId::SuiTestnet,
                sui_address: SuiAddress::random_for_testing_only(),
                eth_chain_id: BridgeChainId::EthSepolia,
                eth_address: EthAddress::random(),
                token_id: TokenId::Sui,
                amount: 100,
            },
        });

        let invalid_sig = BridgeAuthoritySignInfo::new(&event2, &secret);
        let signed_event = SignedBridgeAction::new_from_data_and_sig(event.clone(), invalid_sig);
        let _ = verify_signed_bridge_event(signed_event, &committee).unwrap_err();

        // Signer is not in committee, verification should fail
        let (_, kp2): (_, fastcrypto::secp256k1::Secp256k1KeyPair) = get_key_pair();
        let secret2 = Arc::pin(kp2);
        let sig2 = BridgeAuthoritySignInfo::new(&event, &secret2);
        let signed_event2 = SignedBridgeAction::new_from_data_and_sig(event.clone(), sig2);
        let _ = verify_signed_bridge_event(signed_event2, &committee).unwrap_err();

        // Authority is blocklisted, verification should fail
        authority.is_blocklisted = true;
        let committee = BridgeCommittee::new(vec![authority.clone()]).unwrap();
        let signed_event = SignedBridgeAction::new_from_data_and_sig(event, sig);
        let _ = verify_signed_bridge_event(signed_event, &committee).unwrap_err();

        Ok(())
    }
}
