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
// GetCheckpointResponse
//

impl TryFrom<crate::checkpoints::CheckpointResponse> for GetCheckpointResponse {
    type Error = bcs::Error;

    fn try_from(c: crate::checkpoints::CheckpointResponse) -> Result<Self, Self::Error> {
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

impl TryFrom<GetCheckpointResponse> for crate::checkpoints::CheckpointResponse {
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

impl TryFrom<Vec<crate::checkpoints::CheckpointResponse>> for ListCheckpointResponse {
    type Error = bcs::Error;
    fn try_from(value: Vec<crate::checkpoints::CheckpointResponse>) -> Result<Self, Self::Error> {
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

impl TryFrom<crate::transactions::TransactionResponse> for GetTransactionResponse {
    type Error = bcs::Error;

    fn try_from(value: crate::transactions::TransactionResponse) -> Result<Self, Self::Error> {
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

impl TryFrom<GetTransactionResponse> for crate::transactions::TransactionResponse {
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
