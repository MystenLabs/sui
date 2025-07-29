// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{sync::Arc, time::Duration};

use consensus_core::{
    CommitFinalizer, CommittedSubDag, Context, DagBuilder, DagState, Linearizer, MemStore,
    TransactionCertifier,
};
use criterion::{criterion_group, criterion_main, BatchSize, Criterion, Throughput};
use mysten_metrics::monitored_mpsc;
use parking_lot::RwLock;

// The fixture and helper functions are adapted from consensus/core/src/commit_finalizer.rs tests.
struct BenchFixture {
    context: Arc<Context>,
    dag_state: Arc<RwLock<DagState>>,
    _commit_sender: monitored_mpsc::UnboundedSender<CommittedSubDag>,
    _commit_receiver: monitored_mpsc::UnboundedReceiver<CommittedSubDag>,
    linearizer: Linearizer,
    transaction_certifier: TransactionCertifier,
    commit_finalizer: CommitFinalizer,

    workload_commits: Vec<CommittedSubDag>,
}

impl BenchFixture {
    fn new(num_authorities: usize) -> BenchFixture {
        let (context, _keys) = Context::new_with_test_options(num_authorities, false);
        let context: Arc<Context> = Arc::new(context);
        let dag_state = Arc::new(RwLock::new(DagState::new(
            context.clone(),
            Arc::new(MemStore::new()),
        )));
        let linearizer = Linearizer::new(context.clone(), dag_state.clone());
        let (blocks_sender, _blocks_receiver) =
            monitored_mpsc::unbounded_channel("consensus_block_output");
        let transaction_certifier =
            TransactionCertifier::new(context.clone(), dag_state.clone(), blocks_sender);
        let (commit_sender, _commit_receiver) =
            monitored_mpsc::unbounded_channel("consensus_commit_output");
        let commit_finalizer = CommitFinalizer::new(
            context.clone(),
            dag_state.clone(),
            transaction_certifier.clone(),
            commit_sender.clone(),
        );
        BenchFixture {
            context,
            dag_state,
            linearizer,
            transaction_certifier,
            _commit_sender: commit_sender,
            _commit_receiver,
            commit_finalizer,
            workload_commits: vec![],
        }
    }

    fn populate_commits(
        mut self,
        num_commits: usize,
        transactions_per_block: usize,
        rejected_transactions_pct: u8,
    ) -> Self {
        let highest_round = num_commits as u32 + 2;
        let mut dag_builder = DagBuilder::new(self.context.clone());
        dag_builder
            .layers(1..=highest_round)
            .num_transactions(transactions_per_block as u32)
            .rejected_transactions_pct(rejected_transactions_pct, None)
            .build()
            .persist_layers(self.dag_state.clone());
        self.transaction_certifier.add_voted_blocks(
            dag_builder
                .all_blocks()
                .iter()
                .map(|b| (b.clone(), vec![]))
                .collect(),
        );

        let leaders = dag_builder
            .leader_blocks(1..=num_commits as u32)
            .into_iter()
            .map(Option::unwrap)
            .collect::<Vec<_>>();

        let commits = self.linearizer.handle_commit(leaders);
        self.workload_commits.extend(commits);

        self
    }
}

fn process_commit_direct(c: &mut Criterion) {
    process_commits_with_parameters(
        c,
        /*measurement_time*/ Duration::from_secs(30),
        /*num_authorities*/ 25,
        /*num_commits_per_run*/ 100,
        /*transactions_per_block*/ 100,
        /*rejected_transactions_pct*/ 0,
    );
}

fn process_commit_indirect(c: &mut Criterion) {
    process_commits_with_parameters(
        c,
        /*measurement_time*/ Duration::from_secs(90),
        /*num_authorities*/ 25,
        /*num_commits_per_run*/ 100,
        /*transactions_per_block*/ 100,
        /*rejected_transactions_pct*/ 50,
    );
}

fn process_commits_with_parameters(
    c: &mut Criterion,
    measurement_time: Duration,
    num_authorities: usize,
    num_commits_per_run: usize,
    transactions_per_block: usize,
    rejected_transactions_pct: u8,
) {
    let mut direct = 0;
    let mut indirect = 0;
    let mut rejected = 0;

    let mut group = c.benchmark_group("CommitFinalizer");
    group
        .throughput(Throughput::Elements(num_commits_per_run as u64))
        .measurement_time(measurement_time)
        .bench_function("process_commit_indirect", |b| {
            b.iter_batched(
                || {
                    BenchFixture::new(num_authorities).populate_commits(
                        num_commits_per_run,
                        transactions_per_block,
                        rejected_transactions_pct,
                    )
                },
                |mut fixture| {
                    for sub_dag in fixture.workload_commits {
                        let index = sub_dag.commit_ref.index;
                        let results = fixture.commit_finalizer.process_commit(sub_dag);
                        for result in results {
                            if result.commit_ref.index == index {
                                direct += 1;
                            } else {
                                indirect += 1;
                            }
                            rejected += result
                                .rejected_transactions_by_block
                                .values()
                                .map(|txns| txns.len())
                                .sum::<usize>();
                        }
                    }
                },
                BatchSize::PerIteration,
            )
        });

    println!(
        "Direct commits: {direct}; Indirect commits: {indirect}; Rejected transactions: {rejected}",
    );
}

criterion_group!(
    commit_finalizer_benches,
    process_commit_direct,
    process_commit_indirect
);
criterion_main!(commit_finalizer_benches);
