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
mod object;
mod signatures;
mod transaction_convert;

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
            value: Some(bcs.into()),
            name: None,
        })
    }

    pub fn deserialize<'de, T: serde::Deserialize<'de>>(&'de self) -> Result<T, bcs::Error> {
        bcs::from_bytes(self.value.as_deref().unwrap_or(&[]))
    }
}

impl From<Vec<u8>> for Bcs {
    fn from(value: Vec<u8>) -> Self {
        Self {
            value: Some(value.into()),
            name: None,
        }
    }
}

impl From<&Bcs> for Vec<u8> {
    fn from(value: &Bcs) -> Self {
        value
            .value
            .as_ref()
            .map(|bytes| bytes.to_vec())
            .unwrap_or_default()
    }
}

impl From<Bcs> for Vec<u8> {
    fn from(value: Bcs) -> Self {
        value
            .value
            .as_ref()
            .map(|bytes| bytes.to_vec())
            .unwrap_or_default()
    }
}

impl From<prost::bytes::Bytes> for Bcs {
    fn from(value: prost::bytes::Bytes) -> Self {
        Self {
            value: Some(value),
            name: None,
        }
    }
}

impl From<&Bcs> for prost::bytes::Bytes {
    fn from(value: &Bcs) -> Self {
        value.value.clone().unwrap_or_default()
    }
}

impl From<Bcs> for prost::bytes::Bytes {
    fn from(value: Bcs) -> Self {
        value.value.unwrap_or_default()
    }
}
