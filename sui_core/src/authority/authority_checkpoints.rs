// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashSet, path::Path, sync::Arc};

use rocksdb::Options;
use serde::{Deserialize, Serialize};
use sui_types::{
    base_types::{AuthorityName, TransactionDigest},
    batch::{AuthorityBatch, TxSequenceNumber},
    committee::Committee,
    error::SuiError,
    messages_checkpoint::{
        AuthenticatedCheckpoint, AuthorityCheckpointInfo, CertifiedCheckpoint, CheckpointContents,
        CheckpointRequest, CheckpointRequestType, CheckpointResponse, CheckpointSequenceNumber,
        CheckpointSummary, SignedCheckpoint, SignedCheckpointProposal,
    },
    waypoint::WaypointDiff,
};
use typed_store::{
    reopen,
    rocks::{open_cf_opts, DBMap, TypedStoreError},
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
    pub fn diff_with(
        &self,
        other_proposal: &CheckpointProposal,
    ) -> WaypointDiff<AuthorityName, TransactionDigest> {
        let all_elements = self
            .transactions()
            .chain(other_proposal.transactions.transactions.iter())
            .collect::<HashSet<_>>();

        let my_transactions = self.transactions().collect();
        let iter_missing_me = all_elements.difference(&my_transactions).map(|x| **x);
        let other_transactions = other_proposal.transactions().collect();
        let iter_missing_ot = all_elements.difference(&other_transactions).map(|x| **x);

        WaypointDiff::new(
            *self.name(),
            *self.checkpoint().waypoint.clone(),
            iter_missing_me,
            *other_proposal.name(),
            *other_proposal.checkpoint().waypoint.clone(),
            iter_missing_ot,
        )
    }
}

pub type DBLabel = usize;
const LOCALS: DBLabel = 0;

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct CheckpointLocals {
    pub next_checkpoint_sequence_number: CheckpointSequenceNumber,
    pub next_tx_sequence_number_in_proposal: Option<TxSequenceNumber>,
    pub next_batch_transaction_sequence: TxSequenceNumber,
    pub next_uncommitted_transaction: TxSequenceNumber,

    #[serde(skip)]
    pub current_proposal: Option<CheckpointProposal>,
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

    /// The local sequence at which the proposal for the next checkpoint is created
    /// This is a sequence number containing all unprocessed trasnactions lower than
    /// this sequence number. At this point the unprocessed_transactions sequence
    /// should be empty. It is none if there is no active proposal. We also include here
    /// the proposal, although we could re-create it from the database.
    memory_locals: ArcSwapOption<CheckpointLocals>,

    /// A single entry table to store locals.
    pub locals: DBMap<DBLabel, CheckpointLocals>,
}

impl CheckpointStore {
    // Manage persistent local variables

    pub fn load_locals(&self) -> Result<CheckpointLocals, SuiError> {
        // Loads locals from disk, or inserts initial locals
        let locals = match self.locals.get(&LOCALS)? {
            Some(locals) => locals,
            None => {
                let locals = CheckpointLocals::default();
                self.locals.insert(&LOCALS, &locals)?;
                locals
            }
        };

        // No need to sync exclusive access
        self.memory_locals.store(Some(Arc::new(locals.clone())));
        Ok(locals)
    }

    pub fn get_locals(&self) -> Arc<CheckpointLocals> {
        self.memory_locals.load().clone().unwrap()
    }

    pub fn set_locals(
        &self,
        _previous: Arc<CheckpointLocals>,
        locals: CheckpointLocals,
    ) -> Result<(), SuiError> {
        self.locals.insert(&LOCALS, &locals)?;
        self.memory_locals.store(Some(Arc::new(locals)));
        Ok(())
    }

    /* TODO: Crash recovery logic.

    (1) When we open the checkpoint store, we need to check that the current
    highest checkpoint available at other nodes, is also the highest
    checkpoint recorded in the store. If not, then we should download the
    checkpoints from other authorities and include them in the store, as
    the consensus channel may not provide them.

    (2) We also need to check that the highest batch processed, is the same
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
           of shareds, ie 2^10. Increase in case of lock contentions.
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
            locals,
        ) = reopen! (
            &db,
            "transactions_to_checkpoint";<TransactionDigest,(CheckpointSequenceNumber, TxSequenceNumber)>,
            "checkpoint_contents";<(CheckpointSequenceNumber,TxSequenceNumber),TransactionDigest>,
            "unprocessed_transactions";<TransactionDigest,CheckpointSequenceNumber>,
            "extra_transactions";<TransactionDigest,TxSequenceNumber>,
            "checkpoints";<CheckpointSequenceNumber, AuthenticatedCheckpoint>,
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
            memory_locals: ArcSwapOption::from(None),
            locals,
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
            CheckpointRequestType::DEBUGSetCheckpoint(_box_checkpoint) => {
                self.debug_handle_set_checkpoint(&**_box_checkpoint)
            }
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
        // Process checkpoints once but allow idempotent processing
        if self
            .checkpoints
            .get(&checkpoint.waypoint.sequence_number)?
            .is_some()
        {
            return Ok(());
        }

        // Update the transactions databases.
        let transactions: Vec<_> = contents.transactions.iter().cloned().collect();
        self.update_new_checkpoint(checkpoint.waypoint.sequence_number, &transactions)?;

        // Sign the new checkpoint
        let checkpoint_sequence_number = checkpoint.waypoint.sequence_number;
        let signed_checkpoint = AuthenticatedCheckpoint::Signed(
            SignedCheckpoint::new_from_summary(checkpoint, self.name, &*self.secret),
        );

        // Last store the actual checkpoints.
        self.checkpoints
            .insert(&checkpoint_sequence_number, &signed_checkpoint)?;

        // Clean up our proposal if any
        let locals = self.get_locals();
        if locals
            .current_proposal
            .as_ref()
            .map(|v| *v.sequence_number())
            .unwrap_or(0)
            <= checkpoint_sequence_number
        {
            let mut new_locals = locals.as_ref().clone();
            new_locals.current_proposal = None;
            new_locals.next_tx_sequence_number_in_proposal = None;
            self.set_locals(locals, new_locals)?;
        }

        // Try to set a fresh proposal, and ignore errors if this fails.
        let _ = self.set_proposal();

        Ok(())
    }

    /// Call this function internally to register the latest batch of
    /// transactions processed by this authority. The latest batch is
    /// stored to ensure upon crash recovery all batches are processed.
    pub fn handle_internal_batch(
        &self,
        _batch: &AuthorityBatch,
        transactions: &[(TxSequenceNumber, TransactionDigest)],
    ) -> Result<(), SuiError> {
        self.update_processed_transactions(transactions)?;
        // TODO: Store the batch or at least its sequence number here.
        Ok(())
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
        // Check the certificate is valid

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
            _ => {
                checkpoint.check_digest(&self.committee)?;
                self.checkpoints.insert(
                    &checkpoint.checkpoint.waypoint.sequence_number,
                    &AuthenticatedCheckpoint::Certified(checkpoint.clone()),
                )?;
            }
        };

        Ok(CheckpointResponse {
            info: AuthorityCheckpointInfo::Success,
            detail: None,
        })
    }

    /// NOTE: this is a hack to accept the checkpoint proposed by the first node
    /// in the committee as the checkpoint that all will accept. This is a stop gap
    /// until we have a proper consensus integrated in a short while. After that we
    /// will use this consensus to get a checkpoint to be the union of 2f+1 proposals.
    pub fn debug_handle_set_checkpoint(
        &self,
        _contents: &(SignedCheckpointProposal, CheckpointContents),
    ) -> Result<CheckpointResponse, SuiError> {
        let checkpoint = &_contents.0;
        let trasnactions = &_contents.1;

        // Check it is correct
        checkpoint.0.check_transactions(trasnactions)?;
        // Check it is from the special 'first' authority
        let max_authority_name = self.committee.voting_rights.keys().max().unwrap();
        if *max_authority_name != checkpoint.0.authority {
            return Err(SuiError::GenericAuthorityError {
                error: "DEBUG FUNCTIONALITY: Incorrect master authority.".to_string(),
            });
        }

        // Call the otherwise internal code to update the checkpoint.
        self.handle_internal_set_checkpoint(checkpoint.0.checkpoint.clone(), trasnactions)?;

        // If no error so far repond with success.
        Ok(CheckpointResponse {
            info: AuthorityCheckpointInfo::Success,
            detail: None,
        })
    }

    // Helper functions

    pub fn next_local_tx_number(&self) -> TxSequenceNumber {
        self.extra_transactions
            .values()
            .max()
            .map(|v| v + 1)
            .unwrap_or(0)
    }

    /// Set the next checkpoint proposal.
    pub fn set_proposal(&self) -> Result<CheckpointProposal, SuiError> {
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
        let sequence_number = self.next_checkpoint_sequence();
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
        new_locals.next_tx_sequence_number_in_proposal = Some(next_local_tx_sequence);
        self.set_locals(locals, new_locals)?;

        Ok(proposal_and_transactions)
    }

    /// Return the seq number of the last checkpoint we have recorded.
    pub fn next_checkpoint_sequence(&self) -> CheckpointSequenceNumber {
        self.checkpoint_contents
            .iter()
            .skip_to_last()
            .next()
            .map(|((seq, _), _)| seq + 1)
            .unwrap_or_else(|| 0)
    }

    /// Returns the lowest checkpoint sequence number with unprocessed transactions
    /// if any, otherwise the next checkpoint (not seen).
    pub fn lowest_unprocessed_sequence(&self) -> CheckpointSequenceNumber {
        self.unprocessed_transactions
            .iter()
            .map(|(_, chk_seq)| chk_seq)
            .min()
            .unwrap_or_else(|| self.next_checkpoint_sequence())
    }

    /// Add transactions associated with a new checkpoint in the structure, and
    /// updates all tables including unprocessed and extra transactions.
    pub fn update_new_checkpoint(
        &self,
        seq: CheckpointSequenceNumber,
        transactions: &[TransactionDigest],
    ) -> Result<(), SuiError> {
        // Check that this checkpoint seq is new, and directly follows the last
        // highest checkpoint seen. First checkpoint is always zero.
        let expected_seq = self.next_checkpoint_sequence();

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

        let batch = self.transactions_to_checkpoint.batch();

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

        Ok(())
    }

    /// Updates the store on the basis of transactions that have been processed. This is idempotent
    /// and nothing unsafe happens if it is called twice. Returns the lowest checkpoint number with
    /// unprocessed transactions (this is the low watermark).
    pub fn update_processed_transactions(
        &self, // We take by &mut to prevent concurrent access.
        transactions: &[(TxSequenceNumber, TransactionDigest)],
    ) -> Result<CheckpointSequenceNumber, TypedStoreError> {
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

        Ok(self.lowest_unprocessed_sequence())
    }
}
