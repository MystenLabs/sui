// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod authority_node;
mod authority_service;
mod base_committer;
mod block;
mod block_manager;
mod block_verifier;
mod broadcaster;
mod commit;
mod commit_consumer;
mod commit_observer;
mod commit_syncer;
mod commit_vote_monitor;
mod context;
mod core;
mod core_thread;
mod dag_state;
mod error;
mod leader_schedule;
mod leader_scoring;
mod leader_scoring_strategy;
mod leader_timeout;
mod linearizer;
mod metrics;
mod network;
mod stake_aggregator;
mod storage;
mod subscriber;
mod synchronizer;
mod threshold_clock;
mod transaction;
mod universal_committer;

#[cfg(test)]
#[path = "tests/randomized_tests.rs"]
mod randomized_tests;
#[cfg(test)]
mod test_dag;
#[cfg(test)]
mod test_dag_builder;
#[cfg(test)]
mod test_dag_parser;

/// Exported consensus API.
pub use authority_node::ConsensusAuthority;
pub use block::{BlockAPI, Round};
pub use commit::{CommitDigest, CommitIndex, CommitRef, CommittedSubDag};
pub use commit_consumer::{CommitConsumer, CommitConsumerMonitor};
pub use transaction::{ClientError, TransactionClient, TransactionVerifier, ValidationError};

/// Exported API for testing.
pub use block::{TestBlock, Transaction, VerifiedBlock};
