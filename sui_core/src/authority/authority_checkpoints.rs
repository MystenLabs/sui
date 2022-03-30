// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use rocksdb::Options;
use sui_types::{base_types::TransactionDigest, batch::TxSequenceNumber, error::SuiError};
use typed_store::{
    reopen,
    rocks::{DBMap, TypedStoreError, open_cf_opts},
    Map,
};

pub type CheckpointSequenceNumber = u64;

pub struct CheckpointStore {
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
}

impl CheckpointStore {
    pub fn open<P: AsRef<Path>>(path: P, db_options: Option<Options>) -> CheckpointStore {
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

        let transform = rocksdb::SliceTransform::create("bytes_8_to_16", |key| &key[8..16], None);
        point_lookup.set_prefix_extractor(transform);
        point_lookup.set_memtable_prefix_bloom_ratio(0.2);

        let db = open_cf_opts(
            &path,
            Some(options.clone()),
            &[
                ("transactions_to_checkpoint", &point_lookup),
                ("checkpoint_contents", &options),
                ("unprocessed_transactions", &point_lookup),
                ("extra_transactions", &point_lookup),
            ],
        )
        .expect("Cannot open DB.");

        let (
            transactions_to_checkpoint,
            checkpoint_contents,
            unprocessed_transactions,
            extra_transactions,
        ) = reopen! (
            &db,
            "transactions_to_checkpoint";<TransactionDigest,(CheckpointSequenceNumber, TxSequenceNumber)>,
            "checkpoint_contents";<(CheckpointSequenceNumber,TxSequenceNumber),TransactionDigest>,
            "unprocessed_transactions";<TransactionDigest,CheckpointSequenceNumber>,
            "extra_transactions";<TransactionDigest,TxSequenceNumber>
        );
        CheckpointStore {
            transactions_to_checkpoint,
            checkpoint_contents,
            unprocessed_transactions,
            extra_transactions,
        }
    }

    /// Return the seq number of the last checkpoint we have recorded.
    pub fn next_checkpoint_sequence(&self) -> CheckpointSequenceNumber {
        self.checkpoint_contents
            .iter()
            .last()
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
        &mut self,
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
        &mut self, // We take by &mut to prevent concurrent access.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authority::authority_tests::max_files_authority_tests;
    use std::{env, fs};
    use sui_types::base_types::ObjectID;

    #[test]
    fn make_checkpoint_db() {
        let dir = env::temp_dir();
        let path = dir.join(format!("SC_{:?}", ObjectID::random()));
        fs::create_dir(&path).unwrap();

        // Create an authority
        let mut opts = rocksdb::Options::default();
        opts.set_max_open_files(max_files_authority_tests());

        let mut cps = CheckpointStore::open(path, Some(opts));

        let t1 = TransactionDigest::random();
        let t2 = TransactionDigest::random();
        let t3 = TransactionDigest::random();
        let t4 = TransactionDigest::random();
        let t5 = TransactionDigest::random();
        let t6 = TransactionDigest::random();

        cps.update_processed_transactions(&[(1, t1), (2, t2), (3, t3)])
            .unwrap();
        assert!(cps.checkpoint_contents.iter().count() == 0);
        assert!(cps.extra_transactions.iter().count() == 3);
        assert!(cps.unprocessed_transactions.iter().count() == 0);

        assert!(cps.next_checkpoint_sequence() == 0);

        cps.update_new_checkpoint(0, &[t1, t2, t4, t5]).unwrap();
        assert!(cps.checkpoint_contents.iter().count() == 4);
        assert_eq!(cps.extra_transactions.iter().count(), 1);
        assert!(cps.unprocessed_transactions.iter().count() == 2);

        assert_eq!(cps.lowest_unprocessed_sequence(), 0);

        let (_cp_seq, tx_seq) = cps.transactions_to_checkpoint.get(&t4).unwrap().unwrap();
        assert!(tx_seq >= u64::MAX / 2);

        assert!(cps.next_checkpoint_sequence() == 1);

        cps.update_processed_transactions(&[(4, t4), (5, t5), (6, t6)])
            .unwrap();
        assert!(cps.checkpoint_contents.iter().count() == 4);
        assert_eq!(cps.extra_transactions.iter().count(), 2); // t3 & t6
        assert!(cps.unprocessed_transactions.iter().count() == 0);

        assert_eq!(cps.lowest_unprocessed_sequence(), 1);

        let (_cp_seq, tx_seq) = cps.transactions_to_checkpoint.get(&t4).unwrap().unwrap();
        assert_eq!(tx_seq, 4);
    }
}
