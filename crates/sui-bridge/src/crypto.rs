// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::hash::Keccak256;
use std::{pin::Pin, sync::Arc};
use serde::{Deserialize, Serialize};
use fastcrypto::secp256k1::{Secp256k1PublicKey, Secp256k1Signature};
use crate::{
    error::{BridgeError, BridgeResult},
    types::{
        BridgeCommittee, BridgeEvent, SignedBridgeEvent, VerifiedSignedBridgeEvent,
    },
};
use sui_types::{crypto::Signer, message_envelope::VerifiedEnvelope};
use tap::TapFallible;

pub type BridgeAuthorityPublicKey = Secp256k1PublicKey;
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
    pub fn new(
        msg: &BridgeEvent,
        authority_pub_key: BridgeAuthorityPublicKey,
        secret: &fastcrypto::secp256k1::Secp256k1KeyPair,
    ) -> Self {
        let msg_bytes = msg.to_bytes();

        Self {
            authority_pub_key,
            signature: secret.sign_with_hash::<Keccak256>(&msg_bytes),
        }
    }

    pub fn verify(&self, msg: &BridgeEvent, committee: &BridgeCommittee) -> BridgeResult<()> {
        // 1. verify committee member is in the committee and not blocklisted
        if !committee.is_active_member(&self.authority_pub_key) {
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

}

pub fn verify_signed_bridge_event(
    e: SignedBridgeEvent,
    committee: &BridgeCommittee,
) -> BridgeResult<VerifiedSignedBridgeEvent> {
    e.auth_sig()
        .verify(&e.data(), committee)
        .tap_err(|e| tracing::error!("Failed to verify SignedBridgeEvent. Error {:?}", e))?;
    Ok(VerifiedEnvelope::new_from_verified(e))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use sui_types::multiaddr::Multiaddr;
    use crate::types::BridgeAuthority;
    use crate::{
        events::{SuiBridgeEvent, SuiToEthBridgeEventV1},
        types::{SignedBridgeEvent, TokenId},
    };
    use ethers::types::Address as EthAddress;
    use fastcrypto::traits::KeyPair;
    use prometheus::Registry;
    use sui_types::base_types::{SuiAddress, TransactionDigest};
    use sui_types::crypto::get_key_pair;

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
            bridge_network_address: Multiaddr::try_from(format!("/ip4/127.0.0.1/tcp/9999/http"))
                .unwrap(),
            is_blocklisted: false,
        };
        let committee = BridgeCommittee {
            members: BTreeMap::from_iter(vec![(pubkey.clone(), authority.clone())]),
        };

        let event = BridgeEvent::Sui(SuiBridgeEvent::SuiToEthTokenBridgeV1(
            SuiToEthBridgeEventV1 {
                nonce: 1,
                sui_address: SuiAddress::random_for_testing_only(),
                eth_address: EthAddress::random(),
                sui_tx_digest: TransactionDigest::random(),
                sui_tx_event_index: 1,
                token_id: TokenId::Sui,
                amount: 100,
            },
        ));

        let sig = BridgeAuthoritySignInfo::new(&event, pubkey.clone(), &*secret);

        let signed_event = SignedBridgeEvent::new_from_data_and_sig(event.clone(), sig.clone());

        // Verification should succeed
        let _ = verify_signed_bridge_event(signed_event, &committee).unwrap();

        // Authority is blocklisted, verification should fail
        authority.is_blocklisted = true;
        let committee = BridgeCommittee {
            members: BTreeMap::from_iter(vec![(pubkey.clone(), authority.clone())]),
        };
        let signed_event = SignedBridgeEvent::new_from_data_and_sig(event, sig);
        let _ = verify_signed_bridge_event(signed_event, &committee).unwrap_err();

        Ok(())
    }
}
