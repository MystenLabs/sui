// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This module defines the protocol for specifying an address balance reservation
//! via an ObjectRef, in order to provide backward compatibility for clients that do
//! not understand address balances.
//!
//! The layout of the reservation ObjectRef is as follows:
//!
//!    (ObjectID, SequenceNumber, ObjectDigest)
//!
//! The ObjectID points to an accumulator object (i.e. a dynamic field of the accumulator root object).
//! This identifies both the owner and type (e.g. SUI, USDC, etc) of the balance being spent.
//!
//! It is masked by XORing with the current chain identifier (i.e. genesis checkpoint digest).
//! This prevents cross-chain replay, as an attacker would have to mine an address and currency
//! type such that `dynamic_field_key(address, type) = V` such that
//! `V ^ FOREIGN_CHAIN_IDENTIFIER = TARGET_ACCUMULATOR_OBJECT_ID ^ TARGET_CHAIN_IDENTIFIER`
//! and then trick the target into signing a transaction as V on the foreign chain.
//!
//! The masking also allows read APIs to positively identify attempts to read a "fake" object ID, as
//! follows:
//!   1. First, read the requested object ID.
//!   2. If it does not exist, unmask the ID using the local chain identifier and read it again.
//!   3. If it exists on the second attempt, the ID must have originated by masking an accumulator object ID.
//!
//! The SequenceNumber is a monotonically increasing version number, typically the version of the
//! accumulator root object. It is not used by the protocol, but is intended to help the
//! caching behavior of old clients.
//!
//! ObjectDigest contains the remainder of the payload:
//!
//! 1. The amount of the reservation [8 bytes]
//! 2. The epoch(s) in which the tx is valid [4 bytes] (good enough for 12 million years of 24 hour epochs).
//! 3. A magic number to identify this ObjectRef as a coin reservation [20 bytes].

use thiserror::Error;

use crate::{
    base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress},
    committee::EpochId,
    digests::{ChainIdentifier, ObjectDigest},
    error::UserInputResult,
    transaction::FundsWithdrawalArg,
};

/// Trait for resolving funds withdrawal from a coin reservation
pub trait CoinReservationResolverTrait {
    // Used to check validity of the transaction. If the coin_reservation does not
    // point to an existing accumulator object, the transaction will be rejected.
    fn resolve_funds_withdrawal(
        &self,
        // Note: must be the sender. We do not support sponsorship.
        sender: SuiAddress,
        coin_reservation: ParsedObjectRefWithdrawal,
    ) -> UserInputResult<FundsWithdrawalArg>;
}

pub const COIN_RESERVATION_MAGIC: [u8; 20] = [
    0xac, 0xac, 0xac, 0xac, 0xac, 0xac, 0xac, 0xac, 0xac, 0xac, 0xac, 0xac, 0xac, 0xac, 0xac, 0xac,
    0xac, 0xac, 0xac, 0xac,
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ParsedDigest {
    epoch_id: u32,
    reservation_amount: u64,
}

impl ParsedDigest {
    pub fn epoch_id(&self) -> EpochId {
        self.epoch_id as EpochId
    }

    pub fn reservation_amount(&self) -> u64 {
        self.reservation_amount
    }

    pub fn is_coin_reservation_digest(digest: &ObjectDigest) -> bool {
        let inner = digest.inner();
        // check if the last 20 bytes of digest match the magic number
        let last_20_bytes: &[u8; 20] = inner[12..32].try_into().unwrap();
        *last_20_bytes == COIN_RESERVATION_MAGIC
    }
}

#[derive(Debug, Error)]
#[error("Invalid digest")]
pub struct ParsedDigestError;

impl TryFrom<ObjectDigest> for ParsedDigest {
    type Error = ParsedDigestError;

    fn try_from(digest: ObjectDigest) -> Result<Self, Self::Error> {
        if ParsedDigest::is_coin_reservation_digest(&digest) {
            let inner = digest.inner();
            let reservation_amount_bytes: &[u8; 8] = inner[0..8].try_into().unwrap();
            let epoch_bytes: &[u8; 4] = inner[8..12].try_into().unwrap();

            let epoch_id = u32::from_le_bytes(*epoch_bytes);
            let reservation_amount = u64::from_le_bytes(*reservation_amount_bytes);

            Ok(Self {
                epoch_id,
                reservation_amount,
            })
        } else {
            Err(ParsedDigestError)
        }
    }
}

impl From<ParsedDigest> for ObjectDigest {
    fn from(parsed: ParsedDigest) -> Self {
        let mut inner = [0; 32];
        inner[0..8].copy_from_slice(&parsed.reservation_amount.to_le_bytes());
        inner[8..12].copy_from_slice(&parsed.epoch_id.to_le_bytes());
        inner[12..32].copy_from_slice(&COIN_RESERVATION_MAGIC);
        ObjectDigest::new(inner)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParsedObjectRefWithdrawal {
    pub unmasked_object_id: ObjectID,
    pub parsed_digest: ParsedDigest,
}

impl ParsedObjectRefWithdrawal {
    pub fn new(unmasked_object_id: ObjectID, epoch_id: EpochId, reservation_amount: u64) -> Self {
        Self {
            unmasked_object_id,
            parsed_digest: ParsedDigest {
                epoch_id: epoch_id.try_into().unwrap(),
                reservation_amount,
            },
        }
    }

    pub fn reservation_amount(&self) -> u64 {
        self.parsed_digest.reservation_amount()
    }

    pub fn epoch_id(&self) -> EpochId {
        self.parsed_digest.epoch_id()
    }

    pub fn encode(&self, version: SequenceNumber, chain_identifier: ChainIdentifier) -> ObjectRef {
        let digest = self.parsed_digest.into();
        let masked_id = mask_or_unmask_id(self.unmasked_object_id, chain_identifier);
        (masked_id, version, digest)
    }

    pub fn parse(object_ref: &ObjectRef, chain_identifier: ChainIdentifier) -> Option<Self> {
        let (object_id, _version, digest) = object_ref;
        let parsed_digest = ParsedDigest::try_from(*digest).ok()?;

        let unmasked_object_id = mask_or_unmask_id(*object_id, chain_identifier);

        Some(ParsedObjectRefWithdrawal {
            unmasked_object_id,
            parsed_digest,
        })
    }
}

pub fn mask_or_unmask_id(object_id: ObjectID, chain_identifier: ChainIdentifier) -> ObjectID {
    let mask_bytes: &[u8; 32] = chain_identifier.as_bytes();

    let object_id_bytes: [u8; 32] = object_id.into_bytes();
    let mut masked_object_id_bytes = [0; 32];
    for i in 0..32 {
        masked_object_id_bytes[i] = object_id_bytes[i] ^ mask_bytes[i];
    }
    ObjectID::new(masked_object_id_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_normal_digest() {
        let digest = ObjectDigest::new([0; 32]);
        assert!(ParsedDigest::try_from(digest).is_err());
    }

    #[test]
    fn test_is_coin_reservation_digest() {
        let digest = ObjectDigest::random();
        assert!(!ParsedDigest::is_coin_reservation_digest(&digest));

        let digest = ParsedDigest {
            epoch_id: 42,
            reservation_amount: 1232348999,
        }
        .into();
        assert!(ParsedDigest::is_coin_reservation_digest(&digest));
    }

    #[test]
    fn test_encode_and_parse_digest() {
        let parsed_digest = ParsedDigest {
            epoch_id: 42,
            reservation_amount: 1232348999,
        };

        let digest = ObjectDigest::from(parsed_digest);
        assert_eq!(parsed_digest, ParsedDigest::try_from(digest).unwrap());
    }

    #[test]
    fn test_parse_object_ref() {
        let object_ref = (
            ObjectID::new([0; 32]),
            SequenceNumber::new(),
            ObjectDigest::new([0; 32]),
        );

        assert!(
            ParsedObjectRefWithdrawal::parse(&object_ref, ChainIdentifier::default()).is_none()
        );
    }

    #[test]
    fn test_parse_object_ref_with_valid_digest() {
        let chain_id = ChainIdentifier::random();

        let id = ObjectID::random();
        let parsed_obj_ref = ParsedObjectRefWithdrawal {
            unmasked_object_id: id,
            parsed_digest: ParsedDigest {
                epoch_id: 42,
                reservation_amount: 1232348999,
            },
        };
        let encoded_obj_ref = parsed_obj_ref.encode(SequenceNumber::new(), chain_id);

        assert_ne!(encoded_obj_ref.0, id, "object id should be masked");

        let parsed_obj_ref = ParsedObjectRefWithdrawal::parse(&encoded_obj_ref, chain_id).unwrap();
        assert_eq!(parsed_obj_ref.unmasked_object_id, id);
        assert_eq!(parsed_obj_ref.parsed_digest.epoch_id, 42);
        assert_eq!(parsed_obj_ref.parsed_digest.reservation_amount, 1232348999);
    }
}
