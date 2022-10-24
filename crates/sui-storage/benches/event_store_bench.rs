// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[macro_use]
extern crate criterion;

use std::sync::Mutex;

use criterion::{BenchmarkId, Criterion, Throughput};
use lazy_static::lazy_static;
use tempfile::NamedTempFile;
use tokio::runtime::Builder;

use sui_storage::event_store::{sql::SqlEventStore, test_utils, EventStore};
use sui_types::{
    base_types::{SuiAddress, TransactionDigest},
    event::{EventEnvelope, TransferType},
};

async fn repeat_batch_insert(db: &SqlEventStore, events: &[EventEnvelope], batch_size: usize) {
    lazy_static! {
        static ref NEXT_SEQ: Mutex<u64> = Mutex::new(0);
    }

    let mut events: Vec<EventEnvelope> = events.to_vec();

    let mut seq: u64 = {
        let mut seq = NEXT_SEQ.lock().unwrap();
        let ret: u64 = *seq;
        let len: u64 = events.len().try_into().unwrap();
        *seq += len;
        ret
    };

    for chunk in events.chunks_mut(batch_size) {
        for e in chunk.iter_mut() {
            e.seq_num = seq;
            seq += 1;
        }
        db.add_tx_events(chunk)
            .await
            .expect("Inserts should not fail");
    }
}

/// Try to see what kind of throughput is possible when we ingest a list of events of varying size
fn bench_sqlite_ingestion_varying_batch_size(c: &mut Criterion) {
    let mut group = c.benchmark_group("SQLite ingestion varying batch size");

    let runtime = Builder::new_multi_thread()
        .enable_all()
        .thread_stack_size(32 * 1024 * 1024)
        .worker_threads(usize::min(num_cpus::get(), 24))
        .build()
        .unwrap();

    // Initialize SQLite event store
    let db_path = NamedTempFile::new().expect("Could not create temp file for SQlite db");
    let db = runtime
        .block_on(SqlEventStore::new_from_file(db_path.path()))
        .expect("Could not create new disk SqlEventStore");
    runtime
        .block_on(db.initialize())
        .expect("Could not initialize DB");

    // Create events
    let mut events = Vec::new();
    let sender = SuiAddress::random_for_testing_only();
    for n in 0..100 {
        let transfer_obj = test_utils::new_test_transfer_event(
            1_666_003 + n * 100,
            TransactionDigest::random(),
            0,
            n,
            TransferType::ToAddress,
            None,
            Some(sender),
            None,
        );
        events.push(transfer_obj);
    }

    group.throughput(Throughput::Elements(events.len() as u64));

    for batch_size in [1, 5, 10, 20, 50, 100].iter() {
        // Clear event store: TODO

        // This should be increasing with each batch_size, verifies that inserts are happening
        println!(
            "Event store event count: {}",
            runtime.block_on(db.total_event_count()).unwrap()
        );

        group.bench_with_input(
            BenchmarkId::new(
                "SqlEventStore.add_tx_events() with batch size: ",
                *batch_size,
            ),
            &db,
            |b, db| {
                // Note: each one of repeat_batch_insert inserts 100 events at a time
                b.to_async(&runtime)
                    .iter(|| repeat_batch_insert(db, &events, *batch_size as usize))
            },
        );
    }
}

criterion_group!(benches, bench_sqlite_ingestion_varying_batch_size);
criterion_main!(benches);
