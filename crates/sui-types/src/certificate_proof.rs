// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::committee::EpochId;
use crate::messages_checkpoint::CheckpointSequenceNumber;

use serde::{Deserialize, Serialize};

/// CertificateProof is a placeholder for signatures, which indicates that the wrapped message has
/// been proven valid through indirect means, typically inclusion in a certified checkpoint or
/// via f+1 votes that the message is correct.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CertificateProof(EpochId, CertificateProofKind);

impl CertificateProof {
    pub(crate) fn from_certified(epoch_id: EpochId) -> Self {
        CertificateProof(epoch_id, CertificateProofKind::Certified)
    }

    pub fn from_local_computation(epoch_id: EpochId) -> Self {
        CertificateProof(epoch_id, CertificateProofKind::LocallyComputed)
    }

    pub fn from_checkpoint(checkpoint: &VerifiedCheckpoint) -> Self {
        CertificateProof(
            checkpoint.summary.epoch,
            CertificateProofKind::Checkpoint(checkpoint.summary.sequence_number),
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum CertificateProofKind {
    // Validity was proven by inclusion in the given checkpoint
    Checkpoint(CheckpointSequenceNumber),

    // CertificateProof was converted directly from a certified structure, and
    // the signatures were dropped
    Certified,

    // Validity was proven by a vote of f+1 validators during the given epoch.
    // TODO: This may not be needed anymore
    ValidityVote,

    // The data is valid because it was computed locally - only applicable to
    // TransactionEffects.
    LocallyComputed,
}
