// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use sui_consistent_store::{Db, DbOptions};
use sui_rpc_store::RpcStoreSchema;
use sui_rpc_store::indexer::pruner::prune_history_cohort;
use sui_rpc_store::schema::object_version_by_checkpoint;
use sui_types::base_types::{ObjectID, SequenceNumber, SuiAddress};
use sui_types::digests::{ObjectDigest, TransactionDigest};
use sui_types::effects::TransactionEffects;
use sui_types::execution_status::ExecutionStatus;
use sui_types::gas::GasCostSummary;
use sui_types::object::Owner;

struct PruneInput {
    schema: RpcStoreSchema,
    db: Db,
    pruned_checkpoint: u64,
    effects: Vec<(u64, TransactionEffects)>,
    _dir: tempfile::TempDir,
}

fn benchmark_object_id(index: usize) -> ObjectID {
    let mut bytes = [0u8; ObjectID::LENGTH];
    bytes[ObjectID::LENGTH - 8..].copy_from_slice(&((index as u64 + 1).to_be_bytes()));
    ObjectID::new(bytes)
}

fn effects(modified_at_versions: Vec<(ObjectID, SequenceNumber)>) -> TransactionEffects {
    TransactionEffects::new_from_execution_v1(
        ExecutionStatus::Success,
        0,
        GasCostSummary::default(),
        modified_at_versions,
        vec![],
        TransactionDigest::ZERO,
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        (
            (ObjectID::ZERO, SequenceNumber::MIN, ObjectDigest::MIN),
            Owner::AddressOwner(SuiAddress::ZERO),
        ),
        None,
        vec![],
    )
}

fn setup_many_distinct(delete_count: usize) -> PruneInput {
    let dir = tempfile::tempdir().unwrap();
    let (db, schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();

    let mut batch = db.batch();
    let mut modified = Vec::with_capacity(delete_count);
    for i in 0..delete_count {
        let id = benchmark_object_id(i);
        let (k0, v0) = object_version_by_checkpoint::store(id, 0, SequenceNumber::from_u64(1));
        let (k1, v1) = object_version_by_checkpoint::store(id, 1, SequenceNumber::from_u64(2));
        batch
            .put(&schema.object_version_by_checkpoint, &k0, &v0)
            .unwrap();
        batch
            .put(&schema.object_version_by_checkpoint, &k1, &v1)
            .unwrap();
        modified.push((id, SequenceNumber::from_u64(1)));
    }
    batch.commit().unwrap();
    db.flush().unwrap();

    let effects = vec![(1u64, effects(modified))];
    PruneInput {
        schema,
        db,
        pruned_checkpoint: 1,
        effects,
        _dir: dir,
    }
}

fn setup_hot_objects(delete_count: usize) -> PruneInput {
    let dir = tempfile::tempdir().unwrap();
    let (db, schema) = Db::open::<RpcStoreSchema>(dir.path(), DbOptions::default()).unwrap();

    let depth = delete_count / 16;
    let ids: Vec<ObjectID> = (0..16).map(benchmark_object_id).collect();

    let mut batch = db.batch();
    for &id in &ids {
        for cp in 0..=depth as u64 {
            let (k, v) =
                object_version_by_checkpoint::store(id, cp, SequenceNumber::from_u64(cp + 1));
            batch
                .put(&schema.object_version_by_checkpoint, &k, &v)
                .unwrap();
        }
    }
    batch.commit().unwrap();
    db.flush().unwrap();

    let mut effects_vec = Vec::with_capacity(depth);
    for cp in 1..=depth as u64 {
        let modified: Vec<(ObjectID, SequenceNumber)> = ids
            .iter()
            .map(|&id| (id, SequenceNumber::from_u64(cp)))
            .collect();
        effects_vec.push((cp, effects(modified)));
    }

    PruneInput {
        schema,
        db,
        pruned_checkpoint: depth as u64,
        effects: effects_vec,
        _dir: dir,
    }
}

fn run_prune(input: PruneInput) -> PruneInput {
    prune_history_cohort(
        &input.db,
        &input.schema,
        input.pruned_checkpoint,
        0,
        &input.effects,
    )
    .unwrap();
    input
}

fn history_pruning_bench(c: &mut Criterion) {
    let mut group = c.benchmark_group("history-pruning");
    group.sample_size(10);

    for delete_count in [1_024, 8_192] {
        group.throughput(Throughput::Elements(delete_count as u64));

        group.bench_with_input(
            BenchmarkId::new("many-distinct", delete_count),
            &delete_count,
            |b, &count| {
                b.iter_batched(
                    || setup_many_distinct(count),
                    run_prune,
                    BatchSize::PerIteration,
                );
            },
        );

        group.bench_with_input(
            BenchmarkId::new("hot-objects", delete_count),
            &delete_count,
            |b, &count| {
                b.iter_batched(
                    || setup_hot_objects(count),
                    run_prune,
                    BatchSize::PerIteration,
                );
            },
        );
    }

    group.finish();
}

criterion_group!(benches, history_pruning_bench);
criterion_main!(benches);
