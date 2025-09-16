// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This module defines the protocol for specifying an address balance reservation
//! via an ObjectRef, in order to provide backward compatibility for clients that do
//! not understand address balances.
//! The layout of the reservation ObjectRef is as follows:
//!
//! (ObjectID, SequenceNumber, ObjectDigest)
//!
//! The ObjectID points to an on-chain object which identifies the type of the address balance.
//! (e.g. SUI, USDC, etc.). It is
//!
//! The SequenceNumber is a monotonically increasing version number, typically the version of the
//! accumulator root object. It is not used by the protocol, but is intended to help the
//! caching behavior of old clients.
//!
//! ObjectDigest contains three things:
//!
//! 1. The amount of the reservation [8 bytes]
//! 2. The epoch(s) in which the tx is valid [4 bytes]
//! 3. A magic number to identify this ObjectRef as a coin reservation [20 bytes]

use crate::{
    base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress},
    committee::EpochId,
    digests::ObjectDigest,
    error::{SuiError, SuiErrorKind, UserInputResult},
    transaction::FundsWithdrawalArg,
};

/// Trait for resolving funds withdrawal from a coin reservation
pub trait CoinReservationResolverTrait {
    // Used to check validity of the transaction. If the coin_reservation does not
    // point to an existing accumulator object, the transaction will be rejected.
    fn resolve_funds_withdrawal(
        &self,
        // TODO(address-balances): Should we support sponsored withdrawals here?
        // verify that the coin_reservation points to an existing accumulator object owned by the sender's address
        sender: SuiAddress,
        coin_reservation: ObjectRef,
    ) -> UserInputResult<FundsWithdrawalArg>;
}

// Derived with: echo "accumulator id xor mask" | sha256sum
// This mask is applied to the ID field in order to prevent clients from looking up
// the ID and being confused by what it points to.
const ID_XOR_MASK: [u8; 32] = [
    0xbe, 0x99, 0xb0, 0xc3, 0xab, 0xc6, 0xbe, 0x91, 0xc8, 0x10, 0xc0, 0xc4, 0x75, 0x57, 0x05, 0x07,
    0xeb, 0x6a, 0xe9, 0x26, 0x95, 0xbe, 0xa7, 0x2e, 0xec, 0x52, 0xd2, 0x6c, 0x15, 0x8b, 0x5d, 0x74,
];

pub const COIN_RESERVATION_MAGIC: [u8; 20] = [
    0xac, 0xac, 0xac, 0xac, 0xac, 0xac, 0xac, 0xac, 0xac, 0xac, 0xac, 0xac, 0xac, 0xac, 0xac, 0xac,
    0xac, 0xac, 0xac, 0xac,
];

pub fn parse_digest(digest: &ObjectDigest) -> Option<(EpochId, u64 /* reservation amount */)> {
    // check if the last 20 bytes of digest match the magic number
    let inner = digest.inner();
    let last_20_bytes: &[u8; 20] = inner[12..32].try_into().unwrap();
    if *last_20_bytes == COIN_RESERVATION_MAGIC {
        let reservation_amount_bytes: &[u8; 8] = inner[0..8].try_into().unwrap();
        let epoch_bytes: &[u8; 4] = inner[8..12].try_into().unwrap();

        let epoch_id = u32::from_le_bytes(*epoch_bytes) as u64;
        let reservation_amount = u64::from_le_bytes(*reservation_amount_bytes);

        Some((epoch_id, reservation_amount))
    } else {
        None
    }
}

pub fn is_coin_reservation_digest(digest: &ObjectDigest) -> bool {
    let inner = digest.inner();
    let last_20_bytes: &[u8; 20] = inner[12..32].try_into().unwrap();
    *last_20_bytes == COIN_RESERVATION_MAGIC
}

pub fn encode_digest(epoch_id: EpochId, reservation_amount: u64) -> Result<ObjectDigest, SuiError> {
    let mut inner = [0; 32];
    inner[0..8].copy_from_slice(&reservation_amount.to_le_bytes());

    // Backward compatibility system will stop working at epoch 2^32
    let epoch_id: u32 = epoch_id
        .try_into()
        .map_err(|_| SuiErrorKind::UnsupportedFeatureError {
            error: "Epochs larger than 2^32 are not supported".to_string(),
        })?;

    inner[8..12].copy_from_slice(&epoch_id.to_le_bytes());
    inner[12..32].copy_from_slice(&COIN_RESERVATION_MAGIC);
    Ok(ObjectDigest::new(inner))
}

pub struct ParsedObjectRefWithdrawal {
    pub unmasked_object_id: ObjectID,
    pub epoch_id: EpochId,
    pub reservation_amount: u64,
}

pub fn parse_object_ref(object_ref: &ObjectRef) -> Option<ParsedObjectRefWithdrawal> {
    let (object_id, _version, digest) = object_ref;
    let (epoch_id, reservation_amount) = parse_digest(digest)?;

    let object_id_bytes = object_id.into_bytes();
    let mut unmasked_object_id_bytes = [0; 32];
    for i in 0..32 {
        unmasked_object_id_bytes[i] = object_id_bytes[i] ^ ID_XOR_MASK[i];
    }

    let unmasked_object_id = ObjectID::new(unmasked_object_id_bytes);
    Some(ParsedObjectRefWithdrawal {
        unmasked_object_id,
        epoch_id,
        reservation_amount,
    })
}

pub fn encode_object_ref(
    object_id: ObjectID,
    sequence_number: SequenceNumber,
    epoch_id: EpochId,
    reservation_amount: u64,
) -> Result<ObjectRef, SuiError> {
    let digest = encode_digest(epoch_id, reservation_amount)?;
    let object_id_bytes = object_id.into_bytes();
    let mut masked_object_id_bytes = [0; 32];
    for i in 0..32 {
        masked_object_id_bytes[i] = object_id_bytes[i] ^ ID_XOR_MASK[i];
    }
    let masked_object_id = ObjectID::new(masked_object_id_bytes);
    Ok((masked_object_id, sequence_number, digest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_normal_digest() {
        let digest = ObjectDigest::new([0; 32]);
        assert!(parse_digest(&digest).is_none());
    }

    #[test]
    fn test_is_coin_reservation_digest() {
        let digest = ObjectDigest::random();
        assert!(!is_coin_reservation_digest(&digest));

        let digest = encode_digest(42, 1232348999).unwrap();
        assert!(is_coin_reservation_digest(&digest));
    }

    #[test]
    fn test_encode_and_parse_digest() {
        let original_epoch = 42;
        let original_reservation_amount = 1232348999;

        let digest = encode_digest(original_epoch, original_reservation_amount).unwrap();
        let (epoch_id, reservation_amount) = parse_digest(&digest).unwrap();
        assert_eq!(epoch_id, original_epoch);
        assert_eq!(reservation_amount, original_reservation_amount);
    }

    #[test]
    fn test_parse_object_ref() {
        let object_ref = (
            ObjectID::new([0; 32]),
            SequenceNumber::new(),
            ObjectDigest::new([0; 32]),
        );
        assert!(parse_object_ref(&object_ref).is_none());
    }

    #[test]
    fn test_parse_object_ref_with_valid_digest() {
        let id = ObjectID::random();
        let encoded_obj_ref = encode_object_ref(id, SequenceNumber::new(), 42, 1232348999).unwrap();

        assert_ne!(encoded_obj_ref.0, id, "object id should be masked");

        let parsed_obj_ref = parse_object_ref(&encoded_obj_ref).unwrap();
        assert_eq!(parsed_obj_ref.unmasked_object_id, id);
        assert_eq!(parsed_obj_ref.epoch_id, 42);
        assert_eq!(parsed_obj_ref.reservation_amount, 1232348999);
    }
}
