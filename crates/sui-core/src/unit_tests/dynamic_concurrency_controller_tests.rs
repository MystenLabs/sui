// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::time::Duration;
use sui_core::execution_driver::DynamicConcurrencyController;
use tokio::time::{sleep, timeout};

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn test_basic_concurrency_controller_functionality() {
    let controller = DynamicConcurrencyController::new();
    
    let base_limit = num_cpus::get();
    assert_eq!(controller.current_limit(), base_limit);
    
    let _permit1 = controller.acquire().await;
    let _permit2 = controller.acquire().await;
    
    controller.record_success();
    controller.record_success();
    controller.record_failure();
    
    sleep(Duration::from_secs(6)).await;
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn test_concurrency_permits_acquisition() {
    let controller = DynamicConcurrencyController::new();
    let current_limit = controller.current_limit();
    
    let mut permits = Vec::new();
    for _ in 0..current_limit {
        permits.push(controller.acquire().await);
    }

    let result = timeout(Duration::from_secs(1), controller.acquire()).await;
    assert!(result.is_err(), "Expected timeout when trying to acquire more permits than available");
    
    drop(permits.pop());
    
    let result = timeout(Duration::from_secs(1), controller.acquire()).await;
    assert!(result.is_ok(), "Expected to acquire permit after releasing one");
}

