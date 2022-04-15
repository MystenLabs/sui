use std::collections::{BTreeSet, HashSet};

use crate::batch::SignedBatch;
use crate::crypto::Signable;
use crate::{
    base_types::{AuthorityName, TransactionDigest},
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
    may already have a very simlar set of digests.

    From proposals to checkpoints:
    -----------------------------

    A checkpoint is formed by a set of checkpoint proposals representing
    2/3 of the authorities by stake. The checkpoint contains the union of
    transactions in all the proposals. A checkpoint needs to provide enough
    evidence to ensure all authorities may recover the transactions
    included. Since all authorities need to agree on which checkpoint (out
    of the potentially many sets of 2/3 stake) constitutes the checkpoint
    we need an agreement protocol to detemrine this.

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
      hint about which trasnaction digests are pending processing to get
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


#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CheckpointRequestType {
    // Request the latest proposal and previous checkpoint.
    LatestCheckpointProposal,
    // Requests a past checkpoint
    PastCheckpoint(CheckpointSequenceNumber),

    // DEVNET: until we have a consensus core to collectivelly decide 
    // the checkpoint we allow a trusted client to just force a 
    // checkpoint. This is for early testing and removal at Testnet
    // time.
    DEBUGSetCheckpoint(AuthenticatedCheckpoint, CheckpointContents),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckpointResponse {
    // The response to the request, according to the type
    // and the information available.
    pub info: AuthorityCheckpointInfo,
    // If the detail flag in the request was set, then return
    // the list of transactions as well.
    pub detail: Option<CheckpointContents>,
    // Include in all responses the local state of the sequence
    // of trasacation to allow followers to track the latest 
    // updates.
    pub local_sequence_info: LocalSequenceInfo,
}


#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum AuthorityCheckpointInfo {
    // Returns the current proposal if any, and
    // the previous checkpoint.
    Proposal {
        current: Option<SignedCheckpointProposal>,
        previous: AuthenticatedCheckpoint,
    },
    // Returns the requested checkpoint.
    Past(AuthenticatedCheckpoint),
}


#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LocalSequenceInfo {
    pub last_local_batch: SignedBatch,
}

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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckpointProposalSummary(pub CheckpointSummary);
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SignedCheckpointProposal(pub SignedCheckpoint);

pub type CheckpointDigest = [u8; 32];


// The constituant parts of checkpoints, signed and certified

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct CheckpointSummary {
    sequence_number: CheckpointSequenceNumber,
    digest: CheckpointDigest,
}

impl BcsSignable for CheckpointSummary {}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SignedCheckpoint {
    checkpoint: CheckpointSummary,
    authority: AuthorityName,
    signature: AuthoritySignature,
}

impl SignedCheckpoint {
    /// Create a new signed checkpoint proposal for this authority
    pub fn new(
        sequence_number: CheckpointSequenceNumber,
        authority: AuthorityName,
        signer: &dyn signature::Signer<AuthoritySignature>,
        transactions: BTreeSet<TransactionDigest>,
    ) -> SignedCheckpoint {
        let contents = CheckpointContents { transactions };

        let proposal_digest = contents.digest();

        let proposal = CheckpointSummary {
            sequence_number,
            digest: proposal_digest,
        };

        let signature = AuthoritySignature::new(&proposal, signer);

        SignedCheckpoint {
            checkpoint: proposal,
            authority,
            signature,
        }
    }

    /// Checks that the signature on the digest is correct
    pub fn check_digest(&self) -> Result<(), SuiError> {
        self.signature.verify(&self.checkpoint, self.authority)?;
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
    use crate::utils::make_committee_key;
    use super::*;
    

    #[test]
    fn test_signed_proposal() {
        let (authority_key, _committee) = make_committee_key();
        let name = authority_key[0].public_key_bytes();

        let set: BTreeSet<_> = [TransactionDigest::random()].into_iter().collect();

        let mut proposal = SignedCheckpoint::new(
            1,
            *name,
            &authority_key[0],
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
        proposal.checkpoint.sequence_number = 2;
        assert!(proposal.check_digest().is_err());
    }

    #[test]
    fn test_certified_checkpoint() {
        let (keys, committee) = make_committee_key();

        let set: BTreeSet<_> = [TransactionDigest::random()].into_iter().collect();

        let signed_checkpoints: Vec<_> = keys
            .iter()
            .map(|k| {
                let name = k.public_key_bytes();

                SignedCheckpoint::new(1, *name, k, set.clone())
            })
            .collect();

        let checkpoint_cert =
            CertifiedCheckpoint::aggregate(signed_checkpoints, &committee).expect("Cert is OK");

        // Signature is correct on proposal, and with same transactions
        assert!(checkpoint_cert
            .check_transactions(&committee, &CheckpointContents { transactions: set })
            .is_ok());

        // Make a bad proposal
        let signed_checkpoints: Vec<_> = keys
            .iter()
            .map(|k| {
                let name = k.public_key_bytes();
                let set: BTreeSet<_> = [TransactionDigest::random()].into_iter().collect();

                SignedCheckpoint::new(1, *name, k, set)
            })
            .collect();

        assert!(CertifiedCheckpoint::aggregate(signed_checkpoints, &committee).is_err());
    }
}
