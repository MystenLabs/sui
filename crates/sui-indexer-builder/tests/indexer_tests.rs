// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::indexer_test_utils::{InMemoryPersistent, NoopDataMapper, TestDatasource};
use prometheus::{
    register_int_counter_vec_with_registry, register_int_gauge_vec_with_registry, IntCounterVec,
    IntGaugeVec, Registry,
};
use sui_indexer_builder::indexer_builder::{BackfillStrategy, IndexerBuilder};
use sui_indexer_builder::{Task, LIVE_TASK_TARGET_CHECKPOINT};

mod indexer_test_utils;

#[tokio::test]
async fn indexer_simple_backfill_task_test() {
    telemetry_subscribers::init_for_testing();
    let registry = Registry::new();
    mysten_metrics::init_metrics(&registry);

    let data = (0..=10u64).collect::<Vec<_>>();
    let datasource = TestDatasource {
        data: data.clone(),
        live_task_starting_checkpoint: 5,
        genesis_checkpoint: 0,
        gauge_metric: new_gauge_vec(&registry, "foo"),
        counter_metric: new_counter_vec(&registry),
        inflight_live_tasks: new_gauge_vec(&registry, "bar"),
    };
    let persistent = InMemoryPersistent::new();
    let mut indexer = IndexerBuilder::new(
        "test_indexer",
        datasource,
        NoopDataMapper,
        persistent.clone(),
    )
    .build();
    indexer.test_only_update_tasks().await.unwrap();
    let tasks = indexer
        .test_only_storage()
        .get_all_tasks("test_indexer")
        .await
        .unwrap();
    assert_ranges(&tasks, vec![(5, i64::MAX as u64), (0, 4)]);
    indexer.start().await.unwrap();

    // it should have 2 task created for the indexer - a live task and a backfill task
    let tasks = persistent.get_all_tasks("test_indexer").await.unwrap();
    println!("{:?}", tasks);
    assert_ranges(&tasks, vec![(10, i64::MAX as u64), (4, 4)]);
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
    let datasource = TestDatasource {
        data: data.clone(),
        live_task_starting_checkpoint: 35,
        genesis_checkpoint: 0,
        gauge_metric: new_gauge_vec(&registry, "foo"),
        counter_metric: new_counter_vec(&registry),
        inflight_live_tasks: new_gauge_vec(&registry, "bar"),
    };
    let persistent = InMemoryPersistent::new();
    let mut indexer = IndexerBuilder::new(
        "test_indexer",
        datasource,
        NoopDataMapper,
        persistent.clone(),
    )
    .with_backfill_strategy(BackfillStrategy::Partitioned { task_size: 10 })
    .build();
    indexer.test_only_update_tasks().await.unwrap();
    let tasks = indexer
        .test_only_storage()
        .get_all_tasks("test_indexer")
        .await
        .unwrap();
    assert_ranges(
        &tasks,
        vec![(35, i64::MAX as u64), (30, 34), (20, 29), (10, 19), (0, 9)],
    );
    indexer.start().await.unwrap();

    // it should have 5 task created for the indexer - a live task and 4 backfill task
    let tasks = persistent.get_all_tasks("test_indexer").await.unwrap();
    assert_ranges(
        &tasks,
        vec![(50, i64::MAX as u64), (34, 34), (29, 29), (19, 19), (9, 9)],
    );
    // the data recorded in storage should be the same as the datasource
    let mut recorded_data = persistent.data.lock().await.clone();
    recorded_data.sort();
    assert_eq!(data, recorded_data);
}

#[tokio::test]
async fn indexer_partitioned_task_with_data_already_in_db_test1() {
    telemetry_subscribers::init_for_testing();
    let registry = Registry::new();
    mysten_metrics::init_metrics(&registry);

    let data = (0..=50u64).collect::<Vec<_>>();
    let datasource = TestDatasource {
        data: data.clone(),
        live_task_starting_checkpoint: 31,
        genesis_checkpoint: 0,
        gauge_metric: new_gauge_vec(&registry, "foo"),
        counter_metric: new_counter_vec(&registry),
        inflight_live_tasks: new_gauge_vec(&registry, "bar"),
    };
    let persistent = InMemoryPersistent::new();
    persistent.data.lock().await.append(&mut (0..=30).collect());
    persistent.progress_store.lock().await.insert(
        "test_indexer - backfill - 1".to_string(),
        Task {
            task_name: "test_indexer - backfill - 1".to_string(),
            start_checkpoint: 30,
            target_checkpoint: 30,
            timestamp: 0,
            is_live_task: false,
        },
    );
    let mut indexer = IndexerBuilder::new(
        "test_indexer",
        datasource,
        NoopDataMapper,
        persistent.clone(),
    )
    .with_backfill_strategy(BackfillStrategy::Partitioned { task_size: 10 })
    .build();
    indexer.test_only_update_tasks().await.unwrap();
    let tasks = indexer
        .test_only_storage()
        .get_all_tasks("test_indexer")
        .await
        .unwrap();
    assert_ranges(&tasks, vec![(31, i64::MAX as u64), (30, 30)]);
    indexer.start().await.unwrap();

    // it should have 2 task created for the indexer, one existing task and one live task
    let tasks = persistent.get_all_tasks("test_indexer").await.unwrap();
    assert_ranges(&tasks, vec![(50, i64::MAX as u64), (30, 30)]);
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
    let datasource = TestDatasource {
        data: data.clone(),
        live_task_starting_checkpoint: 35,
        genesis_checkpoint: 0,
        gauge_metric: new_gauge_vec(&registry, "foo"),
        counter_metric: new_counter_vec(&registry),
        inflight_live_tasks: new_gauge_vec(&registry, "bar"),
    };
    let persistent = InMemoryPersistent::new();
    persistent.data.lock().await.append(&mut (0..=30).collect());
    persistent.progress_store.lock().await.insert(
        "test_indexer - backfill - 1".to_string(),
        Task {
            task_name: "test_indexer - backfill - 1".to_string(),
            start_checkpoint: 30,
            target_checkpoint: 30,
            timestamp: 0,
            is_live_task: false,
        },
    );
    let mut indexer = IndexerBuilder::new(
        "test_indexer",
        datasource,
        NoopDataMapper,
        persistent.clone(),
    )
    .with_backfill_strategy(BackfillStrategy::Partitioned { task_size: 10 })
    .build();
    indexer.test_only_update_tasks().await.unwrap();
    let tasks = indexer
        .test_only_storage()
        .get_all_tasks("test_indexer")
        .await
        .unwrap();
    assert_ranges(&tasks, vec![(35, i64::MAX as u64), (31, 34), (30, 30)]);
    indexer.start().await.unwrap();

    // it should have 3 tasks created for the indexer, existing task, a backfill task from cp 31 to cp 34, and a live task
    let tasks = persistent.get_all_tasks("test_indexer").await.unwrap();
    assert_ranges(&tasks, vec![(50, i64::MAX as u64), (34, 34), (30, 30)]);
    // the data recorded in storage should be the same as the datasource
    let mut recorded_data = persistent.data.lock().await.clone();
    recorded_data.sort();
    assert_eq!(data, recorded_data);
}

// `live_task_from_checkpoint` is smaller than the largest checkpoint in DB.
// The live task should start from `live_task_from_checkpoint`.
#[tokio::test]
async fn indexer_partitioned_task_with_data_already_in_db_test3() {
    telemetry_subscribers::init_for_testing();
    let registry = Registry::new();
    mysten_metrics::init_metrics(&registry);

    let data = (0..=50u64).collect::<Vec<_>>();
    let datasource = TestDatasource {
        data: data.clone(),
        live_task_starting_checkpoint: 28,
        genesis_checkpoint: 0,
        gauge_metric: new_gauge_vec(&registry, "foo"),
        counter_metric: new_counter_vec(&registry),
        inflight_live_tasks: new_gauge_vec(&registry, "bar"),
    };
    let persistent = InMemoryPersistent::new();
    persistent.progress_store.lock().await.insert(
        "test_indexer - backfill - 20:30".to_string(),
        Task {
            task_name: "test_indexer - backfill - 20:30".to_string(),
            start_checkpoint: 30,
            target_checkpoint: 30,
            timestamp: 0,
            is_live_task: false,
        },
    );
    persistent.progress_store.lock().await.insert(
        "test_indexer - backfill - 10:19".to_string(),
        Task {
            task_name: "test_indexer - backfill - 10:19".to_string(),
            start_checkpoint: 10,
            target_checkpoint: 19,
            timestamp: 0,
            is_live_task: false,
        },
    );
    let mut indexer = IndexerBuilder::new(
        "test_indexer",
        datasource,
        NoopDataMapper,
        persistent.clone(),
    )
    .with_backfill_strategy(BackfillStrategy::Partitioned { task_size: 10 })
    .build();
    indexer.test_only_update_tasks().await.unwrap();
    let tasks = indexer
        .test_only_storage()
        .get_all_tasks("test_indexer")
        .await
        .unwrap();
    assert_ranges(&tasks, vec![(30, 30), (28, i64::MAX as u64), (10, 19)]);
    indexer.start().await.unwrap();

    let tasks = persistent.get_all_tasks("test_indexer").await.unwrap();
    assert_ranges(&tasks, vec![(50, i64::MAX as u64), (30, 30), (19, 19)]);
}

// `live_task_from_checkpoint` is larger than the largest checkpoint in DB.
// The live task should start from `live_task_from_checkpoint`.
#[tokio::test]
async fn indexer_partitioned_task_with_data_already_in_db_test4() {
    telemetry_subscribers::init_for_testing();
    let registry = Registry::new();
    mysten_metrics::init_metrics(&registry);

    let data = (0..=50u64).collect::<Vec<_>>();
    let datasource = TestDatasource {
        data: data.clone(),
        live_task_starting_checkpoint: 35,
        genesis_checkpoint: 0,
        gauge_metric: new_gauge_vec(&registry, "foo"),
        counter_metric: new_counter_vec(&registry),
        inflight_live_tasks: new_gauge_vec(&registry, "bar"),
    };
    let persistent = InMemoryPersistent::new();
    persistent.progress_store.lock().await.insert(
        "test_indexer - backfill - 20:30".to_string(),
        Task {
            task_name: "test_indexer - backfill - 20:30".to_string(),
            start_checkpoint: 30,
            target_checkpoint: 30,
            timestamp: 0,
            is_live_task: false,
        },
    );
    persistent.progress_store.lock().await.insert(
        "test_indexer - backfill - 10:19".to_string(),
        Task {
            task_name: "test_indexer - backfill - 10:19".to_string(),
            start_checkpoint: 10,
            target_checkpoint: 19,
            timestamp: 0,
            is_live_task: false,
        },
    );
    let mut indexer = IndexerBuilder::new(
        "test_indexer",
        datasource,
        NoopDataMapper,
        persistent.clone(),
    )
    .with_backfill_strategy(BackfillStrategy::Partitioned { task_size: 10 })
    .build();
    indexer.test_only_update_tasks().await.unwrap();
    let tasks = indexer
        .test_only_storage()
        .get_all_tasks("test_indexer")
        .await
        .unwrap();
    assert_ranges(
        &tasks,
        vec![(35, i64::MAX as u64), (31, 34), (30, 30), (10, 19)],
    );
    indexer.start().await.unwrap();

    let tasks = persistent.get_all_tasks("test_indexer").await.unwrap();
    assert_ranges(
        &tasks,
        vec![(50, i64::MAX as u64), (34, 34), (30, 30), (19, 19)],
    );
}

#[tokio::test]
async fn indexer_with_existing_live_task1() {
    telemetry_subscribers::init_for_testing();
    let registry = Registry::new();
    mysten_metrics::init_metrics(&registry);

    let data = (0..=50u64).collect::<Vec<_>>();
    let datasource = TestDatasource {
        data: data.clone(),
        live_task_starting_checkpoint: 35,
        genesis_checkpoint: 10,
        gauge_metric: new_gauge_vec(&registry, "foo"),
        counter_metric: new_counter_vec(&registry),
        inflight_live_tasks: new_gauge_vec(&registry, "bar"),
    };
    let persistent = InMemoryPersistent::new();
    persistent.progress_store.lock().await.insert(
        "test_indexer - Live".to_string(),
        Task {
            task_name: "test_indexer - Live".to_string(),
            start_checkpoint: 30,
            target_checkpoint: LIVE_TASK_TARGET_CHECKPOINT as u64,
            timestamp: 0,
            is_live_task: true,
        },
    );
    let mut indexer = IndexerBuilder::new(
        "test_indexer",
        datasource,
        NoopDataMapper,
        persistent.clone(),
    )
    .with_backfill_strategy(BackfillStrategy::Simple)
    .build();
    indexer.test_only_update_tasks().await.unwrap();
    let tasks = indexer
        .test_only_storage()
        .get_all_tasks("test_indexer")
        .await
        .unwrap();
    assert_ranges(&tasks, vec![(35, i64::MAX as u64), (31, 34)]);
    indexer.start().await.unwrap();

    let tasks = persistent.get_all_tasks("test_indexer").await.unwrap();
    assert_ranges(&tasks, vec![(50, i64::MAX as u64), (34, 34)]);
}

#[tokio::test]
async fn indexer_with_existing_live_task2() {
    telemetry_subscribers::init_for_testing();
    let registry = Registry::new();
    mysten_metrics::init_metrics(&registry);

    let data = (0..=50u64).collect::<Vec<_>>();
    let datasource = TestDatasource {
        data: data.clone(),
        live_task_starting_checkpoint: 25,
        genesis_checkpoint: 10,
        gauge_metric: new_gauge_vec(&registry, "foo"),
        counter_metric: new_counter_vec(&registry),
        inflight_live_tasks: new_gauge_vec(&registry, "bar"),
    };
    let persistent = InMemoryPersistent::new();
    persistent.progress_store.lock().await.insert(
        "test_indexer - Live".to_string(),
        Task {
            task_name: "test_indexer - Live".to_string(),
            start_checkpoint: 30,
            target_checkpoint: LIVE_TASK_TARGET_CHECKPOINT as u64,
            timestamp: 10,
            is_live_task: true,
        },
    );
    let mut indexer = IndexerBuilder::new(
        "test_indexer",
        datasource,
        NoopDataMapper,
        persistent.clone(),
    )
    .with_backfill_strategy(BackfillStrategy::Simple)
    .build();
    indexer.test_only_update_tasks().await.unwrap();
    let tasks = indexer
        .test_only_storage()
        .get_all_tasks("test_indexer")
        .await
        .unwrap();
    println!("{tasks:?}");
    assert_ranges(&tasks, vec![(25, i64::MAX as u64)]);
    indexer.start().await.unwrap();

    let tasks = persistent.get_all_tasks("test_indexer").await.unwrap();
    assert_ranges(&tasks, vec![(50, i64::MAX as u64)]);
}

fn assert_ranges(desc_ordered_tasks: &[Task], ranges: Vec<(u64, u64)>) {
    assert!(desc_ordered_tasks.len() == ranges.len());
    let mut iter = desc_ordered_tasks.iter();
    for (start, end) in ranges {
        let task = iter.next().unwrap();
        assert_eq!(start, task.start_checkpoint);
        assert_eq!(end, task.target_checkpoint);
    }
}

#[tokio::test]
async fn resume_test() {
    telemetry_subscribers::init_for_testing();
    let registry = Registry::new();
    mysten_metrics::init_metrics(&registry);

    let data = (0..=50u64).collect::<Vec<_>>();
    let datasource = TestDatasource {
        data: data.clone(),
        live_task_starting_checkpoint: 31,
        genesis_checkpoint: 0,
        gauge_metric: new_gauge_vec(&registry, "foo"),
        counter_metric: new_counter_vec(&registry),
        inflight_live_tasks: new_gauge_vec(&registry, "bar"),
    };
    let persistent = InMemoryPersistent::new();
    persistent.progress_store.lock().await.insert(
        "test_indexer - backfill - 30".to_string(),
        Task {
            task_name: "test_indexer - backfill - 30".to_string(),
            start_checkpoint: 10,
            target_checkpoint: 30,
            timestamp: 0,
            is_live_task: false,
        },
    );
    let mut indexer = IndexerBuilder::new(
        "test_indexer",
        datasource,
        NoopDataMapper,
        persistent.clone(),
    )
    .with_backfill_strategy(BackfillStrategy::Simple)
    .build();
    indexer.test_only_update_tasks().await.unwrap();
    let tasks = indexer
        .test_only_storage()
        .get_all_tasks("test_indexer")
        .await
        .unwrap();
    assert_ranges(&tasks, vec![(31, i64::MAX as u64), (10, 30)]);
    indexer.start().await.unwrap();

    // it should have 2 task created for the indexer, one existing task and one live task
    let tasks = persistent.get_all_tasks("test_indexer").await.unwrap();
    assert_ranges(&tasks, vec![(50, i64::MAX as u64), (30, 30)]);
    // the data recorded in storage should be the same as the datasource
    let mut recorded_data = persistent.data.lock().await.clone();
    recorded_data.sort();
    assert_eq!((10..=50u64).collect::<Vec<_>>(), recorded_data);
}

#[tokio::test]
async fn resume_with_live_test() {
    telemetry_subscribers::init_for_testing();
    let registry = Registry::new();
    mysten_metrics::init_metrics(&registry);

    let data = (0..=70u64).collect::<Vec<_>>();
    let datasource = TestDatasource {
        data: data.clone(),
        live_task_starting_checkpoint: 60,
        genesis_checkpoint: 0,
        gauge_metric: new_gauge_vec(&registry, "foo"),
        counter_metric: new_counter_vec(&registry),
        inflight_live_tasks: new_gauge_vec(&registry, "bar"),
    };
    let persistent = InMemoryPersistent::new();
    persistent.progress_store.lock().await.insert(
        "test_indexer - backfill - 30".to_string(),
        Task {
            task_name: "test_indexer - backfill - 30".to_string(),
            start_checkpoint: 10,
            target_checkpoint: 30,
            timestamp: 0,
            is_live_task: false,
        },
    );
    persistent.progress_store.lock().await.insert(
        "test_indexer - Live".to_string(),
        Task {
            task_name: "test_indexer - Live".to_string(),
            start_checkpoint: 50,
            target_checkpoint: LIVE_TASK_TARGET_CHECKPOINT as u64,
            timestamp: 10,
            is_live_task: true,
        },
    );
    // the live task have indexed cp 31 to 50 before shutdown
    persistent
        .data
        .lock()
        .await
        .append(&mut (31..=50).collect());
    let mut indexer = IndexerBuilder::new(
        "test_indexer",
        datasource,
        NoopDataMapper,
        persistent.clone(),
    )
    .with_backfill_strategy(BackfillStrategy::Simple)
    .build();
    indexer.test_only_update_tasks().await.unwrap();
    let tasks = indexer
        .test_only_storage()
        .get_all_tasks("test_indexer")
        .await
        .unwrap();
    assert_ranges(&tasks, vec![(60, i64::MAX as u64), (51, 59), (10, 30)]);
    indexer.start().await.unwrap();

    // it should have 2 task created for the indexer, one existing task and one live task
    let tasks = persistent.get_all_tasks("test_indexer").await.unwrap();
    assert_ranges(&tasks, vec![(70, i64::MAX as u64), (59, 59), (30, 30)]);
    // the data recorded in storage should be the same as the datasource
    let mut recorded_data = persistent.data.lock().await.clone();
    recorded_data.sort();
    assert_eq!((10..=70u64).collect::<Vec<_>>(), recorded_data);
}

fn new_gauge_vec(registry: &Registry, name: &str) -> IntGaugeVec {
    register_int_gauge_vec_with_registry!(name, "whatever", &["whatever"], registry,).unwrap()
}

fn new_counter_vec(registry: &Registry) -> IntCounterVec {
    register_int_counter_vec_with_registry!(
        "whatever_counter",
        "whatever",
        &["whatever1", "whatever2"],
        registry,
    )
    .unwrap()
}
