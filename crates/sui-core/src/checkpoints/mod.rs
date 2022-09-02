// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod causal_order_effects;
pub mod reconstruction;

#[cfg(test)]
#[path = "./tests/checkpoint_tests.rs"]
pub(crate) mod checkpoint_tests;

use narwhal_executor::ExecutionIndices;
use rocksdb::Options;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::{collections::HashSet, path::Path, sync::Arc};
use sui_storage::default_db_options;
use sui_types::messages_checkpoint::{CheckpointProposal, CheckpointProposalContents};
use sui_types::{
    base_types::{AuthorityName, ExecutionDigests},
    batch::TxSequenceNumber,
    committee::{Committee, EpochId},
    error::{SuiError, SuiResult},
    fp_ensure,
    messages_checkpoint::{
        AuthenticatedCheckpoint, CertifiedCheckpointSummary, CheckpointContents, CheckpointDigest,
        CheckpointFragment, CheckpointResponse, CheckpointSequenceNumber, CheckpointSummary,
        SignedCheckpointSummary,
    },
};
use tracing::{debug, error, info};
use typed_store::traits::DBMapTableUtil;
use typed_store::{
    rocks::{DBBatch, DBMap},
    Map,
};
use typed_store_macros::DBMapUtils;

use crate::checkpoints::causal_order_effects::CausalOrder;
use crate::{
    authority::StableSyncAuthoritySigner,
    authority_active::execution_driver::PendCertificateForExecution,
};

use self::reconstruction::FragmentReconstruction;

pub type DBLabel = usize;
const LOCALS: DBLabel = 0;

// TODO: Make last checkpoint number of each epoch more flexible.
// TODO: Make this bigger.
pub const CHECKPOINT_COUNT_PER_EPOCH: u64 = 3;

#[derive(Clone, Serialize, Deserialize, Default, Debug)]
pub struct CheckpointLocals {
    // The next checkpoint number expected.
    pub next_checkpoint: CheckpointSequenceNumber,

    // The next transaction after what is included in the proposal.
    // NOTE: This will be set to 0 if the current checkpoint is empty
    // and doesn't contain any transactions.
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

/// DBMap tables for checkpoints
#[derive(DBMapUtils)]
pub struct CheckpointStoreTables {
    /// The list of all transaction/effects that are checkpointed mapping to the checkpoint
    /// sequence number they were assigned to.
    #[default_options_override_fn = "transactions_to_checkpoint_table_default_config"]
    pub transactions_to_checkpoint: DBMap<ExecutionDigests, CheckpointSequenceNumber>,

    /// The mapping from checkpoint to transaction/effects contained within the checkpoint.
    /// The checkpoint content should be causally ordered and is consistent among
    /// all validators.
    /// TODO: CheckpointContents may grow very big and becomes problematic to store as db value.
    pub checkpoint_contents: DBMap<CheckpointSequenceNumber, CheckpointContents>,

    /// The set of transaction/effects this authority has processed but have not yet been
    /// included in a checkpoint, and their sequence number in the local sequence
    /// of this authority.
    #[default_options_override_fn = "extra_transactions_table_default_config"]
    pub extra_transactions: DBMap<ExecutionDigests, TxSequenceNumber>,

    /// The list of checkpoint, along with their authentication information
    #[default_options_override_fn = "checkpoints_table_default_config"]
    pub checkpoints: DBMap<CheckpointSequenceNumber, AuthenticatedCheckpoint>,

    // --- Logic related to fragments on the way to making checkpoints

    // A list of own fragments indexed by the other node that the fragment connects
    // to. These are used for the local node to potentially reconstruct the full
    // transaction set.
    #[default_options_override_fn = "local_fragments_table_default_config"]
    pub local_fragments: DBMap<AuthorityName, CheckpointFragment>,

    /// Store the fragments received in order, the counter is purely internal,
    /// to allow us to provide a list in order they were received. We only store
    /// the fragments that are relevant to the next checkpoints. Past checkpoints
    /// already contain all relevant information from previous checkpoints.
    pub fragments: DBMap<ExecutionIndices, CheckpointFragment>,

    /// A single entry table to store locals.
    #[default_options_override_fn = "locals_table_default_config"]
    pub locals: DBMap<DBLabel, CheckpointLocals>,
}

// These functions are used to initialize the DB tables
fn transactions_to_checkpoint_table_default_config() -> Options {
    default_db_options(None, None).1
}
fn extra_transactions_table_default_config() -> Options {
    default_db_options(None, None).1
}

fn checkpoints_table_default_config() -> Options {
    default_db_options(None, None).1
}
fn local_fragments_table_default_config() -> Options {
    default_db_options(None, None).1
}

fn locals_table_default_config() -> Options {
    default_db_options(None, None).1
}

pub struct CheckpointStore {
    // Fixed size, static, identity of the authority
    /// The name of this authority.
    pub name: AuthorityName,
    /// The signature key of the authority.
    pub secret: StableSyncAuthoritySigner,

    // --- Logic related to fragments on the way to making checkpoints
    /// The local sequence at which the proposal for the next checkpoint is created
    /// This is a sequence number containing all unprocessed transactions lower than
    /// this sequence number. At this point the unprocessed_transactions sequence
    /// should be empty. It is none if there is no active proposal. We also include here
    /// the proposal, although we could re-create it from the database.
    memory_locals: Option<Arc<CheckpointLocals>>,

    // Consensus sender
    sender: Option<Box<dyn ConsensusSender>>,

    /// DBMap tables
    pub tables: CheckpointStoreTables,
}

impl CheckpointStore {
    pub fn get_checkpoint(
        &self,
        seq: CheckpointSequenceNumber,
    ) -> Result<Option<AuthenticatedCheckpoint>, SuiError> {
        Ok(self.tables.checkpoints.get(&seq)?)
    }

    fn get_prev_checkpoint_digest(
        &mut self,
        checkpoint_sequence: CheckpointSequenceNumber,
    ) -> Result<Option<CheckpointDigest>, SuiError> {
        // Extract the previous checkpoint digest if there is one.
        Ok(if checkpoint_sequence > 0 {
            self.get_checkpoint(checkpoint_sequence - 1)?
                .map(|prev_checkpoint| prev_checkpoint.summary().digest())
        } else {
            None
        })
    }

    // Manage persistent local variables

    /// Loads the locals from the store -- do this at init
    fn load_locals(&mut self, epoch: EpochId) -> Result<CheckpointLocals, SuiError> {
        // Loads locals from disk, or inserts initial locals
        let mut locals = match self.tables.locals.get(&LOCALS)? {
            Some(locals) => locals,
            None => {
                let locals = CheckpointLocals::default();
                self.tables.locals.insert(&LOCALS, &locals)?;
                locals
            }
        };

        // Recreate the proposal
        if locals.proposal_next_transaction.is_some() {
            let checkpoint_sequence = locals.next_checkpoint;
            let transactions = self
                .tables
                .extra_transactions
                .iter()
                .filter(|(_, seq)| seq < locals.proposal_next_transaction.as_ref().unwrap())
                .map(|(digest, _)| digest);
            let transactions = CheckpointProposalContents::new(transactions);
            let proposal = CheckpointProposal::new(
                epoch,
                checkpoint_sequence,
                self.name,
                &*self.secret,
                transactions,
            );

            locals.current_proposal = Some(proposal);
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
        self.tables.locals.insert(&LOCALS, &locals)?;
        self.memory_locals = Some(Arc::new(locals));
        Ok(())
    }

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
    pub fn open(
        path: &Path,
        db_options: Option<Options>,
        current_epoch: EpochId,
        name: AuthorityName,
        secret: StableSyncAuthoritySigner,
    ) -> Result<CheckpointStore, SuiError> {
        let mut checkpoint_db = CheckpointStore {
            name,
            secret,
            memory_locals: None,
            sender: None,
            tables: CheckpointStoreTables::open_tables_read_write(
                path.to_path_buf(),
                db_options,
                None,
            ),
        };

        // Initialize the locals
        checkpoint_db.load_locals(current_epoch)?;

        Ok(checkpoint_db)
    }

    // Define handlers for request

    pub fn handle_proposal(&mut self, detail: bool) -> Result<CheckpointResponse, SuiError> {
        let locals = self.get_locals();
        let latest_checkpoint_proposal = &locals.current_proposal;

        let signed_proposal = latest_checkpoint_proposal
            .as_ref()
            .map(|proposal| proposal.signed_summary.clone());

        let contents = match (detail, &latest_checkpoint_proposal) {
            (true, Some(proposal)) => Some(proposal.transactions.clone()),
            _ => None,
        };

        let prev_cert = match &signed_proposal {
            Some(proposal) if proposal.summary.sequence_number > 0 => {
                let seq = proposal.summary.sequence_number;
                let checkpoint = self.tables.checkpoints.get(&(seq - 1))?;
                match checkpoint {
                    Some(AuthenticatedCheckpoint::Signed(_)) | None => {
                        error!(
                            "Invariant violation detected: Validator is making a proposal for checkpoint {:?}, but no certificate exists for checkpoint {:?}",
                            seq,
                            seq - 1,
                        );
                        return Err(SuiError::from(
                            "Checkpoint proposal sequence number inconsistent with latest cert",
                        ));
                    }
                    Some(AuthenticatedCheckpoint::Certified(c)) => Some(c),
                }
            }
            _ => None,
        };

        Ok(CheckpointResponse::CheckpointProposal {
            proposal: signed_proposal,
            prev_cert,
            proposal_contents: contents,
        })
    }

    pub fn handle_authenticated_checkpoint(
        &mut self,
        seq: &Option<CheckpointSequenceNumber>,
        detail: bool,
    ) -> SuiResult<CheckpointResponse> {
        let checkpoint = match seq {
            Some(s) => self.tables.checkpoints.get(s)?,
            None => self.latest_stored_checkpoint(),
        };
        let contents = match (&checkpoint, detail) {
            (Some(c), true) => self
                .tables
                .checkpoint_contents
                .get(&c.summary().sequence_number)?,
            _ => None,
        };
        Ok(CheckpointResponse::AuthenticatedCheckpoint {
            checkpoint,
            contents,
        })
    }

    pub fn sign_new_checkpoint<'a>(
        &mut self,
        epoch: EpochId,
        sequence_number: CheckpointSequenceNumber,
        transactions: impl Iterator<Item = &'a ExecutionDigests> + Clone,
        effects_store: impl CausalOrder + PendCertificateForExecution,
    ) -> SuiResult {
        // Make sure that all transactions in the checkpoint have been executed locally.
        self.check_checkpoint_transactions(transactions.clone(), &effects_store)?;

        let previous_digest = self.get_prev_checkpoint_digest(sequence_number)?;

        // Create a causal order of all transactions in the checkpoint.
        let ordered_contents = CheckpointContents::new_with_causally_ordered_transactions(
            effects_store
                .get_complete_causal_order(transactions, self)?
                .into_iter(),
        );

        let summary =
            CheckpointSummary::new(epoch, sequence_number, &ordered_contents, previous_digest);

        let checkpoint = AuthenticatedCheckpoint::Signed(
            SignedCheckpointSummary::new_from_summary(summary, self.name, &*self.secret),
        );
        self.handle_internal_set_checkpoint(&checkpoint, &ordered_contents)
    }

    /// Call this function internally to update the latest checkpoint.
    /// Internally it is called with an unsigned checkpoint, and results
    /// in the checkpoint being signed, stored and the contents
    /// registered as processed or unprocessed.
    pub fn handle_internal_set_checkpoint(
        &mut self,
        checkpoint: &AuthenticatedCheckpoint,
        contents: &CheckpointContents,
    ) -> SuiResult {
        let summary = checkpoint.summary();
        let checkpoint_sequence_number = *summary.sequence_number();

        debug_assert!(self
            .tables
            .checkpoints
            .get(&checkpoint_sequence_number)?
            .is_none());
        debug_assert!(self.next_checkpoint() == checkpoint_sequence_number);

        debug!(
            "Number of transactions in checkpoint {:?}: {:?}",
            checkpoint_sequence_number,
            contents.size()
        );

        // Make a DB batch
        let batch = self.tables.checkpoints.batch();

        // Last store the actual checkpoints.
        let batch = batch
            .insert_batch(
                &self.tables.checkpoints,
                [(&checkpoint_sequence_number, checkpoint)],
            )?
            // Drop the fragments for the previous checkpoint
            .delete_batch(
                &self.tables.fragments,
                self.tables.fragments.iter().filter_map(|(k, v)| {
                    // Delete all keys for checkpoints smaller than what we are committing now.
                    if v.proposer.summary.sequence_number <= checkpoint_sequence_number {
                        Some(k)
                    } else {
                        None
                    }
                }),
            )?
            .delete_batch(
                &self.tables.local_fragments,
                self.tables.local_fragments.keys(),
            )?;

        // Update the transactions databases.
        self.update_new_checkpoint_inner(checkpoint_sequence_number, contents, batch)
    }

    /// Call this function internally to register the latest batch of
    /// transactions processed by this authority. The latest batch is
    /// stored to ensure upon crash recovery all batches are processed.
    pub fn handle_internal_batch(
        &mut self,
        next_sequence_number: TxSequenceNumber,
        transactions: &[(TxSequenceNumber, ExecutionDigests)],
    ) -> Result<(), SuiError> {
        self.update_processed_transactions(transactions)?;

        // Updates the local sequence number of transactions processed.
        let locals = self.get_locals();
        let mut new_locals = locals.as_ref().clone();
        new_locals.next_transaction_sequence = next_sequence_number;
        self.set_locals(locals, new_locals)?;

        Ok(())
    }

    // TODO: this function should do some basic checks to not submit redundant information to the
    //       consensus, as well as to check it is the right node to submit to consensus.
    pub fn submit_local_fragment_to_consensus(
        &mut self,
        fragment: &CheckpointFragment,
        committee: &Committee,
    ) -> SuiResult {
        // Check structure is correct and signatures verify
        fragment.verify(committee)?;

        // Does the fragment event suggest it is for the current round?
        let next_checkpoint_seq = self.next_checkpoint();
        fp_ensure!(
            fragment.proposer.summary.sequence_number == next_checkpoint_seq,
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
        if !self.tables.local_fragments.contains_key(other_name)? {
            self.tables.local_fragments.insert(other_name, fragment)?;
        } else {
            // We already have this fragment, so we can ignore it.
            return Err(SuiError::GenericAuthorityError {
                error: format!("Already processed fragment with {:?}", other_name),
            });
        }

        // Send to consensus for sequencing.
        if let Some(sender) = &self.sender {
            debug!("Send fragment: {} -- {}", self.name, other_name);
            sender.send_to_consensus(fragment.clone())?;
        } else {
            return Err(SuiError::from("No consensus sender configured"));
        }

        // NOTE: we should charge the node that sends this into consensus
        //       according to the byte length of the fragment, to create
        //       incentives for nodes to submit smaller fragments.

        Ok(())
    }

    /// This function should be called by the consensus output, it is idempotent,
    /// and if called again with the same sequence number will do nothing. However,
    /// fragments should be provided in seq increasing order.
    pub fn handle_internal_fragment(
        &mut self,
        seq: ExecutionIndices,
        fragment: CheckpointFragment,
        handle_pending_cert: impl PendCertificateForExecution,
    ) -> SuiResult {
        // Ensure we have not already processed this fragment.
        if let Some((last_seq, _)) = self.tables.fragments.iter().skip_to_last().next() {
            if seq <= last_seq {
                // We have already processed this fragment, just exit.
                return Ok(());
            }
        }

        // Schedule for execution all the certificates that are included here.
        // TODO: We should not schedule a cert if it has already been executed.
        handle_pending_cert.add_pending_certificates(
            fragment
                .certs
                .iter()
                .map(|(digest, cert)| (digest.transaction, Some(cert.clone())))
                .collect(),
        )?;

        // Save the new fragment in the DB
        self.tables.fragments.insert(&seq, &fragment)?;

        // If the fragment contains us also save it in the list of local fragments
        let next_sequence_number = self.next_checkpoint();
        if fragment.proposer.summary.sequence_number == next_sequence_number {
            if fragment.proposer.authority() == &self.name {
                self.tables
                    .local_fragments
                    .insert(fragment.other.authority(), &fragment)?;
            }
            if fragment.other.authority() == &self.name {
                self.tables
                    .local_fragments
                    .insert(fragment.proposer.authority(), &fragment)?;
            }
        }

        Ok(())
    }

    /// Attempt to construct the next expected checkpoint.
    /// Returns OK if a checkpoint is successfully constructed.
    pub fn attempt_to_construct_checkpoint(
        &mut self,
        committee: &Committee,
    ) -> SuiResult<BTreeSet<ExecutionDigests>> {
        // We have a proposal so lets try to re-construct the checkpoint.
        let locals = self.get_locals();

        // Ok to unwrap because we won't enter the checkpoint process unless we have a proposal.
        let our_proposal = locals.current_proposal.as_ref().unwrap();

        let candidate_transactions = self.reconstruct_contents(committee, our_proposal)?;

        // The checkpoint content is constructed using all fragments received.
        // When receiving the fragments, we have verified that all certs are valid.
        // However, we did not verify that all transactions have not been checkpointed.
        // Here we filter out any transaction that has already been checkpointed.
        let new_transactions: BTreeSet<_> = self
            .tables
            .transactions_to_checkpoint
            .multi_get(candidate_transactions.iter())?
            .into_iter()
            .zip(candidate_transactions.iter())
            .filter_map(
                |(opt_seq, tx)| {
                    if opt_seq.is_none() {
                        Some(*tx)
                    } else {
                        None
                    }
                },
            )
            .collect();
        Ok(new_transactions)
    }

    /// Attempts to reconstruct a checkpoint contents using a local proposals and
    /// the sequence of fragments received.
    pub fn reconstruct_contents(
        &mut self,
        committee: &Committee,
        our_proposal: &CheckpointProposal,
    ) -> SuiResult<BTreeSet<ExecutionDigests>> {
        let next_sequence_number = self.next_checkpoint();
        let fragments: Vec<_> = self
            .tables
            .fragments
            .values()
            .filter(|frag| frag.proposer.summary.sequence_number == next_sequence_number)
            .collect();

        // Run the reconstruction logic to build a checkpoint.
        let reconstructed = FragmentReconstruction::construct(
            self.next_checkpoint(),
            committee.clone(),
            &fragments,
        )?;

        // A little argument about how the fragment -> checkpoint process is live
        //
        // A global checkpoint candidate must contain at least 2f+1 stake. And as
        // a result of this f+1 stake will be from honest nodes that by definition
        // must have submitted a proposal (because it is included!).
        // So f+1 honest authorities will be able to reconstruct and sign the
        // checkpoint. And all other authorities by asking all authorities will be
        // able to get f+1 signatures and construct a checkpoint certificate.

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
            let mut contents = our_proposal.transactions.clone();
            contents.transactions.extend(
                // Add all items missing to reach then global waypoint
                reconstructed.global.authority_waypoints[&self.name]
                    .items
                    .clone(),
            );

            return Ok(contents.transactions.into_iter().collect());
        }

        // Strategy 2 to reconstruct checkpoint -- There is a link between us and the checkpoint set

        let local_links: HashSet<_> = self.tables.local_fragments.keys().collect();
        let checkpoint_keys: HashSet<_> = reconstructed
            .global
            .authority_waypoints
            .keys()
            .cloned()
            .collect();

        if let Some(auth) = local_links.intersection(&checkpoint_keys).next() {
            let fragment = self.tables.local_fragments.get(auth)?.unwrap();

            // Extract the diff
            let diff = if fragment.proposer.authority() == &self.name {
                fragment.diff
            } else {
                fragment.diff.swap()
            };

            if let Ok(contents) = reconstructed.global.checkpoint_items(
                &diff,
                our_proposal
                    .transactions
                    .transactions
                    .iter()
                    .cloned()
                    .collect(),
            ) {
                return Ok(contents);
            }
        }

        // Sets the reconstruction to false, we have all fragments we need, but
        // just cannot reconstruct the contents.
        let locals = self.get_locals();
        let mut new_locals = locals.as_ref().clone();
        new_locals.no_more_fragments = true;
        debug!("no_more_fragments is set");
        self.set_locals(locals, new_locals)?;

        Err(SuiError::from(
            "Missing info to construct known checkpoint.",
        ))
    }

    pub fn promote_signed_checkpoint_to_cert(
        &mut self,
        checkpoint: &CertifiedCheckpointSummary,
        committee: &Committee,
    ) -> SuiResult {
        checkpoint.verify(committee, None)?;
        debug_assert!(matches!(
            self.latest_stored_checkpoint(),
            Some(AuthenticatedCheckpoint::Signed(_))
        ));
        let seq = checkpoint.summary.sequence_number();
        self.tables
            .checkpoints
            .insert(seq, &AuthenticatedCheckpoint::Certified(checkpoint.clone()))?;
        self.clear_proposal(*seq + 1)?;
        Ok(())
    }

    /// Processes a checkpoint certificate that this validator just learned about.
    /// Such certificate may either be created locally based on a quorum of signed checkpoints,
    /// or downloaded from other validators to sync local checkpoint state.
    pub fn process_new_checkpoint_certificate(
        &mut self,
        checkpoint: &CertifiedCheckpointSummary,
        contents: &CheckpointContents,
        committee: &Committee,
        effects_store: impl CausalOrder + PendCertificateForExecution,
    ) -> SuiResult {
        self.check_checkpoint_transactions(contents.iter(), &effects_store)?;
        self.process_synced_checkpoint_certificate(checkpoint, contents, committee)
    }

    /// Unlike process_new_checkpoint_certificate this does not verify that transactions are executed
    /// Checkpoint sync process executes it because it verifies transactions when downloading checkpoint
    pub fn process_synced_checkpoint_certificate(
        &mut self,
        checkpoint: &CertifiedCheckpointSummary,
        contents: &CheckpointContents,
        committee: &Committee,
    ) -> SuiResult {
        let seq = checkpoint.summary.sequence_number();
        debug_assert!(self.tables.checkpoints.get(seq)?.is_none());
        // Check and process contents
        checkpoint.verify(committee, Some(contents))?;

        self.handle_internal_set_checkpoint(
            &AuthenticatedCheckpoint::Certified(checkpoint.clone()),
            contents,
        )?;
        self.clear_proposal(*seq + 1)?;
        Ok(())
    }

    fn clear_proposal(
        &mut self,
        new_expected_next_checkpoint: CheckpointSequenceNumber,
    ) -> SuiResult {
        let locals = self.get_locals();

        let mut new_locals = locals.as_ref().clone();
        new_locals.current_proposal = None;
        new_locals.proposal_next_transaction = None;
        new_locals.no_more_fragments = false;
        new_locals.next_checkpoint = new_expected_next_checkpoint;
        self.set_locals(locals, new_locals)
    }

    // Helper read functions

    /// Return the seq number of the next checkpoint.
    pub fn next_checkpoint(&mut self) -> CheckpointSequenceNumber {
        self.get_locals().next_checkpoint
    }

    /// Returns the next transactions sequence number expected.
    pub fn next_transaction_sequence_expected(&mut self) -> TxSequenceNumber {
        self.get_locals().next_transaction_sequence
    }

    /// Get the latest stored checkpoint if there is one
    pub fn latest_stored_checkpoint(&mut self) -> Option<AuthenticatedCheckpoint> {
        self.tables
            .checkpoints
            .iter()
            .skip_to_last()
            .next()
            .map(|(_, ckp)| ckp)
    }

    pub fn is_ready_to_start_epoch_change(&mut self) -> bool {
        let next_seq = self.next_checkpoint();
        next_seq % CHECKPOINT_COUNT_PER_EPOCH == 0 && next_seq != 0
    }

    pub fn is_ready_to_finish_epoch_change(&mut self) -> bool {
        let next_seq = self.next_checkpoint();
        next_seq % CHECKPOINT_COUNT_PER_EPOCH == 1 && next_seq != 1
    }

    pub fn validators_already_fragmented_with(&mut self) -> BTreeSet<AuthorityName> {
        self.tables
            .local_fragments
            .iter()
            .map(|(name, _)| name)
            .collect()
    }

    // Helper write functions

    /// Set the next checkpoint proposal.
    pub fn set_proposal(&mut self, epoch: EpochId) -> Result<CheckpointProposal, SuiError> {
        // Check that:
        // - there is no current proposal.
        // - there are no unprocessed transactions.

        let locals = self.get_locals();

        if let Some(proposal) = &locals.current_proposal {
            return Ok(proposal.clone());
        }

        // Include the sequence number of all extra transactions not already in a
        // checkpoint. And make a list of the transactions.
        let checkpoint_sequence = self.next_checkpoint();
        let next_local_tx_sequence = if let Some(m) = self.tables.extra_transactions.values().max()
        {
            m + 1
        } else {
            0
        };

        let transactions = CheckpointProposalContents::new(self.tables.extra_transactions.keys());
        let size = transactions.transactions.len();
        info!(cp_seq=?checkpoint_sequence, ?size, "A new checkpoint proposal is created");
        debug!(
            "Transactions included in the checkpoint proposal: {:?}",
            transactions.transactions
        );

        let checkpoint_proposal = CheckpointProposal::new(
            epoch,
            checkpoint_sequence,
            self.name,
            &*self.secret,
            transactions,
        );

        // Record the checkpoint in the locals
        let mut new_locals = locals.as_ref().clone();
        new_locals.current_proposal = Some(checkpoint_proposal.clone());
        new_locals.proposal_next_transaction = Some(next_local_tx_sequence);
        self.set_locals(locals, new_locals)?;

        Ok(checkpoint_proposal)
    }

    fn check_checkpoint_transactions<'a>(
        &self,
        transactions: impl Iterator<Item = &'a ExecutionDigests> + Clone,
        pending_execution: &impl PendCertificateForExecution,
    ) -> SuiResult {
        let extra_tx = self
            .tables
            .extra_transactions
            .multi_get(transactions.clone())?;
        let tx_to_execute: Vec<_> = extra_tx
            .iter()
            .zip(transactions)
            .filter_map(|(opt_seq, digest)| {
                if opt_seq.is_none() {
                    Some(digest.transaction)
                } else {
                    None
                }
            })
            .collect();

        if tx_to_execute.is_empty() {
            Ok(())
        } else {
            debug!("Scheduled transactions for execution: {:?}", tx_to_execute);
            pending_execution.add_pending_certificates(
                tx_to_execute
                    .into_iter()
                    .map(|digest| (digest, None))
                    .collect(),
            )?;
            Err(SuiError::from("Checkpoint blocked by pending certificates"))
        }
    }

    #[cfg(test)]
    pub fn update_new_checkpoint(
        &mut self,
        seq: CheckpointSequenceNumber,
        transactions: &CheckpointContents,
        effects_store: impl PendCertificateForExecution,
    ) -> Result<(), SuiError> {
        // Ensure we have processed all transactions contained in this checkpoint.
        self.check_checkpoint_transactions(transactions.iter(), &effects_store)?;

        let batch = self.tables.transactions_to_checkpoint.batch();
        self.update_new_checkpoint_inner(seq, transactions, batch)?;
        Ok(())
    }

    /// Add transactions associated with a new checkpoint in the structure, and
    /// updates all tables including unprocessed and extra transactions.
    fn update_new_checkpoint_inner(
        &mut self,
        seq: CheckpointSequenceNumber,
        transactions: &CheckpointContents,
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

        let transactions_with_seq = self
            .tables
            .extra_transactions
            .multi_get(transactions.iter())?;

        // Delete the extra transactions now used
        let batch = batch.delete_batch(
            &self.tables.extra_transactions,
            transactions_with_seq
                .iter()
                .zip(transactions.iter())
                .filter_map(|(opt, tx)| if opt.is_some() { Some(tx) } else { None }),
        )?;

        // Now write the checkpoint data to the database

        let transactions_to_checkpoint: Vec<_> = transactions.iter().map(|tx| (*tx, seq)).collect();

        let batch = batch.insert_batch(
            &self.tables.transactions_to_checkpoint,
            transactions_to_checkpoint,
        )?;

        let batch = batch.insert_batch(
            &self.tables.checkpoint_contents,
            std::iter::once((seq, transactions)),
        )?;

        // Write to the database.
        batch.write()?;

        Ok(())
    }

    /// Updates the store on the basis of transactions that have been processed. This is idempotent
    /// and nothing unsafe happens if it is called twice.
    fn update_processed_transactions(
        &mut self, // We take by &mut to prevent concurrent access.
        transactions: &[(TxSequenceNumber, ExecutionDigests)],
    ) -> Result<(), SuiError> {
        let batch = self.tables.extra_transactions.batch();
        let already_in_checkpoint = self
            .tables
            .transactions_to_checkpoint
            .multi_get(transactions.iter().map(|(_seq, digest)| *digest))?;
        let batch = batch.insert_batch(
            &self.tables.extra_transactions,
            transactions
                .iter()
                .zip(already_in_checkpoint.iter())
                .filter_map(|((seq, digest), cpk)| {
                    if cpk.is_some() {
                        None
                    } else {
                        Some((digest, seq))
                    }
                }),
        )?;

        // Write to the database.
        batch.write()?;

        debug!(
            "Transactions added to extra_transactions: {:?}",
            transactions
        );

        Ok(())
    }
}
