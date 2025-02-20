// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::types::proto_to_timestamp_ms;
use super::types::timestamp_ms_to_proto;
use super::TryFromProtoError;
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
// NodeInfo
//

impl From<crate::types::NodeInfo> for GetNodeInfoResponse {
    fn from(
        crate::types::NodeInfo {
            chain_id,
            chain,
            epoch,
            checkpoint_height,
            timestamp_ms,
            lowest_available_checkpoint,
            lowest_available_checkpoint_objects,
            software_version,
        }: crate::types::NodeInfo,
    ) -> Self {
        Self {
            chain_id: Some(chain_id.into()),
            chain: Some(chain.into()),
            epoch: Some(epoch),
            checkpoint_height: Some(checkpoint_height),
            timestamp: Some(timestamp_ms_to_proto(timestamp_ms)),
            lowest_available_checkpoint,
            lowest_available_checkpoint_objects,
            software_version: Some(software_version.into()),
        }
    }
}

impl TryFrom<&GetNodeInfoResponse> for crate::types::NodeInfo {
    type Error = TryFromProtoError;

    fn try_from(
        GetNodeInfoResponse {
            chain_id,
            chain,
            epoch,
            checkpoint_height,
            timestamp,
            lowest_available_checkpoint,
            lowest_available_checkpoint_objects,
            software_version,
        }: &GetNodeInfoResponse,
    ) -> Result<Self, Self::Error> {
        let chain_id = chain_id
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("chain_id"))?
            .pipe(TryInto::try_into)?;
        let chain = chain
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("chain"))?
            .to_owned()
            .into();
        let timestamp_ms = timestamp
            .ok_or_else(|| TryFromProtoError::missing("timestamp"))?
            .pipe(proto_to_timestamp_ms)?;

        let epoch = epoch.ok_or_else(|| TryFromProtoError::missing("epoch"))?;
        let checkpoint_height =
            checkpoint_height.ok_or_else(|| TryFromProtoError::missing("checkpoint_height"))?;

        let software_version = software_version
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("software_version"))?
            .to_owned()
            .into();

        Self {
            chain_id,
            chain,
            epoch,
            checkpoint_height,
            timestamp_ms,
            lowest_available_checkpoint: *lowest_available_checkpoint,
            lowest_available_checkpoint_objects: *lowest_available_checkpoint_objects,
            software_version,
        }
        .pipe(Ok)
    }
}

//
// GetObjectOptions
//

impl GetObjectOptions {
    pub fn all() -> Self {
        Self {
            object: Some(true),
            object_bcs: Some(true),
        }
    }

    pub fn none() -> Self {
        Self {
            object: Some(false),
            object_bcs: Some(false),
        }
    }

    pub fn with_object(mut self) -> Self {
        self.object = Some(true);
        self
    }

    pub fn without_object(mut self) -> Self {
        self.object = Some(false);
        self
    }

    pub fn with_object_bcs(mut self) -> Self {
        self.object_bcs = Some(true);
        self
    }

    pub fn without_object_bcs(mut self) -> Self {
        self.object = Some(false);
        self
    }
}

impl From<crate::types::GetObjectOptions> for GetObjectOptions {
    fn from(
        crate::types::GetObjectOptions { object, object_bcs }: crate::types::GetObjectOptions,
    ) -> Self {
        Self { object, object_bcs }
    }
}

impl From<GetObjectOptions> for crate::types::GetObjectOptions {
    fn from(GetObjectOptions { object, object_bcs }: GetObjectOptions) -> Self {
        Self { object, object_bcs }
    }
}

//
// GetObjectRequest
//

impl GetObjectRequest {
    pub fn new<T: Into<super::types::ObjectId>>(object_id: T) -> Self {
        Self {
            object_id: Some(object_id.into()),
            version: None,
            options: None,
            read_mask: None,
        }
    }

    pub fn with_version(mut self, version: u64) -> Self {
        self.version = Some(version);
        self
    }

    pub fn with_options(mut self, options: GetObjectOptions) -> Self {
        self.options = Some(options);
        self
    }
}

//
// ObjectResponse
//

impl From<crate::types::ObjectResponse> for GetObjectResponse {
    fn from(
        crate::types::ObjectResponse {
            object_id,
            version,
            digest,
            object,
            object_bcs,
        }: crate::types::ObjectResponse,
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

impl TryFrom<&GetObjectResponse> for crate::types::ObjectResponse {
    type Error = TryFromProtoError;

    fn try_from(
        GetObjectResponse {
            object_id,
            version,
            digest,
            object,
            object_bcs,
        }: &GetObjectResponse,
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
// GetCheckpointOptions
//

impl GetCheckpointOptions {
    pub fn all() -> Self {
        Self {
            summary: Some(true),
            summary_bcs: Some(true),
            signature: Some(true),
            contents: Some(true),
            contents_bcs: Some(true),
        }
    }

    pub fn none() -> Self {
        Self {
            summary: Some(false),
            summary_bcs: Some(false),
            signature: Some(false),
            contents: Some(false),
            contents_bcs: Some(false),
        }
    }

    pub fn with_summary(mut self) -> Self {
        self.summary = Some(true);
        self
    }

    pub fn without_summary(mut self) -> Self {
        self.summary = Some(false);
        self
    }

    pub fn with_summary_bcs(mut self) -> Self {
        self.summary_bcs = Some(true);
        self
    }

    pub fn without_summary_bcs(mut self) -> Self {
        self.summary_bcs = Some(false);
        self
    }

    pub fn with_signature(mut self) -> Self {
        self.signature = Some(true);
        self
    }

    pub fn without_signature(mut self) -> Self {
        self.signature = Some(false);
        self
    }

    pub fn with_contents(mut self) -> Self {
        self.contents = Some(true);
        self
    }

    pub fn without_contents(mut self) -> Self {
        self.contents = Some(false);
        self
    }

    pub fn with_contents_bcs(mut self) -> Self {
        self.contents_bcs = Some(true);
        self
    }

    pub fn without_contents_bcs(mut self) -> Self {
        self.contents_bcs = Some(false);
        self
    }
}

impl From<crate::types::GetCheckpointOptions> for GetCheckpointOptions {
    fn from(
        crate::types::GetCheckpointOptions {
            summary,
            summary_bcs,
            signature,
            contents,
            contents_bcs,
        }: crate::types::GetCheckpointOptions,
    ) -> Self {
        Self {
            summary,
            summary_bcs,
            signature,
            contents,
            contents_bcs,
        }
    }
}

impl From<GetCheckpointOptions> for crate::types::GetCheckpointOptions {
    fn from(
        GetCheckpointOptions {
            summary,
            summary_bcs,
            signature,
            contents,
            contents_bcs,
        }: GetCheckpointOptions,
    ) -> Self {
        Self {
            summary,
            summary_bcs,
            signature,
            contents,
            contents_bcs,
        }
    }
}

//
// GetCheckpointRequest
//

impl GetCheckpointRequest {
    pub fn latest() -> Self {
        Self {
            sequence_number: None,
            digest: None,
            options: None,
            read_mask: None,
        }
    }

    pub fn by_digest<T: Into<super::types::Digest>>(digest: T) -> Self {
        Self {
            sequence_number: None,
            digest: Some(digest.into()),
            options: None,
            read_mask: None,
        }
    }

    pub fn by_sequence_number(sequence_number: u64) -> Self {
        Self {
            sequence_number: Some(sequence_number),
            digest: None,
            options: None,
            read_mask: None,
        }
    }

    pub fn with_options(mut self, options: GetCheckpointOptions) -> Self {
        self.options = Some(options);
        self
    }
}

//
// GetTransactionOptions
//

impl GetTransactionOptions {
    pub fn all() -> Self {
        Self {
            transaction: Some(true),
            transaction_bcs: Some(true),
            signatures: Some(true),
            signatures_bytes: Some(true),
            effects: Some(true),
            effects_bcs: Some(true),
            events: Some(true),
            events_bcs: Some(true),
        }
    }

    pub fn none() -> Self {
        Self {
            transaction: Some(false),
            transaction_bcs: Some(false),
            signatures: Some(false),
            signatures_bytes: Some(false),
            effects: Some(false),
            effects_bcs: Some(false),
            events: Some(false),
            events_bcs: Some(false),
        }
    }

    pub fn with_transaction(mut self) -> Self {
        self.transaction = Some(true);
        self
    }

    pub fn without_transaction(mut self) -> Self {
        self.transaction = Some(false);
        self
    }

    pub fn with_transaction_bcs(mut self) -> Self {
        self.transaction_bcs = Some(true);
        self
    }

    pub fn without_transaction_bcs(mut self) -> Self {
        self.transaction_bcs = Some(false);
        self
    }

    pub fn with_signatures(mut self) -> Self {
        self.signatures = Some(true);
        self
    }

    pub fn without_signatures(mut self) -> Self {
        self.signatures = Some(false);
        self
    }

    pub fn with_signatures_bytes(mut self) -> Self {
        self.signatures_bytes = Some(true);
        self
    }

    pub fn without_signatures_bytes(mut self) -> Self {
        self.signatures_bytes = Some(false);
        self
    }

    pub fn with_effects(mut self) -> Self {
        self.effects = Some(true);
        self
    }

    pub fn without_effects(mut self) -> Self {
        self.effects = Some(false);
        self
    }

    pub fn with_effects_bcs(mut self) -> Self {
        self.effects_bcs = Some(true);
        self
    }

    pub fn without_effects_bcs(mut self) -> Self {
        self.effects_bcs = Some(false);
        self
    }

    pub fn with_events(mut self) -> Self {
        self.events = Some(true);
        self
    }

    pub fn without_events(mut self) -> Self {
        self.events = Some(false);
        self
    }

    pub fn with_events_bcs(mut self) -> Self {
        self.events_bcs = Some(true);
        self
    }

    pub fn without_events_bcs(mut self) -> Self {
        self.events_bcs = Some(false);
        self
    }
}

impl From<crate::types::GetTransactionOptions> for GetTransactionOptions {
    fn from(
        crate::types::GetTransactionOptions {
            transaction,
            transaction_bcs,
            signatures,
            signatures_bytes,
            effects,
            effects_bcs,
            events,
            events_bcs,
        }: crate::types::GetTransactionOptions,
    ) -> Self {
        Self {
            transaction,
            transaction_bcs,
            signatures,
            signatures_bytes,
            effects,
            effects_bcs,
            events,
            events_bcs,
        }
    }
}

impl From<GetTransactionOptions> for crate::types::GetTransactionOptions {
    fn from(
        GetTransactionOptions {
            transaction,
            transaction_bcs,
            signatures,
            signatures_bytes,
            effects,
            effects_bcs,
            events,
            events_bcs,
        }: GetTransactionOptions,
    ) -> Self {
        Self {
            transaction,
            transaction_bcs,
            signatures,
            signatures_bytes,
            effects,
            effects_bcs,
            events,
            events_bcs,
        }
    }
}

//
// GetTransactionRequest
//

impl GetTransactionRequest {
    pub fn new<T: Into<super::types::Digest>>(digest: T) -> Self {
        Self {
            digest: Some(digest.into()),
            options: None,
            read_mask: None,
        }
    }

    pub fn with_options(mut self, options: GetTransactionOptions) -> Self {
        self.options = Some(options);
        self
    }
}

//
// ExecuteTransactionOptions
//

impl From<crate::types::ExecuteTransactionOptions> for ExecuteTransactionOptions {
    fn from(
        crate::types::ExecuteTransactionOptions {
            effects,
            effects_bcs,
            events,
            events_bcs,
            balance_changes,
        }: crate::types::ExecuteTransactionOptions,
    ) -> Self {
        Self {
            effects,
            effects_bcs,
            events,
            events_bcs,
            balance_changes,
        }
    }
}

impl From<ExecuteTransactionOptions> for crate::types::ExecuteTransactionOptions {
    fn from(
        ExecuteTransactionOptions {
            effects,
            effects_bcs,
            events,
            events_bcs,
            balance_changes,
        }: ExecuteTransactionOptions,
    ) -> Self {
        Self {
            effects,
            effects_bcs,
            events,
            events_bcs,
            balance_changes,
        }
    }
}

//
// GetFullCheckpointOptions
//

impl GetFullCheckpointOptions {
    pub fn all() -> Self {
        Self {
            summary: Some(true),
            summary_bcs: Some(true),
            signature: Some(true),
            contents: Some(true),
            contents_bcs: Some(true),
            transaction: Some(true),
            transaction_bcs: Some(true),
            effects: Some(true),
            effects_bcs: Some(true),
            events: Some(true),
            events_bcs: Some(true),
            input_objects: Some(true),
            output_objects: Some(true),
            object: Some(true),
            object_bcs: Some(true),
        }
    }

    pub fn none() -> Self {
        Self {
            summary: Some(false),
            summary_bcs: Some(false),
            signature: Some(false),
            contents: Some(false),
            contents_bcs: Some(false),
            transaction: Some(false),
            transaction_bcs: Some(false),
            effects: Some(false),
            effects_bcs: Some(false),
            events: Some(false),
            events_bcs: Some(false),
            input_objects: Some(false),
            output_objects: Some(false),
            object: Some(false),
            object_bcs: Some(false),
        }
    }

    pub fn with_summary(mut self) -> Self {
        self.summary = Some(true);
        self
    }

    pub fn without_summary(mut self) -> Self {
        self.summary = Some(false);
        self
    }

    pub fn with_summary_bcs(mut self) -> Self {
        self.summary_bcs = Some(true);
        self
    }

    pub fn without_summary_bcs(mut self) -> Self {
        self.summary_bcs = Some(false);
        self
    }

    pub fn with_signature(mut self) -> Self {
        self.signature = Some(true);
        self
    }

    pub fn without_signature(mut self) -> Self {
        self.signature = Some(false);
        self
    }

    pub fn with_contents(mut self) -> Self {
        self.contents = Some(true);
        self
    }

    pub fn without_contents(mut self) -> Self {
        self.contents = Some(false);
        self
    }

    pub fn with_contents_bcs(mut self) -> Self {
        self.contents_bcs = Some(true);
        self
    }

    pub fn without_contents_bcs(mut self) -> Self {
        self.contents_bcs = Some(false);
        self
    }

    pub fn with_transaction(mut self) -> Self {
        self.transaction = Some(true);
        self
    }

    pub fn without_transaction(mut self) -> Self {
        self.transaction = Some(false);
        self
    }

    pub fn with_transaction_bcs(mut self) -> Self {
        self.transaction_bcs = Some(true);
        self
    }

    pub fn without_transaction_bcs(mut self) -> Self {
        self.transaction_bcs = Some(false);
        self
    }

    pub fn with_effects(mut self) -> Self {
        self.effects = Some(true);
        self
    }

    pub fn without_effects(mut self) -> Self {
        self.effects = Some(false);
        self
    }

    pub fn with_effects_bcs(mut self) -> Self {
        self.effects_bcs = Some(true);
        self
    }

    pub fn without_effects_bcs(mut self) -> Self {
        self.effects_bcs = Some(false);
        self
    }

    pub fn with_events(mut self) -> Self {
        self.events = Some(true);
        self
    }

    pub fn without_events(mut self) -> Self {
        self.events = Some(false);
        self
    }

    pub fn with_events_bcs(mut self) -> Self {
        self.events_bcs = Some(true);
        self
    }

    pub fn without_events_bcs(mut self) -> Self {
        self.events_bcs = Some(false);
        self
    }

    pub fn with_input_objects(mut self) -> Self {
        self.input_objects = Some(true);
        self
    }

    pub fn without_input_objects(mut self) -> Self {
        self.input_objects = Some(false);
        self
    }

    pub fn with_output_objects(mut self) -> Self {
        self.output_objects = Some(true);
        self
    }

    pub fn without_output_objects(mut self) -> Self {
        self.output_objects = Some(false);
        self
    }

    pub fn with_object(mut self) -> Self {
        self.object = Some(true);
        self
    }

    pub fn without_object(mut self) -> Self {
        self.object = Some(false);
        self
    }

    pub fn with_object_bcs(mut self) -> Self {
        self.object_bcs = Some(true);
        self
    }

    pub fn without_object_bcs(mut self) -> Self {
        self.object = Some(false);
        self
    }
}

impl From<crate::types::GetFullCheckpointOptions> for GetFullCheckpointOptions {
    fn from(
        crate::types::GetFullCheckpointOptions {
            summary,
            summary_bcs,
            signature,
            contents,
            contents_bcs,
            transaction,
            transaction_bcs,
            effects,
            effects_bcs,
            events,
            events_bcs,
            input_objects,
            output_objects,
            object,
            object_bcs,
        }: crate::types::GetFullCheckpointOptions,
    ) -> Self {
        Self {
            summary,
            summary_bcs,
            signature,
            contents,
            contents_bcs,
            transaction,
            transaction_bcs,
            effects,
            effects_bcs,
            events,
            events_bcs,
            input_objects,
            output_objects,
            object,
            object_bcs,
        }
    }
}

impl From<GetFullCheckpointOptions> for crate::types::GetFullCheckpointOptions {
    fn from(
        GetFullCheckpointOptions {
            summary,
            summary_bcs,
            signature,
            contents,
            contents_bcs,
            transaction,
            transaction_bcs,
            effects,
            effects_bcs,
            events,
            events_bcs,
            input_objects,
            output_objects,
            object,
            object_bcs,
        }: GetFullCheckpointOptions,
    ) -> Self {
        Self {
            summary,
            summary_bcs,
            signature,
            contents,
            contents_bcs,
            transaction,
            transaction_bcs,
            effects,
            effects_bcs,
            events,
            events_bcs,
            input_objects,
            output_objects,
            object,
            object_bcs,
        }
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
            options: None,
            read_mask: None,
        }
    }

    pub fn by_digest<T: Into<super::types::Digest>>(digest: T) -> Self {
        Self {
            sequence_number: None,
            digest: Some(digest.into()),
            options: None,
            read_mask: None,
        }
    }

    pub fn by_sequence_number(sequence_number: u64) -> Self {
        Self {
            sequence_number: Some(sequence_number),
            digest: None,
            options: None,
            read_mask: None,
        }
    }

    pub fn with_options(mut self, options: GetFullCheckpointOptions) -> Self {
        self.options = Some(options);
        self
    }
}

//
// TransactionResponse
//

impl From<crate::types::TransactionResponse> for GetTransactionResponse {
    fn from(
        crate::types::TransactionResponse {
            digest,
            transaction,
            transaction_bcs,
            signatures,
            signatures_bytes,
            effects,
            effects_bcs,
            events,
            events_bcs,
            checkpoint,
            timestamp_ms,
        }: crate::types::TransactionResponse,
    ) -> Self {
        let signatures = signatures
            .map(|signatures| signatures.into_iter().map(Into::into).collect())
            .unwrap_or_default();

        let signatures_bytes = signatures_bytes
            .map(|signatures| signatures.into_iter().map(Into::into).collect())
            .unwrap_or_default();

        Self {
            digest: Some(digest.into()),
            transaction: transaction.map(Into::into),
            transaction_bcs: transaction_bcs.map(Into::into),
            signatures,
            signatures_bytes,
            effects: effects.map(Into::into),
            effects_bcs: effects_bcs.map(Into::into),
            events: events.map(Into::into),
            events_bcs: events_bcs.map(Into::into),
            checkpoint,
            timestamp: timestamp_ms.map(timestamp_ms_to_proto),
        }
    }
}

impl TryFrom<&GetTransactionResponse> for crate::types::TransactionResponse {
    type Error = TryFromProtoError;

    fn try_from(
        GetTransactionResponse {
            digest,
            transaction,
            transaction_bcs,
            signatures,
            signatures_bytes,
            effects,
            effects_bcs,
            events,
            events_bcs,
            checkpoint,
            timestamp,
        }: &GetTransactionResponse,
    ) -> Result<Self, Self::Error> {
        let digest = digest
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("digest"))?
            .pipe(TryInto::try_into)?;

        let transaction = transaction.as_ref().map(TryInto::try_into).transpose()?;
        let transaction_bcs = transaction_bcs.as_ref().map(Into::into);

        let signatures = signatures
            .iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()?;

        let signatures = if signatures.is_empty() {
            None
        } else {
            Some(signatures)
        };

        let signatures_bytes = signatures_bytes
            .iter()
            .map(|bytes| bytes.to_vec())
            .collect::<Vec<_>>();

        let signatures_bytes = if signatures_bytes.is_empty() {
            None
        } else {
            Some(signatures_bytes)
        };

        let effects = effects.as_ref().map(TryInto::try_into).transpose()?;
        let effects_bcs = effects_bcs.as_ref().map(Into::into);

        let events = events.as_ref().map(TryInto::try_into).transpose()?;
        let events_bcs = events_bcs.as_ref().map(Into::into);

        let timestamp_ms = timestamp.map(proto_to_timestamp_ms).transpose()?;

        Self {
            digest,
            transaction,
            transaction_bcs,
            signatures,
            signatures_bytes,
            effects,
            effects_bcs,
            events,
            events_bcs,
            checkpoint: *checkpoint,
            timestamp_ms,
        }
        .pipe(Ok)
    }
}

//
// CheckpointResponse
//

impl From<crate::types::CheckpointResponse> for GetCheckpointResponse {
    fn from(
        crate::types::CheckpointResponse {
            sequence_number,
            digest,
            summary,
            summary_bcs,
            signature,
            contents,
            contents_bcs,
        }: crate::types::CheckpointResponse,
    ) -> Self {
        Self {
            sequence_number: Some(sequence_number),
            digest: Some(digest.into()),
            summary: summary.map(Into::into),
            summary_bcs: summary_bcs.map(Into::into),
            signature: signature.map(Into::into),
            contents: contents.map(Into::into),
            contents_bcs: contents_bcs.map(Into::into),
        }
    }
}

impl TryFrom<&GetCheckpointResponse> for crate::types::CheckpointResponse {
    type Error = TryFromProtoError;

    fn try_from(
        GetCheckpointResponse {
            sequence_number,
            digest,
            summary,
            summary_bcs,
            signature,
            contents,
            contents_bcs,
        }: &GetCheckpointResponse,
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

        Self {
            sequence_number,
            digest,
            summary,
            summary_bcs,
            signature,
            contents,
            contents_bcs,
        }
        .pipe(Ok)
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
        let input_objects_old = input_objects.clone().map(|objects| FullCheckpointObjects {
            objects: objects.into_iter().map(Into::into).collect(),
        });
        let output_objects_old = output_objects.clone().map(|objects| FullCheckpointObjects {
            objects: objects.into_iter().map(Into::into).collect(),
        });
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
            input_objects_old,
            output_objects_old,
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
            input_objects_old: _,
            output_objects_old: _,
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
// ExecuteTransactionResponse
//

impl From<crate::types::ExecuteTransactionResponse> for ExecuteTransactionResponse {
    fn from(
        crate::types::ExecuteTransactionResponse {
            finality,
            effects,
            effects_bcs,
            events,
            events_bcs,
            balance_changes,
        }: crate::types::ExecuteTransactionResponse,
    ) -> Self {
        let balance_changes = balance_changes
            .map(|balance_changes| balance_changes.into_iter().map(Into::into).collect())
            .unwrap_or_default();
        Self {
            finality: Some(finality.into()),
            effects: effects.map(Into::into),
            effects_bcs: effects_bcs.map(Into::into),
            events: events.map(Into::into),
            events_bcs: events_bcs.map(Into::into),
            balance_changes,
        }
    }
}

impl TryFrom<&ExecuteTransactionResponse> for crate::types::ExecuteTransactionResponse {
    type Error = TryFromProtoError;

    fn try_from(
        ExecuteTransactionResponse {
            finality,
            effects,
            effects_bcs,
            events,
            events_bcs,
            balance_changes,
        }: &ExecuteTransactionResponse,
    ) -> Result<Self, Self::Error> {
        let finality = finality
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("finality"))?
            .pipe(TryInto::try_into)?;

        let effects = effects.as_ref().map(TryInto::try_into).transpose()?;
        let effects_bcs = effects_bcs.as_ref().map(Into::into);

        let events = events.as_ref().map(TryInto::try_into).transpose()?;
        let events_bcs = events_bcs.as_ref().map(Into::into);

        let balance_changes = balance_changes
            .iter()
            .map(TryInto::try_into)
            .collect::<Result<Vec<_>, _>>()?;
        let balance_changes = if balance_changes.is_empty() {
            None
        } else {
            Some(balance_changes)
        };

        Self {
            finality,
            effects,
            effects_bcs,
            events,
            events_bcs,
            balance_changes,
        }
        .pipe(Ok)
    }
}

//
// EffectsFinality
//

impl From<crate::types::EffectsFinality> for crate::proto::node::v2::EffectsFinality {
    fn from(value: crate::types::EffectsFinality) -> Self {
        use crate::proto::node::v2::effects_finality::Finality;
        use crate::types::EffectsFinality::*;

        let finality = match value {
            Certified { signature } => Finality::Certified(signature.into()),
            Checkpointed { checkpoint } => Finality::Checkpointed(checkpoint),
            QuorumExecuted => Finality::QuorumExecuted(()),
        };

        Self {
            finality: Some(finality),
        }
    }
}

impl TryFrom<&crate::proto::node::v2::EffectsFinality> for crate::types::EffectsFinality {
    type Error = crate::proto::TryFromProtoError;

    fn try_from(value: &crate::proto::node::v2::EffectsFinality) -> Result<Self, Self::Error> {
        use crate::proto::node::v2::effects_finality::Finality;

        match value
            .finality
            .as_ref()
            .ok_or_else(|| crate::proto::TryFromProtoError::missing("finality"))?
        {
            Finality::Certified(signature) => Self::Certified {
                signature: signature.try_into()?,
            },
            Finality::Checkpointed(checkpoint) => Self::Checkpointed {
                checkpoint: *checkpoint,
            },
            Finality::QuorumExecuted(()) => Self::QuorumExecuted,
        }
        .pipe(Ok)
    }
}
