// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::message::{MessageField, MessageFields};

// Include the generated proto definitions
include!("../../generated/sui.rpc.v2beta.rs");

/// Byte encoded FILE_DESCRIPTOR_SET.
pub const FILE_DESCRIPTOR_SET: &[u8] = include_bytes!("../../generated/sui.rpc.v2beta.fds.bin");

#[cfg(test)]
mod tests {
    use super::FILE_DESCRIPTOR_SET;
    use prost::Message as _;

    #[test]
    fn file_descriptor_set_is_valid() {
        prost_types::FileDescriptorSet::decode(FILE_DESCRIPTOR_SET).unwrap();
    }
}

mod balance_change;
mod checkpoint;
mod effects;
mod events;
mod execute_transaction;
mod executed_transaction;
mod execution_status;
mod object;
mod signatures;
mod transaction;

//
// Bcs
//

impl Bcs {
    const NAME_FIELD: &'static MessageField = &MessageField::new("name");
    const VALUE_FIELD: &'static MessageField = &MessageField::new("value");
}

impl MessageFields for Bcs {
    const FIELDS: &'static [&'static MessageField] = &[Self::NAME_FIELD, Self::VALUE_FIELD];
}

impl Bcs {
    pub fn serialize<T: serde::Serialize>(value: &T) -> Result<Self, bcs::Error> {
        bcs::to_bytes(value).map(|bcs| Self {
            name: None,
            value: Some(bcs.into()),
        })
    }

    pub fn deserialize<'de, T: serde::Deserialize<'de>>(&'de self) -> Result<T, bcs::Error> {
        bcs::from_bytes(self.value.as_deref().unwrap_or(&[]))
    }
}

impl From<Vec<u8>> for Bcs {
    fn from(value: Vec<u8>) -> Self {
        Self {
            name: None,
            value: Some(value.into()),
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
            name: None,
            value: Some(value),
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
