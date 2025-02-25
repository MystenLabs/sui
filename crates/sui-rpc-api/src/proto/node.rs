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

pub mod v2alpha {
    include!("generated/sui.node.v2alpha.rs");

    /// Byte encoded FILE_DESCRIPTOR_SET.
    pub const FILE_DESCRIPTOR_SET: &[u8] = include_bytes!("generated/sui.node.v2alpha.fds.bin");

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

impl From<crate::types::FullCheckpointResponse> for GetFullCheckpointResponse {
    fn from(
        crate::types::FullCheckpointResponse {
            sequence_number,
            digest,
            summary,
            summary_bcs,
            signature,
            contents,
            contents_bcs,
            transactions,
        }: crate::types::FullCheckpointResponse,
    ) -> Self {
        Self {
            sequence_number: Some(sequence_number),
            digest: Some(digest.into()),
            summary: summary.map(Into::into),
            summary_bcs: summary_bcs.map(Into::into),
            signature: signature.map(Into::into),
            contents: contents.map(Into::into),
            contents_bcs: contents_bcs.map(Into::into),
            transactions: transactions.into_iter().map(Into::into).collect(),
        }
    }
}

impl TryFrom<&GetFullCheckpointResponse> for crate::types::FullCheckpointResponse {
    type Error = TryFromProtoError;

    fn try_from(
        GetFullCheckpointResponse {
            sequence_number,
            digest,
            summary,
            summary_bcs,
            signature,
            contents,
            contents_bcs,
            transactions,
        }: &GetFullCheckpointResponse,
    ) -> Result<Self, Self::Error> {
        let sequence_number =
            sequence_number.ok_or_else(|| TryFromProtoError::missing("sequence_number"))?;
        let digest = digest
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("digest"))?
            .pipe(TryInto::try_into)?;

        let summary = summary.as_ref().map(TryInto::try_into).transpose()?;
        let summary_bcs = summary_bcs.as_ref().map(Into::into);

        let signature = signature.as_ref().map(TryInto::try_into).transpose()?;

        let contents = contents.as_ref().map(TryInto::try_into).transpose()?;
        let contents_bcs = contents_bcs.as_ref().map(Into::into);

        let transactions = transactions
            .iter()
            .map(TryInto::try_into)
            .collect::<Result<_, _>>()?;

        Self {
            sequence_number,
            digest,
            summary,
            summary_bcs,
            signature,
            contents,
            contents_bcs,
            transactions,
        }
        .pipe(Ok)
    }
}

//
// FullCheckpointObject
//

impl From<crate::types::FullCheckpointObject> for FullCheckpointObject {
    fn from(
        crate::types::FullCheckpointObject {
            object_id,
            version,
            digest,
            object,
            object_bcs,
        }: crate::types::FullCheckpointObject,
    ) -> Self {
        Self {
            object_id: Some(object_id.into()),
            version: Some(version),
            digest: Some(digest.into()),
            object: object.map(Into::into),
            object_bcs: object_bcs.map(Into::into),
        }
    }
}

impl TryFrom<&FullCheckpointObject> for crate::types::FullCheckpointObject {
    type Error = TryFromProtoError;

    fn try_from(
        FullCheckpointObject {
            object_id,
            version,
            digest,
            object,
            object_bcs,
        }: &FullCheckpointObject,
    ) -> Result<Self, Self::Error> {
        let object_id = object_id
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("object_id"))?
            .pipe(TryInto::try_into)?;
        let version = version.ok_or_else(|| TryFromProtoError::missing("version"))?;
        let digest = digest
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("digest"))?
            .pipe(TryInto::try_into)?;

        let object = object.as_ref().map(TryInto::try_into).transpose()?;
        let object_bcs = object_bcs.as_ref().map(Into::into);

        Self {
            object_id,
            version,
            digest,
            object,
            object_bcs,
        }
        .pipe(Ok)
    }
}

//
// FullCheckpointTransaction
//

impl From<crate::types::FullCheckpointTransaction> for FullCheckpointTransaction {
    fn from(
        crate::types::FullCheckpointTransaction {
            digest,
            transaction,
            transaction_bcs,
            effects,
            effects_bcs,
            events,
            events_bcs,
            input_objects,
            output_objects,
        }: crate::types::FullCheckpointTransaction,
    ) -> Self {
        let input_objects = input_objects
            .map(|objects| objects.into_iter().map(Into::into).collect())
            .unwrap_or_default();
        let output_objects = output_objects
            .map(|objects| objects.into_iter().map(Into::into).collect())
            .unwrap_or_default();
        Self {
            digest: Some(digest.into()),
            transaction: transaction.map(Into::into),
            transaction_bcs: transaction_bcs.map(Into::into),
            effects: effects.map(Into::into),
            effects_bcs: effects_bcs.map(Into::into),
            events: events.map(Into::into),
            events_bcs: events_bcs.map(Into::into),
            input_objects,
            output_objects,
        }
    }
}

impl TryFrom<&FullCheckpointTransaction> for crate::types::FullCheckpointTransaction {
    type Error = TryFromProtoError;

    fn try_from(
        FullCheckpointTransaction {
            digest,
            transaction,
            transaction_bcs,
            effects,
            effects_bcs,
            events,
            events_bcs,
            input_objects,
            output_objects,
        }: &FullCheckpointTransaction,
    ) -> Result<Self, Self::Error> {
        let digest = digest
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("digest"))?
            .pipe(TryInto::try_into)?;

        let transaction = transaction.as_ref().map(TryInto::try_into).transpose()?;
        let transaction_bcs = transaction_bcs.as_ref().map(Into::into);

        let effects = effects.as_ref().map(TryInto::try_into).transpose()?;
        let effects_bcs = effects_bcs.as_ref().map(Into::into);

        let events = events.as_ref().map(TryInto::try_into).transpose()?;
        let events_bcs = events_bcs.as_ref().map(Into::into);

        let input_objects = input_objects
            .iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()?;
        let input_objects = if input_objects.is_empty() {
            None
        } else {
            Some(input_objects)
        };

        let output_objects = output_objects
            .iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()?;
        let output_objects = if output_objects.is_empty() {
            None
        } else {
            Some(output_objects)
        };

        Self {
            digest,
            transaction,
            transaction_bcs,
            effects,
            effects_bcs,
            events,
            events_bcs,
            input_objects,
            output_objects,
        }
        .pipe(Ok)
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
