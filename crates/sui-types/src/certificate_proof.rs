// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::committee::EpochId;
use crate::messages_checkpoint::CheckpointSequenceNumber;

use serde::{Deserialize, Serialize};

/// CertificateProof is a placeholder for signatures, which indicates that the wrapped message has
/// been proven valid through indirect means, typically inclusion in a certified checkpoint or
/// via f+1 votes that the message is correct.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CertificateProof(CertificateProofKind);

impl CertificateProof {
    pub(crate) fn from_certified(epoch_id: EpochId) -> Self {
        CertificateProof(CertificateProofKind::Certified(epoch_id))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum CertificateProofKind {
    // Validity was proven by inclusion in the given checkpoint
    Checkpoint(EpochId, CheckpointSequenceNumber),

    // CertificateProof was converted directly from a certified structure, and
    // the signatures were dropped
    Certified(EpochId),

    // Validity was proven by a vote of f+1 validators during the given epoch.
    // TODO: This may not be needed anymore
    ValidityVote(EpochId),
}
