// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::types::CheckpointResponse;
use crate::types::GetCheckpointOptions;
use crate::Result;
use crate::RpcService;
use sui_sdk_types::types::CheckpointContents;
use sui_sdk_types::types::CheckpointDigest;
use sui_sdk_types::types::CheckpointSequenceNumber;
use sui_sdk_types::types::SignedCheckpointSummary;
use tap::Pipe;

impl RpcService {
    pub fn get_checkpoint(
        &self,
        checkpoint: Option<CheckpointId>,
        options: GetCheckpointOptions,
    ) -> Result<CheckpointResponse> {
        let SignedCheckpointSummary {
            checkpoint,
            signature,
        } = match checkpoint {
            Some(checkpoint_id @ CheckpointId::SequenceNumber(s)) => {
                let oldest_checkpoint = self.reader.inner().get_lowest_available_checkpoint()?;
                if s < oldest_checkpoint {
                    return Err(crate::RpcServiceError::new(
                        axum::http::StatusCode::GONE,
                        "Old checkpoints have been pruned",
                    ));
                }

                self.reader
                    .inner()
                    .get_checkpoint_by_sequence_number(s)
                    .ok_or(CheckpointNotFoundError(checkpoint_id))?
            }
            Some(checkpoint_id @ CheckpointId::Digest(d)) => self
                .reader
                .inner()
                .get_checkpoint_by_digest(&d.into())
                .ok_or(CheckpointNotFoundError(checkpoint_id))?,
            None => self.reader.inner().get_latest_checkpoint()?,
        }
        .into_inner()
        .try_into()?;

        let (contents, contents_bcs) =
            if options.include_contents() || options.include_contents_bcs() {
                let contents: CheckpointContents = self
                    .reader
                    .inner()
                    .get_checkpoint_contents_by_sequence_number(checkpoint.sequence_number)
                    .ok_or(CheckpointNotFoundError(CheckpointId::SequenceNumber(
                        checkpoint.sequence_number,
                    )))?
                    .try_into()?;

                let contents_bcs = options
                    .include_contents_bcs()
                    .then(|| bcs::to_bytes(&contents))
                    .transpose()?;

                (options.include_contents().then_some(contents), contents_bcs)
            } else {
                (None, None)
            };

        let summary_bcs = options
            .include_summary_bcs()
            .then(|| bcs::to_bytes(&checkpoint))
            .transpose()?;

        CheckpointResponse {
            sequence_number: checkpoint.sequence_number,
            digest: checkpoint.digest(),
            summary: options.include_summary().then_some(checkpoint),
            summary_bcs,
            signature: options.include_signature().then_some(signature),
            contents,
            contents_bcs,
        }
        .pipe(Ok)
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, schemars::JsonSchema)]
#[schemars(untagged)]
pub enum CheckpointId {
    #[schemars(
        title = "SequenceNumber",
        example = "CheckpointSequenceNumber::default"
    )]
    /// Sequence number or height of a Checkpoint
    SequenceNumber(#[schemars(with = "crate::rest::_schemars::U64")] CheckpointSequenceNumber),
    #[schemars(title = "Digest", example = "example_digest")]
    /// Base58 encoded 32-byte digest of a Checkpoint
    Digest(CheckpointDigest),
}

fn example_digest() -> CheckpointDigest {
    "4btiuiMPvEENsttpZC7CZ53DruC3MAgfznDbASZ7DR6S"
        .parse()
        .unwrap()
}

impl<'de> serde::Deserialize<'de> for CheckpointId {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;

        if let Ok(s) = raw.parse::<CheckpointSequenceNumber>() {
            Ok(Self::SequenceNumber(s))
        } else if let Ok(d) = raw.parse::<CheckpointDigest>() {
            Ok(Self::Digest(d))
        } else {
            Err(serde::de::Error::custom(format!(
                "unrecognized checkpoint-id {raw}"
            )))
        }
    }
}

impl serde::Serialize for CheckpointId {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            CheckpointId::SequenceNumber(s) => serializer.serialize_str(&s.to_string()),
            CheckpointId::Digest(d) => serializer.serialize_str(&d.to_string()),
        }
    }
}

#[derive(Debug)]
pub struct CheckpointNotFoundError(pub CheckpointId);

impl std::fmt::Display for CheckpointNotFoundError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Checkpoint ")?;

        match self.0 {
            CheckpointId::SequenceNumber(n) => write!(f, "{n}")?,
            CheckpointId::Digest(d) => write!(f, "{d}")?,
        }

        write!(f, " not found")
    }
}

impl std::error::Error for CheckpointNotFoundError {}

impl From<CheckpointNotFoundError> for crate::RpcServiceError {
    fn from(value: CheckpointNotFoundError) -> Self {
        Self::new(axum::http::StatusCode::NOT_FOUND, value.to_string())
    }
}
