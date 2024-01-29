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
    pub is_overload: AtomicBool,
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

pub async fn overload_monitor(
    authority_state: Weak<AuthorityState>,
    config: OverloadThresholdConfig,
) {
    info!("Starting system overload monitor.");
    loop {
        if let Some(authority) = authority_state.upgrade() {
            let queueing_latency = authority
                .metrics
                .execution_queueing_latency
                .latency()
                .unwrap_or_default();
            let txn_ready_rate = authority.metrics.txn_ready_rate_tracker.lock().rate();
            let execution_rate = authority.metrics.execution_rate_tracker.lock().rate();

            let (is_overload, load_shedding_percentage) =
                check_system_overload(&config, queueing_latency, txn_ready_rate, execution_rate);
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
        } else {
            break;
        }

        sleep(config.overload_monitor_interval).await;
    }

    info!("Shut down system overload monitor.");
}

fn calculate_load_shedding_percentage(txn_ready_rate: f64, execution_rate: f64) -> u32 {
    if execution_rate * 0.9 > txn_ready_rate {
        return 0;
    }

    (((1.0 - execution_rate * 0.9 / txn_ready_rate) + 0.1).min(1.0) * 100.0).round() as u32
}

fn check_system_overload(
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
    }

    #[test]
    pub fn test_check_system_overload() {
        let config = OverloadThresholdConfig {
            execution_queue_latency_hard_limit: Duration::from_secs(10),
            execution_queue_latency_soft_limit: Duration::from_secs(1),
            max_load_shedding_percentage: 90,
            ..Default::default()
        };

        // When execution queueing latency is within soft limit, don't start overload protection.
        assert_eq!(
            check_system_overload(&config, Duration::from_millis(500), 1000.0, 10.0),
            (false, 0)
        );

        // When execution queueing latency hits soft limit and execution rate is higher, don't
        // start overload protection.
        assert_eq!(
            check_system_overload(&config, Duration::from_secs(2), 100.0, 120.0),
            (false, 0)
        );

        // When execution queueing latency hits soft limit, but not hard limit, start overload
        // protection.
        assert_eq!(
            check_system_overload(&config, Duration::from_secs(2), 100.0, 100.0),
            (true, 20)
        );

        // When execution queueing latency hits hard limit, start more aggressive overload
        // protection.
        assert_eq!(
            check_system_overload(&config, Duration::from_secs(11), 100.0, 100.0),
            (true, 50)
        );

        // When execution queueing latency hits hard limit and calculated shedding percentage
        // is higher than
        assert_eq!(
            check_system_overload(&config, Duration::from_secs(11), 240.0, 100.0),
            (true, 73)
        );

        // Maximum transactions shed is cap by `max_load_shedding_percentage` config.
        assert_eq!(
            check_system_overload(&config, Duration::from_secs(11), 100.0, 0.0),
            (true, 90)
        );
    }
}
