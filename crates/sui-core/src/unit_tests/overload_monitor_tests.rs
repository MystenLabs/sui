// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests that overload monitor only starts on validators.
#[cfg(msim)]
mod simtests {
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;
    use std::sync::Arc;
    use sui_macros::register_fail_point;
    use sui_macros::sim_test;
    use test_cluster::TestClusterBuilder;

    #[sim_test]
    async fn overload_monitor_in_different_nodes() {
        telemetry_subscribers::init_for_testing();

        // Uses a fail point to count the number of nodes that start overload monitor.
        let counter: Arc<AtomicUsize> = Arc::new(AtomicUsize::new(0));
        let counter_clone = counter.clone();
        register_fail_point("starting_overload_monitor", move || {
            counter_clone.fetch_add(1, Ordering::SeqCst);
        });

        // Creates a cluster, and tests that number of nodes with overload monitor is equal to
        // the number of validators.
        let test_cluster = TestClusterBuilder::new().build().await;
        let nodes_with_overload_monitor = counter.load(Ordering::SeqCst);
        assert_eq!(
            nodes_with_overload_monitor,
            test_cluster.swarm.validator_node_handles().len()
        );

        // Tests (indirectly) that fullnodes don't run overload monitor.
        assert!(
            test_cluster.swarm.all_nodes().collect::<Vec<_>>().len() > nodes_with_overload_monitor
        );
    }
}

// TODO: move other overload relate tests from execution_driver_tests.rs to here.
