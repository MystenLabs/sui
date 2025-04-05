// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use sui_swarm_config::genesis_config::{AccountConfig, DEFAULT_GAS_AMOUNT};
use sui_types::base_types::ObjectID;
use surf_strategy::{ExitCondition, SurfStrategy};
use test_cluster::{TestCluster, TestClusterBuilder};
use tracing::info;

use crate::surfer_state::SurfStatistics;
use crate::surfer_task::SurferTask;

pub mod surf_strategy;
pub mod surfer_state;
mod surfer_task;

const VALIDATOR_COUNT: usize = 7;

const ACCOUNT_NUM: usize = 20;
const GAS_OBJECT_COUNT: usize = 3;

type EntryFunctionFilterFn =
    Arc<dyn Fn(&str /* module */, &str /* function */) -> bool + Send + Sync + 'static>;

pub async fn run(
    run_duration: Duration,
    epoch_duration: Duration,
    packages: Vec<PackageSpec>,
    entry_function_filter: Option<EntryFunctionFilterFn>,
) -> SurfStatistics {
    let cluster = TestClusterBuilder::new()
        .with_num_validators(VALIDATOR_COUNT)
        .with_epoch_duration_ms(epoch_duration.as_millis() as u64)
        .with_accounts(vec![
            AccountConfig {
                address: None,
                gas_amounts: vec![DEFAULT_GAS_AMOUNT; GAS_OBJECT_COUNT],
            };
            ACCOUNT_NUM
        ])
        .build()
        .await;
    info!(
        "Started cluster with {} validators and epoch duration of {:?}ms",
        VALIDATOR_COUNT,
        epoch_duration.as_millis()
    );
    run_with_test_cluster(
        run_duration,
        packages,
        entry_function_filter,
        cluster.into(),
        0,
    )
    .await
}

pub async fn run_with_test_cluster(
    run_duration: Duration,
    packages: Vec<PackageSpec>,
    entry_function_filter: Option<EntryFunctionFilterFn>,
    cluster: Arc<TestCluster>,
    // Skips the first N accounts, for use in case this is running concurrently with other
    // processes that also need gas.
    skip_accounts: usize,
) -> SurfStatistics {
    let mut surf_strategy = SurfStrategy::default();
    surf_strategy.set_exit_condition(ExitCondition::Timeout(run_duration));

    run_with_test_cluster_and_strategy(
        surf_strategy,
        packages,
        entry_function_filter,
        cluster,
        skip_accounts,
    )
    .await
}

pub enum PackageSpec {
    Path(PathBuf), // must be published
    Id(ObjectID),  // already published, just needs to be crawled.
}

impl From<PathBuf> for PackageSpec {
    fn from(path: PathBuf) -> Self {
        PackageSpec::Path(path)
    }
}

impl From<ObjectID> for PackageSpec {
    fn from(id: ObjectID) -> Self {
        PackageSpec::Id(id)
    }
}

pub async fn run_with_test_cluster_and_strategy(
    surf_strategy: SurfStrategy,
    package_paths: Vec<PackageSpec>,
    entry_function_filter: Option<EntryFunctionFilterFn>,
    cluster: Arc<TestCluster>,
    // Skips the first N accounts, for use in case this is running concurrently with other
    // processes that also need gas.
    skip_accounts: usize,
) -> SurfStatistics {
    let seed = rand::thread_rng().gen::<u64>();
    info!("Initial Seed: {:?}", seed);
    let mut rng = StdRng::seed_from_u64(seed);

    let mut tasks = SurferTask::create_surfer_tasks(
        cluster.clone(),
        rng.gen::<u64>(),
        skip_accounts,
        surf_strategy,
        entry_function_filter,
    )
    .await;
    info!("Created {} surfer tasks", tasks.len());

    for pkg in &package_paths {
        match pkg {
            PackageSpec::Path(path) => {
                tasks
                    .choose_mut(&mut rng)
                    .unwrap()
                    .state
                    .publish_package(path)
                    .await;
            }
            PackageSpec::Id(id) => {
                tasks
                    .choose_mut(&mut rng)
                    .unwrap()
                    .state
                    .add_package(*id)
                    .await;
            }
        }
    }

    let mut join_set = tokio::task::JoinSet::new();
    for task in tasks {
        join_set.spawn(task.surf());
    }

    let mut all_stats = Vec::new();
    while let Some(result) = join_set.join_next().await {
        match result {
            Ok(stats) => all_stats.push(stats),
            Err(e) => eprintln!("Task failed: {:?}", e),
        }
    }

    SurfStatistics::aggregate(all_stats)

    // TODO: Right now it will panic here complaining about dropping a tokio runtime
    // inside of another tokio runtime. Reason unclear.
}
