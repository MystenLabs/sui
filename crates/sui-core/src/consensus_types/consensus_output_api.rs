// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::consensus_types::AuthorityIndex;
use fastcrypto::hash::Hash;
use narwhal_types::{BatchAPI, CertificateAPI, HeaderAPI};
use sui_types::messages_consensus::ConsensusTransaction;

/// A list of tuples of:
/// (certificate origin authority index, all transactions corresponding to the certificate).
/// For each transaction, returns the serialized transaction and the deserialized transaction.
type ConsensusOutputTransactions = Vec<(AuthorityIndex, Vec<(Vec<u8>, ConsensusTransaction)>)>;

const DIGEST_SIZE: usize = 32;

pub(crate) trait ConsensusOutputAPI: Hash<DIGEST_SIZE> {
    fn reputation_score_sorted_desc(&self) -> Option<Vec<(AuthorityIndex, u64)>>;
    fn leader_round(&self) -> u64;
    fn leader_author_index(&self) -> AuthorityIndex;

    /// Returns epoch UNIX timestamp in milliseconds
    fn commit_timestamp_ms(&self) -> u64;

    /// Returns a unique global index for each committed sub-dag.
    fn commit_sub_dag_index(&self) -> u64;

    /// Returns all transactions in the commit.
    fn into_transactions(self) -> ConsensusOutputTransactions;
}

impl ConsensusOutputAPI for narwhal_types::ConsensusOutput {
    fn reputation_score_sorted_desc(&self) -> Option<Vec<(AuthorityIndex, u64)>> {
        if !self.sub_dag.reputation_score.final_of_schedule {
            return None;
        }
        Some(
            self.sub_dag
                .reputation_score
                .authorities_by_score_desc()
                .into_iter()
                .map(|(id, score)| (id.0, score))
                .collect(),
        )
    }

    fn leader_round(&self) -> u64 {
        self.sub_dag.leader_round()
    }

    fn leader_author_index(&self) -> AuthorityIndex {
        self.sub_dag.leader.origin().0
    }

    fn commit_timestamp_ms(&self) -> u64 {
        self.sub_dag.commit_timestamp()
    }

    fn commit_sub_dag_index(&self) -> u64 {
        self.sub_dag.sub_dag_index
    }

    fn into_transactions(self) -> ConsensusOutputTransactions {
        self.sub_dag
            .certificates
            .iter()
            .zip(self.batches)
            .map(|(cert, batches)| {
                assert_eq!(cert.header().payload().len(), batches.len());
                let transactions: Vec<_> = batches.into_iter().flat_map(move |batch| {
                    let digest = batch.digest();
                    assert!(cert.header().payload().contains_key(&digest));
                    batch.into_transactions().into_iter().map(move |serialized_transaction| {
                        let transaction = match bcs::from_bytes::<ConsensusTransaction>(
                            &serialized_transaction,
                        ) {
                            Ok(transaction) => transaction,
                            Err(err) => {
                                // This should have been prevented by Narwhal batch verification.
                                panic!(
                                    "Unexpected malformed transaction (failed to deserialize): {}\nCertificate={:?} BatchDigest={:?} Transaction={:?}",
                                    err, cert, digest, serialized_transaction
                                );
                            }
                        };
                        (serialized_transaction, transaction)
                    })
                }).collect();
                (cert.origin().0, transactions)
            }).collect()
    }
}
