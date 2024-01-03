// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    error::{BridgeError, BridgeResult},
    types::{BridgeAction, BridgeCommittee, SignedBridgeAction, VerifiedSignedBridgeAction},
};
use fastcrypto::{
    encoding::{Encoding, Hex},
    secp256k1::{
        recoverable::Secp256k1RecoverableSignature, Secp256k1KeyPair, Secp256k1PublicKey,
        Secp256k1PublicKeyAsBytes,
    },
    traits::{RecoverableSigner, VerifyRecoverable},
};
use fastcrypto::{hash::Keccak256, traits::KeyPair};
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::fmt::{Display, Formatter};
use sui_types::{base_types::ConciseableName, message_envelope::VerifiedEnvelope};
use tap::TapFallible;
pub type BridgeAuthorityKeyPair = Secp256k1KeyPair;
pub type BridgeAuthorityPublicKey = Secp256k1PublicKey;
pub type BridgeAuthorityRecoverableSignature = Secp256k1RecoverableSignature;

#[derive(Ord, PartialOrd, PartialEq, Eq, Clone, Debug, Hash)]
pub struct BridgeAuthorityPublicKeyBytes(Secp256k1PublicKeyAsBytes);

impl From<&BridgeAuthorityPublicKey> for BridgeAuthorityPublicKeyBytes {
    fn from(pk: &BridgeAuthorityPublicKey) -> Self {
        Self(Secp256k1PublicKeyAsBytes::from(pk))
    }
}

pub struct ConciseBridgeAuthorityPublicKeyBytesRef<'a>(&'a BridgeAuthorityPublicKeyBytes);

impl Debug for ConciseBridgeAuthorityPublicKeyBytesRef<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        let s = Hex::encode(self.0 .0 .0.get(0..4).ok_or(std::fmt::Error)?);
        write!(f, "k#{}..", s)
    }
}

impl Display for ConciseBridgeAuthorityPublicKeyBytesRef<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), std::fmt::Error> {
        Debug::fmt(self, f)
    }
}

impl AsRef<[u8]> for BridgeAuthorityPublicKeyBytes {
    fn as_ref(&self) -> &[u8] {
        self.0 .0.as_ref()
    }
}

impl<'a> ConciseableName<'a> for BridgeAuthorityPublicKeyBytes {
    type ConciseTypeRef = ConciseBridgeAuthorityPublicKeyBytesRef<'a>;
    type ConciseType = String;

    fn concise(&'a self) -> ConciseBridgeAuthorityPublicKeyBytesRef<'a> {
        ConciseBridgeAuthorityPublicKeyBytesRef(self)
    }

    fn concise_owned(&self) -> String {
        format!("{:?}", ConciseBridgeAuthorityPublicKeyBytesRef(self))
    }
}

// TODO: include epoch ID here to reduce race conditions?
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct BridgeAuthoritySignInfo {
    pub authority_pub_key: BridgeAuthorityPublicKey,
    pub signature: BridgeAuthorityRecoverableSignature,
}

impl BridgeAuthoritySignInfo {
    pub fn new(msg: &BridgeAction, secret: &BridgeAuthorityKeyPair) -> Self {
        let msg_bytes = msg.to_bytes();

        Self {
            authority_pub_key: secret.public().clone(),
            signature: secret.sign_recoverable_with_hash::<Keccak256>(&msg_bytes),
        }
    }

    pub fn verify(&self, msg: &BridgeAction, committee: &BridgeCommittee) -> BridgeResult<()> {
        // 1. verify committee member is in the committee and not blocklisted
        if !committee.is_active_member(&self.authority_pub_key_bytes()) {
            return Err(BridgeError::InvalidBridgeAuthority(
                self.authority_pub_key_bytes(),
            ));
        }

        // 2. verify signature
        let msg_bytes = msg.to_bytes();

        self.authority_pub_key
            .verify_recoverable_with_hash::<Keccak256>(&msg_bytes, &self.signature)
            .map_err(|e| {
                BridgeError::InvalidBridgeAuthoritySignature((
                    self.authority_pub_key_bytes(),
                    e.to_string(),
                ))
            })
    }

    pub fn authority_pub_key_bytes(&self) -> BridgeAuthorityPublicKeyBytes {
        BridgeAuthorityPublicKeyBytes::from(&self.authority_pub_key)
    }
}

/// Verifies a SignedBridgeAction (response from bridge authority to bridge client)
/// represents the right BridgeAction, and is signed by the right authority.
pub fn verify_signed_bridge_action(
    expected_action: &BridgeAction,
    signed_action: SignedBridgeAction,
    expected_signer: &BridgeAuthorityPublicKeyBytes,
    committee: &BridgeCommittee,
) -> BridgeResult<VerifiedSignedBridgeAction> {
    if signed_action.data() != expected_action {
        return Err(BridgeError::MismatchedAction);
    }

    let sig = signed_action.auth_sig();
    if &sig.authority_pub_key_bytes() != expected_signer {
        return Err(BridgeError::MismatchedAuthoritySigner);
    }
    sig.verify(signed_action.data(), committee).tap_err(|e| {
        tracing::error!(
            "Failed to verify SignedBridgeEvent {:?}. Error {:?}",
            signed_action,
            e
        )
    })?;
    Ok(VerifiedEnvelope::new_from_verified(signed_action))
}

#[cfg(test)]
mod tests {
    use crate::test_utils::{get_test_authority_and_key, get_test_sui_to_eth_bridge_action};
    use crate::types::BridgeAction;
    use crate::types::SignedBridgeAction;
    use fastcrypto::traits::KeyPair;
    use prometheus::Registry;
    use std::sync::Arc;
    use sui_types::crypto::get_key_pair;

    use super::*;

    #[test]
    fn test_sign_and_verify_bridge_event_basic() -> anyhow::Result<()> {
        telemetry_subscribers::init_for_testing();
        let registry = Registry::new();
        mysten_metrics::init_metrics(&registry);

        let (mut authority1, pubkey, secret) = get_test_authority_and_key(5000, 9999);
        let pubkey_bytes = BridgeAuthorityPublicKeyBytes::from(&pubkey);

        let (authority2, pubkey2, _secret) = get_test_authority_and_key(5000, 9999);
        let pubkey_bytes2 = BridgeAuthorityPublicKeyBytes::from(&pubkey2);

        let committee = BridgeCommittee::new(vec![authority1.clone(), authority2.clone()]).unwrap();

        let action: BridgeAction =
            get_test_sui_to_eth_bridge_action(None, Some(1), Some(1), Some(100));

        let sig = BridgeAuthoritySignInfo::new(&action, &secret);

        let signed_action = SignedBridgeAction::new_from_data_and_sig(action.clone(), sig.clone());

        // Verification should succeed
        let _ =
            verify_signed_bridge_action(&action, signed_action.clone(), &pubkey_bytes, &committee)
                .unwrap();

        // Verification should fail - mismatched signer
        assert!(matches!(
            verify_signed_bridge_action(&action, signed_action.clone(), &pubkey_bytes2, &committee)
                .unwrap_err(),
            BridgeError::MismatchedAuthoritySigner
        ));

        let mismatched_action: BridgeAction =
            get_test_sui_to_eth_bridge_action(None, Some(2), Some(3), Some(4));
        // Verification should fail - mismatched action
        assert!(matches!(
            verify_signed_bridge_action(
                &mismatched_action,
                signed_action.clone(),
                &pubkey_bytes2,
                &committee
            )
            .unwrap_err(),
            BridgeError::MismatchedAction,
        ));

        // Signature is invalid (signed over different message), verification should fail
        let action2: BridgeAction =
            get_test_sui_to_eth_bridge_action(None, Some(3), Some(5), Some(77));

        let invalid_sig = BridgeAuthoritySignInfo::new(&action2, &secret);
        let signed_action = SignedBridgeAction::new_from_data_and_sig(action.clone(), invalid_sig);
        let _ = verify_signed_bridge_action(&action, signed_action, &pubkey_bytes, &committee)
            .unwrap_err();

        // Signer is not in committee, verification should fail
        let (_, kp2): (_, fastcrypto::secp256k1::Secp256k1KeyPair) = get_key_pair();
        let pubkey_bytes_2 = BridgeAuthorityPublicKeyBytes::from(kp2.public());
        let secret2 = Arc::pin(kp2);
        let sig2 = BridgeAuthoritySignInfo::new(&action, &secret2);
        let signed_action2 = SignedBridgeAction::new_from_data_and_sig(action.clone(), sig2);
        let _ = verify_signed_bridge_action(&action, signed_action2, &pubkey_bytes_2, &committee)
            .unwrap_err();

        // Authority is blocklisted, verification should fail
        authority1.is_blocklisted = true;
        let committee = BridgeCommittee::new(vec![authority1, authority2]).unwrap();
        let signed_action = SignedBridgeAction::new_from_data_and_sig(action.clone(), sig);
        let _ = verify_signed_bridge_action(&action, signed_action, &pubkey_bytes, &committee)
            .unwrap_err();

        Ok(())
    }
}
