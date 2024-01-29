// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::AuthorityState;
use std::cmp::{max, min};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Weak;
use std::time::Duration;
use sui_config::node::OverloadThresholdConfig;
use tokio::time::sleep;
use tracing::info;

#[derive(Default)]
pub struct AuthorityOverloadInfo {
    /// Whether the authority is overloaded.
    pub is_overload: AtomicBool,

    /// The calculated percentage of transactions to drop.
    pub load_shedding_percentage: AtomicU32,
}

impl AuthorityOverloadInfo {
    pub fn set_overload(&self, load_shedding_percentage: u32) {
        self.is_overload.store(true, Ordering::Relaxed);
        self.load_shedding_percentage
            .store(min(load_shedding_percentage, 100), Ordering::Relaxed);
    }

    pub fn clear_overload(&self) {
        self.is_overload.store(false, Ordering::Relaxed);
        self.load_shedding_percentage.store(0, Ordering::Relaxed);
    }
}

// Monitors the overload signals in `authority_state` periodically, and updates its `overload_info`
// when the signals indicates overload.
pub async fn overload_monitor(
    authority_state: Weak<AuthorityState>,
    config: OverloadThresholdConfig,
) {
    info!("Starting system overload monitor.");

    loop {
        let authority_exist = check_authority_overload(&authority_state, &config);
        if !authority_exist {
            // `authority_state` doesn't exist anymore. Quit overload monitor.
            break;
        }
        sleep(config.overload_monitor_interval).await;
    }

    info!("Shut down system overload monitor.");
}

// Checks authority overload signals, and updates authority's `overload_info`.
// Returns whether the authority state exists.
fn check_authority_overload(
    authority_state: &Weak<AuthorityState>,
    config: &OverloadThresholdConfig,
) -> bool {
    let authority_arc = authority_state.upgrade();
    if authority_arc.is_none() {
        // `authority_state` doesn't exist anymore.
        return false;
    }

    let authority = authority_arc.unwrap();
    let queueing_latency = authority
        .metrics
        .execution_queueing_latency
        .latency()
        .unwrap_or_default();
    let txn_ready_rate = authority.metrics.txn_ready_rate_tracker.lock().rate();
    let execution_rate = authority.metrics.execution_rate_tracker.lock().rate();

    let (is_overload, load_shedding_percentage) =
        check_overload_signals(config, queueing_latency, txn_ready_rate, execution_rate);
    if is_overload {
        authority
            .overload_info
            .set_overload(load_shedding_percentage);
    } else {
        authority.overload_info.clear_overload();
    }

    authority
        .metrics
        .authority_overload_status
        .set(is_overload as i64);
    authority
        .metrics
        .authority_load_shedding_percentage
        .set(load_shedding_percentage as i64);
    true
}

// Calculates the percentage of transactions to drop in order to reduce execution queue.
// Returns the integer percentage between 0 and 100.
fn calculate_load_shedding_percentage(txn_ready_rate: f64, execution_rate: f64) -> u32 {
    // When transaction ready rate is practically 0, we aren't adding more load to the
    // execution driver, so no shedding.
    // TODO: consensus handler or transaction manager can also be overloaded.
    if txn_ready_rate < 1e-10 {
        return 0;
    }

    // Deflate the execution rate to account for the case that execution_rate is close to
    // txn_ready_rate.
    if execution_rate * 0.9 > txn_ready_rate {
        return 0;
    }

    // In order to maintain execution queue length, we need to drop at least (1 - executionRate / readyRate).
    // TO reduce the queue length, here we add 10% more transactions to drop.
    (((1.0 - execution_rate * 0.9 / txn_ready_rate) + 0.1).min(1.0) * 100.0).round() as u32
}

// Given overload signals (`queueing_latency`, `txn_ready_rate`, `execution_rate`), return whether
// the authority server should enter load shedding mode, and how much percentage of transactions to drop.
fn check_overload_signals(
    config: &OverloadThresholdConfig,
    queueing_latency: Duration,
    txn_ready_rate: f64,
    execution_rate: f64,
) -> (bool, u32) {
    let overload_status;
    let load_shedding_percentage;
    if queueing_latency > config.execution_queue_latency_hard_limit {
        overload_status = true;
        load_shedding_percentage = max(
            calculate_load_shedding_percentage(txn_ready_rate, execution_rate),
            config.min_load_shedding_percentage_above_hard_limit,
        );
    } else if queueing_latency > config.execution_queue_latency_soft_limit {
        load_shedding_percentage =
            calculate_load_shedding_percentage(txn_ready_rate, execution_rate);
        overload_status = load_shedding_percentage > 0;
    } else {
        overload_status = false;
        load_shedding_percentage = 0;
    }

    let load_shedding_percentage = min(
        load_shedding_percentage,
        config.max_load_shedding_percentage,
    );
    (overload_status, load_shedding_percentage)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::authority::test_authority_builder::TestAuthorityBuilder;
    use std::sync::Arc;

    #[test]
    pub fn test_authority_overload_info() {
        let overload_info = AuthorityOverloadInfo::default();
        assert!(!overload_info.is_overload.load(Ordering::Relaxed));
        assert_eq!(
            overload_info
                .load_shedding_percentage
                .load(Ordering::Relaxed),
            0
        );

        {
            overload_info.set_overload(20);
            assert!(overload_info.is_overload.load(Ordering::Relaxed));
            assert_eq!(
                overload_info
                    .load_shedding_percentage
                    .load(Ordering::Relaxed),
                20
            );
        }

        // Tests that load shedding percentage can't go beyond 100%.
        {
            overload_info.set_overload(110);
            assert!(overload_info.is_overload.load(Ordering::Relaxed));
            assert_eq!(
                overload_info
                    .load_shedding_percentage
                    .load(Ordering::Relaxed),
                100
            );
        }

        {
            overload_info.clear_overload();
            assert!(!overload_info.is_overload.load(Ordering::Relaxed));
            assert_eq!(
                overload_info
                    .load_shedding_percentage
                    .load(Ordering::Relaxed),
                0
            );
        }
    }

    #[test]
    pub fn test_calculate_load_shedding_ratio() {
        assert_eq!(calculate_load_shedding_percentage(90.0, 100.1), 0);
        assert_eq!(calculate_load_shedding_percentage(90.0, 100.0), 10);
        assert_eq!(calculate_load_shedding_percentage(100.0, 100.0), 20);
        assert_eq!(calculate_load_shedding_percentage(110.0, 100.0), 28);
        assert_eq!(calculate_load_shedding_percentage(180.0, 100.0), 60);
        assert_eq!(calculate_load_shedding_percentage(100.0, 0.0), 100);
        assert_eq!(calculate_load_shedding_percentage(0.0, 1.0), 0);
    }

    #[test]
    pub fn test_check_overload_signals() {
        let config = OverloadThresholdConfig {
            execution_queue_latency_hard_limit: Duration::from_secs(10),
            execution_queue_latency_soft_limit: Duration::from_secs(1),
            max_load_shedding_percentage: 90,
            ..Default::default()
        };

        // When execution queueing latency is within soft limit, don't start overload protection.
        assert_eq!(
            check_overload_signals(&config, Duration::from_millis(500), 1000.0, 10.0),
            (false, 0)
        );

        // When execution queueing latency hits soft limit and execution rate is higher, don't
        // start overload protection.
        assert_eq!(
            check_overload_signals(&config, Duration::from_secs(2), 100.0, 120.0),
            (false, 0)
        );

        // When execution queueing latency hits soft limit, but not hard limit, start overload
        // protection.
        assert_eq!(
            check_overload_signals(&config, Duration::from_secs(2), 100.0, 100.0),
            (true, 20)
        );

        // When execution queueing latency hits hard limit, start more aggressive overload
        // protection.
        assert_eq!(
            check_overload_signals(&config, Duration::from_secs(11), 100.0, 100.0),
            (true, 50)
        );

        // When execution queueing latency hits hard limit and calculated shedding percentage
        // is higher than
        assert_eq!(
            check_overload_signals(&config, Duration::from_secs(11), 240.0, 100.0),
            (true, 73)
        );

        // Maximum transactions shed is cap by `max_load_shedding_percentage` config.
        assert_eq!(
            check_overload_signals(&config, Duration::from_secs(11), 100.0, 0.0),
            (true, 90)
        );
    }

    #[tokio::test(flavor = "current_thread")]
    pub async fn test_check_authority_overload() {
        let state = TestAuthorityBuilder::new().build().await;
        let config = OverloadThresholdConfig::default();

        // Creates a simple case to see if authority state overload_info can be updated
        // correctly by check_authority_overload.
        state
            .metrics
            .execution_queueing_latency
            .report(Duration::from_secs(20));
        let authority = Arc::downgrade(&state);
        assert!(check_authority_overload(&authority, &config));
        assert!(state.overload_info.is_overload.load(Ordering::Relaxed));
        assert_eq!(
            state
                .overload_info
                .load_shedding_percentage
                .load(Ordering::Relaxed),
            config.min_load_shedding_percentage_above_hard_limit
        );

        // Checks that check_authority_overload should return false when the input
        // authority state doesn't exist.
        let authority = Arc::downgrade(&state);
        drop(state);
        assert!(!check_authority_overload(&authority, &config));
    }
}
