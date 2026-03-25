// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_types::base_types::{ObjectID, SequenceNumber};
use sui_types::committee::EpochId;
use sui_types::storage::{ConsensusObjectKey, FullObjectKey, ObjectKey};

/// Fixed 56-byte key codec for `(EpochId, FullObjectKey)` in TideHunter.
///
/// The standard bincode encoding is variable-length because `FullObjectKey::Fastpath` (52 bytes)
/// and `FullObjectKey::Consensus` (60 bytes) differ in size. This codec produces a uniform
/// 56-byte key by stealing the MSB of the EpochId field for a discriminant bit.
/// Epoch IDs are guaranteed never to reach 2^63, so this bit is always zero in practice.
///
/// Key layout:
///   `[0..8]`   EpochId (u64, big-endian) with bit 63 = discriminant: 0=Fastpath, 1=Consensus
///   `[8..40]`  ObjectID (32 bytes)
///   `[40..48]` Consensus start version, or 0 padding for Fastpath (u64, big-endian)
///   `[48..56]` Object version (u64, big-endian)
pub(crate) struct EpochMarkerKeyCodec;

const CONSENSUS_BIT: u64 = 1 << 63;

impl EpochMarkerKeyCodec {
    pub(crate) const KEY_SIZE: usize = 56;
    pub(crate) const MIN_KEY: [u8; Self::KEY_SIZE] = [0x00; Self::KEY_SIZE];
    pub(crate) const MAX_KEY: [u8; Self::KEY_SIZE] = [0xFF; Self::KEY_SIZE];

    pub(crate) fn encode(key: &(EpochId, FullObjectKey)) -> Vec<u8> {
        let mut buf = [0u8; Self::KEY_SIZE];
        let (epoch_id, full_key) = key;
        assert!(
            *epoch_id < CONSENSUS_BIT,
            "EpochId {epoch_id} overflows the 63-bit epoch field"
        );
        match full_key {
            FullObjectKey::Fastpath(ObjectKey(id, version)) => {
                // MSB of epoch word = 0 (Fastpath)
                buf[0..8].copy_from_slice(&epoch_id.to_be_bytes());
                buf[8..40].copy_from_slice(id.as_ref());
                // bytes [40..48] already zeroed (no start_version for fastpath)
                buf[48..56].copy_from_slice(&u64::from(*version).to_be_bytes());
            }
            FullObjectKey::Consensus(ConsensusObjectKey((id, start_version), version)) => {
                // MSB of epoch word = 1 (Consensus)
                buf[0..8].copy_from_slice(&(epoch_id | CONSENSUS_BIT).to_be_bytes());
                buf[8..40].copy_from_slice(id.as_ref());
                buf[40..48].copy_from_slice(&u64::from(*start_version).to_be_bytes());
                buf[48..56].copy_from_slice(&u64::from(*version).to_be_bytes());
            }
        }
        buf.to_vec()
    }

    pub(crate) fn decode(bytes: Vec<u8>) -> (EpochId, FullObjectKey) {
        assert_eq!(
            bytes.len(),
            Self::KEY_SIZE,
            "EpochMarkerKeyCodec::decode: expected {} bytes, got {}",
            Self::KEY_SIZE,
            bytes.len()
        );
        let epoch_word = u64::from_be_bytes(bytes[0..8].try_into().unwrap());
        let is_consensus = (epoch_word & CONSENSUS_BIT) != 0;
        let epoch_id: EpochId = epoch_word & !CONSENSUS_BIT;
        let object_id = ObjectID::from_bytes(&bytes[8..40])
            .expect("EpochMarkerKeyCodec::decode: invalid ObjectID bytes");
        let start_version_raw = u64::from_be_bytes(bytes[40..48].try_into().unwrap());
        let version = SequenceNumber::from(u64::from_be_bytes(bytes[48..56].try_into().unwrap()));
        if is_consensus {
            let start_version = SequenceNumber::from(start_version_raw);
            (
                epoch_id,
                FullObjectKey::Consensus(ConsensusObjectKey((object_id, start_version), version)),
            )
        } else {
            (
                epoch_id,
                FullObjectKey::Fastpath(ObjectKey(object_id, version)),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fastpath(epoch: EpochId, id: ObjectID, version: u64) -> (EpochId, FullObjectKey) {
        (
            epoch,
            FullObjectKey::Fastpath(ObjectKey(id, SequenceNumber::from(version))),
        )
    }

    fn consensus(
        epoch: EpochId,
        id: ObjectID,
        start: u64,
        version: u64,
    ) -> (EpochId, FullObjectKey) {
        (
            epoch,
            FullObjectKey::Consensus(ConsensusObjectKey(
                (id, SequenceNumber::from(start)),
                SequenceNumber::from(version),
            )),
        )
    }

    #[test]
    fn test_key_size() {
        let id = ObjectID::random();
        assert_eq!(
            EpochMarkerKeyCodec::encode(&fastpath(0, id, 0)).len(),
            EpochMarkerKeyCodec::KEY_SIZE
        );
        assert_eq!(
            EpochMarkerKeyCodec::encode(&consensus(0, id, 0, 0)).len(),
            EpochMarkerKeyCodec::KEY_SIZE
        );
    }

    #[test]
    fn test_fastpath_roundtrip() {
        let key = fastpath(42, ObjectID::random(), 100);
        assert_eq!(
            EpochMarkerKeyCodec::decode(EpochMarkerKeyCodec::encode(&key)),
            key
        );
    }

    #[test]
    fn test_consensus_roundtrip() {
        let key = consensus(42, ObjectID::random(), 5, 100);
        assert_eq!(
            EpochMarkerKeyCodec::decode(EpochMarkerKeyCodec::encode(&key)),
            key
        );
    }

    #[test]
    fn test_zero_epoch_and_version() {
        let id = ObjectID::random();
        let fp = fastpath(0, id, 0);
        assert_eq!(
            EpochMarkerKeyCodec::decode(EpochMarkerKeyCodec::encode(&fp)),
            fp
        );
        let cn = consensus(0, id, 0, 0);
        assert_eq!(
            EpochMarkerKeyCodec::decode(EpochMarkerKeyCodec::encode(&cn)),
            cn
        );
    }

    #[test]
    fn test_max_valid_epoch() {
        let id = ObjectID::random();
        let max_epoch = CONSENSUS_BIT - 1;
        let key = fastpath(max_epoch, id, 1);
        assert_eq!(
            EpochMarkerKeyCodec::decode(EpochMarkerKeyCodec::encode(&key)),
            key
        );
    }

    #[test]
    fn test_discriminant_bit_placement() {
        let id = ObjectID::random();
        let fp_bytes = EpochMarkerKeyCodec::encode(&fastpath(42, id, 1));
        let cn_bytes = EpochMarkerKeyCodec::encode(&consensus(42, id, 1, 1));
        // MSB of byte 0: 0 for Fastpath, 1 for Consensus
        assert_eq!(fp_bytes[0] & 0x80, 0x00);
        assert_eq!(cn_bytes[0] & 0x80, 0x80);
        // Remaining epoch bits are identical
        let fp_epoch = u64::from_be_bytes(fp_bytes[0..8].try_into().unwrap());
        let cn_epoch = u64::from_be_bytes(cn_bytes[0..8].try_into().unwrap()) & !CONSENSUS_BIT;
        assert_eq!(fp_epoch, 42);
        assert_eq!(cn_epoch, 42);
    }

    #[test]
    fn test_epoch_ordering() {
        let id = ObjectID::random();
        // Larger epoch sorts after smaller epoch for same object/variant.
        let e1 = EpochMarkerKeyCodec::encode(&fastpath(1, id, 1));
        let e2 = EpochMarkerKeyCodec::encode(&fastpath(2, id, 1));
        assert!(e1 < e2);
    }

    #[test]
    fn test_fastpath_sorts_before_consensus_same_epoch() {
        // Fastpath (discriminant 0) must sort before Consensus (discriminant 1)
        // for the same epoch so epoch-range scans can use a clean split.
        let id = ObjectID::random();
        let fp = EpochMarkerKeyCodec::encode(&fastpath(42, id, 1));
        let cn = EpochMarkerKeyCodec::encode(&consensus(42, id, 1, 1));
        assert!(fp < cn);
    }

    #[test]
    #[should_panic(expected = "overflows the 63-bit epoch field")]
    fn test_epoch_overflow_panics() {
        EpochMarkerKeyCodec::encode(&fastpath(CONSENSUS_BIT, ObjectID::random(), 0));
    }

    #[test]
    #[should_panic]
    fn test_decode_wrong_length_panics() {
        EpochMarkerKeyCodec::decode(vec![0u8; 55]);
    }
}
