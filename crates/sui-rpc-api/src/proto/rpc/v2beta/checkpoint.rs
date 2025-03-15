// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::Bcs;
use super::Checkpoint;
use super::CheckpointContents;
use super::CheckpointSummary;
use crate::message::{MessageField, MessageFields, MessageMerge};
use crate::proto::TryFromProtoError;
use tap::Pipe;

//
// CheckpointSummary
//

impl CheckpointSummary {
    const BCS_FIELD: &'static MessageField =
        &MessageField::new("bcs").with_message_fields(Bcs::FIELDS);
    const DIGEST_FIELD: &'static MessageField = &MessageField::new("digest");
    const EPOCH_FIELD: &'static MessageField = &MessageField::new("epoch");
    const SEQUENCE_NUMBER_FIELD: &'static MessageField = &MessageField::new("sequence_number");
    const TOTAL_NETWORK_TRANSACTIONS_FIELD: &'static MessageField =
        &MessageField::new("total_network_transactions");
    const CONTENT_DIGEST_FIELD: &'static MessageField = &MessageField::new("content_digest");
    const PREVIOUS_DIGEST_FIELD: &'static MessageField = &MessageField::new("previous_digest");
    const EPOCH_ROLLING_GAS_COST_SUMMARY_FIELD: &'static MessageField =
        &MessageField::new("epoch_rolling_gas_cost_summary");
    const TIMESTAMP_FIELD: &'static MessageField = &MessageField::new("timestamp");
    const COMMITMENTS_FIELD: &'static MessageField = &MessageField::new("commitments");
    const END_OF_EPOCH_DATA_FIELD: &'static MessageField = &MessageField::new("end_of_epoch_data");
    const VERSION_SPECIFIC_DATA_FIELD: &'static MessageField =
        &MessageField::new("version_specific_data");
}

impl MessageFields for CheckpointSummary {
    const FIELDS: &'static [&'static MessageField] = &[
        Self::BCS_FIELD,
        Self::DIGEST_FIELD,
        Self::EPOCH_FIELD,
        Self::SEQUENCE_NUMBER_FIELD,
        Self::TOTAL_NETWORK_TRANSACTIONS_FIELD,
        Self::CONTENT_DIGEST_FIELD,
        Self::PREVIOUS_DIGEST_FIELD,
        Self::EPOCH_ROLLING_GAS_COST_SUMMARY_FIELD,
        Self::TIMESTAMP_FIELD,
        Self::COMMITMENTS_FIELD,
        Self::END_OF_EPOCH_DATA_FIELD,
        Self::VERSION_SPECIFIC_DATA_FIELD,
    ];
}

impl From<sui_sdk_types::CheckpointSummary> for CheckpointSummary {
    fn from(summary: sui_sdk_types::CheckpointSummary) -> Self {
        let mut message = Self::default();
        message.merge(summary, &crate::field_mask::FieldMaskTree::new_wildcard());
        message
    }
}

impl MessageMerge<sui_sdk_types::CheckpointSummary> for CheckpointSummary {
    fn merge(
        &mut self,
        source: sui_sdk_types::CheckpointSummary,
        mask: &crate::field_mask::FieldMaskTree,
    ) {
        if mask.contains(Self::BCS_FIELD.name) {
            self.bcs = Some(Bcs::serialize(&source).unwrap());
        }

        if mask.contains(Self::DIGEST_FIELD.name) {
            self.digest = Some(source.digest().to_string());
        }

        let sui_sdk_types::CheckpointSummary {
            epoch,
            sequence_number,
            network_total_transactions,
            content_digest,
            previous_digest,
            epoch_rolling_gas_cost_summary,
            timestamp_ms,
            checkpoint_commitments,
            end_of_epoch_data,
            version_specific_data,
        } = source;

        if mask.contains(Self::EPOCH_FIELD.name) {
            self.epoch = Some(epoch);
        }

        if mask.contains(Self::SEQUENCE_NUMBER_FIELD.name) {
            self.sequence_number = Some(sequence_number);
        }

        if mask.contains(Self::TOTAL_NETWORK_TRANSACTIONS_FIELD.name) {
            self.total_network_transactions = Some(network_total_transactions);
        }

        if mask.contains(Self::CONTENT_DIGEST_FIELD.name) {
            self.content_digest = Some(content_digest.to_string());
        }

        if mask.contains(Self::PREVIOUS_DIGEST_FIELD.name) {
            self.previous_digest = previous_digest.map(|d| d.to_string());
        }

        if mask.contains(Self::EPOCH_ROLLING_GAS_COST_SUMMARY_FIELD.name) {
            self.epoch_rolling_gas_cost_summary = Some(epoch_rolling_gas_cost_summary.into());
        }

        if mask.contains(Self::TIMESTAMP_FIELD.name) {
            self.timestamp = Some(crate::proto::types::timestamp_ms_to_proto(timestamp_ms));
        }

        if mask.contains(Self::COMMITMENTS_FIELD.name) {
            self.commitments = checkpoint_commitments.into_iter().map(Into::into).collect();
        }

        if mask.contains(Self::END_OF_EPOCH_DATA_FIELD.name) {
            self.end_of_epoch_data = end_of_epoch_data.map(Into::into);
        }

        if mask.contains(Self::VERSION_SPECIFIC_DATA_FIELD.name) {
            self.version_specific_data = Some(version_specific_data.into());
        }
    }
}

impl TryFrom<&CheckpointSummary> for sui_sdk_types::CheckpointSummary {
    type Error = TryFromProtoError;

    fn try_from(
        CheckpointSummary {
            bcs: _,
            digest: _,
            epoch,
            sequence_number,
            total_network_transactions,
            content_digest,
            previous_digest,
            epoch_rolling_gas_cost_summary,
            timestamp,
            commitments,
            end_of_epoch_data,
            version_specific_data,
        }: &CheckpointSummary,
    ) -> Result<Self, Self::Error> {
        let epoch = epoch.ok_or_else(|| TryFromProtoError::missing("epoch"))?;
        let sequence_number =
            sequence_number.ok_or_else(|| TryFromProtoError::missing("sequence_number"))?;
        let network_total_transactions = total_network_transactions
            .ok_or_else(|| TryFromProtoError::missing("total_network_transactions"))?;
        let content_digest = content_digest
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("content_digest"))?
            .parse()
            .map_err(TryFromProtoError::from_error)?;
        let previous_digest = previous_digest
            .as_ref()
            .map(|s| s.parse().map_err(TryFromProtoError::from_error))
            .transpose()?;
        let epoch_rolling_gas_cost_summary = epoch_rolling_gas_cost_summary
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("epoch_rolling_gas_cost_summary"))?
            .try_into()?;

        let timestamp_ms = timestamp
            .ok_or_else(|| TryFromProtoError::missing("timestamp_ms"))?
            .pipe(crate::proto::types::proto_to_timestamp_ms)?;

        let checkpoint_commitments = commitments
            .iter()
            .map(TryInto::try_into)
            .collect::<Result<_, _>>()?;

        let end_of_epoch_data = end_of_epoch_data
            .as_ref()
            .map(TryInto::try_into)
            .transpose()?;

        let version_specific_data = version_specific_data
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("version_specific_data"))?
            .to_vec();

        Ok(Self {
            epoch,
            sequence_number,
            network_total_transactions,
            content_digest,
            previous_digest,
            epoch_rolling_gas_cost_summary,
            timestamp_ms,
            checkpoint_commitments,
            end_of_epoch_data,
            version_specific_data,
        })
    }
}

//
// GasCostSummary
//

impl From<sui_sdk_types::GasCostSummary> for super::GasCostSummary {
    fn from(
        sui_sdk_types::GasCostSummary {
            computation_cost,
            storage_cost,
            storage_rebate,
            non_refundable_storage_fee,
        }: sui_sdk_types::GasCostSummary,
    ) -> Self {
        Self {
            computation_cost: Some(computation_cost),
            storage_cost: Some(storage_cost),
            storage_rebate: Some(storage_rebate),
            non_refundable_storage_fee: Some(non_refundable_storage_fee),
        }
    }
}

impl TryFrom<&super::GasCostSummary> for sui_sdk_types::GasCostSummary {
    type Error = TryFromProtoError;

    fn try_from(
        super::GasCostSummary {
            computation_cost,
            storage_cost,
            storage_rebate,
            non_refundable_storage_fee,
        }: &super::GasCostSummary,
    ) -> Result<Self, Self::Error> {
        let computation_cost =
            computation_cost.ok_or_else(|| TryFromProtoError::missing("computation_cost"))?;
        let storage_cost =
            storage_cost.ok_or_else(|| TryFromProtoError::missing("storage_cost"))?;
        let storage_rebate =
            storage_rebate.ok_or_else(|| TryFromProtoError::missing("storage_rebate"))?;
        let non_refundable_storage_fee = non_refundable_storage_fee
            .ok_or_else(|| TryFromProtoError::missing("non_refundable_storage_fee"))?;
        Ok(Self {
            computation_cost,
            storage_cost,
            storage_rebate,
            non_refundable_storage_fee,
        })
    }
}

//
// CheckpointCommitment
//

impl From<sui_sdk_types::CheckpointCommitment> for super::CheckpointCommitment {
    fn from(value: sui_sdk_types::CheckpointCommitment) -> Self {
        use super::checkpoint_commitment::CheckpointCommitmentKind;

        let mut message = Self::default();

        let kind = match value {
            sui_sdk_types::CheckpointCommitment::EcmhLiveObjectSet { digest } => {
                message.digest = Some(digest.to_string());
                CheckpointCommitmentKind::EcmhLiveObjectSet
            }
        };

        message.set_kind(kind);
        message
    }
}

impl TryFrom<&super::CheckpointCommitment> for sui_sdk_types::CheckpointCommitment {
    type Error = TryFromProtoError;

    fn try_from(value: &super::CheckpointCommitment) -> Result<Self, Self::Error> {
        use super::checkpoint_commitment::CheckpointCommitmentKind;

        match value.kind() {
            CheckpointCommitmentKind::Unknown => {
                return Err(TryFromProtoError::from_error(
                    "unknown CheckpointCommitmentKind",
                ))
            }
            CheckpointCommitmentKind::EcmhLiveObjectSet => Self::EcmhLiveObjectSet {
                digest: value
                    .digest()
                    .parse()
                    .map_err(TryFromProtoError::from_error)?,
            },
        }
        .pipe(Ok)
    }
}

//
// EndOfEpochData
//

impl From<sui_sdk_types::EndOfEpochData> for super::EndOfEpochData {
    fn from(
        sui_sdk_types::EndOfEpochData {
            next_epoch_committee,
            next_epoch_protocol_version,
            epoch_commitments,
        }: sui_sdk_types::EndOfEpochData,
    ) -> Self {
        Self {
            next_epoch_committee: next_epoch_committee.into_iter().map(Into::into).collect(),
            next_epoch_protocol_version: Some(next_epoch_protocol_version),
            epoch_commitments: epoch_commitments.into_iter().map(Into::into).collect(),
        }
    }
}

impl TryFrom<&super::EndOfEpochData> for sui_sdk_types::EndOfEpochData {
    type Error = TryFromProtoError;

    fn try_from(
        super::EndOfEpochData {
            next_epoch_committee,
            next_epoch_protocol_version,
            epoch_commitments,
        }: &super::EndOfEpochData,
    ) -> Result<Self, Self::Error> {
        let next_epoch_protocol_version = next_epoch_protocol_version
            .ok_or_else(|| TryFromProtoError::missing("next_epoch_protocol_version"))?;

        Ok(Self {
            next_epoch_committee: next_epoch_committee
                .iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
            next_epoch_protocol_version,
            epoch_commitments: epoch_commitments
                .iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
        })
    }
}

//
// CheckpointedTransactionInfo
//

impl From<sui_sdk_types::CheckpointTransactionInfo> for super::CheckpointedTransactionInfo {
    fn from(value: sui_sdk_types::CheckpointTransactionInfo) -> Self {
        Self {
            transaction: Some(value.transaction.to_string()),
            effects: Some(value.effects.to_string()),
            signatures: value.signatures.into_iter().map(Into::into).collect(),
        }
    }
}

impl TryFrom<&super::CheckpointedTransactionInfo> for sui_sdk_types::CheckpointTransactionInfo {
    type Error = TryFromProtoError;

    fn try_from(value: &super::CheckpointedTransactionInfo) -> Result<Self, Self::Error> {
        let transaction = value
            .transaction
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("transaction"))?
            .parse()
            .map_err(TryFromProtoError::from_error)?;

        let effects = value
            .effects
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("effects"))?
            .parse()
            .map_err(TryFromProtoError::from_error)?;

        let signatures = value
            .signatures
            .iter()
            .map(TryInto::try_into)
            .collect::<Result<_, _>>()?;

        Ok(Self {
            transaction,
            effects,
            signatures,
        })
    }
}

//
// CheckpointContents
//

impl CheckpointContents {
    const BCS_FIELD: &'static MessageField =
        &MessageField::new("bcs").with_message_fields(Bcs::FIELDS);
    const DIGEST_FIELD: &'static MessageField = &MessageField::new("digest");
    const VERSION_FIELD: &'static MessageField = &MessageField::new("version");
    const TRANSACTIONS_FIELD: &'static MessageField = &MessageField::new("transactions");
}

impl MessageFields for CheckpointContents {
    const FIELDS: &'static [&'static MessageField] = &[
        Self::BCS_FIELD,
        Self::DIGEST_FIELD,
        Self::VERSION_FIELD,
        Self::TRANSACTIONS_FIELD,
    ];
}

impl From<sui_sdk_types::CheckpointContents> for CheckpointContents {
    fn from(value: sui_sdk_types::CheckpointContents) -> Self {
        let mut message = Self::default();
        message.merge(value, &crate::field_mask::FieldMaskTree::new_wildcard());
        message
    }
}

impl MessageMerge<sui_sdk_types::CheckpointContents> for CheckpointContents {
    fn merge(
        &mut self,
        source: sui_sdk_types::CheckpointContents,
        mask: &crate::field_mask::FieldMaskTree,
    ) {
        if mask.contains(Self::BCS_FIELD.name) {
            self.bcs = Some(Bcs::serialize(&source).unwrap());
        }

        if mask.contains(Self::DIGEST_FIELD.name) {
            self.digest = Some(source.digest().to_string());
        }

        if mask.contains(Self::VERSION_FIELD.name) {
            self.version = Some(1);
        }

        if mask.contains(Self::TRANSACTIONS_FIELD.name) {
            self.transactions = source.into_v1().into_iter().map(Into::into).collect();
        }
    }
}

impl TryFrom<&CheckpointContents> for sui_sdk_types::CheckpointContents {
    type Error = TryFromProtoError;

    fn try_from(value: &CheckpointContents) -> Result<Self, Self::Error> {
        match value.version {
            Some(1) => {}
            _ => {
                return Err(TryFromProtoError::from_error("unknown type version"));
            }
        }

        Ok(Self::new(
            value
                .transactions
                .iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
        ))
    }
}

//
// Checkpoint
//

impl Checkpoint {
    const SEQUENCE_NUMBER_FIELD: &'static MessageField = &MessageField::new("sequence_number");
    const DIGEST_FIELD: &'static MessageField = &MessageField::new("digest");
    const SUMMARY_FIELD: &'static MessageField =
        &MessageField::new("summary").with_message_fields(CheckpointSummary::FIELDS);
    const SIGNATURE_FIELD: &'static MessageField = &MessageField::new("signature");
    const CONTENTS_FIELD: &'static MessageField =
        &MessageField::new("contents").with_message_fields(CheckpointContents::FIELDS);
    const TRANSACTIONS_FIELD: &'static MessageField =
        &MessageField::new("transactions").with_message_fields(super::ExecutedTransaction::FIELDS);
}

impl MessageFields for Checkpoint {
    const FIELDS: &'static [&'static MessageField] = &[
        Self::SEQUENCE_NUMBER_FIELD,
        Self::DIGEST_FIELD,
        Self::SUMMARY_FIELD,
        Self::SIGNATURE_FIELD,
        Self::CONTENTS_FIELD,
        Self::TRANSACTIONS_FIELD,
    ];
}
