// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::slice::Iter;

use crate::base_types::ExecutionDigests;
use crate::committee::EpochId;
use crate::crypto::{AuthoritySignInfo, AuthorityWeakQuorumSignInfo, Signable};
use crate::error::SuiResult;
use crate::messages::CertifiedTransaction;
use crate::waypoint::{Waypoint, WaypointDiff};
use crate::{
    base_types::AuthorityName,
    committee::Committee,
    crypto::{sha3_hash, AuthoritySignature, BcsSignable, VerificationObligation},
    error::SuiError,
};
use serde::{Deserialize, Serialize};

/*

    The checkpoint messages, structures and protocol: A gentle overview
    -------------------------------------------------------------------

    Checkpoint proposals:
    --------------------

    Authorities operate and process certified transactions. When they have
    processed all transactions included in a previous checkpoint (we will
    see how this is set) each authority proposes a signed proposed
    checkpoint (SignedCheckpointProposal) for the next sequence number.

    A proposal is built on the basis of a set of transactions that the
    authority has processed and wants to include in the next checkpoint.
    Right now we just list these as transaction digests but down the line
    we will rely on more efficient ways to determine the set for parties that
    may already have a very similar set of digests.

    From proposals to checkpoints:
    -----------------------------

    A checkpoint is formed by a set of checkpoint proposals representing
    2/3 of the authorities by stake. The checkpoint contains the union of
    transactions in all the proposals. A checkpoint needs to provide enough
    evidence to ensure all authorities may recover the transactions
    included. Since all authorities need to agree on which checkpoint (out
    of the potentially many sets of 2/3 stake) constitutes the checkpoint
    we need an agreement protocol to determine this.

    Checkpoint confirmation:
    -----------------------

    Once a checkpoint is determined each authority forms a CheckpointSummary
    with all the transactions in the checkpoint, and signs it with its
    authority key to form a SignedCheckpoint. A collection of 2/3 authority
    signatures on a checkpoint forms a CertifiedCheckpoint. And this is the
    structure that is kept in the long term to attest of the sequence of
    checkpoints. Once a CertifiedCheckpoint is recoded for a checkpoint
    all other information leading to the checkpoint may be deleted.

    Reads:
    -----

    To facilitate the protocol authorities always provide facilities for
    reads:
    - To get past checkpoints signatures, certificates and the transactions
      associated with them.
    - To get the current signed proposal. Or if there is no proposal a
      hint about which transaction digests are pending processing to get
      a proposal.

*/

pub type CheckpointSequenceNumber = u64;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckpointRequest {
    // Type of checkpoint request
    pub request_type: CheckpointRequestType,
    // A flag, if true also return the contents of the
    // checkpoint besides the meta-data.
    pub detail: bool,
}

impl CheckpointRequest {
    /// Create a request for the latest checkpoint proposal from the authority
    pub fn latest(detail: bool) -> CheckpointRequest {
        CheckpointRequest {
            request_type: CheckpointRequestType::LatestCheckpointProposal,
            detail,
        }
    }

    /// Create a request for a past checkpoint from the authority
    pub fn past(seq: CheckpointSequenceNumber, detail: bool) -> CheckpointRequest {
        CheckpointRequest {
            request_type: CheckpointRequestType::PastCheckpoint(seq),
            detail,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CheckpointRequestType {
    // Request the latest proposal and previous checkpoint.
    LatestCheckpointProposal,
    // Requests a past checkpoint
    PastCheckpoint(CheckpointSequenceNumber),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckpointResponse {
    // The response to the request, according to the type
    // and the information available.
    pub info: AuthorityCheckpointInfo,
    // If the detail flag in the request was set, then return
    // the list of transactions as well.
    pub detail: Option<CheckpointContents>,
}

impl CheckpointResponse {
    pub fn verify(&self, committee: &Committee) -> SuiResult {
        match &self.info {
            AuthorityCheckpointInfo::Success => Ok(()),
            AuthorityCheckpointInfo::Proposal { current, previous } => {
                if let Some(current) = current {
                    current.verify(committee, self.detail.as_ref())?;
                    // detail pertains to the current proposal, not the previous
                    previous.verify(committee, None)?;
                }
                Ok(())
            }
            AuthorityCheckpointInfo::Past(ckpt) => ckpt.verify(committee, self.detail.as_ref()),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum AuthorityCheckpointInfo {
    // Denotes success of he operation with no return
    Success,
    // Returns the current proposal if any, and
    // the previous checkpoint.
    Proposal {
        current: Option<SignedCheckpointProposalSummary>,
        previous: AuthenticatedCheckpoint,
        // Include in all responses the local state of the sequence
        // of transaction to allow followers to track the latest
        // updates.
        // last_local_sequence: TxSequenceNumber,
    },
    // Returns the requested checkpoint.
    Past(AuthenticatedCheckpoint),
}

// TODO: Rename to AuthenticatedCheckpointSummary
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum AuthenticatedCheckpoint {
    // No authentication information is available
    // or checkpoint is not available on this authority.
    None,
    // The checkpoint with just a single authority
    // signature.
    Signed(SignedCheckpointSummary),
    // The checkpoint with a quorum of signatures.
    Certified(CertifiedCheckpointSummary),
}

impl AuthenticatedCheckpoint {
    pub fn summary(&self) -> &CheckpointSummary {
        match self {
            Self::Signed(s) => &s.summary,
            Self::Certified(c) => &c.summary,
            Self::None => unreachable!(),
        }
    }

    pub fn verify(&self, committee: &Committee, detail: Option<&CheckpointContents>) -> SuiResult {
        match self {
            Self::Signed(s) => s.verify(committee, detail),
            Self::Certified(c) => c.verify(committee, detail),
            Self::None => Ok(()),
        }
    }
}

pub type CheckpointDigest = [u8; 32];
pub type CheckpointContentsDigest = [u8; 32];

// The constituent parts of checkpoints, signed and certified

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CheckpointSummary {
    pub epoch: EpochId,
    pub sequence_number: CheckpointSequenceNumber,
    pub content_digest: CheckpointContentsDigest,
    pub previous_digest: Option<CheckpointDigest>,
}

impl CheckpointSummary {
    pub fn new(
        epoch: EpochId,
        sequence_number: CheckpointSequenceNumber,
        transactions: &CheckpointContents,
        previous_digest: Option<CheckpointDigest>,
    ) -> CheckpointSummary {
        let mut waypoint = Box::new(Waypoint::default());
        transactions.transactions.iter().for_each(|tx| {
            waypoint.insert(tx);
        });

        let content_digest = transactions.digest();

        Self {
            epoch,
            sequence_number,
            content_digest,
            previous_digest,
        }
    }

    pub fn sequence_number(&self) -> &CheckpointSequenceNumber {
        &self.sequence_number
    }

    pub fn digest(&self) -> CheckpointDigest {
        sha3_hash(self)
    }
}

impl BcsSignable for CheckpointSummary {}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckpointSummaryEnvelope<S> {
    pub summary: CheckpointSummary,
    pub auth_signature: S,
}

pub type SignedCheckpointSummary = CheckpointSummaryEnvelope<AuthoritySignInfo>;

impl SignedCheckpointSummary {
    /// Create a new signed checkpoint proposal for this authority
    pub fn new(
        epoch: EpochId,
        sequence_number: CheckpointSequenceNumber,
        authority: AuthorityName,
        signer: &dyn signature::Signer<AuthoritySignature>,
        transactions: &CheckpointContents,
        previous_digest: Option<CheckpointDigest>,
    ) -> SignedCheckpointSummary {
        let checkpoint =
            CheckpointSummary::new(epoch, sequence_number, transactions, previous_digest);
        SignedCheckpointSummary::new_from_summary(checkpoint, authority, signer)
    }

    pub fn new_from_summary(
        checkpoint: CheckpointSummary,
        authority: AuthorityName,
        signer: &dyn signature::Signer<AuthoritySignature>,
    ) -> SignedCheckpointSummary {
        let signature = AuthoritySignature::new(&checkpoint, signer);

        let epoch = checkpoint.epoch;
        SignedCheckpointSummary {
            summary: checkpoint,
            auth_signature: AuthoritySignInfo {
                epoch,
                authority,
                signature,
            },
        }
    }

    pub fn authority(&self) -> &AuthorityName {
        &self.auth_signature.authority
    }

    /// Checks that the signature on the digest is correct, and verify the contents as well if
    /// provided.
    pub fn verify(
        &self,
        committee: &Committee,
        contents: Option<&CheckpointContents>,
    ) -> Result<(), SuiError> {
        fp_ensure!(
            self.summary.epoch == self.auth_signature.epoch,
            SuiError::from("Epoch in the summary doesn't match with the signature")
        );

        self.auth_signature.verify(&self.summary, committee)?;

        if let Some(contents) = contents {
            fp_ensure!(
                contents.digest() == self.summary.content_digest,
                SuiError::from("Checkpoint contents digest mismatch")
            );
        }

        Ok(())
    }
}

// Checkpoints are signed by an authority and 2f+1 form a
// certificate that others can use to catch up. The actual
// content of the digest must at the very least commit to
// the set of transactions contained in the certificate but
// we might extend this to contain roots of merkle trees,
// or other authenticated data structures to support light
// clients and more efficient sync protocols.

pub type CertifiedCheckpointSummary = CheckpointSummaryEnvelope<AuthorityWeakQuorumSignInfo>;

impl CertifiedCheckpointSummary {
    /// Aggregate many checkpoint signatures to form a checkpoint certificate.
    pub fn aggregate(
        signed_checkpoints: Vec<SignedCheckpointSummary>,
        committee: &Committee,
    ) -> Result<CertifiedCheckpointSummary, SuiError> {
        fp_ensure!(
            !signed_checkpoints.is_empty(),
            SuiError::from("Need at least one signed checkpoint to aggregate")
        );
        fp_ensure!(
            signed_checkpoints
                .iter()
                .all(|c| c.summary.epoch == committee.epoch),
            SuiError::from("SignedCheckpoint is from different epoch as committee")
        );

        let certified_checkpoint = CertifiedCheckpointSummary {
            summary: signed_checkpoints[0].summary.clone(),
            auth_signature: AuthorityWeakQuorumSignInfo::new_with_signatures(
                committee.epoch,
                signed_checkpoints
                    .into_iter()
                    .map(|v| (v.auth_signature.authority, v.auth_signature.signature))
                    .collect(),
                committee,
            )?,
        };

        certified_checkpoint.verify(committee, None)?;
        Ok(certified_checkpoint)
    }

    pub fn signatory_authorities<'a>(
        &'a self,
        committee: &'a Committee,
    ) -> impl Iterator<Item = SuiResult<&AuthorityName>> {
        self.auth_signature.authorities(committee)
    }

    /// Check that a certificate is valid, and signed by a quorum of authorities
    pub fn verify(
        &self,
        committee: &Committee,
        contents: Option<&CheckpointContents>,
    ) -> Result<(), SuiError> {
        fp_ensure!(
            self.summary.epoch == committee.epoch,
            SuiError::from("Epoch in the summary doesn't match with the committee")
        );
        let mut obligation = VerificationObligation::default();
        let mut message = Vec::new();
        self.summary.write(&mut message);
        let idx = obligation.add_message(message);
        self.auth_signature
            .add_to_verification_obligation(committee, &mut obligation, idx)?;
        obligation.verify_all()?;

        if let Some(contents) = contents {
            fp_ensure!(
                contents.digest() == self.summary.content_digest,
                SuiError::from("Checkpoint contents digest mismatch")
            );
        }

        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckpointContents {
    pub transactions: Vec<ExecutionDigests>,
}

impl BcsSignable for CheckpointContents {}

// TODO: We should create a type for ordered contents,
// instead of mixing them in the same type.
// https://github.com/MystenLabs/sui/issues/3038
impl CheckpointContents {
    pub fn new<T>(contents: T) -> CheckpointContents
    where
        T: Iterator<Item = ExecutionDigests>,
    {
        CheckpointContents {
            transactions: contents.collect::<BTreeSet<_>>().into_iter().collect(),
        }
    }

    pub fn iter(&self) -> Iter<'_, ExecutionDigests> {
        self.transactions.iter()
    }

    pub fn digest(&self) -> CheckpointContentsDigest {
        sha3_hash(self)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CheckpointProposalSummary {
    pub sequence_number: CheckpointSequenceNumber,
    pub waypoint: Box<Waypoint>, // Bigger structure, can live on heap.
    pub content_digest: CheckpointContentsDigest,
}

impl CheckpointProposalSummary {
    pub fn new(
        sequence_number: CheckpointSequenceNumber,
        transactions: &CheckpointContents,
    ) -> Self {
        let mut waypoint = Box::new(Waypoint::default());
        transactions.transactions.iter().for_each(|tx| {
            waypoint.insert(tx);
        });

        Self {
            sequence_number,
            waypoint,
            content_digest: transactions.digest(),
        }
    }

    pub fn digest(&self) -> [u8; 32] {
        sha3_hash(self)
    }
}

impl BcsSignable for CheckpointProposalSummary {}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SignedCheckpointProposalSummary {
    pub summary: CheckpointProposalSummary,
    pub auth_signature: AuthoritySignInfo,
}

impl SignedCheckpointProposalSummary {
    pub fn authority(&self) -> &AuthorityName {
        &self.auth_signature.authority
    }

    pub fn verify(
        &self,
        committee: &Committee,
        contents: Option<&CheckpointContents>,
    ) -> SuiResult {
        self.auth_signature.verify(&self.summary, committee)?;
        if let Some(contents) = contents {
            // Taking advantage of the constructor to check both content digest and waypoint.
            let recomputed = CheckpointProposalSummary::new(self.summary.sequence_number, contents);
            fp_ensure!(
                recomputed == self.summary,
                SuiError::from("Checkpoint proposal content doesn't match with the summary")
            );
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckpointProposal {
    /// Summary of the checkpoint proposal.
    pub signed_summary: SignedCheckpointProposalSummary,
    /// The transactions included in the proposal.
    /// TODO: only include a commitment by default.
    pub transactions: CheckpointContents,
}

impl CheckpointProposal {
    pub fn new_from_signed_proposal_summary(
        signed_summary: SignedCheckpointProposalSummary,
        transactions: CheckpointContents,
    ) -> Self {
        debug_assert!(signed_summary.summary.content_digest == transactions.digest());
        Self {
            signed_summary,
            transactions,
        }
    }

    /// Create a proposal for a checkpoint at a particular height
    /// This contains a signed proposal summary and the list of transactions
    /// in the proposal.
    pub fn new(
        epoch: EpochId,
        sequence_number: CheckpointSequenceNumber,
        authority: AuthorityName,
        signer: &dyn signature::Signer<AuthoritySignature>,
        transactions: CheckpointContents,
    ) -> Self {
        let proposal_summary = CheckpointProposalSummary::new(sequence_number, &transactions);
        let signature = AuthoritySignature::new(&proposal_summary, signer);
        Self {
            signed_summary: SignedCheckpointProposalSummary {
                summary: proposal_summary,
                auth_signature: AuthoritySignInfo {
                    epoch,
                    authority,
                    signature,
                },
            },
            transactions,
        }
    }

    /// Returns the sequence number of this proposal
    pub fn sequence_number(&self) -> &CheckpointSequenceNumber {
        &self.signed_summary.summary.sequence_number
    }

    // Iterate over all transaction/effects
    pub fn transactions(&self) -> impl Iterator<Item = &ExecutionDigests> {
        self.transactions.transactions.iter()
    }

    // Get the authority name
    pub fn name(&self) -> &AuthorityName {
        &self.signed_summary.auth_signature.authority
    }

    /// Construct a Diff structure between this proposal and another
    /// proposal. A diff structure has to contain keys. The diff represents
    /// the elements that each proposal need to be augmented by to
    /// contain the same elements.
    ///
    /// TODO: down the line we can include other methods to get diffs
    /// line MerkleTrees or IBLT filters that do not require O(n) download
    /// of both proposals.
    pub fn fragment_with(&self, other_proposal: &CheckpointProposal) -> CheckpointFragment {
        let all_elements = self
            .transactions()
            .chain(other_proposal.transactions())
            .collect::<HashSet<_>>();

        let my_transactions = self.transactions().collect();
        let iter_missing_me = all_elements.difference(&my_transactions).map(|x| **x);
        let other_transactions = other_proposal.transactions().collect();
        let iter_missing_other = all_elements.difference(&other_transactions).map(|x| **x);

        let diff = WaypointDiff::new(
            *self.name(),
            *self.signed_summary.summary.waypoint.clone(),
            iter_missing_me,
            *other_proposal.name(),
            *other_proposal.signed_summary.summary.waypoint.clone(),
            iter_missing_other,
        );

        CheckpointFragment {
            proposer: self.signed_summary.clone(),
            other: other_proposal.signed_summary.clone(),
            diff,
            certs: BTreeMap::new(),
        }
    }
}

// The construction of checkpoints is based on the aggregation of fragments.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckpointFragment {
    pub proposer: SignedCheckpointProposalSummary,
    pub other: SignedCheckpointProposalSummary,
    pub diff: WaypointDiff<AuthorityName, ExecutionDigests>,
    pub certs: BTreeMap<ExecutionDigests, CertifiedTransaction>,
}

impl CheckpointFragment {
    pub fn verify(&self, committee: &Committee) -> Result<(), SuiError> {
        // Check the signatures of proposer and other
        self.proposer.verify(committee, None)?;
        self.other.verify(committee, None)?;

        // Check consistency between checkpoint summary and waypoints.
        fp_ensure!(
            self.diff.first.waypoint == *self.proposer.summary.waypoint
                && self.diff.second.waypoint == *self.other.summary.waypoint
                && &self.diff.first.key == self.proposer.authority()
                && &self.diff.second.key == self.other.authority(),
            SuiError::from("Waypoint diff and checkpoint summary inconsistent")
        );

        // Check consistency of waypoint diff
        fp_ensure!(
            self.diff.check(),
            SuiError::from("Waypoint diff is not valid")
        );

        // TODO:
        // - check that the certs includes all missing certs on either side.

        Ok(())
    }

    pub fn proposer_sequence_number(&self) -> &CheckpointSequenceNumber {
        &self.proposer.summary.sequence_number
    }
}

#[cfg(test)]
mod tests {
    use rand::prelude::StdRng;
    use rand::SeedableRng;
    use std::collections::BTreeSet;

    use super::*;
    use crate::utils::make_committee_key;

    // TODO use the file name as a seed
    const RNG_SEED: [u8; 32] = [
        21, 23, 199, 200, 234, 250, 252, 178, 94, 15, 202, 178, 62, 186, 88, 137, 233, 192, 130,
        157, 179, 179, 65, 9, 31, 249, 221, 123, 225, 112, 199, 247,
    ];

    #[test]
    fn test_signed_proposal() {
        let mut rng = StdRng::from_seed(RNG_SEED);
        let (authority_key, committee) = make_committee_key(&mut rng);
        let name = authority_key[0].public_key_bytes();

        let set = [ExecutionDigests::random()];
        let set = CheckpointContents::new(set.iter().cloned());

        let mut proposal =
            SignedCheckpointSummary::new(committee.epoch, 1, *name, &authority_key[0], &set, None);

        // Signature is correct on proposal, and with same transactions
        assert!(proposal.verify(&committee, Some(&set)).is_ok());

        // Error on different transactions
        let contents = CheckpointContents {
            transactions: [ExecutionDigests::random()].into_iter().collect(),
        };
        assert!(proposal.verify(&committee, Some(&contents)).is_err());

        // Modify the proposal, and observe the signature fail
        proposal.summary.sequence_number = 2;
        assert!(proposal.verify(&committee, None).is_err());
    }

    #[test]
    fn test_signed_checkpoint() {
        let mut rng = StdRng::from_seed(RNG_SEED);
        let (keys, committee) = make_committee_key(&mut rng);
        let (_, committee2) = make_committee_key(&mut rng);

        let set = [ExecutionDigests::random()];
        let set = CheckpointContents::new(set.iter().cloned());

        let signed_checkpoints: Vec<_> = keys
            .iter()
            .map(|k| {
                let name = k.public_key_bytes();

                SignedCheckpointSummary::new(committee.epoch, 1, *name, k, &set, None)
            })
            .collect();

        signed_checkpoints
            .iter()
            .for_each(|c| c.verify(&committee, None).expect("signature ok"));

        // fails when not signed by member of committee
        signed_checkpoints
            .iter()
            .for_each(|c| assert!(c.verify(&committee2, None).is_err()));
    }

    #[test]
    fn test_certified_checkpoint() {
        let mut rng = StdRng::from_seed(RNG_SEED);
        let (keys, committee) = make_committee_key(&mut rng);

        let set = [ExecutionDigests::random()];
        let set = CheckpointContents::new(set.iter().cloned());

        let signed_checkpoints: Vec<_> = keys
            .iter()
            .map(|k| {
                let name = k.public_key_bytes();

                SignedCheckpointSummary::new(committee.epoch, 1, *name, k, &set, None)
            })
            .collect();

        let checkpoint_cert = CertifiedCheckpointSummary::aggregate(signed_checkpoints, &committee)
            .expect("Cert is OK");

        // Signature is correct on proposal, and with same transactions
        assert!(checkpoint_cert.verify(&committee, Some(&set)).is_ok());

        // Make a bad proposal
        let signed_checkpoints: Vec<_> = keys
            .iter()
            .map(|k| {
                let name = k.public_key_bytes();
                let set: BTreeSet<_> = [ExecutionDigests::random()].into_iter().collect();
                let set = CheckpointContents::new(set.iter().cloned());

                SignedCheckpointSummary::new(committee.epoch, 1, *name, k, &set, None)
            })
            .collect();

        assert!(CertifiedCheckpointSummary::aggregate(signed_checkpoints, &committee).is_err());
    }
}
