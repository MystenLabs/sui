// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::Path,
    sync::Arc,
};

use rocksdb::Options;
use serde::{Deserialize, Serialize};
use sui_types::{
    base_types::{AuthorityName, TransactionDigest},
    batch::TxSequenceNumber,
    committee::Committee,
    error::SuiError,
    fp_ensure,
    messages_checkpoint::{
        AuthenticatedCheckpoint, AuthorityCheckpointInfo, CertifiedCheckpoint, CheckpointContents,
        CheckpointFragment, CheckpointRequest, CheckpointRequestType, CheckpointResponse,
        CheckpointSequenceNumber, CheckpointSummary, SignedCheckpoint, SignedCheckpointProposal,
    },
    waypoint::{GlobalCheckpoint, WaypointDiff},
};
use typed_store::{
    reopen,
    rocks::{open_cf_opts, DBBatch, DBMap},
    Map,
};

use super::StableSyncAuthoritySigner;
use arc_swap::ArcSwapOption;

#[cfg(test)]
#[path = "../unit_tests/checkpoint_tests.rs"]
mod checkpoint_tests;

#[derive(Clone, Serialize, Deserialize)]
pub struct CheckpointProposal {
    /// Name of the authority
    pub proposal: SignedCheckpointProposal,
    /// The transactions included in the proposal.
    /// TODO: only include a commitment by default.
    pub transactions: CheckpointContents,
}

impl CheckpointProposal {
    /// Create a proposal for a checkpoint at a partiular height
    /// This contains a sequence number, waypoint and a list of
    /// proposed trasnactions.
    /// TOOD: Add an identifier for the proposer, probably
    ///       an AuthorityName.
    pub fn new(proposal: SignedCheckpointProposal, transactions: CheckpointContents) -> Self {
        CheckpointProposal {
            proposal,
            transactions,
        }
    }

    /// Returns the sequence number of this proposal
    pub fn sequence_number(&self) -> &CheckpointSequenceNumber {
        &self.proposal.0.checkpoint.waypoint.sequence_number
    }

    // Iterate over all transactions
    pub fn transactions(&self) -> impl Iterator<Item = &TransactionDigest> {
        self.transactions.transactions.iter()
    }

    // Get the inner checkpoint
    pub fn checkpoint(&self) -> &CheckpointSummary {
        &self.proposal.0.checkpoint
    }

    // Get the authority name
    pub fn name(&self) -> &AuthorityName {
        &self.proposal.0.authority
    }

    /// Construct a Diff structure between this proposal and another
    /// proposal. A diff structure has to contain keys. The diff represents
    /// the elements that each proposal need to be augmented by to
    /// contain the same elements.
    ///
    /// TODO: down the line we can include other methods to get diffs
    /// line MerkleTrees or IBLT filters that do not require O(n) download
    /// of both proposals.
    pub fn diff_with(&self, other_proposal: &CheckpointProposal) -> CheckpointFragment {
        let all_elements = self
            .transactions()
            .chain(other_proposal.transactions.transactions.iter())
            .collect::<HashSet<_>>();

        let my_transactions = self.transactions().collect();
        let iter_missing_me = all_elements.difference(&my_transactions).map(|x| **x);
        let other_transactions = other_proposal.transactions().collect();
        let iter_missing_ot = all_elements.difference(&other_transactions).map(|x| **x);

        let diff = WaypointDiff::new(
            *self.name(),
            *self.checkpoint().waypoint.clone(),
            iter_missing_me,
            *other_proposal.name(),
            *other_proposal.checkpoint().waypoint.clone(),
            iter_missing_ot,
        );

        CheckpointFragment {
            proposer: self.proposal.clone(),
            other: other_proposal.proposal.clone(),
            diff,
            certs: BTreeMap::new(),
        }
    }
}

pub type DBLabel = usize;
const LOCALS: DBLabel = 0;

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct CheckpointLocals {
    // The next checkpoint number expected.
    pub next_checkpoint: CheckpointSequenceNumber,

    // The next transaction after what is included in the proposal
    pub proposal_next_transaction: Option<TxSequenceNumber>,

    // The next trasnaction sequence number of transactions processed
    pub next_transaction_sequence: TxSequenceNumber,

    // True if no more fragments are to be added.
    pub no_more_fragments: bool,

    // The current checkpoint proposal if any
    #[serde(skip)]
    pub current_proposal: Option<CheckpointProposal>,
}

pub trait ConsensusSender: Send + Sync + 'static {
    // Sned an item to the consensus
    fn send_to_consensus(&self, fragment: CheckpointFragment) -> Result<(), SuiError>;
}

pub struct CheckpointStore {
    // Fixed size, static, identity of the authority
    /// The name of this authority.
    pub name: AuthorityName,
    /// Committee of this Sui instance.
    pub committee: Committee,
    /// The signature key of the authority.
    pub secret: StableSyncAuthoritySigner,

    /// The list of all transactions that are checkpointed mapping to the checkpoint
    /// sequence number they were assigned to.
    pub transactions_to_checkpoint:
        DBMap<TransactionDigest, (CheckpointSequenceNumber, TxSequenceNumber)>,

    /// The mapping from checkpoint to transactions contained within the checkpoint.
    /// The second part of the key is the local sequence number if the transaction was
    /// processed or Max(u64) / 2 + offset if not. It allows the authority to store and serve
    /// checkpoints in a causal order that can be processed in order. (Note the set
    /// of transactions in the checkpoint is global but not the order.)
    pub checkpoint_contents: DBMap<(CheckpointSequenceNumber, TxSequenceNumber), TransactionDigest>,

    /// The set of pending transactions that were included in the last checkpoint
    /// but that this authority has not yet processed.
    pub unprocessed_transactions: DBMap<TransactionDigest, CheckpointSequenceNumber>,

    /// The set of transactions this authority has processed but have not yet been
    /// included in a checkpoint, and their sequence number in the local sequence
    /// of this authority.
    pub extra_transactions: DBMap<TransactionDigest, TxSequenceNumber>,

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
    pub fragments: DBMap<u64, CheckpointFragment>,

    /// The local sequence at which the proposal for the next checkpoint is created
    /// This is a sequence number containing all unprocessed trasnactions lower than
    /// this sequence number. At this point the unprocessed_transactions sequence
    /// should be empty. It is none if there is no active proposal. We also include here
    /// the proposal, although we could re-create it from the database.
    memory_locals: ArcSwapOption<CheckpointLocals>,

    /// A single entry table to store locals.
    pub locals: DBMap<DBLabel, CheckpointLocals>,

    // Consensus sender
    sender: Option<Box<dyn ConsensusSender>>,
}

impl CheckpointStore {
    // Manage persistent local variables

    /// Loads the locals from the store -- do this at init
    fn load_locals(&self) -> Result<CheckpointLocals, SuiError> {
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
            let checkpoint = locals.next_checkpoint;
            let transactions = self
                .extra_transactions
                .iter()
                .filter(|(_, seq)| seq < locals.proposal_next_transaction.as_ref().unwrap())
                .map(|(digest, _)| digest);

            let transactions = CheckpointContents::new(transactions);
            let proposal = SignedCheckpointProposal(SignedCheckpoint::new(
                checkpoint,
                self.name,
                &*self.secret,
                &transactions,
            ));

            let proposal_and_transactions = CheckpointProposal::new(proposal, transactions);
            locals.current_proposal = Some(proposal_and_transactions);
        }

        // No need to sync exclusive access
        self.memory_locals.store(Some(Arc::new(locals.clone())));
        Ok(locals)
    }

    /// Set the local variables in memory and store
    fn set_locals(
        &self,
        _previous: Arc<CheckpointLocals>,
        locals: CheckpointLocals,
    ) -> Result<(), SuiError> {
        self.locals.insert(&LOCALS, &locals)?;
        self.memory_locals.store(Some(Arc::new(locals)));
        Ok(())
    }

    /// Read the local variables
    pub fn get_locals(&self) -> Arc<CheckpointLocals> {
        self.memory_locals.load().clone().unwrap()
    }

    /// Set the consensus sender for this checkpointing function
    pub fn set_consensus(&mut self, sender: Box<dyn ConsensusSender>) -> Result<(), SuiError> {
        self.sender = Some(sender);
        Ok(())
    }

    /* TODO: Crash recovery logic.

    We need to check that the highest batch processed, is the same
    as within the authority store. If not we should also update the checkpoint
    store with all the batches since the last batch processed.

    */

    pub fn open<P: AsRef<Path>>(
        path: P,
        db_options: Option<Options>,
        name: AuthorityName,
        committee: Committee,
        secret: StableSyncAuthoritySigner,
    ) -> Result<CheckpointStore, SuiError> {
        let mut options = db_options.unwrap_or_default();

        /* The table cache is locked for updates and this determines the number
           of shards, ie 2^10. Increase in case of lock contentions.
        */
        let row_cache = rocksdb::Cache::new_lru_cache(1_000_000).expect("Cache is ok");
        options.set_row_cache(&row_cache);
        options.set_table_cache_num_shard_bits(10);
        options.set_compression_type(rocksdb::DBCompressionType::None);

        let mut point_lookup = options.clone();
        point_lookup.optimize_for_point_lookup(1024 * 1024);
        point_lookup.set_memtable_whole_key_filtering(true);

        let db = open_cf_opts(
            &path,
            Some(options.clone()),
            &[
                ("transactions_to_checkpoint", &point_lookup),
                ("checkpoint_contents", &options),
                ("unprocessed_transactions", &point_lookup),
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
            unprocessed_transactions,
            extra_transactions,
            checkpoints,
            local_fragments,
            fragments,
            locals,
        ) = reopen! (
            &db,
            "transactions_to_checkpoint";<TransactionDigest,(CheckpointSequenceNumber, TxSequenceNumber)>,
            "checkpoint_contents";<(CheckpointSequenceNumber,TxSequenceNumber),TransactionDigest>,
            "unprocessed_transactions";<TransactionDigest,CheckpointSequenceNumber>,
            "extra_transactions";<TransactionDigest,TxSequenceNumber>,
            "checkpoints";<CheckpointSequenceNumber, AuthenticatedCheckpoint>,
            "local_fragments";<AuthorityName, CheckpointFragment>,
            "fragments";<u64, CheckpointFragment>,
            "locals";<DBLabel, CheckpointLocals>
        );

        let check_point_db = CheckpointStore {
            name,
            committee,
            secret,
            transactions_to_checkpoint,
            checkpoint_contents,
            unprocessed_transactions,
            extra_transactions,
            checkpoints,
            local_fragments,
            fragments,
            memory_locals: ArcSwapOption::from(None),
            locals,
            sender: None,
        };

        // Initialize the locals
        check_point_db.load_locals()?;

        Ok(check_point_db)
    }

    // Define handlers for request

    pub fn handle_checkpoint_request(
        &self,
        request: &CheckpointRequest,
    ) -> Result<CheckpointResponse, SuiError> {
        match &request.request_type {
            CheckpointRequestType::LatestCheckpointProposal => self.handle_latest_proposal(request),
            CheckpointRequestType::PastCheckpoint(seq) => {
                self.handle_past_checkpoint(request, *seq)
            }
            CheckpointRequestType::SetCertificate(cert, opt_contents) => {
                self.handle_checkpoint_certificate(cert, opt_contents)
            }
            CheckpointRequestType::SetFragment(fragment) => self.handle_receive_fragment(fragment),
        }
    }

    pub fn handle_latest_proposal(
        &self,
        request: &CheckpointRequest,
    ) -> Result<CheckpointResponse, SuiError> {
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
            .map(|proposal| proposal.proposal.clone());

        // If requested include either the trasnactions in the latest checkpoint proposal
        // or the unprocessed transactions that block the generation of a proposal.
        let detail = if request.detail {
            latest_checkpoint_proposal
                .as_ref()
                // If the checkpoint exist return its contents.
                .map(|proposal| proposal.transactions.clone())
                // If the checkpoint does not exist return the unprocessed transactions
                .or_else(|| {
                    Some(CheckpointContents::new(
                        self.unprocessed_transactions.keys(),
                    ))
                })
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
        &self,
        request: &CheckpointRequest,
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
        } else if request.detail {
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
    /// in the signed checkpoint being signed, stored and the contents
    /// registered as processed or unprocessed.
    pub fn handle_internal_set_checkpoint(
        &self,
        checkpoint: CheckpointSummary,
        contents: &CheckpointContents,
    ) -> Result<(), SuiError> {
        let checkpoint_sequence_number = checkpoint.waypoint.sequence_number;

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

        // Sign the new checkpoint
        let signed_checkpoint = AuthenticatedCheckpoint::Signed(
            SignedCheckpoint::new_from_summary(checkpoint, self.name, &*self.secret),
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
            .delete_batch(&self.fragments, self.fragments.keys())?
            .delete_batch(&self.local_fragments, self.local_fragments.keys())?;

        // Update the transactions databases.
        let transactions: Vec<_> = contents.transactions.iter().cloned().collect();
        self.update_new_checkpoint_inner(checkpoint_sequence_number, &transactions, batch)?;

        // Try to set a fresh proposal, and ignore errors if this fails.
        let _ = self.set_proposal();

        Ok(())
    }

    /// Call this function internally to register the latest batch of
    /// transactions processed by this authority. The latest batch is
    /// stored to ensure upon crash recovery all batches are processed.
    pub fn handle_internal_batch(
        &self,
        next_sequence_number: TxSequenceNumber,
        transactions: &[(TxSequenceNumber, TransactionDigest)],
    ) -> Result<(), SuiError> {
        self.update_processed_transactions(transactions)?;

        // Updates the local sequence number of transactions processed.
        let locals = self.get_locals();
        let mut new_locals = locals.as_ref().clone();
        new_locals.next_transaction_sequence = next_sequence_number;
        self.set_locals(locals, new_locals)?;

        Ok(())
    }

    // TODO: this function should submit the received fragment to the
    //       consensus algorithm for sequencing. It should also do some
    //       basic checks to not submit redundant information to the
    //       consensus, as well as to check it is the right node to
    //       submit to consensus.
    pub fn handle_receive_fragment(
        &self,
        _fragment: &CheckpointFragment,
    ) -> Result<CheckpointResponse, SuiError> {
        // Check structure is correct and signatures verify
        _fragment.verify(&self.committee)?;

        // Does the fragment event suggest it is for the current round?
        let next_checkpoint_seq = self.next_checkpoint();
        fp_ensure!(
            *_fragment.proposer.0.checkpoint.sequence_number() == next_checkpoint_seq,
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
            _fragment.proposer.0.authority == self.name || _fragment.other.0.authority == self.name,
            SuiError::GenericAuthorityError {
                error: "Fragment does not involve this node".to_string(),
            }
        );

        // Save in the list of local fragments for this sequence.
        let other_name = if _fragment.proposer.0.authority == self.name {
            _fragment.other.0.authority
        } else {
            _fragment.proposer.0.authority
        };
        if !self.local_fragments.contains_key(&other_name)? {
            self.local_fragments.insert(&other_name, _fragment)?;
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
                sender.send_to_consensus(_fragment.clone())?;
            } else {
                return Err(SuiError::GenericAuthorityError {
                    error: "No consensus sender configured".to_string(),
                });
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

    /// This function should be called by the conseusus output, it is idempotent,
    /// and if called again with the same sequence number will do nothing. However,
    /// fragments should be provided in seq increasing order.
    pub fn handle_internal_fragment(
        &self,
        _seq: u64,
        _fragment: CheckpointFragment,
    ) -> Result<(), SuiError> {
        // Ensure we have not already processed this fragment.
        if let Some((last_seq, _)) = self.fragments.iter().skip_to_last().next() {
            if _seq <= last_seq {
                // We have already processed this fragment, just exit.
                return Ok(());
            }
        }

        // Check structure is correct and signatures verify
        _fragment.verify(&self.committee)?;

        // Save the new fragment in the DB
        let locals = self.get_locals();
        if !locals.no_more_fragments {
            self.fragments.insert(&_seq, &_fragment)?;
        }

        let fragments: Vec<_> = self.fragments.values().collect();

        // Run the reconstruction logic to build a checkpoint.
        let _potential_checkpoint = FragmentReconstruction::construct(
            self.next_checkpoint(),
            self.committee.clone(),
            &fragments,
        )?;

        if let Some(reconstructed) = _potential_checkpoint {
            if let Some(proposal) = &self.get_locals().current_proposal {
                // By definition the proposal and the new checkpoint must be in the
                // same sequence number of checkpoint.
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

                    // TODO: Take all certificates and schedule them for execution here.
                    //       We need to at the very least save the certificates that we
                    //       have not executed, to make sure they are available.

                    // Now create the new checkpoint and move all locals forward.
                    let summary = CheckpointSummary::new(self.next_checkpoint(), &contents);
                    return self.handle_internal_set_checkpoint(summary, &contents);
                }

                // NOTE:
                // We can also try to reconstruct on the basis of not just being in the
                // checkpoint but having a local fragment connecting to someone else in
                // the checkpoint. However this may be a rare thing.
            }

            // TODO: here define what we do if we do not have enough info
            //       to reconstruct the checkpoint. We can stroe the global waypoints
            //       and activelly wait for someone else to give us the data?
            let mut new_locals = locals.as_ref().clone();
            new_locals.no_more_fragments = true;
            self.set_locals(locals, new_locals)?;

            // A little argument about how the fragment -> checkpoint process is live
            //
            // A global checkpoint candidate must contain at least 2f+1 stake. And as
            // a result of this f+1 stake will be from honest nodes that by definition
            // must have submitted a proposal (because it is included!).
            // So f+1 honest authorities will be able to reconstruct and sign the
            // checkpoint. And all other authorities by asking all authorities will be
            // able to get f+1 signatures and construct a checkpoint certificate.

            Err(SuiError::GenericAuthorityError {
                error: "Missing info to construct known checkpoint.".to_string(),
            })
        } else {
            Ok(())
        }
    }

    /// Handles the submission of a full checkpoint externally, and stores
    /// the certificate. It may be used to upload a certificate, or induce
    /// the authority to catch up with the latest checkpoints.
    ///
    /// A cert without contents is only stored if we have already processed
    /// internally the checkpoint. A cert with contents is processed as if
    /// it came from the internal consensus.
    pub fn handle_checkpoint_certificate(
        &self,
        checkpoint: &CertifiedCheckpoint,
        contents: &Option<CheckpointContents>,
    ) -> Result<CheckpointResponse, SuiError> {
        // Get the record in our checkpoint database for this sequence number.
        let current = self
            .checkpoints
            .get(&checkpoint.checkpoint.waypoint.sequence_number)?;

        match &current {
            // If cert exists, do nothing (idempotent)
            Some(AuthenticatedCheckpoint::Certified(_current_cert)) => {}
            // If no such checkpoint is known, then return an error
            // NOTE: a checkpoint must first be confirmed internally before an external
            // certificate is registered.
            None => {
                if let &Some(contents) = &contents {
                    // Check and process contents
                    checkpoint.check_transactions(&self.committee, contents)?;
                    self.handle_internal_set_checkpoint(checkpoint.checkpoint.clone(), contents)?;
                    // Then insert it
                    self.checkpoints.insert(
                        &checkpoint.checkpoint.waypoint.sequence_number,
                        &AuthenticatedCheckpoint::Certified(checkpoint.clone()),
                    )?;
                } else {
                    return Err(SuiError::GenericAuthorityError {
                        error: "No checkpoint set at this sequence.".to_string(),
                    });
                }
            }
            // In this case we have an internal signed checkpoint so we propote it to a
            // full certificate.
            Some(AuthenticatedCheckpoint::Signed(_)) => {
                checkpoint.check_digest(&self.committee)?;
                self.checkpoints.insert(
                    &checkpoint.checkpoint.waypoint.sequence_number,
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
    pub fn next_checkpoint(&self) -> CheckpointSequenceNumber {
        self.get_locals().next_checkpoint
    }

    /// Returns the lowest checkpoint sequence number with unprocessed transactions
    /// if any, otherwise the next checkpoint (not seen).
    pub fn lowest_unprocessed_checkpoint(&self) -> CheckpointSequenceNumber {
        self.unprocessed_transactions
            .iter()
            .map(|(_, chk_seq)| chk_seq)
            .min()
            .unwrap_or_else(|| self.next_checkpoint())
    }

    /// Returns the next transactions sequence number expected.
    pub fn next_transaction_sequence_expected(&self) -> TxSequenceNumber {
        self.get_locals().next_transaction_sequence
    }

    // Helper write functions

    /// Set the next checkpoint proposal.
    fn set_proposal(&self) -> Result<CheckpointProposal, SuiError> {
        // Check that:
        // - there is no current proposal.
        // - there are no unprocessed transactions.
        // - there are some extra transactions to include.

        let locals = self.get_locals();

        if locals.current_proposal.is_some() {
            return Err(SuiError::GenericAuthorityError {
                error: "Proposal already set.".to_string(),
            });
        }

        if self.unprocessed_transactions.iter().count() > 0 {
            return Err(SuiError::GenericAuthorityError {
                error: "Cannot propose with unprocessed transactions from the previous checkpoint."
                    .to_string(),
            });
        }

        if self.extra_transactions.iter().count() == 0 {
            return Err(SuiError::GenericAuthorityError {
                error: "Cannot propose an empty set.".to_string(),
            });
        }

        // Include the sequence number of all extra transactions not already in a
        // checkpoint. And make a list of the transactions.
        let sequence_number = self.next_checkpoint();
        let next_local_tx_sequence = self.extra_transactions.values().max().unwrap() + 1;

        let transactions = CheckpointContents::new(self.extra_transactions.keys());
        let proposal = SignedCheckpointProposal(SignedCheckpoint::new(
            sequence_number,
            self.name,
            &*self.secret,
            &transactions,
        ));

        let proposal_and_transactions = CheckpointProposal::new(proposal, transactions);

        // Record the checkpoint in the locals
        let mut new_locals = locals.as_ref().clone();
        new_locals.current_proposal = Some(proposal_and_transactions.clone());
        new_locals.proposal_next_transaction = Some(next_local_tx_sequence);
        self.set_locals(locals, new_locals)?;

        Ok(proposal_and_transactions)
    }

    pub fn update_new_checkpoint(
        &self,
        seq: CheckpointSequenceNumber,
        transactions: &[TransactionDigest],
    ) -> Result<(), SuiError> {
        let batch = self.transactions_to_checkpoint.batch();
        self.update_new_checkpoint_inner(seq, transactions, batch)?;
        Ok(())
    }

    /// Add transactions associated with a new checkpoint in the structure, and
    /// updates all tables including unprocessed and extra transactions.
    fn update_new_checkpoint_inner(
        &self,
        seq: CheckpointSequenceNumber,
        transactions: &[TransactionDigest],
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

        // Update the unprocessed transactions
        let batch = batch.insert_batch(
            &self.unprocessed_transactions,
            transactions_with_seq
                .iter()
                .zip(new_transactions.iter())
                .filter_map(
                    |(opt, tx)| {
                        if opt.is_none() {
                            Some((tx, seq))
                        } else {
                            None
                        }
                    },
                ),
        )?;

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
        &self, // We take by &mut to prevent concurrent access.
        transactions: &[(TxSequenceNumber, TransactionDigest)],
    ) -> Result<CheckpointSequenceNumber, SuiError> {
        let in_checkpoint = self
            .transactions_to_checkpoint
            .multi_get(transactions.iter().map(|(_, tx)| tx))?;

        let batch = self.transactions_to_checkpoint.batch();

        // If the transactions were in a checkpoint but we had not processed them yet, then
        // we delete them from the unprocessed transaction set.
        let batch = batch.delete_batch(
            &self.unprocessed_transactions,
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
                ),
        )?;

        // Delete the entries with the old sequence numbers
        let batch = batch.delete_batch(
            &self.transactions_to_checkpoint,
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
                ),
        )?;

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
        // of `extra` trasnactions, that we should be activelly propagating to others.
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

        Ok(self.lowest_unprocessed_checkpoint())
    }
}

pub struct FragmentReconstruction {
    pub committee: Committee,
    pub global: GlobalCheckpoint<AuthorityName, TransactionDigest>,
}

impl FragmentReconstruction {
    pub fn construct(
        seq: u64,
        committee: Committee,
        fragments: &[CheckpointFragment],
    ) -> Result<Option<FragmentReconstruction>, SuiError> {
        // First extract the greatest connected component
        let mut links: HashMap<AuthorityName, HashSet<AuthorityName>> = HashMap::new();
        let mut link_fragment: HashMap<(AuthorityName, AuthorityName), _> = HashMap::new();

        // TODO Here: Check that any subsequence proposal from the same authority
        // is the same as any previous proposal seen to avoid equivocation. If a
        // contradiction is found exclude the authority from the checkpoint -- it
        // is faulty for sure.
        //
        // let _entities: HashMap<AuthorityName, SignedCheckpointProposal> = HashMap::new();

        // Insert each link both ways, to construct the graph.
        for fragment in fragments {
            let proposer = fragment.proposer.0.authority;
            let other = fragment.other.0.authority;
            links
                .entry(proposer)
                .or_insert_with(HashSet::new)
                .insert(other);
            links
                .entry(other)
                .or_insert_with(HashSet::new)
                .insert(proposer);

            // Make an index of the fragments
            link_fragment.entry((proposer, other)).or_insert(fragment);
            link_fragment.entry((other, proposer)).or_insert(fragment);
        }

        let mut candidates: HashSet<_> = links.keys().collect();

        // Loop back here!
        loop {
            let mut current_component = Vec::new();
            let mut add_set = HashSet::new();

            // This list is getting smaller with each pop, and when empty we
            // exit, therefore this loop will terminate with an error in case
            // we do not find a checkpoint-size component.
            if let Some(add_item) = candidates.iter().next() {
                // Take the next available node to create the next connected
                // component.
                add_set.insert(*add_item);
            } else {
                // If we run out of candidates with no checkpoint, there is no
                // checkpoint yet.
                return Ok(None);
            }

            // Extract the connected component starting at "add_item".
            while !add_set.is_empty() {
                let start_entity = *add_set.iter().next().unwrap();
                add_set.remove(start_entity);
                candidates.remove(start_entity);
                // Note this lists nodes in causal order of connection.
                current_component.push(start_entity);
                add_set.extend(
                    links[start_entity]
                        .iter()
                        .filter(|item| candidates.contains(*item)),
                );
            }

            // Measure the amount of stake in this component
            let total_weight: usize = current_component
                .iter()
                .map(|item| committee.weight(item))
                .sum();

            // The weight is too small for this component to be a checkpoint so we
            // skip and try to go make another component. (Or exit.)
            if total_weight < committee.quorum_threshold() {
                continue;
            }

            // Here we are dealing with a global checkpoint!
            // NOTE: Since a global checkpoint has 2f+1 stake there can only be one of them.
            let mut global = GlobalCheckpoint::new(seq);
            for first_item in current_component {
                for second_item in &links[first_item] {
                    // Rearrange so that the first item connects with the existing graph.
                    let mut fragment_diff =
                        link_fragment[&(*first_item, *second_item)].diff.clone();
                    if fragment_diff.first.key != *first_item {
                        fragment_diff = fragment_diff.swap();
                    }
                    // Insert into the graph.
                    let _ = global.insert(fragment_diff);
                }
            }

            // We have found a large enough component, so we now return it!
            return Ok(Some(FragmentReconstruction { global, committee }));
        }
    }
}
