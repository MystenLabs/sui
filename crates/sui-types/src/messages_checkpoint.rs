// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fmt::{Display, Formatter};
use std::slice::Iter;

use crate::base_types::ExecutionDigests;
use crate::committee::EpochId;
use crate::crypto::{AuthoritySignInfo, AuthoritySignInfoTrait, AuthorityWeakQuorumSignInfo};
use crate::error::SuiResult;
use crate::messages::CertifiedTransaction;
use crate::waypoint::{Waypoint, WaypointDiff};
use crate::{
    base_types::AuthorityName,
    committee::Committee,
    crypto::{sha3_hash, AuthoritySignature, SuiAuthoritySignature, VerificationObligation},
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
    pub fn proposal(detail: bool) -> CheckpointRequest {
        CheckpointRequest {
            request_type: CheckpointRequestType::CheckpointProposal,
            detail,
        }
    }

    pub fn authenticated(seq: Option<CheckpointSequenceNumber>, detail: bool) -> CheckpointRequest {
        CheckpointRequest {
            request_type: CheckpointRequestType::AuthenticatedCheckpoint(seq),
            detail,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CheckpointRequestType {
    /// Request a stored authenticated checkpoint.
    /// if a sequence number is specified, return the checkpoint with that sequence number;
    /// otherwise if None returns the latest authenticated checkpoint stored.
    AuthenticatedCheckpoint(Option<CheckpointSequenceNumber>),
    /// Request the current checkpoint proposal.
    CheckpointProposal,
}

#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CheckpointResponse {
    AuthenticatedCheckpoint {
        checkpoint: Option<AuthenticatedCheckpoint>,
        contents: Option<CheckpointContents>,
    },
    /// The latest proposal must be signed by the validator.
    /// For any proposal with sequence number > 0, a certified checkpoint for the previous
    /// checkpoint must be returned, in order prove that this validator can indeed make a proposal
    /// for its sequence number.
    CheckpointProposal {
        proposal: Option<SignedCheckpointProposalSummary>,
        prev_cert: Option<CertifiedCheckpointSummary>,
        proposal_contents: Option<CheckpointProposalContents>,
    },
}

// TODO: Rename to AuthenticatedCheckpointSummary
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum AuthenticatedCheckpoint {
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
        }
    }

    pub fn verify(&self, committee: &Committee, detail: Option<&CheckpointContents>) -> SuiResult {
        match self {
            Self::Signed(s) => s.verify(committee, detail),
            Self::Certified(c) => c.verify(committee, detail),
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
        transactions.iter().for_each(|tx| {
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

impl Display for CheckpointSummary {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "CheckpointSummary {{ epoch: {:?}, seq: {:?}, content_digest: {} }}",
            self.epoch,
            self.sequence_number,
            hex::encode(&self.content_digest),
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckpointSummaryEnvelope<S> {
    pub summary: CheckpointSummary,
    pub auth_signature: S,
}

impl Display for SignedCheckpointSummary {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "SignedCheckpointSummary {{ summary: {}, signature: {} }}",
            self.summary, self.auth_signature,
        )
    }
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
            self.summary.epoch == committee.epoch,
            SuiError::from("Epoch in the summary doesn't match with the signature")
        );

        self.auth_signature.verify(&self.summary, committee)?;

        if let Some(contents) = contents {
            let content_digest = contents.digest();
            fp_ensure!(
                content_digest == self.summary.content_digest,
                SuiError::GenericAuthorityError{error:format!("Checkpoint contents digest mismatch: summary={:?}, received content digest {:?}, received {} transactions", self.summary, content_digest, contents.size())}
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

impl Display for CertifiedCheckpointSummary {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "CertifiedCheckpointSummary {{ summary: {}, signature: {} }}",
            self.summary, self.auth_signature,
        )
    }
}

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
        let idx = obligation.add_message(&self.summary);
        self.auth_signature
            .add_to_verification_obligation(committee, &mut obligation, idx)?;

        obligation.verify_all()?;

        if let Some(contents) = contents {
            let content_digest = contents.digest();
            fp_ensure!(
                content_digest == self.summary.content_digest,
                SuiError::GenericAuthorityError{error:format!("Checkpoint contents digest mismatch: summary={:?}, content digest = {:?}, transactions {}", self.summary, content_digest, contents.size())}
            );
        }

        Ok(())
    }
}

/// CheckpointProposalContents represents the contents of a proposal.
/// Contents in a proposal are not yet causally ordered, and hence we don't care about
/// the order of transactions in the content. It's only important that two proposal
/// contents with the same transactions should have the same digest. Hence we use BTreeSet
/// as the container. This also has the benefit of removing any duplicate transactions.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckpointProposalContents {
    // TODO: Currently we are not really using the effects digests, but in the future we may be
    // able to use it to optimize the sync process.
    pub transactions: BTreeSet<ExecutionDigests>,
}

impl CheckpointProposalContents {
    pub fn new<T>(contents: T) -> Self
    where
        T: Iterator<Item = ExecutionDigests>,
    {
        Self {
            transactions: contents.collect(),
        }
    }

    pub fn digest(&self) -> CheckpointContentsDigest {
        sha3_hash(self)
    }
}

/// CheckpointContents are the transactions included in an upcoming checkpoint.
/// They must have already been causally ordered. Since the causal order algorithm
/// is the same among validators, we expect all honest validators to come up with
/// the same order for each checkpoint content.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckpointContents {
    transactions: Vec<ExecutionDigests>,
}

impl CheckpointContents {
    pub fn new_with_causally_ordered_transactions<T>(contents: T) -> Self
    where
        T: Iterator<Item = ExecutionDigests>,
    {
        Self {
            transactions: contents.collect(),
        }
    }

    pub fn iter(&self) -> Iter<'_, ExecutionDigests> {
        self.transactions.iter()
    }

    pub fn size(&self) -> usize {
        self.transactions.len()
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
        transactions: &CheckpointProposalContents,
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
        contents: Option<&CheckpointProposalContents>,
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
    pub transactions: CheckpointProposalContents,
}

impl CheckpointProposal {
    pub fn new_from_signed_proposal_summary(
        signed_summary: SignedCheckpointProposalSummary,
        transactions: CheckpointProposalContents,
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
        transactions: CheckpointProposalContents,
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
    pub fn verify(&self, committee: &Committee) -> SuiResult {
        fp_ensure!(
            self.proposer.summary.sequence_number == self.other.summary.sequence_number,
            SuiError::from("Proposer and other have inconsistent sequence number")
        );
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

        // Check that the fragment contains all missing certs indicated in diff.
        let digests = self
            .diff
            .first
            .items
            .iter()
            .chain(self.diff.second.items.iter());
        for digest in digests {
            let cert = self.certs.get(digest).ok_or_else(|| {
                SuiError::from(format!("Missing cert with digest {digest:?}").as_str())
            })?;
            cert.verify(committee)?;
        }

        Ok(())
    }

    pub fn proposer_sequence_number(&self) -> &CheckpointSequenceNumber {
        &self.proposer.summary.sequence_number
    }
}

#[cfg(test)]
mod tests {
    use fastcrypto::traits::KeyPair;
    use rand::prelude::StdRng;
    use rand::SeedableRng;

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
        let name: AuthorityName = authority_key[0].public().into();

        let set = CheckpointProposalContents::new([ExecutionDigests::random()].into_iter());

        let mut proposal =
            CheckpointProposal::new(committee.epoch, 1, name, &authority_key[0], set.clone());

        // Signature is correct on proposal, and with same transactions
        assert!(proposal
            .signed_summary
            .verify(&committee, Some(&set))
            .is_ok());

        // Error on different transactions
        let contents = CheckpointProposalContents::new([ExecutionDigests::random()].into_iter());
        assert!(proposal
            .signed_summary
            .verify(&committee, Some(&contents))
            .is_err());

        // Modify the proposal, and observe the signature fail
        proposal.signed_summary.summary.sequence_number = 2;
        assert!(proposal.signed_summary.verify(&committee, None).is_err());
    }

    #[test]
    fn test_signed_checkpoint() {
        let mut rng = StdRng::from_seed(RNG_SEED);
        let (keys, committee) = make_committee_key(&mut rng);
        let (_, committee2) = make_committee_key(&mut rng);

        let set = CheckpointContents::new_with_causally_ordered_transactions(
            [ExecutionDigests::random()].into_iter(),
        );

        let signed_checkpoints: Vec<_> = keys
            .iter()
            .map(|k| {
                let name = k.public().into();

                SignedCheckpointSummary::new(committee.epoch, 1, name, k, &set, None)
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

        let set = CheckpointContents::new_with_causally_ordered_transactions(
            [ExecutionDigests::random()].into_iter(),
        );

        let signed_checkpoints: Vec<_> = keys
            .iter()
            .map(|k| {
                let name = k.public().into();

                SignedCheckpointSummary::new(committee.epoch, 1, name, k, &set, None)
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
                let name = k.public().into();
                let set = CheckpointContents::new_with_causally_ordered_transactions(
                    [ExecutionDigests::random()].into_iter(),
                );

                SignedCheckpointSummary::new(committee.epoch, 1, name, k, &set, None)
            })
            .collect();

        assert!(CertifiedCheckpointSummary::aggregate(signed_checkpoints, &committee).is_err());
    }

    #[test]
    fn test_fragment() {
        let mut rng = StdRng::from_seed(RNG_SEED);
        let (authority_key, committee) = make_committee_key(&mut rng);
        let name1: AuthorityName = authority_key[0].public().into();
        let name2: AuthorityName = authority_key[1].public().into();

        let set = CheckpointProposalContents::new([ExecutionDigests::random()].into_iter());

        let proposal1 =
            CheckpointProposal::new(committee.epoch, 1, name1, &authority_key[0], set.clone());
        let proposal2 =
            CheckpointProposal::new(committee.epoch, 1, name2, &authority_key[1], set.clone());
        let fragment1 = proposal1.fragment_with(&proposal2);
        assert!(fragment1.verify(&committee).is_ok());

        let proposal3 = CheckpointProposal::new(committee.epoch, 2, name2, &authority_key[1], set);
        let fragment2 = proposal1.fragment_with(&proposal3);
        assert!(fragment2.verify(&committee).is_err());
    }
}
