// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::TryFromProtoError;

// Include the generated proto definitions
include!("../generated/sui.types.rs");

/// Byte encoded FILE_DESCRIPTOR_SET.
pub const FILE_DESCRIPTOR_SET: &[u8] = include_bytes!("../generated/sui.types.fds.bin");

#[cfg(test)]
mod tests {
    use super::FILE_DESCRIPTOR_SET;
    use prost::Message as _;

    #[test]
    fn file_descriptor_set_is_valid() {
        prost_types::FileDescriptorSet::decode(FILE_DESCRIPTOR_SET).unwrap();
    }
}

mod checkpoint;
mod effects;
mod events;
mod execution_status;
mod move_types;
mod object;
mod signatures;
mod transaction_convert;

//
// Address
//

impl From<sui_sdk_types::Address> for Address {
    fn from(value: sui_sdk_types::Address) -> Self {
        Self {
            address: Some(value.as_bytes().to_vec().into()),
        }
    }
}

impl TryFrom<&Address> for sui_sdk_types::Address {
    type Error = TryFromProtoError;

    fn try_from(Address { address }: &Address) -> Result<Self, Self::Error> {
        let address = address
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("address"))?
            .as_ref()
            .try_into()?;

        Ok(Self::new(address))
    }
}

//
// ObjectId
//

impl From<sui_sdk_types::ObjectId> for ObjectId {
    fn from(value: sui_sdk_types::ObjectId) -> Self {
        Self {
            object_id: Some(value.as_bytes().to_vec().into()),
        }
    }
}

impl TryFrom<&ObjectId> for sui_sdk_types::ObjectId {
    type Error = TryFromProtoError;

    fn try_from(ObjectId { object_id }: &ObjectId) -> Result<Self, Self::Error> {
        let object_id = object_id
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("object_id"))?
            .as_ref()
            .try_into()?;

        Ok(Self::new(object_id))
    }
}

//
// Digest
//

impl From<sui_sdk_types::Digest> for Digest {
    fn from(value: sui_sdk_types::Digest) -> Self {
        Self {
            digest: Some(value.as_bytes().to_vec().into()),
        }
    }
}

impl TryFrom<&Digest> for sui_sdk_types::Digest {
    type Error = TryFromProtoError;

    fn try_from(Digest { digest }: &Digest) -> Result<Self, Self::Error> {
        let digest = digest
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("digest"))?
            .as_ref()
            .try_into()?;

        Ok(Self::new(digest))
    }
}

macro_rules! impl_digest_proto {
    ($t:ident) => {
        impl From<sui_sdk_types::$t> for Digest {
            fn from(value: sui_sdk_types::$t) -> Self {
                sui_sdk_types::Digest::from(value).into()
            }
        }

        impl TryFrom<&Digest> for sui_sdk_types::$t {
            type Error = TryFromProtoError;

            fn try_from(digest: &Digest) -> Result<Self, Self::Error> {
                sui_sdk_types::Digest::try_from(digest).map(Into::into)
            }
        }
    };
}

impl_digest_proto!(CheckpointDigest);
impl_digest_proto!(CheckpointContentsDigest);
impl_digest_proto!(TransactionDigest);
impl_digest_proto!(TransactionEffectsDigest);
impl_digest_proto!(TransactionEventsDigest);
impl_digest_proto!(ObjectDigest);
impl_digest_proto!(ConsensusCommitDigest);
impl_digest_proto!(EffectsAuxiliaryDataDigest);

//
// TimeStamp
//

pub fn timestamp_ms_to_proto(timestamp_ms: u64) -> prost_types::Timestamp {
    let timestamp = std::time::Duration::from_millis(timestamp_ms);
    prost_types::Timestamp {
        seconds: timestamp.as_secs() as i64,
        nanos: timestamp.subsec_nanos() as i32,
    }
}

pub fn proto_to_timestamp_ms(timestamp: prost_types::Timestamp) -> Result<u64, TryFromProtoError> {
    let seconds = std::time::Duration::from_secs(timestamp.seconds.try_into()?);
    let nanos = std::time::Duration::from_nanos(timestamp.nanos.try_into()?);

    Ok((seconds + nanos).as_millis().try_into()?)
}

//
// Bcs
//

impl Bcs {
    pub fn serialize<T: serde::Serialize>(value: &T) -> Result<Self, bcs::Error> {
        bcs::to_bytes(value).map(|bcs| Self {
            bcs: Some(bcs.into()),
        })
    }

    pub fn deserialize<'de, T: serde::Deserialize<'de>>(&'de self) -> Result<T, bcs::Error> {
        bcs::from_bytes(self.bcs.as_deref().unwrap_or(&[]))
    }
}

impl From<Vec<u8>> for Bcs {
    fn from(value: Vec<u8>) -> Self {
        Self {
            bcs: Some(value.into()),
        }
    }
}

impl From<&Bcs> for Vec<u8> {
    fn from(value: &Bcs) -> Self {
        value
            .bcs
            .as_ref()
            .map(|bytes| bytes.to_vec())
            .unwrap_or_default()
    }
}

impl From<Bcs> for Vec<u8> {
    fn from(value: Bcs) -> Self {
        value
            .bcs
            .as_ref()
            .map(|bytes| bytes.to_vec())
            .unwrap_or_default()
    }
}

impl From<prost::bytes::Bytes> for Bcs {
    fn from(value: prost::bytes::Bytes) -> Self {
        Self { bcs: Some(value) }
    }
}

impl From<&Bcs> for prost::bytes::Bytes {
    fn from(value: &Bcs) -> Self {
        value.bcs.clone().unwrap_or_default()
    }
}

impl From<Bcs> for prost::bytes::Bytes {
    fn from(value: Bcs) -> Self {
        value.bcs.unwrap_or_default()
    }
}

//
// U128
//

impl From<u128> for U128 {
    fn from(value: u128) -> Self {
        Self {
            bytes: Some(value.to_le_bytes().to_vec().into()),
        }
    }
}

impl TryFrom<&U128> for u128 {
    type Error = std::array::TryFromSliceError;

    fn try_from(value: &U128) -> Result<Self, Self::Error> {
        Ok(u128::from_le_bytes(value.bytes().try_into()?))
    }
}

//
// I128
//

impl From<i128> for I128 {
    fn from(value: i128) -> Self {
        Self {
            bytes: Some(value.to_le_bytes().to_vec().into()),
        }
    }
}

impl TryFrom<&I128> for i128 {
    type Error = std::array::TryFromSliceError;

    fn try_from(value: &I128) -> Result<Self, Self::Error> {
        Ok(i128::from_le_bytes(value.bytes().try_into()?))
    }
}
