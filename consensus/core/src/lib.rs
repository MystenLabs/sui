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
mod commit_observer;
mod commit_syncer;
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
mod test_dag;
#[cfg(test)]
mod test_dag_builder;
#[cfg(test)]
mod test_dag_parser;

pub use authority_node::ConsensusAuthority;
pub use block::{BlockAPI, Round};
pub use commit::{CommitConsumer, CommitDigest, CommitIndex, CommitRef, CommittedSubDag};
pub use transaction::{TransactionClient, TransactionVerifier, ValidationError};

#[cfg(test)]
#[path = "tests/randomized_tests.rs"]
mod randomized_tests;
