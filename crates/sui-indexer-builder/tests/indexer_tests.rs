// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::indexer_test_utils::{InMemoryPersistent, NoopDataMapper, TestDatasource};
use prometheus::Registry;
use sui_indexer_builder::indexer_builder::{
    BackfillStrategy, IndexerBuilder, IndexerProgressStore,
};
use sui_indexer_builder::Task;

mod indexer_test_utils;

#[tokio::test]
async fn indexer_simple_backfill_task_test() {
    telemetry_subscribers::init_for_testing();
    let registry = Registry::new();
    mysten_metrics::init_metrics(&registry);

    let data = (0..=10u64).collect::<Vec<_>>();
    let datasource = TestDatasource { data: data.clone() };
    let persistent = InMemoryPersistent::new();
    let indexer = IndexerBuilder::new("test_indexer", datasource, NoopDataMapper).build(
        5,
        0,
        persistent.clone(),
    );

    indexer.start().await.unwrap();

    // it should have 2 task created for the indexer - a live task and a backfill task
    let tasks = persistent.tasks("test_indexer").await.unwrap();
    assert_eq!(2, tasks.len());
    // the tasks should be ordered by checkpoint number,
    // the first one will be the live task and second one will be the backfill
    assert_eq!(10, tasks.first().unwrap().checkpoint);
    assert_eq!(i64::MAX as u64, tasks.first().unwrap().target_checkpoint);
    assert_eq!(4, tasks.last().unwrap().checkpoint);
    assert_eq!(4, tasks.last().unwrap().target_checkpoint);

    // the data recorded in storage should be the same as the datasource
    let mut recorded_data = persistent.data.lock().await.clone();
    recorded_data.sort();
    assert_eq!(data, recorded_data);
}

#[tokio::test]
async fn indexer_partitioned_backfill_task_test() {
    telemetry_subscribers::init_for_testing();
    let registry = Registry::new();
    mysten_metrics::init_metrics(&registry);

    let data = (0..=50u64).collect::<Vec<_>>();
    let datasource = TestDatasource { data: data.clone() };
    let persistent = InMemoryPersistent::new();
    let indexer = IndexerBuilder::new("test_indexer", datasource, NoopDataMapper)
        .with_backfill_strategy(BackfillStrategy::Partitioned { task_size: 10 })
        .build(35, 0, persistent.clone());
    indexer.start().await.unwrap();

    // it should have 5 task created for the indexer - a live task and 4 backfill task
    let tasks = persistent.tasks("test_indexer").await.unwrap();
    assert_eq!(5, tasks.len());
    // the tasks should be ordered by checkpoint number,
    // the first one will be the live task and rest will be the backfills
    assert_eq!(50, tasks.first().unwrap().checkpoint);
    assert_eq!(i64::MAX as u64, tasks.first().unwrap().target_checkpoint);
    assert_eq!(34, tasks.get(1).unwrap().checkpoint);
    assert_eq!(34, tasks.get(1).unwrap().target_checkpoint);
    assert_eq!(29, tasks.get(2).unwrap().checkpoint);
    assert_eq!(29, tasks.get(2).unwrap().target_checkpoint);
    assert_eq!(19, tasks.get(3).unwrap().checkpoint);
    assert_eq!(19, tasks.get(3).unwrap().target_checkpoint);
    assert_eq!(9, tasks.get(4).unwrap().checkpoint);
    assert_eq!(9, tasks.get(4).unwrap().target_checkpoint);
    // the data recorded in storage should be the same as the datasource
    let mut recorded_data = persistent.data.lock().await.clone();
    recorded_data.sort();
    assert_eq!(data, recorded_data);
}

#[tokio::test]
async fn indexer_partitioned_task_with_data_already_in_db_test() {
    telemetry_subscribers::init_for_testing();
    let registry = Registry::new();
    mysten_metrics::init_metrics(&registry);

    let data = (0..=50u64).collect::<Vec<_>>();
    let datasource = TestDatasource { data: data.clone() };
    let persistent = InMemoryPersistent::new();
    persistent.data.lock().await.append(&mut (0..=30).collect());
    persistent.progress_store.lock().await.insert(
        "test_indexer - backfill - 1".to_string(),
        Task {
            task_name: "test_indexer - backfill - 1".to_string(),
            checkpoint: 30,
            target_checkpoint: 30,
            timestamp: 0,
        },
    );
    let indexer = IndexerBuilder::new("test_indexer", datasource, NoopDataMapper)
        .with_backfill_strategy(BackfillStrategy::Partitioned { task_size: 10 })
        .build(25, 0, persistent.clone());
    indexer.start().await.unwrap();

    // it should have 2 task created for the indexer, one existing task and one live task
    let tasks = persistent.tasks("test_indexer").await.unwrap();
    assert_eq!(2, tasks.len());
    // the first one will be the live task
    assert_eq!(50, tasks.first().unwrap().checkpoint);
    assert_eq!(i64::MAX as u64, tasks.first().unwrap().target_checkpoint);
    // the data recorded in storage should be the same as the datasource
    let mut recorded_data = persistent.data.lock().await.clone();
    recorded_data.sort();
    assert_eq!(data, recorded_data);
}

#[tokio::test]
async fn indexer_partitioned_task_with_data_already_in_db_test2() {
    telemetry_subscribers::init_for_testing();
    let registry = Registry::new();
    mysten_metrics::init_metrics(&registry);

    let data = (0..=50u64).collect::<Vec<_>>();
    let datasource = TestDatasource { data: data.clone() };
    let persistent = InMemoryPersistent::new();
    persistent.data.lock().await.append(&mut (0..=30).collect());
    persistent.progress_store.lock().await.insert(
        "test_indexer - backfill - 1".to_string(),
        Task {
            task_name: "test_indexer - backfill - 1".to_string(),
            checkpoint: 30,
            target_checkpoint: 30,
            timestamp: 0,
        },
    );
    let indexer = IndexerBuilder::new("test_indexer", datasource, NoopDataMapper)
        .with_backfill_strategy(BackfillStrategy::Partitioned { task_size: 10 })
        .build(35, 0, persistent.clone());
    indexer.start().await.unwrap();

    // it should have 3 task created for the indexer, existing task, a backfill task from cp 31 to cp 34, and a live task
    let tasks = persistent.tasks("test_indexer").await.unwrap();
    assert_eq!(3, tasks.len());
    // the tasks should be ordered by checkpoint number,
    // the first one will be the live task and rest will be the backfills
    assert_eq!(50, tasks.first().unwrap().checkpoint);
    assert_eq!(i64::MAX as u64, tasks.first().unwrap().target_checkpoint);
    assert_eq!(34, tasks.get(1).unwrap().checkpoint);
    assert_eq!(34, tasks.get(1).unwrap().target_checkpoint);
    assert_eq!(30, tasks.get(2).unwrap().checkpoint);
    assert_eq!(30, tasks.get(2).unwrap().target_checkpoint);
    // the data recorded in storage should be the same as the datasource
    let mut recorded_data = persistent.data.lock().await.clone();
    recorded_data.sort();
    assert_eq!(data, recorded_data);
}

#[tokio::test]
async fn resume_test() {
    telemetry_subscribers::init_for_testing();
    let registry = Registry::new();
    mysten_metrics::init_metrics(&registry);

    let data = (0..=50u64).collect::<Vec<_>>();
    let datasource = TestDatasource { data: data.clone() };
    let persistent = InMemoryPersistent::new();
    persistent.progress_store.lock().await.insert(
        "test_indexer - backfill - 30".to_string(),
        Task {
            task_name: "test_indexer - backfill - 30".to_string(),
            checkpoint: 10,
            target_checkpoint: 30,
            timestamp: 0,
        },
    );
    let indexer = IndexerBuilder::new("test_indexer", datasource, NoopDataMapper)
        .with_backfill_strategy(BackfillStrategy::Simple)
        .build(30, 0, persistent.clone());
    indexer.start().await.unwrap();

    // it should have 2 task created for the indexer, one existing task and one live task
    let tasks = persistent.tasks("test_indexer").await.unwrap();
    assert_eq!(2, tasks.len());
    // the first one will be the live task
    assert_eq!(50, tasks.first().unwrap().checkpoint);
    assert_eq!(i64::MAX as u64, tasks.first().unwrap().target_checkpoint);
    // the data recorded in storage should be the same as the datasource
    let mut recorded_data = persistent.data.lock().await.clone();
    recorded_data.sort();
    assert_eq!((10..=50u64).collect::<Vec<_>>(), recorded_data);
}
