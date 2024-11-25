// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[path = "generated/sui.rest.rs"]
mod generated;
pub use generated::*;
use tap::Pipe;

//
// Transaction
//

impl TryFrom<&sui_sdk_types::types::Transaction> for Transaction {
    type Error = bcs::Error;

    fn try_from(value: &sui_sdk_types::types::Transaction) -> Result<Self, Self::Error> {
        bcs::to_bytes(&value).map(|bytes| Self {
            transaction: bytes.into(),
        })
    }
}

impl TryFrom<&Transaction> for sui_sdk_types::types::Transaction {
    type Error = bcs::Error;

    fn try_from(value: &Transaction) -> Result<Self, Self::Error> {
        bcs::from_bytes(&value.transaction)
    }
}

impl TryFrom<&sui_types::transaction::TransactionData> for Transaction {
    type Error = bcs::Error;

    fn try_from(value: &sui_types::transaction::TransactionData) -> Result<Self, Self::Error> {
        bcs::to_bytes(&value).map(|bytes| Self {
            transaction: bytes.into(),
        })
    }
}

impl TryFrom<&Transaction> for sui_types::transaction::TransactionData {
    type Error = bcs::Error;

    fn try_from(value: &Transaction) -> Result<Self, Self::Error> {
        bcs::from_bytes(&value.transaction)
    }
}

//
// TransactionEffects
//

impl TryFrom<&sui_sdk_types::types::TransactionEffects> for TransactionEffects {
    type Error = bcs::Error;

    fn try_from(value: &sui_sdk_types::types::TransactionEffects) -> Result<Self, Self::Error> {
        bcs::to_bytes(&value).map(|bytes| Self {
            effects: bytes.into(),
        })
    }
}

impl TryFrom<&TransactionEffects> for sui_sdk_types::types::TransactionEffects {
    type Error = bcs::Error;

    fn try_from(value: &TransactionEffects) -> Result<Self, Self::Error> {
        bcs::from_bytes(&value.effects)
    }
}

impl TryFrom<&sui_types::effects::TransactionEffects> for TransactionEffects {
    type Error = bcs::Error;

    fn try_from(value: &sui_types::effects::TransactionEffects) -> Result<Self, Self::Error> {
        bcs::to_bytes(&value).map(|bytes| Self {
            effects: bytes.into(),
        })
    }
}

impl TryFrom<&TransactionEffects> for sui_types::effects::TransactionEffects {
    type Error = bcs::Error;

    fn try_from(value: &TransactionEffects) -> Result<Self, Self::Error> {
        bcs::from_bytes(&value.effects)
    }
}

//
// TransactionEvents
//

impl TryFrom<&sui_sdk_types::types::TransactionEvents> for TransactionEvents {
    type Error = bcs::Error;

    fn try_from(value: &sui_sdk_types::types::TransactionEvents) -> Result<Self, Self::Error> {
        bcs::to_bytes(&value).map(|bytes| Self {
            events: bytes.into(),
        })
    }
}

impl TryFrom<&TransactionEvents> for sui_sdk_types::types::TransactionEvents {
    type Error = bcs::Error;

    fn try_from(value: &TransactionEvents) -> Result<Self, Self::Error> {
        bcs::from_bytes(&value.events)
    }
}

impl TryFrom<&sui_types::effects::TransactionEvents> for TransactionEvents {
    type Error = bcs::Error;

    fn try_from(value: &sui_types::effects::TransactionEvents) -> Result<Self, Self::Error> {
        bcs::to_bytes(&value).map(|bytes| Self {
            events: bytes.into(),
        })
    }
}

impl TryFrom<&TransactionEvents> for sui_types::effects::TransactionEvents {
    type Error = bcs::Error;

    fn try_from(value: &TransactionEvents) -> Result<Self, Self::Error> {
        bcs::from_bytes(&value.events)
    }
}

//
// Object
//

impl TryFrom<&sui_sdk_types::types::Object> for Object {
    type Error = bcs::Error;

    fn try_from(value: &sui_sdk_types::types::Object) -> Result<Self, Self::Error> {
        bcs::to_bytes(&value).map(|bytes| Self {
            object: bytes.into(),
        })
    }
}

impl TryFrom<&Object> for sui_sdk_types::types::Object {
    type Error = bcs::Error;

    fn try_from(value: &Object) -> Result<Self, Self::Error> {
        bcs::from_bytes(&value.object)
    }
}

impl TryFrom<&sui_types::object::Object> for Object {
    type Error = bcs::Error;

    fn try_from(value: &sui_types::object::Object) -> Result<Self, Self::Error> {
        bcs::to_bytes(&value).map(|bytes| Self {
            object: bytes.into(),
        })
    }
}

impl TryFrom<&Object> for sui_types::object::Object {
    type Error = bcs::Error;

    fn try_from(value: &Object) -> Result<Self, Self::Error> {
        bcs::from_bytes(&value.object)
    }
}

//
// CheckpointSummary
//

impl TryFrom<&sui_sdk_types::types::CheckpointSummary> for CheckpointSummary {
    type Error = bcs::Error;

    fn try_from(value: &sui_sdk_types::types::CheckpointSummary) -> Result<Self, Self::Error> {
        bcs::to_bytes(&value).map(|bytes| Self {
            summary: bytes.into(),
        })
    }
}

impl TryFrom<&CheckpointSummary> for sui_sdk_types::types::CheckpointSummary {
    type Error = bcs::Error;

    fn try_from(value: &CheckpointSummary) -> Result<Self, Self::Error> {
        bcs::from_bytes(&value.summary)
    }
}

impl TryFrom<&sui_types::messages_checkpoint::CheckpointSummary> for CheckpointSummary {
    type Error = bcs::Error;

    fn try_from(
        value: &sui_types::messages_checkpoint::CheckpointSummary,
    ) -> Result<Self, Self::Error> {
        bcs::to_bytes(&value).map(|bytes| Self {
            summary: bytes.into(),
        })
    }
}

impl TryFrom<&CheckpointSummary> for sui_types::messages_checkpoint::CheckpointSummary {
    type Error = bcs::Error;

    fn try_from(value: &CheckpointSummary) -> Result<Self, Self::Error> {
        bcs::from_bytes(&value.summary)
    }
}

//
// CheckpointContents
//

impl TryFrom<&sui_sdk_types::types::CheckpointContents> for CheckpointContents {
    type Error = bcs::Error;

    fn try_from(value: &sui_sdk_types::types::CheckpointContents) -> Result<Self, Self::Error> {
        bcs::to_bytes(&value).map(|bytes| Self {
            contents: bytes.into(),
        })
    }
}

impl TryFrom<&CheckpointContents> for sui_sdk_types::types::CheckpointContents {
    type Error = bcs::Error;

    fn try_from(value: &CheckpointContents) -> Result<Self, Self::Error> {
        bcs::from_bytes(&value.contents)
    }
}

impl TryFrom<&sui_types::messages_checkpoint::CheckpointContents> for CheckpointContents {
    type Error = bcs::Error;

    fn try_from(
        value: &sui_types::messages_checkpoint::CheckpointContents,
    ) -> Result<Self, Self::Error> {
        bcs::to_bytes(&value).map(|bytes| Self {
            contents: bytes.into(),
        })
    }
}

impl TryFrom<&CheckpointContents> for sui_types::messages_checkpoint::CheckpointContents {
    type Error = bcs::Error;

    fn try_from(value: &CheckpointContents) -> Result<Self, Self::Error> {
        bcs::from_bytes(&value.contents)
    }
}

//
// ValidatorAggregatedSignature
//

impl TryFrom<&sui_sdk_types::types::ValidatorAggregatedSignature> for ValidatorAggregatedSignature {
    type Error = bcs::Error;

    fn try_from(
        value: &sui_sdk_types::types::ValidatorAggregatedSignature,
    ) -> Result<Self, Self::Error> {
        bcs::to_bytes(&value).map(|bytes| Self {
            signature: bytes.into(),
        })
    }
}

impl TryFrom<&ValidatorAggregatedSignature> for sui_sdk_types::types::ValidatorAggregatedSignature {
    type Error = bcs::Error;

    fn try_from(value: &ValidatorAggregatedSignature) -> Result<Self, Self::Error> {
        bcs::from_bytes(&value.signature)
    }
}

impl TryFrom<&sui_types::crypto::AuthorityStrongQuorumSignInfo> for ValidatorAggregatedSignature {
    type Error = bcs::Error;

    fn try_from(
        value: &sui_types::crypto::AuthorityStrongQuorumSignInfo,
    ) -> Result<Self, Self::Error> {
        bcs::to_bytes(&value).map(|bytes| Self {
            signature: bytes.into(),
        })
    }
}

impl TryFrom<&ValidatorAggregatedSignature> for sui_types::crypto::AuthorityStrongQuorumSignInfo {
    type Error = bcs::Error;

    fn try_from(value: &ValidatorAggregatedSignature) -> Result<Self, Self::Error> {
        bcs::from_bytes(&value.signature)
    }
}

//
// UserSignature
//

impl TryFrom<&sui_sdk_types::types::UserSignature> for UserSignature {
    type Error = bcs::Error;

    fn try_from(value: &sui_sdk_types::types::UserSignature) -> Result<Self, Self::Error> {
        Ok(Self {
            signature: value.to_bytes().into(),
        })
    }
}

impl TryFrom<&UserSignature> for sui_sdk_types::types::UserSignature {
    type Error = bcs::Error;

    fn try_from(value: &UserSignature) -> Result<Self, Self::Error> {
        Self::from_bytes(&value.signature).map_err(|e| bcs::Error::Custom(e.to_string()))
    }
}

impl TryFrom<&sui_types::signature::GenericSignature> for UserSignature {
    type Error = bcs::Error;

    fn try_from(value: &sui_types::signature::GenericSignature) -> Result<Self, Self::Error> {
        Ok(Self {
            signature: sui_types::crypto::ToFromBytes::as_bytes(value)
                .to_vec()
                .into(),
        })
    }
}

impl TryFrom<&UserSignature> for sui_types::signature::GenericSignature {
    type Error = bcs::Error;

    fn try_from(value: &UserSignature) -> Result<Self, Self::Error> {
        sui_types::crypto::ToFromBytes::from_bytes(&value.signature)
            .map_err(|e| bcs::Error::Custom(e.to_string()))
    }
}

//
// GetObjectResponse
//

impl TryFrom<crate::rest::objects::ObjectResponse> for GetObjectResponse {
    type Error = bcs::Error;

    fn try_from(value: crate::rest::objects::ObjectResponse) -> Result<Self, Self::Error> {
        Ok(Self {
            digest: value.digest.as_bytes().to_vec().into(),
            object: Some(Object::try_from(&value.object)?),
        })
    }
}

impl TryFrom<GetObjectResponse> for crate::rest::objects::ObjectResponse {
    type Error = bcs::Error;

    fn try_from(value: GetObjectResponse) -> Result<Self, Self::Error> {
        Ok(Self {
            digest: sui_sdk_types::types::ObjectDigest::from_bytes(&value.digest)
                .map_err(|e| bcs::Error::Custom(e.to_string()))?,
            object: value
                .object
                .ok_or_else(|| bcs::Error::Custom("missing object".into()))?
                .pipe_ref(TryInto::try_into)?,
        })
    }
}

//
// GetCheckpointResponse
//

impl TryFrom<crate::rest::checkpoints::CheckpointResponse> for GetCheckpointResponse {
    type Error = bcs::Error;

    fn try_from(c: crate::rest::checkpoints::CheckpointResponse) -> Result<Self, Self::Error> {
        Ok(Self {
            digest: c.digest.as_bytes().to_vec().into(),
            summary: Some(CheckpointSummary::try_from(&c.summary)?),
            signature: Some(ValidatorAggregatedSignature::try_from(&c.signature)?),
            contents: c
                .contents
                .as_ref()
                .map(CheckpointContents::try_from)
                .transpose()?,
        })
    }
}

impl TryFrom<GetCheckpointResponse> for crate::rest::checkpoints::CheckpointResponse {
    type Error = bcs::Error;

    fn try_from(value: GetCheckpointResponse) -> Result<Self, Self::Error> {
        let summary = value
            .summary
            .ok_or_else(|| bcs::Error::Custom("missing summary".into()))?
            .pipe_ref(TryInto::try_into)?;
        let signature = value
            .signature
            .ok_or_else(|| bcs::Error::Custom("missing signature".into()))?
            .pipe_ref(TryInto::try_into)?;

        let contents = value.contents.as_ref().map(TryInto::try_into).transpose()?;

        Ok(Self {
            digest: sui_sdk_types::types::CheckpointDigest::from_bytes(&value.digest)
                .map_err(|e| bcs::Error::Custom(e.to_string()))?,
            summary,
            signature,
            contents,
        })
    }
}

impl TryFrom<Vec<crate::rest::checkpoints::CheckpointResponse>> for ListCheckpointResponse {
    type Error = bcs::Error;
    fn try_from(
        value: Vec<crate::rest::checkpoints::CheckpointResponse>,
    ) -> Result<Self, Self::Error> {
        let checkpoints = value
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<_, _>>()?;

        Ok(Self { checkpoints })
    }
}

//
// GetTransactionResponse
//

impl TryFrom<crate::rest::transactions::TransactionResponse> for GetTransactionResponse {
    type Error = bcs::Error;

    fn try_from(
        value: crate::rest::transactions::TransactionResponse,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            digest: value.digest.as_bytes().to_vec().into(),
            transaction: Some(Transaction::try_from(&value.transaction)?),
            signatures: value
                .signatures
                .iter()
                .map(UserSignature::try_from)
                .collect::<Result<_, _>>()?,
            effects: Some(TransactionEffects::try_from(&value.effects)?),
            events: value
                .events
                .as_ref()
                .map(TransactionEvents::try_from)
                .transpose()?,
            checkpoint: value.checkpoint,
            timestamp_ms: value.timestamp_ms,
        })
    }
}

impl TryFrom<GetTransactionResponse> for crate::rest::transactions::TransactionResponse {
    type Error = bcs::Error;

    fn try_from(value: GetTransactionResponse) -> Result<Self, Self::Error> {
        Ok(Self {
            digest: sui_sdk_types::types::TransactionDigest::from_bytes(&value.digest)
                .map_err(|e| bcs::Error::Custom(e.to_string()))?,
            transaction: value
                .transaction
                .ok_or_else(|| bcs::Error::Custom("missing transaction".into()))?
                .pipe_ref(TryInto::try_into)?,
            signatures: value
                .signatures
                .iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
            effects: value
                .effects
                .ok_or_else(|| bcs::Error::Custom("missing effects".into()))?
                .pipe_ref(TryInto::try_into)?,
            events: value.events.as_ref().map(TryInto::try_into).transpose()?,
            checkpoint: value.checkpoint,
            timestamp_ms: value.timestamp_ms,
        })
    }
}

//
// CheckpointTransaction
//

impl TryFrom<sui_types::full_checkpoint_content::CheckpointTransaction> for CheckpointTransaction {
    type Error = bcs::Error;

    fn try_from(
        transaction: sui_types::full_checkpoint_content::CheckpointTransaction,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            transaction: Some(Transaction::try_from(
                &transaction.transaction.intent_message().value,
            )?),
            signatures: transaction
                .transaction
                .tx_signatures()
                .iter()
                .map(UserSignature::try_from)
                .collect::<Result<_, _>>()?,
            effects: Some(TransactionEffects::try_from(&transaction.effects)?),
            events: transaction
                .events
                .as_ref()
                .map(TransactionEvents::try_from)
                .transpose()?,
            input_objects: transaction
                .input_objects
                .iter()
                .map(Object::try_from)
                .collect::<Result<_, _>>()?,
            output_objects: transaction
                .output_objects
                .iter()
                .map(Object::try_from)
                .collect::<Result<_, _>>()?,
        })
    }
}

impl TryFrom<CheckpointTransaction> for sui_types::full_checkpoint_content::CheckpointTransaction {
    type Error = bcs::Error;

    fn try_from(transaction: CheckpointTransaction) -> Result<Self, Self::Error> {
        let transaction_data = transaction
            .transaction
            .ok_or_else(|| bcs::Error::Custom("missing transaction".into()))?
            .pipe_ref(TryInto::try_into)?;
        let user_signatures = transaction
            .signatures
            .iter()
            .map(TryInto::try_into)
            .collect::<Result<_, _>>()?;

        Ok(Self {
            transaction: sui_types::transaction::Transaction::new(
                sui_types::transaction::SenderSignedData::new(transaction_data, user_signatures),
            ),
            effects: transaction
                .effects
                .ok_or_else(|| bcs::Error::Custom("missing Effects".into()))?
                .pipe_ref(TryInto::try_into)?,
            events: transaction
                .events
                .as_ref()
                .map(TryInto::try_into)
                .transpose()?,
            input_objects: transaction
                .input_objects
                .iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
            output_objects: transaction
                .output_objects
                .iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
        })
    }
}

//
// FullCheckpoint
//

impl TryFrom<sui_types::full_checkpoint_content::CheckpointData> for FullCheckpoint {
    type Error = bcs::Error;

    fn try_from(
        c: sui_types::full_checkpoint_content::CheckpointData,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            summary: Some(CheckpointSummary::try_from(c.checkpoint_summary.data())?),
            signature: Some(ValidatorAggregatedSignature::try_from(
                c.checkpoint_summary.auth_sig(),
            )?),
            contents: Some(CheckpointContents::try_from(&c.checkpoint_contents)?),
            transactions: c
                .transactions
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
        })
    }
}

impl TryFrom<FullCheckpoint> for sui_types::full_checkpoint_content::CheckpointData {
    type Error = bcs::Error;

    fn try_from(checkpoint: FullCheckpoint) -> Result<Self, Self::Error> {
        let summary = checkpoint
            .summary
            .ok_or_else(|| bcs::Error::Custom("missing summary".into()))?
            .pipe_ref(TryInto::try_into)?;
        let signature = checkpoint
            .signature
            .ok_or_else(|| bcs::Error::Custom("missing signature".into()))?
            .pipe_ref(TryInto::try_into)?;
        let checkpoint_summary =
            sui_types::messages_checkpoint::CertifiedCheckpointSummary::new_from_data_and_sig(
                summary, signature,
            );

        let contents = checkpoint
            .contents
            .ok_or_else(|| bcs::Error::Custom("missing checkpoint contents".into()))?
            .pipe_ref(TryInto::try_into)?;

        let transactions = checkpoint
            .transactions
            .into_iter()
            .map(TryInto::try_into)
            .collect::<Result<_, _>>()?;

        Ok(Self {
            checkpoint_summary,
            checkpoint_contents: contents,
            transactions,
        })
    }
}

//
// Address
//

impl From<&sui_sdk_types::types::Address> for Address {
    fn from(value: &sui_sdk_types::types::Address) -> Self {
        Self {
            address: value.as_bytes().to_vec().into(),
        }
    }
}

impl TryFrom<&Address> for sui_sdk_types::types::Address {
    type Error = bcs::Error;

    fn try_from(value: &Address) -> Result<Self, Self::Error> {
        Self::from_bytes(&value.address).map_err(|e| bcs::Error::Custom(e.to_string()))
    }
}

impl TryFrom<&Address> for sui_types::base_types::SuiAddress {
    type Error = bcs::Error;

    fn try_from(value: &Address) -> Result<Self, Self::Error> {
        Self::from_bytes(&value.address).map_err(|e| bcs::Error::Custom(e.to_string()))
    }
}

//
// TypeTag
//

impl From<&sui_sdk_types::types::TypeTag> for TypeTag {
    fn from(value: &sui_sdk_types::types::TypeTag) -> Self {
        Self {
            type_tag: value.to_string(),
        }
    }
}

impl TryFrom<&TypeTag> for sui_sdk_types::types::TypeTag {
    type Error = sui_sdk_types::types::TypeParseError;

    fn try_from(value: &TypeTag) -> Result<Self, Self::Error> {
        value.type_tag.parse()
    }
}

impl TryFrom<&TypeTag> for sui_types::TypeTag {
    type Error = bcs::Error;

    fn try_from(value: &TypeTag) -> Result<Self, Self::Error> {
        value
            .type_tag
            .parse::<sui_types::TypeTag>()
            .map_err(|e| bcs::Error::Custom(e.to_string()))
    }
}

//
// I128
//

impl From<i128> for I128 {
    fn from(value: i128) -> Self {
        Self {
            little_endian_bytes: value.to_le_bytes().to_vec().into(),
        }
    }
}

impl TryFrom<&I128> for i128 {
    type Error = std::array::TryFromSliceError;

    fn try_from(value: &I128) -> Result<Self, Self::Error> {
        Ok(i128::from_le_bytes(
            value.little_endian_bytes.as_ref().try_into()?,
        ))
    }
}

//
// BalanceChange
//

impl From<&sui_sdk_types::types::BalanceChange> for BalanceChange {
    fn from(value: &sui_sdk_types::types::BalanceChange) -> Self {
        Self {
            address: Some(Address::from(&value.address)),
            coin_type: Some(TypeTag::from(&value.coin_type)),
            amount: Some(I128::from(value.amount)),
        }
    }
}

impl TryFrom<&BalanceChange> for sui_sdk_types::types::BalanceChange {
    type Error = bcs::Error;

    fn try_from(value: &BalanceChange) -> Result<Self, Self::Error> {
        let address = value
            .address
            .as_ref()
            .ok_or_else(|| bcs::Error::Custom("missing address".into()))?
            .try_into()?;

        let coin_type = value
            .coin_type
            .as_ref()
            .ok_or_else(|| bcs::Error::Custom("missing coin_type".into()))?
            .pipe(sui_sdk_types::types::TypeTag::try_from)
            .map_err(|e| bcs::Error::Custom(e.to_string()))?;

        let amount = value
            .amount
            .as_ref()
            .ok_or_else(|| bcs::Error::Custom("missing amount".into()))?
            .pipe(i128::try_from)
            .map_err(|e| bcs::Error::Custom(e.to_string()))?;

        Ok(Self {
            address,
            coin_type,
            amount,
        })
    }
}

impl TryFrom<&BalanceChange> for crate::client::BalanceChange {
    type Error = bcs::Error;

    fn try_from(value: &BalanceChange) -> Result<Self, Self::Error> {
        let address = value
            .address
            .as_ref()
            .ok_or_else(|| bcs::Error::Custom("missing address".into()))?
            .try_into()?;

        let coin_type = value
            .coin_type
            .as_ref()
            .ok_or_else(|| bcs::Error::Custom("missing coin_type".into()))?
            .pipe(sui_types::TypeTag::try_from)
            .map_err(|e| bcs::Error::Custom(e.to_string()))?;

        let amount = value
            .amount
            .as_ref()
            .ok_or_else(|| bcs::Error::Custom("missing amount".into()))?
            .pipe(i128::try_from)
            .map_err(|e| bcs::Error::Custom(e.to_string()))?;

        Ok(Self {
            address,
            coin_type,
            amount,
        })
    }
}
//
// EffectsFinality
//

impl TryFrom<&crate::rest::transactions::EffectsFinality> for EffectsFinality {
    type Error = bcs::Error;

    fn try_from(value: &crate::rest::transactions::EffectsFinality) -> Result<Self, Self::Error> {
        let (signature, checkpoint, quorum_executed) = match value {
            crate::rest::transactions::EffectsFinality::Certified { signature } => {
                (Some(signature.try_into()?), None, None)
            }
            crate::rest::transactions::EffectsFinality::Checkpointed { checkpoint } => {
                (None, Some(*checkpoint), None)
            }
            crate::rest::transactions::EffectsFinality::QuorumExecuted => (None, None, Some(true)),
        };

        Ok(Self {
            signature,
            checkpoint,
            quorum_executed,
        })
    }
}

impl TryFrom<&EffectsFinality> for crate::rest::transactions::EffectsFinality {
    type Error = bcs::Error;

    fn try_from(value: &EffectsFinality) -> Result<Self, Self::Error> {
        let signature = value
            .signature
            .as_ref()
            .map(sui_sdk_types::types::ValidatorAggregatedSignature::try_from)
            .transpose()?;
        match (signature, value.checkpoint, value.quorum_executed) {
            (Some(signature), None, None) => {
                crate::rest::transactions::EffectsFinality::Certified { signature }
            }
            (None, Some(checkpoint), None) => {
                crate::rest::transactions::EffectsFinality::Checkpointed { checkpoint }
            }
            (None, None, Some(true)) => crate::rest::transactions::EffectsFinality::QuorumExecuted,
            _ => return Err(bcs::Error::Custom("invalid EffectsFinality message".into())),
        }
        .pipe(Ok)
    }
}

impl TryFrom<&EffectsFinality> for crate::client::EffectsFinality {
    type Error = bcs::Error;

    fn try_from(value: &EffectsFinality) -> Result<Self, Self::Error> {
        let signature = value
            .signature
            .as_ref()
            .map(sui_types::crypto::AuthorityStrongQuorumSignInfo::try_from)
            .transpose()?;
        match (signature, value.checkpoint) {
            (Some(signature), _) => crate::client::EffectsFinality::Certified { signature },
            (None, Some(checkpoint)) => crate::client::EffectsFinality::Checkpointed { checkpoint },
            (None, None) => {
                return Err(bcs::Error::Custom(
                    "missing signature or checkpoint field".into(),
                ))
            }
        }
        .pipe(Ok)
    }
}

//
// TransactionExecutionResponse
//

impl TryFrom<crate::rest::transactions::TransactionExecutionResponse>
    for TransactionExecutionResponse
{
    type Error = bcs::Error;

    fn try_from(
        value: crate::rest::transactions::TransactionExecutionResponse,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            effects: Some(TransactionEffects::try_from(&value.effects)?),
            finality: Some(EffectsFinality::try_from(&value.finality)?),
            events: value
                .events
                .as_ref()
                .map(TransactionEvents::try_from)
                .transpose()?,
            balance_changes: value
                .balance_changes
                .iter()
                .flat_map(|balance_changes| balance_changes.iter())
                .map(BalanceChange::from)
                .collect(),
            input_objects: value
                .input_objects
                .iter()
                .flat_map(|objects| objects.iter())
                .map(Object::try_from)
                .collect::<Result<_, _>>()?,
            output_objects: value
                .output_objects
                .iter()
                .flat_map(|objects| objects.iter())
                .map(Object::try_from)
                .collect::<Result<_, _>>()?,
        })
    }
}

impl TryFrom<TransactionExecutionResponse>
    for crate::rest::transactions::TransactionExecutionResponse
{
    type Error = bcs::Error;

    fn try_from(value: TransactionExecutionResponse) -> Result<Self, Self::Error> {
        Ok(Self {
            effects: value
                .effects
                .ok_or_else(|| bcs::Error::Custom("missing Effects".into()))?
                .pipe_ref(TryInto::try_into)?,
            events: value.events.as_ref().map(TryInto::try_into).transpose()?,
            input_objects: Some(
                value
                    .input_objects
                    .iter()
                    .map(TryInto::try_into)
                    .collect::<Result<_, _>>()?,
            ),
            output_objects: Some(
                value
                    .output_objects
                    .iter()
                    .map(TryInto::try_into)
                    .collect::<Result<_, _>>()?,
            ),
            finality: value
                .finality
                .ok_or_else(|| bcs::Error::Custom("missing finality".into()))?
                .pipe_ref(TryInto::try_into)?,
            balance_changes: Some(
                value
                    .balance_changes
                    .iter()
                    .map(TryInto::try_into)
                    .collect::<Result<_, _>>()?,
            ),
        })
    }
}

impl TryFrom<TransactionExecutionResponse> for crate::client::TransactionExecutionResponse {
    type Error = bcs::Error;

    fn try_from(value: TransactionExecutionResponse) -> Result<Self, Self::Error> {
        Ok(Self {
            effects: value
                .effects
                .ok_or_else(|| bcs::Error::Custom("missing Effects".into()))?
                .pipe_ref(TryInto::try_into)?,
            events: value.events.as_ref().map(TryInto::try_into).transpose()?,
            input_objects: Some(
                value
                    .input_objects
                    .iter()
                    .map(TryInto::try_into)
                    .collect::<Result<_, _>>()?,
            ),
            output_objects: Some(
                value
                    .output_objects
                    .iter()
                    .map(TryInto::try_into)
                    .collect::<Result<_, _>>()?,
            ),
            finality: value
                .finality
                .ok_or_else(|| bcs::Error::Custom("missing finality".into()))?
                .pipe_ref(TryInto::try_into)?,
            balance_changes: Some(
                value
                    .balance_changes
                    .iter()
                    .map(TryInto::try_into)
                    .collect::<Result<_, _>>()?,
            ),
        })
    }
}

//
// TransactionSimulationResponse
//

impl TryFrom<crate::rest::transactions::TransactionSimulationResponse>
    for TransactionSimulationResponse
{
    type Error = bcs::Error;

    fn try_from(
        value: crate::rest::transactions::TransactionSimulationResponse,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            effects: Some(TransactionEffects::try_from(&value.effects)?),
            events: value
                .events
                .as_ref()
                .map(TransactionEvents::try_from)
                .transpose()?,
            balance_changes: value
                .balance_changes
                .iter()
                .flat_map(|balance_changes| balance_changes.iter())
                .map(BalanceChange::from)
                .collect(),
            input_objects: value
                .input_objects
                .iter()
                .flat_map(|objects| objects.iter())
                .map(Object::try_from)
                .collect::<Result<_, _>>()?,
            output_objects: value
                .output_objects
                .iter()
                .flat_map(|objects| objects.iter())
                .map(Object::try_from)
                .collect::<Result<_, _>>()?,
        })
    }
}

impl TryFrom<TransactionSimulationResponse>
    for crate::rest::transactions::TransactionSimulationResponse
{
    type Error = bcs::Error;

    fn try_from(value: TransactionSimulationResponse) -> Result<Self, Self::Error> {
        Ok(Self {
            effects: value
                .effects
                .ok_or_else(|| bcs::Error::Custom("missing Effects".into()))?
                .pipe_ref(TryInto::try_into)?,
            events: value.events.as_ref().map(TryInto::try_into).transpose()?,
            input_objects: Some(
                value
                    .input_objects
                    .iter()
                    .map(TryInto::try_into)
                    .collect::<Result<_, _>>()?,
            ),
            output_objects: Some(
                value
                    .output_objects
                    .iter()
                    .map(TryInto::try_into)
                    .collect::<Result<_, _>>()?,
            ),
            balance_changes: Some(
                value
                    .balance_changes
                    .iter()
                    .map(TryInto::try_into)
                    .collect::<Result<_, _>>()?,
            ),
        })
    }
}

//
// ResolveTransactionResponse
//

impl TryFrom<crate::rest::transactions::ResolveTransactionResponse> for ResolveTransactionResponse {
    type Error = bcs::Error;

    fn try_from(
        value: crate::rest::transactions::ResolveTransactionResponse,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            transaction: Some(Transaction::try_from(&value.transaction)?),
            simulation: value
                .simulation
                .map(TransactionSimulationResponse::try_from)
                .transpose()?,
        })
    }
}

impl TryFrom<ResolveTransactionResponse> for crate::rest::transactions::ResolveTransactionResponse {
    type Error = bcs::Error;

    fn try_from(value: ResolveTransactionResponse) -> Result<Self, Self::Error> {
        Ok(Self {
            transaction: value
                .transaction
                .ok_or_else(|| bcs::Error::Custom("missing transaction".into()))?
                .pipe_ref(TryInto::try_into)?,
            simulation: value.simulation.map(TryInto::try_into).transpose()?,
        })
    }
}

//
// ExecuteTransactionRequest
//

impl TryFrom<sui_sdk_types::types::SignedTransaction> for ExecuteTransactionRequest {
    type Error = bcs::Error;

    fn try_from(value: sui_sdk_types::types::SignedTransaction) -> Result<Self, Self::Error> {
        Ok(Self {
            transaction: Some(Transaction::try_from(&value.transaction)?),
            signatures: value
                .signatures
                .iter()
                .map(UserSignature::try_from)
                .collect::<Result<_, _>>()?,
        })
    }
}

impl TryFrom<ExecuteTransactionRequest> for sui_sdk_types::types::SignedTransaction {
    type Error = bcs::Error;

    fn try_from(value: ExecuteTransactionRequest) -> Result<Self, Self::Error> {
        Ok(Self {
            transaction: value
                .transaction
                .ok_or_else(|| bcs::Error::Custom("missing transaction".into()))?
                .pipe_ref(TryInto::try_into)?,
            signatures: value
                .signatures
                .iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
        })
    }
}

//
// SimulateTransactionRequest
//

impl TryFrom<sui_sdk_types::types::Transaction> for SimulateTransactionRequest {
    type Error = bcs::Error;

    fn try_from(value: sui_sdk_types::types::Transaction) -> Result<Self, Self::Error> {
        Ok(Self {
            transaction: Some(Transaction::try_from(&value)?),
        })
    }
}

impl TryFrom<SimulateTransactionRequest> for sui_sdk_types::types::Transaction {
    type Error = bcs::Error;

    fn try_from(value: SimulateTransactionRequest) -> Result<Self, Self::Error> {
        value
            .transaction
            .ok_or_else(|| bcs::Error::Custom("missing transaction".into()))?
            .pipe_ref(TryInto::try_into)
    }
}

//
// ValidatorCommitteeMember
//

impl From<&sui_sdk_types::types::ValidatorCommitteeMember> for ValidatorCommitteeMember {
    fn from(value: &sui_sdk_types::types::ValidatorCommitteeMember) -> Self {
        Self {
            public_key: value.public_key.as_bytes().to_vec().into(),
            stake: value.stake,
        }
    }
}

impl TryFrom<ValidatorCommitteeMember> for sui_sdk_types::types::ValidatorCommitteeMember {
    type Error = bcs::Error;

    fn try_from(value: ValidatorCommitteeMember) -> Result<Self, Self::Error> {
        Ok(Self {
            public_key: sui_sdk_types::types::Bls12381PublicKey::from_bytes(&value.public_key)
                .map_err(|e| bcs::Error::Custom(e.to_string()))?,
            stake: value.stake,
        })
    }
}

//
// ValidatorCommittee
//

impl From<sui_sdk_types::types::ValidatorCommittee> for ValidatorCommittee {
    fn from(value: sui_sdk_types::types::ValidatorCommittee) -> Self {
        Self {
            epoch: value.epoch,
            members: value
                .members
                .iter()
                .map(ValidatorCommitteeMember::from)
                .collect(),
        }
    }
}

impl TryFrom<ValidatorCommittee> for sui_sdk_types::types::ValidatorCommittee {
    type Error = bcs::Error;

    fn try_from(value: ValidatorCommittee) -> Result<Self, Self::Error> {
        Ok(Self {
            epoch: value.epoch,
            members: value
                .members
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
        })
    }
}
