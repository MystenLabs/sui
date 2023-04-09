// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[macro_use]
extern crate criterion;

use std::env;
use std::time::Duration;

use chrono::Utc;
use criterion::Criterion;
use tokio::runtime::Runtime;

use sui_indexer::models::checkpoints::Checkpoint;
use sui_indexer::models::objects::{NamedBcsBytes, Object as DBObject, ObjectStatus};
use sui_indexer::models::owners::OwnerType;
use sui_indexer::models::transactions::Transaction;
use sui_indexer::new_pg_connection_pool;
use sui_indexer::store::{
    IndexerStore, PgIndexerStore, TemporaryCheckpointStore, TransactionObjectChanges,
};
use sui_indexer::utils::reset_database;
use sui_json_rpc_types::CheckpointId;
use sui_types::base_types::{ObjectDigest, ObjectID, SequenceNumber, SuiAddress};
use sui_types::crypto::AggregateAuthoritySignature;
use sui_types::digests::TransactionDigest;
use sui_types::gas_coin::GasCoin;
use sui_types::messages::{TransactionData, TEST_ONLY_GAS_UNIT_FOR_TRANSFER};
use sui_types::messages_checkpoint::CheckpointDigest;
use sui_types::object::Object;

fn indexer_benchmark(c: &mut Criterion) {
    let pg_host = env::var("POSTGRES_HOST").unwrap_or_else(|_| "localhost".into());
    let pg_port = env::var("POSTGRES_PORT").unwrap_or_else(|_| "32770".into());
    let pw = env::var("POSTGRES_PASSWORD").unwrap_or_else(|_| "postgrespw".into());
    let db_url = format!("postgres://postgres:{pw}@{pg_host}:{pg_port}");

    let rt = Runtime::new().unwrap();
    let (mut checkpoints, store) = rt.block_on(async {
        let (blocking_cp, async_cp) = new_pg_connection_pool(&db_url).await.unwrap();
        reset_database(&mut blocking_cp.get().unwrap(), true).unwrap();
        let store = PgIndexerStore::new(async_cp, blocking_cp).await;

        let checkpoints = (0..150).map(create_checkpoint).collect::<Vec<_>>();
        (checkpoints, store)
    });

    c.bench_function("persist_checkpoint", |b| {
        b.iter(|| rt.block_on(store.persist_checkpoint(&checkpoints.pop().unwrap())))
    });

    let mut checkpoints = (20..100).cycle().map(CheckpointId::SequenceNumber);
    c.bench_function("get_checkpoint", |b| {
        b.to_async(Runtime::new().unwrap())
            .iter(|| store.get_checkpoint(checkpoints.next().unwrap()))
    });
}

fn create_checkpoint(sequence_number: i64) -> TemporaryCheckpointStore {
    TemporaryCheckpointStore {
        checkpoint: Checkpoint {
            sequence_number,
            checkpoint_digest: CheckpointDigest::random().base58_encode(),
            epoch: 0,
            transactions: vec![],
            previous_checkpoint_digest: Some(CheckpointDigest::random().base58_encode()),
            end_of_epoch: false,
            validator_signature: AggregateAuthoritySignature::default().to_string(),
            total_gas_cost: i64::MAX,
            total_computation_cost: i64::MAX,
            total_storage_cost: i64::MAX,
            total_storage_rebate: i64::MAX,
            total_transaction_blocks: 1000,
            total_transactions: 1000,
            network_total_transactions: 0,
            timestamp_ms: Utc::now().timestamp_millis(),
        },
        transactions: (1..1000)
            .map(|_| create_transaction(sequence_number))
            .collect(),
        events: vec![],
        objects_changes: vec![TransactionObjectChanges {
            changed_objects: (1..1000).map(|_| create_object(sequence_number)).collect(),
            deleted_objects: vec![],
        }],
        addresses: vec![],
        packages: vec![],
        input_objects: vec![],
        move_calls: vec![],
        recipients: vec![],
    }
}

fn create_transaction(sequence_number: i64) -> Transaction {
    let gas_price = 1000;
    let tx = TransactionData::new_pay_sui(
        SuiAddress::random_for_testing_only(),
        vec![],
        vec![SuiAddress::random_for_testing_only()],
        vec![100000],
        (
            ObjectID::random(),
            SequenceNumber::new(),
            ObjectDigest::random(),
        ),
        gas_price * TEST_ONLY_GAS_UNIT_FOR_TRANSFER,
        gas_price,
    )
    .unwrap();

    Transaction {
        id: None,
        transaction_digest: TransactionDigest::random().base58_encode(),
        sender: SuiAddress::random_for_testing_only().to_string(),
        recipients: vec![],
        checkpoint_sequence_number: Some(sequence_number),
        timestamp_ms: Some(Utc::now().timestamp_millis()),
        transaction_kind: "test".to_string(),
        transaction_count: 0,
        created: vec![],
        mutated: vec![],
        deleted: vec![],
        unwrapped: vec![],
        wrapped: vec![],
        move_calls: vec![],
        gas_object_id: ObjectID::random().to_string(),
        gas_object_sequence: 0,
        gas_object_digest: ObjectDigest::random().base58_encode(),
        gas_budget: 0,
        total_gas_cost: 0,
        computation_cost: 0,
        storage_cost: 0,
        storage_rebate: 0,
        non_refundable_storage_fee: 0,
        gas_price: 0,
        raw_transaction: bcs::to_bytes(&tx).unwrap(),
        transaction_content: serde_json::to_string(&tx).unwrap(),
        transaction_effects_content: "".to_string(),
        confirmed_local_execution: None,
    }
}

fn create_object(sequence_number: i64) -> DBObject {
    DBObject {
        epoch: 0,
        checkpoint: sequence_number,
        object_id: ObjectID::random().to_string(),
        version: 0,
        object_digest: ObjectDigest::random().to_string(),
        owner_type: OwnerType::AddressOwner,
        owner_address: Some(SuiAddress::random_for_testing_only().to_string()),
        initial_shared_version: None,
        previous_transaction: TransactionDigest::random().base58_encode(),
        object_type: GasCoin::type_().to_string(),
        object_status: ObjectStatus::Created,
        has_public_transfer: true,
        storage_rebate: 0,
        bcs: vec![NamedBcsBytes(
            "data".to_string(),
            Object::new_gas_for_testing()
                .data
                .try_as_move()
                .unwrap()
                .contents()
                .to_vec(),
        )],
    }
}
criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(50).measurement_time(Duration::from_secs(10));
    targets = indexer_benchmark
}
criterion_main!(benches);
