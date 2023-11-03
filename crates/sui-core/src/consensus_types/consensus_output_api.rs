// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::consensus_types::AuthorityIndex;
use fastcrypto::hash::Hash;
use mysticeti_core::types::BaseStatement;
use narwhal_types::{BatchAPI, CertificateAPI, HeaderAPI};
use std::fmt::Display;
use sui_types::messages_consensus::ConsensusTransaction;
use sui_types::transaction::CertifiedTransaction;

/// A list of tuples of:
/// (certificate origin authority index, all transactions corresponding to the certificate).
/// For each transaction, returns the serialized transaction and the deserialized transaction.
type ConsensusOutputTransactions<'a> = Vec<(AuthorityIndex, Vec<(&'a [u8], ConsensusTransaction)>)>;

pub(crate) trait ConsensusOutputAPI: Display {
    fn reputation_score_sorted_desc(&self) -> Option<Vec<(AuthorityIndex, u64)>>;
    fn leader_round(&self) -> u64;
    fn leader_author_index(&self) -> AuthorityIndex;

    /// Returns epoch UNIX timestamp in milliseconds
    fn commit_timestamp_ms(&self) -> u64;

    /// Returns a unique global index for each committed sub-dag.
    fn commit_sub_dag_index(&self) -> u64;

    /// Returns all transactions in the commit.
    fn transactions(&self) -> ConsensusOutputTransactions<'_>;
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

    fn transactions(&self) -> ConsensusOutputTransactions {
        self.sub_dag
            .certificates
            .iter()
            .zip(&self.batches)
            .map(|(cert, batches)| {
                assert_eq!(cert.header().payload().len(), batches.len());
                let transactions: Vec<(&[u8], ConsensusTransaction)> = batches.iter().flat_map(|batch| {
                    let digest = batch.digest();
                    assert!(cert.header().payload().contains_key(&digest));
                    batch.transactions().iter().map(move |serialized_transaction| {
                        let transaction = match bcs::from_bytes::<ConsensusTransaction>(
                            serialized_transaction,
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
                        (serialized_transaction.as_ref(), transaction)
                    })
                }).collect();
                (cert.origin().0, transactions)
            }).collect()
    }
}

impl ConsensusOutputAPI for mysticeti_core::consensus::linearizer::CommittedSubDag {
    fn reputation_score_sorted_desc(&self) -> Option<Vec<(AuthorityIndex, u64)>> {
        // TODO: Implement this in Mysticeti.
        None
    }

    fn leader_round(&self) -> u64 {
        self.anchor.round
    }

    fn leader_author_index(&self) -> AuthorityIndex {
        self.anchor.authority as AuthorityIndex
    }

    fn commit_timestamp_ms(&self) -> u64 {
        // TODO: Enforce ordered timestamp in Mysticeti
        0
    }

    fn commit_sub_dag_index(&self) -> u64 {
        // TODO: Mysticeti doesn't have this concept right now
        self.anchor.round
    }

    fn transactions(&self) -> ConsensusOutputTransactions {
        self.blocks
            .iter()
            .map(|block| {
                let round = block.round();
                let author = block.author() as AuthorityIndex;
                let transactions: Vec<_> = block
                    .statements()
                    .iter()
                    .enumerate()
                    .flat_map(|(idx, stmt)| match stmt {
                        BaseStatement::Share(tx) => {
                            let cert = bcs::from_bytes::<CertifiedTransaction>(tx.data());
                            match cert {
                                Ok(cert) => Some((
                                    tx.data(),
                                    ConsensusTransaction::new_mysticeti_certificate(
                                        round, idx, cert,
                                    ),
                                )),
                                Err(_) => None,
                            }
                        }
                        BaseStatement::Vote(..) | BaseStatement::VoteRange(_) => None,
                    })
                    .collect();
                (author, transactions)
            })
            .collect()
    }
}
