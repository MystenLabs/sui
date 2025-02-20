use super::TryFromProtoError;
use tap::Pipe;

//
// CheckpointSummary
//

impl From<sui_sdk_types::CheckpointSummary> for super::CheckpointSummary {
    fn from(
        sui_sdk_types::CheckpointSummary {
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
        }: sui_sdk_types::CheckpointSummary,
    ) -> Self {
        Self {
            epoch: Some(epoch),
            sequence_number: Some(sequence_number),
            total_network_transactions: Some(network_total_transactions),
            content_digest: Some(content_digest.into()),
            previous_digest: previous_digest.map(Into::into),
            epoch_rolling_gas_cost_summary: Some(epoch_rolling_gas_cost_summary.into()),
            timestamp_ms: Some(timestamp_ms),
            commitments: checkpoint_commitments.into_iter().map(Into::into).collect(),
            end_of_epoch_data: end_of_epoch_data.map(Into::into),
            version_specific_data: Some(version_specific_data.into()),
        }
    }
}

impl TryFrom<&super::CheckpointSummary> for sui_sdk_types::CheckpointSummary {
    type Error = TryFromProtoError;

    fn try_from(
        super::CheckpointSummary {
            epoch,
            sequence_number,
            total_network_transactions,
            content_digest,
            previous_digest,
            epoch_rolling_gas_cost_summary,
            timestamp_ms,
            commitments,
            end_of_epoch_data,
            version_specific_data,
        }: &super::CheckpointSummary,
    ) -> Result<Self, Self::Error> {
        let epoch = epoch.ok_or_else(|| TryFromProtoError::missing("epoch"))?;
        let sequence_number =
            sequence_number.ok_or_else(|| TryFromProtoError::missing("sequence_number"))?;
        let network_total_transactions = total_network_transactions
            .ok_or_else(|| TryFromProtoError::missing("total_network_transactions"))?;
        let content_digest = content_digest
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("content_digest"))?
            .try_into()?;
        let previous_digest = previous_digest
            .as_ref()
            .map(TryInto::try_into)
            .transpose()?;
        let epoch_rolling_gas_cost_summary = epoch_rolling_gas_cost_summary
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("epoch_rolling_gas_cost_summary"))?
            .try_into()?;

        let timestamp_ms =
            timestamp_ms.ok_or_else(|| TryFromProtoError::missing("timestamp_ms"))?;

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
        let commitment = match value {
            sui_sdk_types::CheckpointCommitment::EcmhLiveObjectSet { digest } => {
                super::checkpoint_commitment::Commitment::EcmhLiveObjectSet(digest.into())
            }
        };

        Self {
            commitment: Some(commitment),
        }
    }
}

impl TryFrom<&super::CheckpointCommitment> for sui_sdk_types::CheckpointCommitment {
    type Error = TryFromProtoError;

    fn try_from(value: &super::CheckpointCommitment) -> Result<Self, Self::Error> {
        match value
            .commitment
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("commitment"))?
        {
            super::checkpoint_commitment::Commitment::EcmhLiveObjectSet(digest) => {
                Self::EcmhLiveObjectSet {
                    digest: digest.try_into()?,
                }
            }
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
            transaction: Some(value.transaction.into()),
            effects: Some(value.effects.into()),
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
            .try_into()?;

        let effects = value
            .effects
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("effects"))?
            .try_into()?;

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

impl From<sui_sdk_types::CheckpointContents> for super::CheckpointContents {
    fn from(value: sui_sdk_types::CheckpointContents) -> Self {
        let contents = super::checkpoint_contents::Contents::V1(super::checkpoint_contents::V1 {
            transactions: value.into_v1().into_iter().map(Into::into).collect(),
        });

        Self {
            contents: Some(contents),
        }
    }
}

impl TryFrom<&super::CheckpointContents> for sui_sdk_types::CheckpointContents {
    type Error = TryFromProtoError;

    fn try_from(value: &super::CheckpointContents) -> Result<Self, Self::Error> {
        match value
            .contents
            .as_ref()
            .ok_or_else(|| TryFromProtoError::missing("commitment"))?
        {
            super::checkpoint_contents::Contents::V1(v1) => Self::new(
                v1.transactions
                    .iter()
                    .map(TryInto::try_into)
                    .collect::<Result<_, _>>()?,
            ),
        }
        .pipe(Ok)
    }
}
