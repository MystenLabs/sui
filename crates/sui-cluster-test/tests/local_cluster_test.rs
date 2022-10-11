// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_cluster_test::{config::ClusterTestOpt, ClusterTest};

#[tokio::test]
async fn cluster_test() {
    telemetry_subscribers::init_for_testing();

    ClusterTest::run(ClusterTestOpt::new_local()).await;
}
