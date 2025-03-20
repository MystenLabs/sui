// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::TryFromProtoError;
use prost_types::FieldMask;
use tap::Pipe;

pub mod v2 {
    include!("generated/sui.node.v2.rs");

    /// Byte encoded FILE_DESCRIPTOR_SET.
    pub const FILE_DESCRIPTOR_SET: &[u8] = include_bytes!("generated/sui.node.v2.fds.bin");

    #[cfg(test)]
    mod tests {
        use super::FILE_DESCRIPTOR_SET;
        use prost::Message as _;

        #[test]
        fn file_descriptor_set_is_valid() {
            prost_types::FileDescriptorSet::decode(FILE_DESCRIPTOR_SET).unwrap();
        }
    }
}

use v2::*;

//
// BalanceChange
//

impl From<sui_sdk_types::BalanceChange> for BalanceChange {
    fn from(value: sui_sdk_types::BalanceChange) -> Self {
        Self {
            address: Some(value.address.into()),
            coin_type: Some(value.coin_type.into()),
            amount: Some(value.amount.into()),
        }
    }
}

impl TryFrom<&BalanceChange> for sui_sdk_types::BalanceChange {
    type Error = TryFromProtoError;

    fn try_from(value: &BalanceChange) -> Result<Self, Self::Error> {
        let address = value
            .address
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("address"))?
            .pipe(TryInto::try_into)?;
        let coin_type = value
            .coin_type
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("coin_type"))?
            .pipe(TryInto::try_into)?;
        let amount = value
            .amount
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("amount"))?
            .pipe(TryInto::try_into)?;
        Ok(Self {
            address,
            coin_type,
            amount,
        })
    }
}

//
// GetObjectRequest
//

impl GetObjectRequest {
    pub const READ_MASK_DEFAULT: &str = "object_id,version,digest";

    pub fn new<T: Into<super::types::ObjectId>>(object_id: T) -> Self {
        Self {
            object_id: Some(object_id.into()),
            version: None,
            read_mask: None,
        }
    }

    pub fn with_version(mut self, version: u64) -> Self {
        self.version = Some(version);
        self
    }

    pub fn with_read_mask(mut self, read_mask: FieldMask) -> Self {
        self.read_mask = Some(read_mask);
        self
    }
}

//
// GetObjectResponse
//

impl GetObjectResponse {
    pub fn validate_read_mask(read_mask: &FieldMask) -> Result<(), &str> {
        for path in &read_mask.paths {
            match path.as_str() {
                "object_id" | "version" | "digest" | "object" | "object_bcs" => {}
                path => {
                    return Err(path);
                }
            }
        }

        Ok(())
    }
}

//
// GetCheckpointRequest
//

impl GetCheckpointRequest {
    pub const READ_MASK_DEFAULT: &str = "sequence_number,digest";

    pub fn latest() -> Self {
        Self {
            sequence_number: None,
            digest: None,
            read_mask: None,
        }
    }

    pub fn by_digest<T: Into<super::types::Digest>>(digest: T) -> Self {
        Self {
            sequence_number: None,
            digest: Some(digest.into()),
            read_mask: None,
        }
    }

    pub fn by_sequence_number(sequence_number: u64) -> Self {
        Self {
            sequence_number: Some(sequence_number),
            digest: None,
            read_mask: None,
        }
    }

    pub fn with_read_mask(mut self, read_mask: FieldMask) -> Self {
        self.read_mask = Some(read_mask);
        self
    }
}

//
// GetTransactionRequest
//

impl GetTransactionRequest {
    pub const READ_MASK_DEFAULT: &str = "digest";

    pub fn new<T: Into<super::types::Digest>>(digest: T) -> Self {
        Self {
            digest: Some(digest.into()),
            read_mask: None,
        }
    }

    pub fn with_read_mask(mut self, read_mask: FieldMask) -> Self {
        self.read_mask = Some(read_mask);
        self
    }
}

//
// GetTransactionResponse
//

impl GetTransactionResponse {
    pub fn validate_read_mask(read_mask: &FieldMask) -> Result<(), &str> {
        for path in &read_mask.paths {
            match path.as_str() {
                "digest" | "transaction" | "transaction_bcs" | "signatures"
                | "signatures_bytes" | "effects" | "effects_bcs" | "events" | "events_bcs"
                | "checkpoint" | "timestamp" => {}
                path => {
                    return Err(path);
                }
            }
        }

        Ok(())
    }
}

//
// GetFullCheckpointRequest
//

impl GetFullCheckpointRequest {
    pub const READ_MASK_DEFAULT: &str = "sequence_number,digest";

    pub fn latest() -> Self {
        Self {
            sequence_number: None,
            digest: None,
            read_mask: None,
        }
    }

    pub fn by_digest<T: Into<super::types::Digest>>(digest: T) -> Self {
        Self {
            sequence_number: None,
            digest: Some(digest.into()),
            read_mask: None,
        }
    }

    pub fn by_sequence_number(sequence_number: u64) -> Self {
        Self {
            sequence_number: Some(sequence_number),
            digest: None,
            read_mask: None,
        }
    }

    pub fn with_read_mask(mut self, read_mask: FieldMask) -> Self {
        self.read_mask = Some(read_mask);
        self
    }
}

//
// CheckpointResponse
//

impl GetCheckpointResponse {
    pub fn validate_read_mask(read_mask: &FieldMask) -> Result<(), &str> {
        for path in &read_mask.paths {
            match path.as_str() {
                "sequence_number" | "digest" | "summary" | "summary_bcs" | "signature"
                | "contents" | "contents_bcs" => {}
                path => {
                    return Err(path);
                }
            }
        }

        Ok(())
    }
}

//
// FullCheckpointResponse
//

impl GetFullCheckpointResponse {
    pub fn validate_read_mask(read_mask: &FieldMask) -> Result<(), &str> {
        for path in &read_mask.paths {
            if !Self::validate_field_path(path) {
                return Err(path);
            }
        }

        Ok(())
    }

    pub fn validate_field_path(path: &str) -> bool {
        if let Some(remaining) = path.strip_prefix("transactions.") {
            return FullCheckpointTransaction::validate_field_path(remaining);
        }

        matches!(
            path,
            "sequence_number"
                | "digest"
                | "summary"
                | "summary_bcs"
                | "signature"
                | "contents"
                | "contents_bcs"
                | "transactions"
        )
    }
}

//
// FullCheckpointTransaction
//

impl FullCheckpointTransaction {
    pub fn validate_field_path(path: &str) -> bool {
        if let Some(remaining) = path.strip_prefix("input_objects.") {
            return FullCheckpointObject::validate_field_path(remaining);
        }

        if let Some(remaining) = path.strip_prefix("output_objects.") {
            return FullCheckpointObject::validate_field_path(remaining);
        }

        matches!(
            path,
            "digest"
                | "transaction"
                | "transaction_bcs"
                | "effects"
                | "effects_bcs"
                | "events"
                | "events_bcs"
                | "input_objects"
                | "output_objects"
        )
    }
}

//
// FullCheckpointObject
//

impl FullCheckpointObject {
    pub fn validate_field_path(path: &str) -> bool {
        matches!(
            path,
            "object_id" | "version" | "digest" | "object" | "object_bcs"
        )
    }
}

//
// ExecuteTransactionRequest
//

impl ExecuteTransactionRequest {
    pub const READ_MASK_DEFAULT: &str = "effects,events,finality";
}

//
// ExecuteTransactionResponse
//

impl ExecuteTransactionResponse {
    pub fn validate_read_mask(read_mask: &FieldMask) -> Result<(), &str> {
        for path in &read_mask.paths {
            match path.as_str() {
                "finality" | "effects" | "effects_bcs" | "events" | "events_bcs"
                | "balance_changes" => {}
                path => {
                    return Err(path);
                }
            }
        }

        Ok(())
    }
}
