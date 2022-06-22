// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod proposal;
pub mod reconstruction;

#[cfg(test)]
#[path = "./tests/checkpoint_tests.rs"]
pub(crate) mod checkpoint_tests;

use narwhal_executor::ExecutionIndices;
use rocksdb::Options;
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, path::Path, sync::Arc};
use sui_storage::default_db_options;
use sui_types::{
    base_types::{AuthorityName, ExecutionDigests},
    batch::TxSequenceNumber,
    committee::{Committee, EpochId},
    error::{SuiError, SuiResult},
    fp_ensure,
    // messages::CertifiedTransaction,
    messages_checkpoint::{
        AuthenticatedCheckpoint, AuthorityCheckpointInfo, CertifiedCheckpointSummary,
        CheckpointContents, CheckpointDigest, CheckpointFragment, CheckpointRequest,
        CheckpointResponse, CheckpointSequenceNumber, CheckpointSummary, SignedCheckpointSummary,
    },
};
use typed_store::{
    reopen,
    rocks::{open_cf_opts, DBBatch, DBMap},
    Map,
};

use crate::{
    authority::StableSyncAuthoritySigner,
    authority_active::execution_driver::PendCertificateForExecution,
};

use self::proposal::CheckpointProposal;
use self::reconstruction::FragmentReconstruction;

pub type DBLabel = usize;
const LOCALS: DBLabel = 0;

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct CheckpointLocals {
    // The next checkpoint number expected.
    pub next_checkpoint: CheckpointSequenceNumber,

    // The next transaction after what is included in the proposal
    pub proposal_next_transaction: Option<TxSequenceNumber>,

    // The next transaction sequence number of transactions processed
    pub next_transaction_sequence: TxSequenceNumber,

    // True if no more fragments are to be added.
    pub no_more_fragments: bool,

    // The current checkpoint proposal if any
    #[serde(skip)]
    pub current_proposal: Option<CheckpointProposal>,
}

/// A simple interface for sending a transaction to consensus for
/// sequencing. The trait is useful to test this component away
/// from real consensus.
pub trait ConsensusSender: Send + Sync + 'static {
    // Send an item to the consensus
    fn send_to_consensus(&self, fragment: CheckpointFragment) -> Result<(), SuiError>;
}

#[derive(Debug)]
pub enum FragmentInternalError {
    Error(SuiError),
    Retry(Box<CheckpointFragment>),
}

pub struct CheckpointStore {
    // Fixed size, static, identity of the authority
    /// The name of this authority.
    pub name: AuthorityName,
    /// The signature key of the authority.
    pub secret: StableSyncAuthoritySigner,

    /// The list of all transaction/effects that are checkpointed mapping to the checkpoint
    /// sequence number they were assigned to.
    pub transactions_to_checkpoint:
        DBMap<ExecutionDigests, (CheckpointSequenceNumber, TxSequenceNumber)>,

    /// The mapping from checkpoint to transaction/effects contained within the checkpoint.
    /// The second part of the key is the local sequence number if the transaction was
    /// processed or Max(u64) / 2 + offset if not. It allows the authority to store and serve
    /// checkpoints in a causal order that can be processed in order. (Note the set
    /// of transactions in the checkpoint is global but not the order.)
    pub checkpoint_contents: DBMap<(CheckpointSequenceNumber, TxSequenceNumber), ExecutionDigests>,

    /// The set of transaction/effects this authority has processed but have not yet been
    /// included in a checkpoint, and their sequence number in the local sequence
    /// of this authority.
    pub extra_transactions: DBMap<ExecutionDigests, TxSequenceNumber>,

    /// The list of checkpoint, along with their authentication information
    pub checkpoints: DBMap<CheckpointSequenceNumber, AuthenticatedCheckpoint>,

    // --- Logic related to fragments on the way to making checkpoints

    // A list of own fragments indexed by the other node that the fragment connects
    // to. These are used for the local node to potentially reconstruct the full
    // transaction set.
    pub local_fragments: DBMap<AuthorityName, CheckpointFragment>,

    /// Store the fragments received in order, the counter is purely internal,
    /// to allow us to provide a list in order they were received. We only store
    /// the fragments that are relevant to the next checkpoints. Past checkpoints
    /// already contain all relevant information from previous checkpoints.
    pub fragments: DBMap<ExecutionIndices, CheckpointFragment>,

    /// The local sequence at which the proposal for the next checkpoint is created
    /// This is a sequence number containing all unprocessed transactions lower than
    /// this sequence number. At this point the unprocessed_transactions sequence
    /// should be empty. It is none if there is no active proposal. We also include here
    /// the proposal, although we could re-create it from the database.
    memory_locals: Option<Arc<CheckpointLocals>>,

    /// A single entry table to store locals.
    pub locals: DBMap<DBLabel, CheckpointLocals>,

    // Consensus sender
    sender: Option<Box<dyn ConsensusSender>>,
}

impl CheckpointStore {
    fn get_prev_checkpoint_digest(
        &mut self,
        checkpoint_sequence: CheckpointSequenceNumber,
    ) -> Result<Option<CheckpointDigest>, SuiError> {
        // Extract the previous checkpoint digest if there is one.
        Ok(if checkpoint_sequence > 0 {
            self.checkpoints
                .get(&(checkpoint_sequence - 1))?
                .map(|prev_checkpoint| match prev_checkpoint {
                    AuthenticatedCheckpoint::Certified(cert) => cert.summary.digest(),
                    AuthenticatedCheckpoint::Signed(signed) => signed.summary.digest(),
                    _ => {
                        unreachable!();
                    }
                })
        } else {
            None
        })
    }

    // Manage persistent local variables

    /// Loads the locals from the store -- do this at init
    fn load_locals(&mut self, epoch: EpochId) -> Result<CheckpointLocals, SuiError> {
        // Loads locals from disk, or inserts initial locals
        let mut locals = match self.locals.get(&LOCALS)? {
            Some(locals) => locals,
            None => {
                let locals = CheckpointLocals::default();
                self.locals.insert(&LOCALS, &locals)?;
                locals
            }
        };

        // Recreate the proposal
        if locals.proposal_next_transaction.is_some() {
            let checkpoint_sequence = locals.next_checkpoint;
            let transactions = self
                .extra_transactions
                .iter()
                .filter(|(_, seq)| seq < locals.proposal_next_transaction.as_ref().unwrap())
                .map(|(digest, _)| digest);
            let transactions = CheckpointContents::new(transactions);
            let previous_digest = self.get_prev_checkpoint_digest(checkpoint_sequence)?;
            let summary = SignedCheckpointSummary::new(
                epoch,
                checkpoint_sequence,
                self.name,
                &*self.secret,
                &transactions,
                previous_digest,
            );

            let proposal_and_transactions = CheckpointProposal::new(summary, transactions);
            locals.current_proposal = Some(proposal_and_transactions);
        }

        // No need to sync exclusive access
        self.memory_locals = Some(Arc::new(locals.clone()));
        Ok(locals)
    }

    /// Set the local variables in memory and store
    fn set_locals(
        &mut self,
        _previous: Arc<CheckpointLocals>,
        locals: CheckpointLocals,
    ) -> Result<(), SuiError> {
        self.locals.insert(&LOCALS, &locals)?;
        self.memory_locals = Some(Arc::new(locals));
        Ok(())
    }

    #[cfg(test)]
    pub fn set_locals_for_testing(&mut self, locals: CheckpointLocals) -> Result<(), SuiError> {
        self.set_locals(Arc::new(locals.clone()), locals)
    }

    /// Read the local variables
    pub fn get_locals(&mut self) -> Arc<CheckpointLocals> {
        self.memory_locals.clone().unwrap()
    }

    /// Set the consensus sender for this checkpointing function
    pub fn set_consensus(&mut self, sender: Box<dyn ConsensusSender>) -> Result<(), SuiError> {
        self.sender = Some(sender);
        Ok(())
    }

    /// Open a checkpoint store to use to generate checkpoints, incl the information
    /// needed to sign new checkpoints.
    pub fn open<P: AsRef<Path>>(
        path: P,
        db_options: Option<Options>,
        current_epoch: EpochId,
        name: AuthorityName,
        secret: StableSyncAuthoritySigner,
    ) -> Result<CheckpointStore, SuiError> {
        let (options, point_lookup) = default_db_options(db_options, None);

        let db = open_cf_opts(
            &path,
            Some(options.clone()),
            &[
                ("transactions_to_checkpoint", &point_lookup),
                ("checkpoint_contents", &options),
                ("extra_transactions", &point_lookup),
                ("checkpoints", &point_lookup),
                ("local_fragments", &point_lookup),
                ("fragments", &options),
                ("locals", &point_lookup),
            ],
        )
        .expect("Cannot open DB.");

        let (
            transactions_to_checkpoint,
            checkpoint_contents,
            extra_transactions,
            checkpoints,
            local_fragments,
            fragments,
            locals,
        ) = reopen! (
            &db,
            "transactions_to_checkpoint";<ExecutionDigests,(CheckpointSequenceNumber, TxSequenceNumber)>,
            "checkpoint_contents";<(CheckpointSequenceNumber,TxSequenceNumber),ExecutionDigests>,
            "extra_transactions";<ExecutionDigests,TxSequenceNumber>,
            "checkpoints";<CheckpointSequenceNumber, AuthenticatedCheckpoint>,
            "local_fragments";<AuthorityName, CheckpointFragment>,
            "fragments";<ExecutionIndices, CheckpointFragment>,
            "locals";<DBLabel, CheckpointLocals>
        );

        let mut checkpoint_db = CheckpointStore {
            name,
            secret,
            transactions_to_checkpoint,
            checkpoint_contents,
            extra_transactions,
            checkpoints,
            local_fragments,
            fragments,
            memory_locals: None,
            locals,
            sender: None,
        };

        // Initialize the locals
        checkpoint_db.load_locals(current_epoch)?;

        Ok(checkpoint_db)
    }

    // Define handlers for request

    pub fn handle_latest_proposal(
        &mut self,
        epoch: EpochId,
        request: &CheckpointRequest,
    ) -> Result<CheckpointResponse, SuiError> {
        // Set a proposal if there is not one, and one could be set
        // TODO: check some minimum time passed since the last one
        //       and only set after that time.
        let _ = self.new_proposal(epoch);

        // Try to load any latest proposal
        let locals = self.get_locals();
        let latest_checkpoint_proposal = &locals.current_proposal;

        // Load the latest checkpoint from the database
        let previous_checkpoint = self
            .checkpoints
            .iter()
            .skip_to_last()
            .next()
            .map(|(_, c)| c)
            .unwrap_or(AuthenticatedCheckpoint::None);

        // Get the current proposal if there is one.
        let current = latest_checkpoint_proposal
            .as_ref()
            .map(|proposal| proposal.signed_summary.clone());

        // If requested include either the transactions in the latest checkpoint proposal
        // or the unprocessed transactions that block the generation of a proposal.
        let detail = if request.detail {
            latest_checkpoint_proposal
                .as_ref()
                // If the checkpoint exist return its contents.
                .map(|proposal| proposal.transactions.clone())
        } else {
            None
        };

        // Make the response
        Ok(CheckpointResponse {
            info: AuthorityCheckpointInfo::Proposal {
                current,
                previous: previous_checkpoint,
            },
            detail,
        })
    }

    pub fn handle_past_checkpoint(
        &mut self,
        detail: bool,
        seq: CheckpointSequenceNumber,
    ) -> Result<CheckpointResponse, SuiError> {
        // Get the checkpoint with a given sequence number
        let checkpoint = self
            .checkpoints
            .get(&seq)?
            .unwrap_or(AuthenticatedCheckpoint::None);

        // If a checkpoint is found, and if requested, return the list of transaction digest in it.
        let detail = if let &AuthenticatedCheckpoint::None = &checkpoint {
            None
        } else if detail {
            Some(CheckpointContents::new(
                self.checkpoint_contents
                    .iter()
                    .skip_to(&(seq, 0))?
                    .take_while(|((k, _), _)| *k == seq)
                    .map(|(_, digest)| digest),
            ))
        } else {
            None
        };

        Ok(CheckpointResponse {
            info: AuthorityCheckpointInfo::Past(checkpoint),
            detail,
        })
    }

    /// Call this function internally to update the latest checkpoint.
    /// Internally it is called with an unsigned checkpoint, and results
    /// in the checkpoint being signed, stored and the contents
    /// registered as processed or unprocessed.
    pub fn handle_internal_set_checkpoint(
        &mut self,
        epoch: EpochId,
        checkpoint: CheckpointSummary,
        contents: &CheckpointContents,
    ) -> Result<(), SuiError> {
        let checkpoint_sequence_number = *checkpoint.sequence_number();

        // Process checkpoints once but allow idempotent processing
        if self.checkpoints.get(&checkpoint_sequence_number)?.is_some() {
            return Ok(());
        }

        // Is this the next expected certificate?
        fp_ensure!(
            self.next_checkpoint() == checkpoint_sequence_number,
            SuiError::GenericAuthorityError {
                error: format!(
                    "Unexpected certificate, expected next seq={}",
                    self.next_checkpoint()
                ),
            }
        );

        // Ensure we have processed all transactions contained in this checkpoint.
        if !self.all_checkpoint_transactions_executed(contents)? {
            return Err(SuiError::from(
                "Checkpoint contains unexecuted transactions.",
            ));
        }

        // Sign the new checkpoint
        let signed_checkpoint = AuthenticatedCheckpoint::Signed(
            SignedCheckpointSummary::new_from_summary(checkpoint, self.name, &*self.secret),
        );

        // Make a DB batch
        let batch = self.checkpoints.batch();

        // Last store the actual checkpoints.
        let batch = batch
            .insert_batch(
                &self.checkpoints,
                [(&checkpoint_sequence_number, &signed_checkpoint)],
            )?
            // Drop the fragments for the previous checkpoint
            .delete_batch(
                &self.fragments,
                self.fragments.iter().filter_map(|(k, v)| {
                    // Delete all keys for checkpoints smaller than what we are committing now.
                    if *v.proposer.summary.sequence_number() <= checkpoint_sequence_number {
                        Some(k)
                    } else {
                        None
                    }
                }),
            )?
            .delete_batch(&self.local_fragments, self.local_fragments.keys())?;

        // Update the transactions databases.
        let transactions: Vec<_> = contents.transactions.iter().cloned().collect();
        self.update_new_checkpoint_inner(checkpoint_sequence_number, &transactions, batch)?;

        // Try to set a fresh proposal, and ignore errors if this fails.
        let _ = self.new_proposal(epoch);

        Ok(())
    }

    /// Call this function internally to register the latest batch of
    /// transactions processed by this authority. The latest batch is
    /// stored to ensure upon crash recovery all batches are processed.
    pub fn handle_internal_batch(
        &mut self,
        next_sequence_number: TxSequenceNumber,
        transactions: &[(TxSequenceNumber, ExecutionDigests)],
        committee: &Committee,
    ) -> Result<(), SuiError> {
        self.update_processed_transactions(transactions)?;

        // Updates the local sequence number of transactions processed.
        let locals = self.get_locals();
        let mut new_locals = locals.as_ref().clone();
        new_locals.next_transaction_sequence = next_sequence_number;
        self.set_locals(locals, new_locals)?;

        // Attempt to move forward, as many times as we can
        while self
            .attempt_to_construct_checkpoint(committee)
            .unwrap_or(false)
        {}

        Ok(())
    }

    // TODO: this function should submit the received fragment to the
    //       consensus algorithm for sequencing. It should also do some
    //       basic checks to not submit redundant information to the
    //       consensus, as well as to check it is the right node to
    //       submit to consensus.
    pub fn handle_receive_fragment(
        &mut self,
        fragment: &CheckpointFragment,
        committee: &Committee,
    ) -> Result<CheckpointResponse, SuiError> {
        // Check structure is correct and signatures verify
        fragment.verify(committee)?;

        // Does the fragment event suggest it is for the current round?
        let next_checkpoint_seq = self.next_checkpoint();
        fp_ensure!(
            *fragment.proposer.summary.sequence_number() == next_checkpoint_seq,
            SuiError::GenericAuthorityError {
                error: format!(
                    "Incorrect sequence number, expected {}",
                    next_checkpoint_seq
                )
            }
        );

        // Only a fragment that involves ourselves to be sequenced through
        // this node.
        fp_ensure!(
            fragment.proposer.authority() == &self.name || fragment.other.authority() == &self.name,
            SuiError::from("Fragment does not involve this node")
        );

        // Save in the list of local fragments for this sequence.
        let other_name = if fragment.proposer.authority() == &self.name {
            fragment.other.authority()
        } else {
            fragment.proposer.authority()
        };
        if !self.local_fragments.contains_key(other_name)? {
            self.local_fragments.insert(other_name, fragment)?;
        } else {
            // We already have this fragment, so we can ignore it.
            return Err(SuiError::GenericAuthorityError {
                error: format!("Already processed fragment with {:?}", other_name),
            });
        }

        // TODO: Checks here that the fragment makes progress over the existing
        //       construction of components using the self.fragments table. This
        //       is an optimization for later.

        let locals = self.get_locals();
        if !locals.no_more_fragments {
            // Send to consensus for sequencing.
            if let Some(sender) = &self.sender {
                sender.send_to_consensus(fragment.clone())?;
            } else {
                return Err(SuiError::from("No consensus sender configured"));
            }
        } else {
            // Maybe the fragment we received allows us to complete the current checkpoint?
            // Since we seem to be missing information to complete it (ie there is a checkpoint
            // but we are not included in it.)
            loop {
                let construct = self.attempt_to_construct_checkpoint(committee);
                // Exit if checkpoint construction leads to an error or returns false
                // (ie no new checkpoint is created.)
                if construct.is_err() || !construct.unwrap() {
                    break;
                }
            }
        }

        // NOTE: we should charge the node that sends this into consensus
        //       according to the byte length of the fragment, to create
        //       incentives for nodes to submit smaller fragments.

        Ok(CheckpointResponse {
            info: AuthorityCheckpointInfo::Success,
            detail: None,
        })
    }

    /// This function should be called by the consensus output, it is idempotent,
    /// and if called again with the same sequence number will do nothing. However,
    /// fragments should be provided in seq increasing order.
    pub fn handle_internal_fragment<P: PendCertificateForExecution>(
        &mut self,
        _seq: ExecutionIndices,
        _fragment: CheckpointFragment,
        committee: &Committee,
        handle_pending_cert: &P,
    ) -> Result<(), FragmentInternalError> {
        // Ensure we have not already processed this fragment.
        if let Some((last_seq, _)) = self.fragments.iter().skip_to_last().next() {
            if _seq <= last_seq {
                // We have already processed this fragment, just exit.
                return Ok(());
            }
        }

        // Check structure is correct and signatures verify
        _fragment
            .verify(committee)
            .map_err(FragmentInternalError::Error)?;

        // Schedule for execution all the certificates that are included here.
        handle_pending_cert
            .pending_execution(
                _fragment
                    .certs
                    .iter()
                    .map(|(digest, cert)| (digest.transaction, cert.clone()))
                    .collect(),
            )
            .map_err(|_err| {
                // There is a possibility this was not stored!
                let fragment = _fragment.clone();
                FragmentInternalError::Retry(Box::new(fragment))
            })?;

        // Save the new fragment in the DB
        self.fragments.insert(&_seq, &_fragment).map_err(|_err| {
            // There is a possibility this was not stored!
            let fragment = _fragment.clone();
            FragmentInternalError::Retry(Box::new(fragment))
        })?;

        // If the fragment contains us also save it in the list of local fragments
        let next_sequence_number = self.next_checkpoint();
        if *_fragment.proposer.summary.sequence_number() == next_sequence_number {
            if _fragment.proposer.authority() == &self.name {
                self.local_fragments
                    .insert(_fragment.other.authority(), &_fragment)
                    .map_err(|_err| {
                        // There is a possibility this was not stored!
                        let fragment = _fragment.clone();
                        FragmentInternalError::Retry(Box::new(fragment))
                    })?;
            }
            if _fragment.other.authority() == &self.name {
                self.local_fragments
                    .insert(_fragment.proposer.authority(), &_fragment)
                    .map_err(|_err| {
                        // There is a possibility this was not stored!
                        let fragment = _fragment.clone();
                        FragmentInternalError::Retry(Box::new(fragment))
                    })?;
            }
        }

        // Attempt to move forward, as many times as we can
        while self
            .attempt_to_construct_checkpoint(committee)
            .unwrap_or(false)
        {}
        Ok(())
    }

    /// Attempt to construct the next expected checkpoint, and return true if a new
    /// checkpoint is created or false if it is not.
    fn attempt_to_construct_checkpoint(
        &mut self,
        committee: &Committee,
    ) -> Result<bool, FragmentInternalError> {
        // We only attempt to reconstruct if we have a local proposal.
        // By limiting reconstruction to when we have proposals we are
        // sure that we delay doing work to when it is needed.
        if self.get_locals().current_proposal.is_none() {
            return Ok(false);
        }

        // We have a proposal so lets try to re-construct the checkpoint.
        let next_sequence_number = self.next_checkpoint();

        if let Ok(Some(contents)) = self.reconstruct_contents(committee) {
            // Here we check, and ensure, all transactions are processed before we
            // move to sign the checkpoint.
            if !self
                .all_checkpoint_transactions_executed(&contents)
                .map_err(FragmentInternalError::Error)?
            {
                return Ok(false);
            }

            let previous_digest = self
                .get_prev_checkpoint_digest(next_sequence_number)
                .map_err(FragmentInternalError::Error)?;
            let summary = CheckpointSummary::new(committee.epoch, next_sequence_number, &contents, previous_digest);
            self.handle_internal_set_checkpoint(committee.epoch, summary, &contents)
                .map_err(FragmentInternalError::Error)?;

            return Ok(true);
        }

        Ok(false)
    }

    /// Attempts to reconstruct a checkpoint contents using a local proposals and
    /// the sequence of fragments received.
    pub fn reconstruct_contents(
        &mut self,
        committee: &Committee,
    ) -> Result<Option<CheckpointContents>, FragmentInternalError> {
        let next_sequence_number = self.next_checkpoint();
        let fragments: Vec<_> = self
            .fragments
            .values()
            .filter(|frag| *frag.proposer.summary.sequence_number() == next_sequence_number)
            .collect();

        // Run the reconstruction logic to build a checkpoint.
        let _potential_checkpoint = FragmentReconstruction::construct(
            self.next_checkpoint(),
            committee.clone(),
            &fragments,
        )
        .map_err(FragmentInternalError::Error)?;

        if let Some(reconstructed) = _potential_checkpoint {
            if let Some(proposal) = &self.get_locals().current_proposal {
                // By definition the proposal and the new checkpoint must be in the
                // same sequence number of checkpoint.

                // Strategy 1 to reconstruct checkpoint -- we are included in it!

                if reconstructed
                    .global
                    .authority_waypoints
                    .contains_key(&self.name)
                {
                    // We are included in the proposal, so we can go ahead and construct the
                    // full checkpoint!
                    let mut contents = proposal.transactions.clone();
                    contents.transactions.extend(
                        // Add all items missing to reach then global waypoint
                        reconstructed.global.authority_waypoints[&self.name]
                            .items
                            .clone(),
                    );

                    return Ok(Some(contents));
                }

                // Strategy 2 to reconstruct checkpoint -- There is a link between us and the checkpoint set

                let local_links: HashSet<_> = self.local_fragments.keys().collect();
                let checkpoint_keys: HashSet<_> = reconstructed
                    .global
                    .authority_waypoints
                    .keys()
                    .cloned()
                    .collect();

                if let Some(auth) = local_links.intersection(&checkpoint_keys).next() {
                    let fragment = self
                        .local_fragments
                        .get(auth)
                        .map_err(|err| FragmentInternalError::Error(err.into()))?
                        .unwrap();

                    // Extract the diff
                    let diff = if fragment.proposer.authority() == &self.name {
                        fragment.diff
                    } else {
                        fragment.diff.swap()
                    };

                    if let Ok(contents) = reconstructed
                        .global
                        .checkpoint_items(&diff, proposal.transactions.transactions.clone())
                    {
                        let contents = CheckpointContents::new(contents.into_iter());
                        return Ok(Some(contents));
                    }
                }
            }
        } else {
            // Sets the reconstruction to false, we have all fragments we need, but
            // just cannot reconstruct the contents.
            let locals = self.get_locals();
            let mut new_locals = locals.as_ref().clone();
            new_locals.no_more_fragments = true;
            self.set_locals(locals, new_locals)
                .map_err(FragmentInternalError::Error)?;

            // A little argument about how the fragment -> checkpoint process is live
            //
            // A global checkpoint candidate must contain at least 2f+1 stake. And as
            // a result of this f+1 stake will be from honest nodes that by definition
            // must have submitted a proposal (because it is included!).
            // So f+1 honest authorities will be able to reconstruct and sign the
            // checkpoint. And all other authorities by asking all authorities will be
            // able to get f+1 signatures and construct a checkpoint certificate.

            return Err(FragmentInternalError::Error(SuiError::from(
                "Missing info to construct known checkpoint.",
            )));
        }

        Ok(None)
    }

    /// Handles the submission of a full checkpoint externally, and stores
    /// the certificate. It may be used to upload a certificate, or induce
    /// the authority to catch up with the latest checkpoints.
    ///
    /// A cert without contents is only stored if we have already processed
    /// internally the checkpoint. A cert with contents is processed as if
    /// it came from the internal consensus.
    pub fn handle_checkpoint_certificate(
        &mut self,
        checkpoint: &CertifiedCheckpointSummary,
        contents: &Option<CheckpointContents>,
        committee: &Committee,
    ) -> Result<CheckpointResponse, SuiError> {
        // Get the record in our checkpoint database for this sequence number.
        let current = self.checkpoints.get(checkpoint.summary.sequence_number())?;

        match &current {
            // If cert exists, do nothing (idempotent)
            Some(AuthenticatedCheckpoint::Certified(_current_cert)) => {}
            // If no such checkpoint is known, then return an error
            // NOTE: a checkpoint must first be confirmed internally before an external
            // certificate is registered.
            None => {
                if let &Some(contents) = &contents {
                    // Check and process contents
                    checkpoint.verify_with_transactions(committee, contents)?;
                    self.handle_internal_set_checkpoint(
                        committee.epoch,
                        checkpoint.summary.clone(),
                        contents,
                    )?;
                    // Then insert it
                    self.checkpoints.insert(
                        checkpoint.summary.sequence_number(),
                        &AuthenticatedCheckpoint::Certified(checkpoint.clone()),
                    )?;

                    // Now that we have the new checkpoint we try to move forward the checkpoint creation
                    // process. We try to use fragments in the sequence to create past checkpoints.
                    loop {
                        let construct = self.attempt_to_construct_checkpoint(committee);
                        // Exit if checkpoint construction leads to an error or returns false
                        // (ie no new checkpoint is created.)
                        if construct.is_err() || !construct.unwrap() {
                            break;
                        }
                    }
                } else {
                    return Err(SuiError::from("No checkpoint set at this sequence."));
                }
            }
            // In this case we have an internal signed checkpoint so we promote it to a
            // full certificate.
            Some(AuthenticatedCheckpoint::Signed(_)) => {
                checkpoint.verify(committee)?;
                self.checkpoints.insert(
                    checkpoint.summary.sequence_number(),
                    &AuthenticatedCheckpoint::Certified(checkpoint.clone()),
                )?;
            }
            Some(AuthenticatedCheckpoint::None) => {
                // If we are here there was a bug? We never assign the None case
                // to a stored value.
                unreachable!();
            }
        };

        Ok(CheckpointResponse {
            info: AuthorityCheckpointInfo::Success,
            detail: None,
        })
    }

    // Helper read functions

    /// Return the seq number of the last checkpoint we have recorded.
    pub fn next_checkpoint(&mut self) -> CheckpointSequenceNumber {
        self.get_locals().next_checkpoint
    }

    /// Returns the next transactions sequence number expected.
    pub fn next_transaction_sequence_expected(&mut self) -> TxSequenceNumber {
        self.get_locals().next_transaction_sequence
    }

    /// Creates a new proposal, but only if the previous checkpoint certificate
    /// is known and stored. This ensures that any validator in checkpoint round
    /// X can serve certificates for all rounds < X.
    pub fn new_proposal(&mut self, epoch: EpochId) -> Result<CheckpointProposal, SuiError> {
        let sequence_number = self.next_checkpoint();

        // Only move to propose when we have the full checkpoint certificate
        if sequence_number > 0 {
            // Check that we have the full certificate for the previous checkpoint
            if !matches!(
                self.checkpoints.get(&(sequence_number - 1)),
                Ok(Some(AuthenticatedCheckpoint::Certified(..)))
            ) {
                return Err(SuiError::from("Cannot propose before having a certificate"));
            }
        }

        self.set_proposal(epoch)
    }

    /// Get the latest stored checkpoint if there is one
    pub fn latest_stored_checkpoint(
        &mut self,
    ) -> Result<Option<AuthenticatedCheckpoint>, SuiError> {
        Ok(self
            .checkpoints
            .iter()
            .skip_to_last()
            .next()
            .map(|(_, ckp)| ckp))
    }

    // Helper write functions

    /// Set the next checkpoint proposal.
    pub fn set_proposal(&mut self, epoch: EpochId) -> Result<CheckpointProposal, SuiError> {
        // Check that:
        // - there is no current proposal.
        // - there are no unprocessed transactions.
        // - there are some extra transactions to include.

        let locals = self.get_locals();

        if let Some(proposal) = &locals.current_proposal {
            return Ok(proposal.clone());
        }

        if self.extra_transactions.iter().count() == 0 {
            return Err(SuiError::from("Cannot propose an empty set."));
        }

        // Include the sequence number of all extra transactions not already in a
        // checkpoint. And make a list of the transactions.
        let checkpoint_sequence = self.next_checkpoint();
        let next_local_tx_sequence = self.extra_transactions.values().max().unwrap() + 1;

        // Extract the previous checkpoint digest if there is one.
        let previous_digest = self.get_prev_checkpoint_digest(checkpoint_sequence)?;

        let transactions = CheckpointContents::new(self.extra_transactions.keys());
        let summary = SignedCheckpointSummary::new(
            epoch,
            checkpoint_sequence,
            self.name,
            &*self.secret,
            &transactions,
            previous_digest,
        );

        let proposal_and_transactions = CheckpointProposal::new(summary, transactions);

        // Record the checkpoint in the locals
        let mut new_locals = locals.as_ref().clone();
        new_locals.current_proposal = Some(proposal_and_transactions.clone());
        new_locals.proposal_next_transaction = Some(next_local_tx_sequence);
        self.set_locals(locals, new_locals)?;

        Ok(proposal_and_transactions)
    }

    /// Returns whether a list of transactions is fully executed.
    pub fn all_checkpoint_transactions_executed(
        &self,
        transactions: &CheckpointContents,
    ) -> SuiResult<bool> {
        let new_transactions = self
            .extra_transactions
            .multi_get(transactions.transactions.iter())?
            .into_iter()
            .zip(transactions.transactions.iter())
            .filter_map(
                |(opt_seq, tx)| {
                    if opt_seq.is_none() {
                        Some(*tx)
                    } else {
                        None
                    }
                },
            )
            .count();

        Ok(new_transactions == 0)
    }

    #[cfg(test)]
    pub fn update_new_checkpoint(
        &mut self,
        seq: CheckpointSequenceNumber,
        transactions: &[ExecutionDigests],
    ) -> Result<(), SuiError> {
        let batch = self.transactions_to_checkpoint.batch();
        self.update_new_checkpoint_inner(seq, transactions, batch)?;
        Ok(())
    }

    /// Add transactions associated with a new checkpoint in the structure, and
    /// updates all tables including unprocessed and extra transactions.
    fn update_new_checkpoint_inner(
        &mut self,
        seq: CheckpointSequenceNumber,
        transactions: &[ExecutionDigests],
        batch: DBBatch,
    ) -> Result<(), SuiError> {
        // Check that this checkpoint seq is new, and directly follows the last
        // highest checkpoint seen. First checkpoint is always zero.
        let expected_seq = self.next_checkpoint();

        if seq != expected_seq {
            return Err(SuiError::CheckpointingError {
                error: "Unexpected checkpoint sequence number.".to_string(),
            });
        }

        // Process transactions not already in a checkpoint
        let new_transactions = self
            .transactions_to_checkpoint
            .multi_get(transactions.iter())?
            .into_iter()
            .zip(transactions.iter())
            .filter_map(
                |(opt_seq, tx)| {
                    if opt_seq.is_none() {
                        Some(*tx)
                    } else {
                        None
                    }
                },
            )
            .collect::<Vec<_>>();

        let high_seq = u64::MAX / 2;
        let transactions_with_seq = self.extra_transactions.multi_get(new_transactions.iter())?;

        // Delete the extra transactions now used
        let batch = batch.delete_batch(
            &self.extra_transactions,
            transactions_with_seq
                .iter()
                .zip(new_transactions.iter())
                .filter_map(|(opt, tx)| if opt.is_some() { Some(tx) } else { None }),
        )?;

        // Now write the checkpoint data to the database
        //
        // All unknown sequence numbers are replaced with high sequence number
        // of u64::max / 2 and greater.

        let checkpoint_data: Vec<_> = new_transactions
            .iter()
            .zip(transactions_with_seq.iter())
            .enumerate()
            .map(|(i, (tx, opt))| {
                let iseq = opt.unwrap_or(i as u64 + high_seq);
                ((seq, iseq), *tx)
            })
            .collect();

        let batch = batch.insert_batch(
            &self.transactions_to_checkpoint,
            checkpoint_data.iter().map(|(a, b)| (b, a)),
        )?;

        let batch = batch.insert_batch(&self.checkpoint_contents, checkpoint_data.into_iter())?;

        // Write to the database.
        batch.write()?;

        // Clean up our proposal if any
        let locals = self.get_locals();

        let mut new_locals = locals.as_ref().clone();
        new_locals.current_proposal = None;
        new_locals.proposal_next_transaction = None;
        new_locals.no_more_fragments = false;
        new_locals.next_checkpoint = expected_seq + 1;
        self.set_locals(locals, new_locals)?;

        Ok(())
    }

    /// Updates the store on the basis of transactions that have been processed. This is idempotent
    /// and nothing unsafe happens if it is called twice. Returns the lowest checkpoint number with
    /// unprocessed transactions (this is the low watermark).
    fn update_processed_transactions(
        &mut self, // We take by &mut to prevent concurrent access.
        transactions: &[(TxSequenceNumber, ExecutionDigests)],
    ) -> Result<(), SuiError> {
        let in_checkpoint = self
            .transactions_to_checkpoint
            .multi_get(transactions.iter().map(|(_, tx)| tx))?;

        let batch = self.transactions_to_checkpoint.batch();

        let already_in_checkpoint_tx =
            transactions
                .iter()
                .zip(&in_checkpoint)
                .filter_map(
                    |((_seq, tx), in_chk)| {
                        if in_chk.is_some() {
                            Some(tx)
                        } else {
                            None
                        }
                    },
                );

        // Delete the entries with the old sequence numbers.
        // They will be updated with the new sequence numbers latter.
        let batch =
            batch.delete_batch(&self.transactions_to_checkpoint, already_in_checkpoint_tx)?;

        let batch = batch.delete_batch(
            &self.checkpoint_contents,
            transactions
                .iter()
                .zip(&in_checkpoint)
                .filter_map(|((_seq, _tx), in_chk)| {
                    if in_chk.is_some() {
                        Some(in_chk.unwrap())
                    } else {
                        None
                    }
                }),
        )?;

        // Update the entry to the transactions_to_checkpoint

        let batch = batch.insert_batch(
            &self.transactions_to_checkpoint,
            transactions
                .iter()
                .zip(&in_checkpoint)
                .filter_map(|((seq, tx), in_chk)| {
                    if in_chk.is_some() {
                        Some((tx, (in_chk.unwrap().0, *seq)))
                    } else {
                        None
                    }
                }),
        )?;

        // Update the checkpoint local sequence number
        let batch = batch.insert_batch(
            &self.checkpoint_contents,
            transactions
                .iter()
                .zip(&in_checkpoint)
                .filter_map(|((seq, tx), in_chk)| {
                    if in_chk.is_some() {
                        Some(((in_chk.unwrap().0, *seq), tx))
                    } else {
                        None
                    }
                }),
        )?;

        // If the transactions processed did not belong to a checkpoint yet, we add them to the list
        // of `extra` transactions, that we should be actively propagating to others.
        let batch = batch.insert_batch(
            &self.extra_transactions,
            transactions
                .iter()
                .zip(&in_checkpoint)
                .filter_map(|((seq, tx), in_chk)| {
                    if in_chk.is_none() {
                        Some((tx, seq))
                    } else {
                        None
                    }
                }),
        )?;

        // Write to the database.
        batch.write()?;

        Ok(())
    }
}
