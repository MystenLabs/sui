use std::collections::{BTreeSet, HashSet};

use crate::crypto::Signable;
use crate::{
    base_types::{AuthorityName, SequenceNumber, TransactionDigest},
    committee::Committee,
    crypto::{sha3_hash, AuthoritySignature, BcsSignable, VerificationObligation},
    error::SuiError,
};
use serde::{Deserialize, Serialize};

/*

    The checkpoint messages, structures and protocol: A gentle overview
    -------------------------------------------------------------------

    Authorities operate and process certified transactions. When they have
    processed all transactions included in a previous checkpoint (we will
    see how this is set) each authority proposes a signed proposed
    checkpoint (SignedCheckpointProposal) for the next sequence number.

    A proposal is built on the basis of a set of trasnactions that the
    authority has processed and wants to include in the next checkpoint.
    Right now we just list these as transaction digests but down the line
    we will rely on more efficient ways to determine the set for parties that
    may already have a very simlar set of digests.

    A checkpoint is formed by a set of checkpoint proposals representing
    2/3 of the authorities by stake. The checkpoint contains the union of
    transactions in all the proposals. A checkpoint needs to provide enough
    evidence to ensure all authorities may recover the transactions
    included. Since all authorities need to agree on which checkpoint (out
    of the potentially many sets of 2/3 stake) constitutes the checkpoint
    we need an agreement protocol to detemrine this.

    Once a checkpoint is determined each authority forms a CheckpointSummary
    with all the trasnactions in the checkpoint, and signs it with its
    authority key to form a SignedCheckpoint. A collection of 2/3 authority
    signatures on a checkpoint forms a CertifiedCheckpoint. And this is the
    structure that is kept in the long term to attest of the sequence of
    checkpoints. Once a CertifiedCheckpoint is recoded for a checkpoint
    all other information leading to the checkpoint may be deleted.

    To facilitate the protocol authorities always provide facilities for
    reads:
    - To get past checkpoints signatures, certificates and the transactions
      associated with them.
    - To get the current signed proposal. Or if there is no proposal a
      hint about which trasnaction digests are pending processing to get
      a proposal.

*/

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CheckpointRequest {
    LatestCheckpointProposal,
    PastCheckpoint(SequenceNumber),
    DebugSubmitCheckpoint(SequenceNumber, Vec<TransactionDigest>),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CheckpointResponse {
    Proposal {
        proposal_sequence_number: SequenceNumber,
        transactions: Vec<TransactionDigest>,
    },
    NoProposal {
        last_sequence_number: SequenceNumber,
        missing: Vec<TransactionDigest>,
    },
    Past {
        past_sequence_number: SequenceNumber,
        transactions: Vec<TransactionDigest>,
    },
}

// Proposals are signed by a single authority, and 2f+1 are collected
// to actually form a checkpoint, so we never expect a certificate on
// a proposal.

pub type CheckpointDigest = [u8; 32];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CheckpointProposalSummary {
    sequence_number: SequenceNumber,
    digest: CheckpointDigest,
}

impl BcsSignable for CheckpointProposalSummary {}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SignedCheckpointProposal {
    checkpoint: CheckpointProposalSummary,
    authority: AuthorityName,
    signature: AuthoritySignature,
}

impl SignedCheckpointProposal {
    /// Create a new signed checkpoint proposal for this authority
    pub fn new(
        sequence_number: SequenceNumber,
        authority: AuthorityName,
        signer: &dyn signature::Signer<AuthoritySignature>,
        transactions: BTreeSet<TransactionDigest>,
    ) -> SignedCheckpointProposal {
        let contents = CheckpointContents { transactions };

        let proposal_digest = contents.digest();

        let proposal = CheckpointProposalSummary {
            sequence_number,
            digest: proposal_digest,
        };

        let signature = AuthoritySignature::new(&proposal, signer);

        SignedCheckpointProposal {
            checkpoint: proposal,
            authority,
            signature,
        }
    }

    /// Checks that the signature on the digest is correct
    pub fn check_digest(&self) -> Result<(), SuiError> {
        self.signature.check(&self.checkpoint, self.authority)?;
        Ok(())
    }

    // Check that the digest and transactions are correctly signed
    pub fn check_transactions(&self, contents: &CheckpointContents) -> Result<(), SuiError> {
        self.check_digest()?;
        fp_ensure!(
            contents.digest() == self.checkpoint.digest,
            SuiError::GenericAuthorityError {
                error: "Transaction digest mismatch".to_string()
            }
        );
        Ok(())
    }
}

// Checkpoints are signed by an authority and 2f+1 form a
// certificate that others can use to catch up. The actual
// content of the digest must at the very least commit to
// the set of transactions contained in the certificate but
// we might extend this to contain roots of merkle trees,
// or other authenticated data strucures to support light
// clients and more efficient sync protocols.

pub type CheckpointSummary = CheckpointProposalSummary;
pub type SignedCheckpoint = SignedCheckpointProposal;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CertifiedCheckpoint {
    checkpoint: CheckpointSummary,
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
            SuiError::GenericAuthorityError {
                error: "Need at least one signed checkpoint to aggregate".to_string()
            }
        );

        let certified_checkpoint = CertifiedCheckpoint {
            checkpoint: signed_checkpoints[0].checkpoint.clone(),
            signatures: signed_checkpoints
                .into_iter()
                .map(|v| (v.authority, v.signature))
                .collect(),
        };

        certified_checkpoint.check_digest(committee)?;
        Ok(certified_checkpoint)
    }

    /// Check that a certificate is valid, and signed by a quorum of authorities
    pub fn check_digest(&self, committee: &Committee) -> Result<(), SuiError> {
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
            weight >= committee.quorum_threshold(),
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
    pub fn check_transactions(
        &self,
        committee: &Committee,
        contents: &CheckpointContents,
    ) -> Result<(), SuiError> {
        self.check_digest(committee)?;
        fp_ensure!(
            contents.digest() == self.checkpoint.digest,
            SuiError::GenericAuthorityError {
                error: "Transaction digest mismatch".to_string()
            }
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckpointContents {
    transactions: BTreeSet<TransactionDigest>,
}

impl BcsSignable for CheckpointContents {}

impl CheckpointContents {
    pub fn digest(&self) -> [u8; 32] {
        sha3_hash(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::{get_key_pair, KeyPair};
    use std::collections::BTreeMap;

    fn make_committee_key() -> (KeyPair, Committee) {
        let (_, authority_key) = get_key_pair();
        let mut authorities = BTreeMap::new();
        authorities.insert(
            /* address */ *authority_key.public_key_bytes(),
            /* voting right */ 1,
        );

        for _ in 0..3 {
            let (_, inner_authority_key) = get_key_pair();
            authorities.insert(
                /* address */ *inner_authority_key.public_key_bytes(),
                /* voting right */ 1,
            );
        }

        let committee = Committee::new(authorities);
        (authority_key, committee)
    }

    #[test]
    fn test_signed_proposal() {
        let (authority_key, _committee) = make_committee_key();
        let name = authority_key.public_key_bytes();

        let set: BTreeSet<_> = [TransactionDigest::random()].into_iter().collect();

        let mut proposal = SignedCheckpointProposal::new(
            SequenceNumber::from(1),
            *name,
            &authority_key,
            set.clone(),
        );

        // Signature is correct on proposal, and with same transactions
        assert!(proposal.check_digest().is_ok());

        let contents = CheckpointContents { transactions: set };
        assert!(proposal.check_transactions(&contents).is_ok());

        // Error on different transactions
        let contents = CheckpointContents {
            transactions: [TransactionDigest::random()].into_iter().collect(),
        };
        assert!(proposal.check_transactions(&contents).is_err());

        // Modify the proposal, and observe the signature fail
        proposal.checkpoint.sequence_number = SequenceNumber::from(2);
        assert!(proposal.check_digest().is_err());
    }
}
