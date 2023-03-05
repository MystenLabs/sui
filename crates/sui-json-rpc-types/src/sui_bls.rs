// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt::{self, Display, Formatter, Write};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sui_types::base_types::{EpochId, TransactionEffectsDigest};
use sui_types::crypto::SuiAuthorityStrongQuorumSignInfo;
use sui_types::messages::EffectsFinalityInfo;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;

use crate::SuiTransactionEffects;

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename = "EffectsFinalityInfo", rename_all = "camelCase")]
pub enum SuiEffectsFinalityInfo {
    Certified(SuiAuthorityStrongQuorumSignInfo),
    Checkpointed(EpochId, CheckpointSequenceNumber),
}

impl From<EffectsFinalityInfo> for SuiEffectsFinalityInfo {
    fn from(info: EffectsFinalityInfo) -> Self {
        match info {
            EffectsFinalityInfo::Certified(cert) => {
                Self::Certified(SuiAuthorityStrongQuorumSignInfo::from(&cert))
            }
            EffectsFinalityInfo::Checkpointed(epoch, checkpoint) => {
                Self::Checkpointed(epoch, checkpoint)
            }
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename = "FinalizedEffects", rename_all = "camelCase")]
pub struct SuiFinalizedEffects {
    pub transaction_effects_digest: TransactionEffectsDigest,
    pub effects: SuiTransactionEffects,
    pub finality_info: SuiEffectsFinalityInfo,
}

impl Display for SuiFinalizedEffects {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut writer = String::new();
        writeln!(
            writer,
            "Transaction Effects Digest: {:?}",
            self.transaction_effects_digest
        )?;
        writeln!(writer, "Transaction Effects: {:?}", self.effects)?;
        match &self.finality_info {
            SuiEffectsFinalityInfo::Certified(cert) => {
                writeln!(writer, "Signed Authorities Bitmap: {:?}", cert.signers_map)?;
            }
            SuiEffectsFinalityInfo::Checkpointed(epoch, checkpoint) => {
                writeln!(
                    writer,
                    "Finalized at epoch {:?}, checkpoint {:?}",
                    epoch, checkpoint
                )?;
            }
        }

        write!(f, "{}", writer)
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Serialize, Deserialize, Debug, JsonSchema)]
pub enum SuiTBlsSignObjectCommitmentType {
    /// Check that the object is committed by the consensus.
    ConsensusCommitted,
    /// Check that the object is committed using the effects certificate.
    FastPathCommitted(SuiFinalizedEffects),
}

#[derive(Serialize, Deserialize, Debug, JsonSchema)]
pub struct SuiTBlsSignRandomnessObjectResponse {
    pub signature: fastcrypto_tbls::types::RawSignature,
}
