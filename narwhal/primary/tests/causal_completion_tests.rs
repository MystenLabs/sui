// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use std::time::Duration;
use test_utils::cluster::Cluster;
use tracing::info;

#[ignore]
#[tokio::test]
async fn test_read_causal_signed_certificates() {
    const CURRENT_ROUND_METRIC: &str = "narwhal_primary_current_round";

    // Enabled debug tracing so we can easily observe the
    // nodes logs.
    setup_tracing();

    let mut cluster = Cluster::new(None, None);

    // start the cluster
    cluster.start(Some(4), Some(1)).await;

    // Let primaries advance little bit
    tokio::time::sleep(Duration::from_secs(10)).await;

    // Ensure all nodes advanced
    for authority in cluster.authorities() {
        let metric_family = authority.primary.registry.gather();

        for metric in metric_family {
            if metric.get_name() == CURRENT_ROUND_METRIC {
                let value = metric.get_metric().first().unwrap().get_gauge().get_value();

                info!("Metrics name {} -> {:?}", metric.get_name(), value);

                // If the current round is increasing then it means that the
                // node starts catching up and is proposing.
                assert!(value > 1.0, "Node didn't progress further than the round 1");
            }
        }
    }

    // Now stop node 0
    cluster.stop_node(0);

    // Let other primaries advance
    tokio::time::sleep(Duration::from_secs(10)).await;

    // Now start the validator 0 again
    cluster.start_node(0, true, Some(1)).await;

    // Now check that the current round advances. Give the opportunity with a few
    // iterations. If metric hasn't picked up then we know that node can't make
    // progress.
    let mut node_made_progress = false;
    let node = cluster.authority(0);

    for _ in 0..10 {
        tokio::time::sleep(Duration::from_secs(1)).await;

        let metric_family = node.primary.registry.gather();

        for metric in metric_family {
            if metric.get_name() == CURRENT_ROUND_METRIC {
                let value = metric.get_metric().first().unwrap().get_gauge().get_value();

                info!("Metrics name {} -> {:?}", metric.get_name(), value);

                // If the current round is increasing then it means that the
                // node starts catching up and is proposing.
                if value > 1.0 {
                    node_made_progress = true;
                    break;
                }
            }
        }
    }

    assert!(
        node_made_progress,
        "Node 0 didn't make progress - causal completion didn't succeed"
    );
}

fn setup_tracing() {
    // Setup tracing
    let tracing_level = "debug";
    let network_tracing_level = "info";

    let log_filter = format!("{tracing_level},h2={network_tracing_level},tower={network_tracing_level},hyper={network_tracing_level},tonic::transport={network_tracing_level}");

    let _guard = telemetry_subscribers::TelemetryConfig::new("narwhal")
        // load env variables
        .with_env()
        // load special log filter
        .with_log_level(&log_filter)
        .init();
}
