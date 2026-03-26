// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(tidehunter)]
use sui_types::base_types::{ObjectID, SequenceNumber};
use sui_types::committee::EpochId;
#[cfg(tidehunter)]
use sui_types::storage::ConsensusObjectKey;
use sui_types::storage::FullObjectKey;
#[cfg(tidehunter)]
use sui_types::storage::ObjectKey;

/// A key type for `object_per_epoch_marker_table_v2`.
///
/// Wraps `(EpochId, FullObjectKey)` with two distinct serialization formats selected at
/// compile time via the `tidehunter` cfg flag:
///
/// - **Without `tidehunter`**: delegates to the `(EpochId, FullObjectKey)` tuple encoding,
///   preserving on-disk compatibility with existing RocksDB data.
///
/// - **With `tidehunter`**: produces a uniform 56-byte fixed-length encoding. Fixed-length
///   keys enable TideHunter's fast flat-buffer path and O(1) `drop_cells_in_range`
///   bulk-delete for epoch cleanup.
///
/// TideHunter key layout:
///   `[0..8]`   EpochId (u64, big-endian) with bit 63 = discriminant: 0=Fastpath, 1=Consensus
///   `[8..40]`  ObjectID (32 bytes)
///   `[40..48]` Consensus start version, or 0 padding for Fastpath (u64, big-endian)
///   `[48..56]` Object version (u64, big-endian)
///
/// The discriminant-in-MSB trick preserves natural sort order: all Fastpath keys for
/// epoch N sort before all Consensus keys for epoch N, which sort before epoch N+1.
/// Epoch IDs will never reach 2^63 in practice; serialization asserts this.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EpochMarkerKey(pub EpochId, pub FullObjectKey);

#[cfg(tidehunter)]
const CONSENSUS_BIT: u64 = 1 << 63;
#[cfg(tidehunter)]
pub(crate) const EPOCH_MARKER_KEY_SIZE: usize = 56;

#[cfg(tidehunter)]
impl EpochMarkerKey {
    pub(crate) const MIN_KEY: [u8; EPOCH_MARKER_KEY_SIZE] = [0x00; EPOCH_MARKER_KEY_SIZE];
    pub(crate) const MAX_KEY: [u8; EPOCH_MARKER_KEY_SIZE] = [0xFF; EPOCH_MARKER_KEY_SIZE];
}

/// Non-TideHunter: delegate to the tuple, matching the existing bincode encoding of
/// `(EpochId, FullObjectKey)` for RocksDB compatibility.
#[cfg(not(tidehunter))]
impl serde::Serialize for EpochMarkerKey {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        (&self.0, &self.1).serialize(s)
    }
}

#[cfg(not(tidehunter))]
impl<'de> serde::Deserialize<'de> for EpochMarkerKey {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let (epoch, key) = <(EpochId, FullObjectKey)>::deserialize(d)?;
        Ok(EpochMarkerKey(epoch, key))
    }
}

/// TideHunter: fixed 56-byte encoding so the table uses TideHunter's fast fixed-length path.
#[cfg(tidehunter)]
impl serde::Serialize for EpochMarkerKey {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::{Error, SerializeTuple};
        let EpochMarkerKey(epoch_id, full_key) = self;
        if *epoch_id >= CONSENSUS_BIT {
            return Err(S::Error::custom(format!(
                "EpochId {epoch_id} overflows the 63-bit epoch field"
            )));
        }
        let mut buf = [0u8; EPOCH_MARKER_KEY_SIZE];
        match full_key {
            FullObjectKey::Fastpath(ObjectKey(id, version)) => {
                buf[0..8].copy_from_slice(&epoch_id.to_be_bytes());
                buf[8..40].copy_from_slice(id.as_ref());
                // bytes [40..48] zeroed (no start_version for Fastpath)
                buf[48..56].copy_from_slice(&u64::from(*version).to_be_bytes());
            }
            FullObjectKey::Consensus(ConsensusObjectKey((id, start_version), version)) => {
                buf[0..8].copy_from_slice(&(epoch_id | CONSENSUS_BIT).to_be_bytes());
                buf[8..40].copy_from_slice(id.as_ref());
                buf[40..48].copy_from_slice(&u64::from(*start_version).to_be_bytes());
                buf[48..56].copy_from_slice(&u64::from(*version).to_be_bytes());
            }
        }
        let mut tup = s.serialize_tuple(EPOCH_MARKER_KEY_SIZE)?;
        for byte in &buf {
            tup.serialize_element(byte)?;
        }
        tup.end()
    }
}

#[cfg(tidehunter)]
impl<'de> serde::Deserialize<'de> for EpochMarkerKey {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use serde::de::{Error, SeqAccess, Visitor};
        use std::fmt;

        struct EpochMarkerKeyVisitor;
        impl<'de> Visitor<'de> for EpochMarkerKeyVisitor {
            type Value = EpochMarkerKey;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(
                    f,
                    "a {EPOCH_MARKER_KEY_SIZE}-byte fixed-length epoch marker key"
                )
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut buf = [0u8; EPOCH_MARKER_KEY_SIZE];
                for (i, b) in buf.iter_mut().enumerate() {
                    *b = seq
                        .next_element()?
                        .ok_or_else(|| A::Error::invalid_length(i, &self))?;
                }
                let epoch_word = u64::from_be_bytes(buf[0..8].try_into().unwrap());
                let is_consensus = (epoch_word & CONSENSUS_BIT) != 0;
                let epoch_id: EpochId = epoch_word & !CONSENSUS_BIT;
                let object_id = ObjectID::from_bytes(&buf[8..40])
                    .map_err(|e| A::Error::custom(format!("invalid ObjectID: {e}")))?;
                let start_version_raw = u64::from_be_bytes(buf[40..48].try_into().unwrap());
                let version =
                    SequenceNumber::from(u64::from_be_bytes(buf[48..56].try_into().unwrap()));
                let full_key = if is_consensus {
                    FullObjectKey::Consensus(ConsensusObjectKey(
                        (object_id, SequenceNumber::from(start_version_raw)),
                        version,
                    ))
                } else {
                    FullObjectKey::Fastpath(ObjectKey(object_id, version))
                };
                Ok(EpochMarkerKey(epoch_id, full_key))
            }
        }

        d.deserialize_tuple(EPOCH_MARKER_KEY_SIZE, EpochMarkerKeyVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sui_types::base_types::{ObjectID, SequenceNumber};
    use sui_types::storage::ObjectKey;
    use typed_store::be_fix_int_ser;

    #[cfg(tidehunter)]
    fn fastpath(epoch: EpochId, id: ObjectID, version: u64) -> EpochMarkerKey {
        EpochMarkerKey(
            epoch,
            FullObjectKey::Fastpath(ObjectKey(id, SequenceNumber::from(version))),
        )
    }

    #[cfg(tidehunter)]
    fn consensus(epoch: EpochId, id: ObjectID, start: u64, version: u64) -> EpochMarkerKey {
        EpochMarkerKey(
            epoch,
            FullObjectKey::Consensus(ConsensusObjectKey(
                (id, SequenceNumber::from(start)),
                SequenceNumber::from(version),
            )),
        )
    }

    #[cfg(not(tidehunter))]
    #[test]
    fn test_bincode_matches_tuple() {
        let id = ObjectID::random();
        let epoch = 42u64;
        let full_key = FullObjectKey::Fastpath(ObjectKey(id, SequenceNumber::from(7u64)));
        let marker_key = EpochMarkerKey(epoch, full_key);
        assert_eq!(
            be_fix_int_ser(&(epoch, full_key)),
            be_fix_int_ser(&marker_key),
        );
    }

    #[cfg(tidehunter)]
    #[test]
    fn test_key_size() {
        let id = ObjectID::random();
        assert_eq!(
            be_fix_int_ser(&fastpath(0, id, 0)).len(),
            EPOCH_MARKER_KEY_SIZE
        );
        assert_eq!(
            be_fix_int_ser(&consensus(0, id, 0, 0)).len(),
            EPOCH_MARKER_KEY_SIZE
        );
    }

    #[cfg(tidehunter)]
    #[test]
    fn test_fastpath_roundtrip() {
        use bincode::Options;
        let key = fastpath(42, ObjectID::random(), 100);
        let bytes = be_fix_int_ser(&key);
        let decoded: EpochMarkerKey = bincode::DefaultOptions::new()
            .with_big_endian()
            .with_fixint_encoding()
            .deserialize(&bytes)
            .unwrap();
        assert_eq!(decoded, key);
    }

    #[cfg(tidehunter)]
    #[test]
    fn test_consensus_roundtrip() {
        use bincode::Options;
        let key = consensus(42, ObjectID::random(), 5, 100);
        let bytes = be_fix_int_ser(&key);
        let decoded: EpochMarkerKey = bincode::DefaultOptions::new()
            .with_big_endian()
            .with_fixint_encoding()
            .deserialize(&bytes)
            .unwrap();
        assert_eq!(decoded, key);
    }

    #[cfg(tidehunter)]
    #[test]
    fn test_discriminant_bit_placement() {
        let id = ObjectID::random();
        let fp_bytes = be_fix_int_ser(&fastpath(42, id, 1));
        let cn_bytes = be_fix_int_ser(&consensus(42, id, 1, 1));
        assert_eq!(fp_bytes[0] & 0x80, 0x00);
        assert_eq!(cn_bytes[0] & 0x80, 0x80);
    }

    #[cfg(tidehunter)]
    #[test]
    fn test_epoch_ordering() {
        let id = ObjectID::random();
        assert!(be_fix_int_ser(&fastpath(1, id, 1)) < be_fix_int_ser(&fastpath(2, id, 1)));
    }

    #[cfg(tidehunter)]
    #[test]
    fn test_fastpath_sorts_before_consensus_same_epoch() {
        let id = ObjectID::random();
        assert!(be_fix_int_ser(&fastpath(42, id, 1)) < be_fix_int_ser(&consensus(42, id, 1, 1)));
    }
}
