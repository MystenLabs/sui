// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    error::{BridgeError, BridgeResult},
    types::{BridgeAction, BridgeCommittee, SignedBridgeAction, VerifiedSignedBridgeAction},
};
use ethers::core::k256::ecdsa::VerifyingKey;
use ethers::core::k256::elliptic_curve::sec1::ToEncodedPoint;
use ethers::types::Address as EthAddress;
use fastcrypto::hash::HashFunction;
use fastcrypto::{
    encoding::{Encoding, Hex},
    error::FastCryptoError,
    secp256k1::{
        recoverable::Secp256k1RecoverableSignature, Secp256k1KeyPair, Secp256k1PublicKey,
        Secp256k1PublicKeyAsBytes,
    },
    traits::{RecoverableSigner, ToFromBytes, VerifyRecoverable},
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

#[derive(Ord, PartialOrd, PartialEq, Eq, Clone, Debug, Hash, Serialize, Deserialize)]
pub struct BridgeAuthorityPublicKeyBytes(Secp256k1PublicKeyAsBytes);

impl BridgeAuthorityPublicKeyBytes {
    pub fn to_eth_address(&self) -> EthAddress {
        // unwrap: the conversion should not fail
        let pubkey = VerifyingKey::from_sec1_bytes(self.as_bytes()).unwrap();
        let affine: &ethers::core::k256::AffinePoint = pubkey.as_ref();
        let encoded = affine.to_encoded_point(false);
        let pubkey = &encoded.as_bytes()[1..];
        assert_eq!(pubkey.len(), 64, "raw public key must be 64 bytes");
        let hash = Keccak256::digest(pubkey).digest;
        EthAddress::from_slice(&hash[12..])
    }
}

impl From<&BridgeAuthorityPublicKey> for BridgeAuthorityPublicKeyBytes {
    fn from(pk: &BridgeAuthorityPublicKey) -> Self {
        Self(Secp256k1PublicKeyAsBytes::from(pk))
    }
}

impl ToFromBytes for BridgeAuthorityPublicKeyBytes {
    /// Parse an object from its byte representation
    fn from_bytes(bytes: &[u8]) -> Result<Self, FastCryptoError> {
        let pk = BridgeAuthorityPublicKey::from_bytes(bytes)?;
        Ok(Self::from(&pk))
    }

    /// Borrow a byte slice representing the serialized form of this object
    fn as_bytes(&self) -> &[u8] {
        self.as_ref()
    }
}

/// implement `FromStr` for `BridgeAuthorityPublicKeyBytes`
/// to convert a hex-string to public key bytes.
impl std::str::FromStr for BridgeAuthorityPublicKeyBytes {
    type Err = FastCryptoError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = Hex::decode(s).map_err(|e| {
            FastCryptoError::GeneralError(format!("Failed to decode hex string: {}", e))
        })?;
        Self::from_bytes(&bytes)
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
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
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
    use crate::events::EmittedSuiToEthTokenBridgeV1;
    use crate::test_utils::{get_test_authority_and_key, get_test_sui_to_eth_bridge_action};
    use crate::types::SignedBridgeAction;
    use crate::types::{BridgeAction, BridgeAuthority, SuiToEthBridgeAction};
    use ethers::types::Address as EthAddress;
    use fastcrypto::traits::{KeyPair, ToFromBytes};
    use prometheus::Registry;
    use std::str::FromStr;
    use std::sync::Arc;
    use sui_types::base_types::SuiAddress;
    use sui_types::bridge::{BridgeChainId, TOKEN_ID_ETH};
    use sui_types::crypto::get_key_pair;
    use sui_types::digests::TransactionDigest;

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
            get_test_sui_to_eth_bridge_action(None, Some(1), Some(1), Some(100), None, None, None);

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
            get_test_sui_to_eth_bridge_action(None, Some(2), Some(3), Some(4), None, None, None);
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
            get_test_sui_to_eth_bridge_action(None, Some(3), Some(5), Some(77), None, None, None);

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

    #[test]
    fn test_bridge_sig_verification_regression_test() {
        telemetry_subscribers::init_for_testing();
        let registry = Registry::new();
        mysten_metrics::init_metrics(&registry);

        let public_key_bytes =
            Hex::decode("02321ede33d2c2d7a8a152f275a1484edef2098f034121a602cb7d767d38680aa4")
                .unwrap();
        let pubkey1 = BridgeAuthorityPublicKey::from_bytes(&public_key_bytes).unwrap();
        let authority1 = BridgeAuthority {
            sui_address: SuiAddress::random_for_testing_only(),
            pubkey: pubkey1.clone(),
            voting_power: 2500,
            is_blocklisted: false,
            base_url: "".into(),
        };

        let public_key_bytes =
            Hex::decode("027f1178ff417fc9f5b8290bd8876f0a157a505a6c52db100a8492203ddd1d4279")
                .unwrap();
        let pubkey2 = BridgeAuthorityPublicKey::from_bytes(&public_key_bytes).unwrap();
        let authority2 = BridgeAuthority {
            sui_address: SuiAddress::random_for_testing_only(),
            pubkey: pubkey2.clone(),
            voting_power: 2500,
            is_blocklisted: false,
            base_url: "".into(),
        };

        let public_key_bytes =
            Hex::decode("026f311bcd1c2664c14277c7a80e4857c690626597064f89edc33b8f67b99c6bc0")
                .unwrap();
        let pubkey3 = BridgeAuthorityPublicKey::from_bytes(&public_key_bytes).unwrap();
        let authority3 = BridgeAuthority {
            sui_address: SuiAddress::random_for_testing_only(),
            pubkey: pubkey3.clone(),
            voting_power: 2500,
            is_blocklisted: false,
            base_url: "".into(),
        };

        let public_key_bytes =
            Hex::decode("03a57b85771aedeb6d31c808be9a6e73194e4b70e679608f2bca68bcc684773736")
                .unwrap();
        let pubkey4 = BridgeAuthorityPublicKey::from_bytes(&public_key_bytes).unwrap();
        let authority4 = BridgeAuthority {
            sui_address: SuiAddress::random_for_testing_only(),
            pubkey: pubkey4.clone(),
            voting_power: 2500,
            is_blocklisted: false,
            base_url: "".into(),
        };

        let committee = BridgeCommittee::new(vec![
            authority1.clone(),
            authority2.clone(),
            authority3.clone(),
            authority4.clone(),
        ])
        .unwrap();

        let action = BridgeAction::SuiToEthBridgeAction(SuiToEthBridgeAction {
            sui_tx_digest: TransactionDigest::random(),
            sui_tx_event_index: 0,
            sui_bridge_event: EmittedSuiToEthTokenBridgeV1 {
                nonce: 1,
                sui_chain_id: BridgeChainId::SuiTestnet,
                sui_address: SuiAddress::from_str(
                    "0x80ab1ee086210a3a37355300ca24672e81062fcdb5ced6618dab203f6a3b291c",
                )
                .unwrap(),
                eth_chain_id: BridgeChainId::EthSepolia,
                eth_address: EthAddress::from_str("0xb18f79Fe671db47393315fFDB377Da4Ea1B7AF96")
                    .unwrap(),
                token_id: TOKEN_ID_ETH,
                amount_sui_adjusted: 100000u64,
            },
        });
        let sig = BridgeAuthoritySignInfo {
            authority_pub_key: pubkey1,
            signature: BridgeAuthorityRecoverableSignature::from_bytes(
                &Hex::decode("e1cf11b380855ff1d4a451ebc2fd68477cf701b7d4ec88da3082709fe95201a5061b4b60cf13815a80ba9dfead23e220506aa74c4a863ba045d95715b4cc6b6e00").unwrap(),
            ).unwrap(),
        };
        sig.verify(&action, &committee).unwrap();

        let sig = BridgeAuthoritySignInfo {
            authority_pub_key: pubkey4.clone(),
            signature: BridgeAuthorityRecoverableSignature::from_bytes(
                &Hex::decode("8ba9ec92c2d5a44ecc123182f689b901a93921fd35f581354fea20b25a0ded6d055b96a64bdda77dd5a62b93d29abe93640aa3c1a136348093cd7a2418c6bfa301").unwrap(),
            ).unwrap(),
        };
        sig.verify(&action, &committee).unwrap();

        let sig = BridgeAuthoritySignInfo {
            authority_pub_key: pubkey4,
            signature: BridgeAuthorityRecoverableSignature::from_bytes(
                // invalid sdig
                &Hex::decode("8ba9ec92c2d5a44ecc123182f689b901a93921fd35f581354fea20b25a0ded6d055b96a64bdda77dd5a62b93d29abe93640aa3c1a136348093cd7a2418c6bfa302").unwrap(),
            ).unwrap(),
        };
        sig.verify(&action, &committee).unwrap_err();
    }

    #[test]
    fn test_bridge_authority_public_key_bytes_to_eth_address() {
        let pub_key_bytes = BridgeAuthorityPublicKeyBytes::from_bytes(
            &Hex::decode("02321ede33d2c2d7a8a152f275a1484edef2098f034121a602cb7d767d38680aa4")
                .unwrap(),
        )
        .unwrap();
        let addr = "0x68b43fd906c0b8f024a18c56e06744f7c6157c65"
            .parse::<EthAddress>()
            .unwrap();
        assert_eq!(pub_key_bytes.to_eth_address(), addr);

        // Example from: https://github.com/gakonst/ethers-rs/blob/master/ethers-core/src/utils/mod.rs#L1235
        let pub_key_bytes = BridgeAuthorityPublicKeyBytes::from_bytes(
            &Hex::decode("0376698beebe8ee5c74d8cc50ab84ac301ee8f10af6f28d0ffd6adf4d6d3b9b762")
                .unwrap(),
        )
        .unwrap();
        let addr = "0Ac1dF02185025F65202660F8167210A80dD5086"
            .parse::<EthAddress>()
            .unwrap();
        assert_eq!(pub_key_bytes.to_eth_address(), addr);
    }
}
