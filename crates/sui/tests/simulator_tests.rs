// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::{
    stream::{FuturesOrdered, FuturesUnordered},
    StreamExt,
};
use rand::{
    distributions::{Distribution, Uniform},
    rngs::OsRng,
    Rng,
};
use std::collections::{HashMap, HashSet};
use tokio::time::{sleep, Duration, Instant};
use tracing::{debug, trace};

use sui_macros::*;
use test_utils::{
    network::{init_cluster_builder_env_aware, start_a_fullnode},
    transaction::{transfer_coin, wait_for_tx},
};

async fn make_fut(i: usize) -> usize {
    let count_dist = Uniform::from(1..5);
    let sleep_dist = Uniform::from(1000..10000);

    let count = count_dist.sample(&mut OsRng);
    for _ in 0..count {
        let dur = Duration::from_millis(sleep_dist.sample(&mut OsRng));
        trace!("sleeping for {:?}", dur);
        sleep(dur).await;
    }

    trace!("future {} finished at {:?}", i, Instant::now());
    i
}

#[sim_test(check_determinism)]
async fn test_futures_ordered() {
    telemetry_subscribers::init_for_testing();

    let mut futures = FuturesOrdered::from_iter((0..200).map(make_fut));

    while (futures.next().await).is_some() {
        // mix rng state as futures finish
        OsRng.gen::<u32>();
    }
    debug!("final rng state: {}", OsRng.gen::<u32>());
}

#[sim_test(check_determinism)]
async fn test_futures_unordered() {
    telemetry_subscribers::init_for_testing();

    let mut futures = FuturesUnordered::from_iter((0..200).map(make_fut));

    while let Some(i) = futures.next().await {
        // mix rng state depending on the order futures finish in
        for _ in 0..i {
            OsRng.gen::<u32>();
        }
    }
    debug!("final rng state: {}", OsRng.gen::<u32>());
}

#[sim_test(check_determinism)]
async fn test_select_unbiased() {
    let mut f1 = FuturesUnordered::from_iter((0..200).map(make_fut));
    let mut f2 = FuturesUnordered::from_iter((0..200).map(make_fut));

    loop {
        tokio::select! {

            Some(i) = f1.next() => {
                for _ in 0..i {
                    OsRng.gen::<u32>();
                }
            }

            Some(i) = f2.next() => {
                for _ in 0..i {
                    // mix differently when f2 yields.
                    OsRng.gen::<u32>();
                    OsRng.gen::<u32>();
                }
            }

            else => break
        }
    }

    assert!(f1.is_empty());
    assert!(f2.is_empty());
    debug!("final rng state: {}", OsRng.gen::<u32>());
}

#[sim_test(check_determinism)]
async fn test_hash_collections() {
    telemetry_subscribers::init_for_testing();

    let mut map = HashMap::new();
    let mut set = HashSet::new();

    for i in 0..1000 {
        map.insert(i, i);
        set.insert(i);
    }

    // mix the random state according to the first 500 elements of each map
    // so that if iteration order changes, we get different results.
    for (i, _) in map.iter().take(500) {
        for _ in 0..*i {
            OsRng.gen::<u32>();
        }
    }

    for i in set.iter().take(500) {
        for _ in 0..*i {
            OsRng.gen::<u32>();
        }
    }

    debug!("final rng state: {}", OsRng.gen::<u32>());
}

// Test that starting up a network + fullnode, and sending one transaction through that network is
// repeatable and deterministic.
#[sim_test(check_determinism)]
async fn test_net_determinism() {
    let mut test_cluster = init_cluster_builder_env_aware().build().await.unwrap();
    let context = &mut test_cluster.wallet;

    let (_transferred_object, _, _, digest, _, _) = transfer_coin(context).await.unwrap();

    sleep(Duration::from_millis(1000)).await;

    let node = start_a_fullnode(&test_cluster.swarm, false).await.unwrap();

    wait_for_tx(digest, node.state().clone()).await;
}
