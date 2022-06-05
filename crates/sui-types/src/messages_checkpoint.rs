// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::base_types::ExecutionDigests;
use crate::crypto::Signable;
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

    pub fn set_checkpoint(
        certificate: CertifiedCheckpoint,
        contents: Option<CheckpointContents>,
    ) -> CheckpointRequest {
        CheckpointRequest {
            request_type: CheckpointRequestType::SetCertificate(certificate, contents),
            detail: false,
        }
    }

    pub fn set_fragment(fragment: CheckpointFragment) -> CheckpointRequest {
        CheckpointRequest {
            request_type: CheckpointRequestType::SetFragment(Box::new(fragment)),
            detail: false,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CheckpointRequestType {
    // Request the latest proposal and previous checkpoint.
    LatestCheckpointProposal,
    // Requests a past checkpoint
    PastCheckpoint(CheckpointSequenceNumber),
    // Set a checkpoint certificate
    SetCertificate(CertifiedCheckpoint, Option<CheckpointContents>),
    // Submit a consensus fragment to a node
    SetFragment(Box<CheckpointFragment>),
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum AuthorityCheckpointInfo {
    // Denotes success of he operation with no return
    Success,
    // Returns the current proposal if any, and
    // the previous checkpoint.
    Proposal {
        current: Option<SignedCheckpointProposal>,
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
    Signed(SignedCheckpoint),
    // The checkpoint with a quorum of signatures.
    Certified(CertifiedCheckpoint),
}

// Proposals are signed by a single authority, and 2f+1 are collected
// to actually form a checkpoint, so we never expect a certificate on
// a proposal.
// TODO: SignedCheckpointProposal is redundant of SignedCheckpoint, should merge.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SignedCheckpointProposal(pub SignedCheckpoint);

pub type CheckpointDigest = [u8; 32];

// The constituent parts of checkpoints, signed and certified

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CheckpointSummary {
    pub sequence_number: CheckpointSequenceNumber,
    pub waypoint: Box<Waypoint>, // Bigger structure, can live on heap.
    pub digest: CheckpointDigest,
    // TODO: add digest of previous checkpoint summary
}

impl CheckpointSummary {
    pub fn new(
        sequence_number: CheckpointSequenceNumber,
        transactions: &CheckpointContents,
    ) -> CheckpointSummary {
        let mut waypoint = Box::new(Waypoint::default());
        transactions.transactions.iter().for_each(|tx| {
            waypoint.insert(tx);
        });

        let proposal_digest = transactions.digest();

        Self {
            sequence_number,
            waypoint,
            digest: proposal_digest,
        }
    }

    pub fn sequence_number(&self) -> &CheckpointSequenceNumber {
        &self.sequence_number
    }

    pub fn digest(&self) -> [u8; 32] {
        sha3_hash(self)
    }
}

impl BcsSignable for CheckpointSummary {}

// TODO: Rename SignedCheckpoint to SignedCheckpointSummary
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SignedCheckpoint {
    pub checkpoint: CheckpointSummary,
    pub authority: AuthorityName,
    signature: AuthoritySignature,
}

impl SignedCheckpoint {
    /// Create a new signed checkpoint proposal for this authority
    pub fn new(
        sequence_number: CheckpointSequenceNumber,
        authority: AuthorityName,
        signer: &dyn signature::Signer<AuthoritySignature>,
        transactions: &CheckpointContents,
    ) -> SignedCheckpoint {
        let checkpoint = CheckpointSummary::new(sequence_number, transactions);
        SignedCheckpoint::new_from_summary(checkpoint, authority, signer)
    }

    pub fn new_from_summary(
        checkpoint: CheckpointSummary,
        authority: AuthorityName,
        signer: &dyn signature::Signer<AuthoritySignature>,
    ) -> SignedCheckpoint {
        let signature = AuthoritySignature::new(&checkpoint, signer);

        SignedCheckpoint {
            checkpoint,
            authority,
            signature,
        }
    }

    /// Checks that the signature on the digest is correct
    pub fn verify(&self) -> Result<(), SuiError> {
        self.signature.verify(&self.checkpoint, self.authority)?;
        Ok(())
    }

    // Check that the digest and transactions are correctly signed
    pub fn verify_with_transactions(&self, contents: &CheckpointContents) -> Result<(), SuiError> {
        self.verify()?;
        let recomputed = CheckpointSummary::new(*self.checkpoint.sequence_number(), contents);

        fp_ensure!(
            recomputed == self.checkpoint,
            SuiError::from("Transaction digest mismatch")
        );
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CertifiedCheckpoint {
    pub checkpoint: CheckpointSummary,
    signatures: Vec<(AuthorityName, AuthoritySignature)>,
}

impl CertifiedCheckpoint {
    /// Aggregate many checkpoint signatures to form a checkpoint certificate.
    pub fn aggregate(
        signed_checkpoints: Vec<SignedCheckpoint>,
        committee: &Committee,
    ) -> Result<CertifiedCheckpoint, SuiError> {
        fp_ensure!(
            !signed_checkpoints.is_empty(),
            SuiError::from("Need at least one signed checkpoint to aggregate")
        );

        let certified_checkpoint = CertifiedCheckpoint {
            checkpoint: signed_checkpoints[0].checkpoint.clone(),
            signatures: signed_checkpoints
                .into_iter()
                .map(|v| (v.authority, v.signature))
                .collect(),
        };

        certified_checkpoint.verify(committee)?;
        Ok(certified_checkpoint)
    }

    pub fn signatory_authorities(&self) -> impl Iterator<Item = &AuthorityName> {
        self.signatures.iter().map(|(name, _)| name)
    }

    /// Check that a certificate is valid, and signed by a quorum of authorities
    pub fn verify(&self, committee: &Committee) -> Result<(), SuiError> {
        // Note: this code is nearly the same as the code that checks
        // transaction certificates. There is an opportunity to unify this
        // logic.

        let mut weight = 0;
        let mut used_authorities = HashSet::new();
        for (authority, _) in self.signatures.iter() {
            // Check that each authority only appears once.
            fp_ensure!(
                !used_authorities.contains(authority),
                SuiError::CertificateAuthorityReuse
            );
            used_authorities.insert(*authority);
            // Update weight.
            let voting_rights = committee.weight(authority);
            fp_ensure!(voting_rights > 0, SuiError::UnknownSigner);
            weight += voting_rights;
        }
        fp_ensure!(
            // NOTE: here we only require f+1 weight to accept it, since
            //       we only need to ensure one honest node signs it, and
            //       do not require quorum intersection properties between
            //       any two sets of signers. Further f+1 is the most honest
            //       nodes we can be sure is in the set of 2f+1 that were
            //       used to create the checkpoint from fragments.
            weight >= committee.validity_threshold(),
            SuiError::CertificateRequiresQuorum
        );

        let mut obligation = VerificationObligation::default();

        // We verify the same message, so that ensures all signatures are
        // one a single and same message.
        let mut message = Vec::new();
        self.checkpoint.write(&mut message);

        let idx = obligation.messages.len();
        obligation.messages.push(message);

        for tuple in self.signatures.iter() {
            let (authority, signature) = tuple;
            // do we know, or can we build a valid public key?
            match committee.expanded_keys.get(authority) {
                Some(v) => obligation.public_keys.push(*v),
                None => {
                    let public_key = (*authority).try_into()?;
                    obligation.public_keys.push(public_key);
                }
            }

            // build a signature
            obligation.signatures.push(signature.0);

            // collect the message
            obligation.message_index.push(idx);
        }

        obligation.verify_all().map(|_| ())?;
        Ok(())
    }

    /// Check the certificate and whether it matches with a set of transactions.
    pub fn verify_with_transactions(
        &self,
        committee: &Committee,
        contents: &CheckpointContents,
    ) -> Result<(), SuiError> {
        self.verify(committee)?;
        fp_ensure!(
            contents.digest() == self.checkpoint.digest,
            SuiError::from("Transaction digest mismatch")
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckpointContents {
    pub transactions: BTreeSet<ExecutionDigests>,
}

impl BcsSignable for CheckpointContents {}

impl CheckpointContents {
    pub fn new<T>(contents: T) -> CheckpointContents
    where
        T: Iterator<Item = ExecutionDigests>,
    {
        CheckpointContents {
            transactions: contents.collect(),
        }
    }

    pub fn digest(&self) -> [u8; 32] {
        sha3_hash(self)
    }
}

// The construction of checkpoints is based on the aggregation of fragments.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckpointFragment {
    pub proposer: SignedCheckpointProposal,
    pub other: SignedCheckpointProposal,
    pub diff: WaypointDiff<AuthorityName, ExecutionDigests>,
    pub certs: BTreeMap<ExecutionDigests, CertifiedTransaction>,
}

impl CheckpointFragment {
    pub fn verify(&self, _committee: &Committee) -> Result<(), SuiError> {
        // Check the signatures of proposer and other
        self.proposer.0.verify()?;
        self.other.0.verify()?;

        // Check the proposers are authorities
        fp_ensure!(
            _committee.weight(&self.proposer.0.authority) > 0
                && _committee.weight(&self.other.0.authority) > 0,
            SuiError::from("Authorities not in the committee")
        );

        // Check consistency between checkpoint summary and waypoints.
        fp_ensure!(
            self.diff.first.waypoint == *self.proposer.0.checkpoint.waypoint
                && self.diff.second.waypoint == *self.other.0.checkpoint.waypoint
                && self.diff.first.key == self.proposer.0.authority
                && self.diff.second.key == self.other.0.authority,
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
        self.proposer.0.checkpoint.sequence_number()
    }
}

#[cfg(test)]
mod tests {
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
        let (authority_key, _committee) = make_committee_key(&mut rng);
        let name = authority_key[0].public_key_bytes();

        let set = [ExecutionDigests::random()];
        let set = CheckpointContents::new(set.iter().cloned());

        let mut proposal = SignedCheckpoint::new(1, *name, &authority_key[0], &set);

        // Signature is correct on proposal, and with same transactions
        assert!(proposal.verify().is_ok());
        assert!(proposal.verify_with_transactions(&set).is_ok());

        // Error on different transactions
        let contents = CheckpointContents {
            transactions: [ExecutionDigests::random()].into_iter().collect(),
        };
        assert!(proposal.verify_with_transactions(&contents).is_err());

        // Modify the proposal, and observe the signature fail
        proposal.checkpoint.sequence_number = 2;
        assert!(proposal.verify().is_err());
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

                SignedCheckpoint::new(1, *name, k, &set)
            })
            .collect();

        let checkpoint_cert =
            CertifiedCheckpoint::aggregate(signed_checkpoints, &committee).expect("Cert is OK");

        // Signature is correct on proposal, and with same transactions
        assert!(checkpoint_cert
            .verify_with_transactions(&committee, &set)
            .is_ok());

        // Make a bad proposal
        let signed_checkpoints: Vec<_> = keys
            .iter()
            .map(|k| {
                let name = k.public_key_bytes();
                let set: BTreeSet<_> = [ExecutionDigests::random()].into_iter().collect();
                let set = CheckpointContents::new(set.iter().cloned());

                SignedCheckpoint::new(1, *name, k, &set)
            })
            .collect();

        assert!(CertifiedCheckpoint::aggregate(signed_checkpoints, &committee).is_err());
    }
}
