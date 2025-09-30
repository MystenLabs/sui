// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::validator_client_monitor::stats::{ClientObservedStats, ValidatorClientStats};
use std::sync::Arc;
use std::time::Duration;
use sui_config::validator_client_monitor_config::ValidatorClientMonitorConfig;
use sui_types::base_types::{AuthorityName, ConciseableName};
use sui_types::committee::Committee;
use sui_types::crypto::{get_key_pair, AuthorityKeyPair, KeypairTraits};
use tokio::time::sleep;

mod client_stats_tests {

    use super::*;
    use crate::validator_client_monitor::metrics::ValidatorClientMetrics;
    use prometheus::Registry;
    use sui_types::messages_grpc::TxType;

    /// Helper to create test validator names
    fn create_test_validator_names(n: usize) -> Vec<AuthorityName> {
        (0..n)
            .map(|_| {
                let (_, key_pair): (_, AuthorityKeyPair) = get_key_pair();
                key_pair.public().into()
            })
            .collect()
    }

    /// Helper to create test metrics
    fn create_test_metrics() -> ValidatorClientMetrics {
        ValidatorClientMetrics::new(&Registry::default())
    }

    #[tokio::test]
    async fn test_client_stats_record_success() {
        let config = ValidatorClientMonitorConfig::default();
        let mut stats = ClientObservedStats::new(config);
        let metrics = create_test_metrics();

        let validators = create_test_validator_names(1);
        let validator = validators[0];

        // Record successful operation
        let feedback = OperationFeedback {
            authority_name: validator,
            display_name: validator.concise().to_string(),
            operation: OperationType::Submit,
            result: Ok(Duration::from_millis(100)),
        };

        stats.record_interaction_result(feedback, &metrics);

        // Check validator stats were created and updated
        let validator_stats = stats.validator_stats.get(&validator).unwrap();
        assert_eq!(validator_stats.consecutive_failures, 0);
        assert!(validator_stats.exclusion_time.is_none());
        assert_eq!(validator_stats.reliability.get(), 1.0);

        // Check latency was recorded
        let submit_latency = validator_stats
            .average_latencies
            .get(&OperationType::Submit)
            .unwrap();
        assert_eq!(submit_latency.get(), Duration::from_millis(100));
    }

    #[tokio::test]
    async fn test_client_stats_record_failure() {
        let config = ValidatorClientMonitorConfig {
            max_consecutive_failures: 3,
            ..Default::default()
        };
        let mut stats = ClientObservedStats::new(config);
        let metrics = create_test_metrics();

        let validators = create_test_validator_names(1);
        let validator = validators[0];

        // Record multiple failures
        for i in 0..3 {
            let feedback = OperationFeedback {
                authority_name: validator,
                display_name: validator.concise().to_string(),
                operation: OperationType::Submit,
                result: Err(()),
            };
            stats.record_interaction_result(feedback, &metrics);

            let validator_stats = stats.validator_stats.get(&validator).unwrap();
            assert_eq!(validator_stats.consecutive_failures, i + 1);

            // Should be excluded after 3rd failure
            if i == 2 {
                assert!(validator_stats.exclusion_time.is_some());
            } else {
                assert!(validator_stats.exclusion_time.is_none());
            }
        }
    }

    #[tokio::test]
    async fn test_client_stats_calculate_latencies() {
        let config = ValidatorClientMonitorConfig::default();
        let mut stats = ClientObservedStats::new(config);
        let metrics = create_test_metrics();

        // Create two validators with different performance
        let validators = create_test_validator_names(2);
        let validator1 = validators[0];
        let validator2 = validators[1];

        // Validator 1: good performance
        stats.record_interaction_result(
            OperationFeedback {
                authority_name: validator1,
                display_name: validator1.concise().to_string(),
                operation: OperationType::FastPath,
                result: Ok(Duration::from_millis(50)),
            },
            &metrics,
        );

        // Validator 2: worse performance
        stats.record_interaction_result(
            OperationFeedback {
                authority_name: validator2,
                display_name: validator2.concise().to_string(),
                operation: OperationType::FastPath,
                result: Ok(Duration::from_millis(200)),
            },
            &metrics,
        );

        // Add one failure for validator2
        stats.record_interaction_result(
            OperationFeedback {
                authority_name: validator2,
                display_name: validator2.concise().to_string(),
                operation: OperationType::Submit,
                result: Err(()),
            },
            &metrics,
        );

        // Create a committee with both validators
        let committee = Committee::new_for_testing_with_normalized_voting_power(
            0,
            vec![(validator1, 1), (validator2, 1)].into_iter().collect(),
        );

        let all_stats = stats.get_all_validator_stats(&committee, TxType::SingleWriter);
        assert_eq!(all_stats.len(), 2);

        // Validator 1 should be faster (lower latency) than validator 2
        let latency_1 = *all_stats.get(&validator1).unwrap();
        let latency_2 = *all_stats.get(&validator2).unwrap();
        assert!(latency_1 < latency_2);
    }

    #[tokio::test]
    async fn test_client_stats_exclusion() {
        let config = ValidatorClientMonitorConfig {
            max_consecutive_failures: 2,
            failure_cooldown: Duration::from_millis(100),
            ..Default::default()
        };
        let mut stats = ClientObservedStats::new(config);
        let metrics = create_test_metrics();

        let validators = create_test_validator_names(1);
        let validator = validators[0];

        stats.record_interaction_result(
            OperationFeedback {
                authority_name: validator,
                display_name: validator.concise().to_string(),
                operation: OperationType::Consensus,
                result: Ok(Duration::from_millis(50)),
            },
            &metrics,
        );

        // Cause exclusion
        for _ in 0..2 {
            stats.record_interaction_result(
                OperationFeedback {
                    authority_name: validator,
                    display_name: validator.concise().to_string(),
                    operation: OperationType::Submit,
                    result: Err(()),
                },
                &metrics,
            );
        }

        // Create a committee with the validator
        let committee = Committee::new_for_testing_with_normalized_voting_power(
            0,
            vec![(validator, 1)].into_iter().collect(),
        );

        // Should be excluded (max latency should be assigned)
        let all_stats = stats.get_all_validator_stats(&committee, TxType::SharedObject);
        let latency = *all_stats.get(&validator).unwrap();
        assert_eq!(latency, Duration::from_secs(10));

        // Wait for cooldown
        sleep(Duration::from_millis(150)).await;

        // Should be included again (latency < 10.0)
        let all_stats = stats.get_all_validator_stats(&committee, TxType::SingleWriter);
        let latency = *all_stats.get(&validator).unwrap();
        assert!(latency > Duration::ZERO);
    }

    #[tokio::test]
    async fn test_client_stats_refresh_validator_set() {
        let config = ValidatorClientMonitorConfig::default();
        let mut stats = ClientObservedStats::new(config);

        // Add stats for 3 validators
        let validators = create_test_validator_names(3);

        let metrics = create_test_metrics();
        for validator in &validators {
            stats.record_interaction_result(
                OperationFeedback {
                    authority_name: *validator,
                    display_name: validator.concise().to_string(),
                    operation: OperationType::Submit,
                    result: Ok(Duration::from_millis(100)),
                },
                &metrics,
            );
        }

        assert_eq!(stats.validator_stats.len(), 3);

        // Refresh with only first 2 validators
        let remaining_validators: Vec<_> = validators.iter().take(2).cloned().collect();
        stats.retain_validators(&remaining_validators);

        assert_eq!(stats.validator_stats.len(), 2);
        assert!(stats.validator_stats.contains_key(&validators[0]));
        assert!(stats.validator_stats.contains_key(&validators[1]));
        assert!(!stats.validator_stats.contains_key(&validators[2]));
    }

    #[tokio::test]
    async fn test_validator_stats_update_latency() {
        let mut stats = ValidatorClientStats::new(1.0);

        // First update creates the entry
        stats.update_average_latency(OperationType::Submit, Duration::from_millis(100));
        assert_eq!(stats.average_latencies.len(), 1);
        assert_eq!(
            stats
                .average_latencies
                .get(&OperationType::Submit)
                .unwrap()
                .get(),
            Duration::from_millis(100)
        );

        // Second update calculates arithmetic mean of the moving window
        stats.update_average_latency(OperationType::Submit, Duration::from_millis(200));
        let latency = stats
            .average_latencies
            .get(&OperationType::Submit)
            .unwrap()
            .get();

        // With MovingWindow: (100ms + 200ms) / 2 = 150ms
        assert_eq!(latency, Duration::from_millis(150));
    }

    #[tokio::test]
    async fn test_latency_calculation_with_missing_operations() {
        let config = ValidatorClientMonitorConfig::default();
        let mut stats = ClientObservedStats::new(config);
        let metrics = create_test_metrics();

        let validators = create_test_validator_names(1);
        let validator = validators[0];

        // Only record Submit operation, missing Effects and HealthCheck
        stats.record_interaction_result(
            OperationFeedback {
                authority_name: validator,
                display_name: validator.concise().to_string(),
                operation: OperationType::Submit,
                result: Ok(Duration::from_millis(100)),
            },
            &metrics,
        );

        // Create a committee with the validator
        let committee = Committee::new_for_testing_with_normalized_voting_power(
            0,
            vec![(validator, 1)].into_iter().collect(),
        );

        let all_stats = stats.get_all_validator_stats(&committee, TxType::SingleWriter);
        // Should have a partial latency even with only one operation type
        let latency = *all_stats.get(&validator).unwrap();
        assert!(latency > Duration::ZERO);
    }

    #[tokio::test]
    async fn test_reliability_decay() {
        let config = ValidatorClientMonitorConfig::default();
        let mut stats = ClientObservedStats::new(config);
        let metrics = create_test_metrics();

        let validators = create_test_validator_names(1);
        let validator = validators[0];

        // Start with success
        stats.record_interaction_result(
            OperationFeedback {
                authority_name: validator,
                display_name: validator.concise().to_string(),
                operation: OperationType::Submit,
                result: Ok(Duration::from_millis(100)),
            },
            &metrics,
        );

        let initial_reliability = stats
            .validator_stats
            .get(&validator)
            .unwrap()
            .reliability
            .get();
        assert_eq!(initial_reliability, 1.0);

        // Add failure
        stats.record_interaction_result(
            OperationFeedback {
                authority_name: validator,
                display_name: validator.concise().to_string(),
                operation: OperationType::Submit,
                result: Err(()),
            },
            &metrics,
        );

        let new_reliability = stats
            .validator_stats
            .get(&validator)
            .unwrap()
            .reliability
            .get();
        assert!((new_reliability - (2.0 / 3.0)).abs() < 1e-10);
    }

    #[tokio::test]
    async fn test_moving_window_average_differences() {
        let config = ValidatorClientMonitorConfig::default();
        let mut stats = ClientObservedStats::new(config);
        let metrics = create_test_metrics();

        let validator = create_test_validator_names(1)[0];

        // Initial values for both validator latency and global max
        stats.record_interaction_result(
            OperationFeedback {
                authority_name: validator,
                display_name: validator.concise().to_string(),
                operation: OperationType::Submit,
                result: Ok(Duration::from_millis(100)),
            },
            &metrics,
        );

        // Both should start at 100ms
        let validator_latency = stats
            .validator_stats
            .get(&validator)
            .unwrap()
            .average_latencies
            .get(&OperationType::Submit)
            .unwrap()
            .get();
        assert_eq!(validator_latency, Duration::from_millis(100));

        // Update with lower value (50ms)
        stats.record_interaction_result(
            OperationFeedback {
                authority_name: validator,
                display_name: validator.concise().to_string(),
                operation: OperationType::Submit,
                result: Ok(Duration::from_millis(50)),
            },
            &metrics,
        );

        // Validator latency should average now at 75ms
        let validator_latency = stats
            .validator_stats
            .get(&validator)
            .unwrap()
            .average_latencies
            .get(&OperationType::Submit)
            .unwrap()
            .get();
        assert_eq!(validator_latency, Duration::from_millis(75));
    }

    #[tokio::test]
    async fn test_calculate_client_latency() {
        let config = ValidatorClientMonitorConfig {
            failure_cooldown: Duration::from_millis(100),
            max_consecutive_failures: 2,
            reliability_weight: 0.5,
            ..Default::default()
        };
        let mut stats = ClientObservedStats::new(config);
        let metrics = create_test_metrics();

        let validators = create_test_validator_names(3);
        let validator1 = validators[0]; // Good validator
        let validator2 = validators[1]; // Unreliable validator
        let validator3 = validators[2]; // Unknown validator

        let committee = Committee::new_for_testing_with_normalized_voting_power(
            0,
            validators.iter().map(|v| (*v, 1)).collect(),
        );

        println!("Case 1: Unknown validator should return MAX_LATENCY");
        {
            let latency = stats.get_all_validator_stats(&committee, TxType::SingleWriter);
            // MAX_LATENCY
            assert_eq!(*latency.get(&validator3).unwrap(), Duration::from_secs(10));
        }

        println!("Case 2: Good validator with FastPath operation");
        {
            stats.record_interaction_result(
                OperationFeedback {
                    authority_name: validator1,
                    display_name: validator1.concise().to_string(),
                    operation: OperationType::FastPath,
                    result: Ok(Duration::from_millis(100)), // 0.1s
                },
                &metrics,
            );

            let latency = stats.get_all_validator_stats(&committee, TxType::SingleWriter);
            // 100ms from history, without reliability penalty.
            assert_eq!(
                *latency.get(&validator1).unwrap(),
                Duration::from_millis(100)
            );
        }

        println!("Case 3: Good validator with Consensus operation");
        {
            stats.record_interaction_result(
                OperationFeedback {
                    authority_name: validator1,
                    display_name: validator1.concise().to_string(),
                    operation: OperationType::Consensus,
                    result: Ok(Duration::from_millis(200)), // 0.2s
                },
                &metrics,
            );

            let latency_shared = stats.get_all_validator_stats(&committee, TxType::SharedObject);
            // 200ms from history, without reliability penalty.
            assert_eq!(
                *latency_shared.get(&validator1).unwrap(),
                Duration::from_millis(200)
            );
        }

        println!("Case 4: Validator with reduced reliability");
        {
            stats.record_interaction_result(
                OperationFeedback {
                    authority_name: validator2,
                    display_name: validator2.concise().to_string(),
                    operation: OperationType::FastPath,
                    result: Ok(Duration::from_millis(100)),
                },
                &metrics,
            );

            // Add a failure to reduce reliability
            stats.record_interaction_result(
                OperationFeedback {
                    authority_name: validator2,
                    display_name: validator2.concise().to_string(),
                    operation: OperationType::Submit,
                    result: Err(()),
                },
                &metrics,
            );

            let latency = stats.get_all_validator_stats(&committee, TxType::SingleWriter);
            let validator2_latency = *latency.get(&validator2).unwrap();
            // Reliability should be 0.66, so latency ~= 0.1s + (1 - 0.66) * 0.5 * 10s ~= 0.1 + 1.66s ~= 1.76s
            assert!(
                (validator2_latency.as_secs_f64() - 1.766).abs() < 0.001,
                "{}",
                validator2_latency.as_secs_f64()
            );
        }

        println!("Case 5: Excluded validator should return MAX_LATENCY");
        {
            // Add enough failures to cause exclusion
            stats.record_interaction_result(
                OperationFeedback {
                    authority_name: validator2,
                    display_name: validator2.concise().to_string(),
                    operation: OperationType::Submit,
                    result: Err(()),
                },
                &metrics,
            );

            let latency = stats.get_all_validator_stats(&committee, TxType::SingleWriter);
            // MAX_LATENCY due to exclusion
            assert_eq!(*latency.get(&validator2).unwrap(), Duration::from_secs(10));
        }

        println!("Case 6: After cooldown, validator should be included again");
        {
            tokio::time::sleep(Duration::from_millis(150)).await;
            let latency = stats.get_all_validator_stats(&committee, TxType::SingleWriter);
            let validator2_latency = *latency.get(&validator2).unwrap();
            // Should be back to calculated latency, not MAX_LATENCY
            assert!(validator2_latency < Duration::from_secs(10));
            assert!(validator2_latency > Duration::ZERO);
        }
    }

    #[tokio::test]
    async fn test_reliability_weight() {
        let validators = create_test_validator_names(2);
        let validator = validators[0];
        let metrics = create_test_metrics();

        println!("Case 1: Test with reliability_weight = 1.0");
        {
            let config_half_weight = ValidatorClientMonitorConfig {
                reliability_weight: 1.0,
                ..Default::default()
            };
            let mut stats = ClientObservedStats::new(config_half_weight);

            // Good validator - no failures
            stats.record_interaction_result(
                OperationFeedback {
                    authority_name: validator,
                    display_name: validator.concise().to_string(),
                    operation: OperationType::FastPath,
                    result: Ok(Duration::from_millis(100)), // 0.1s
                },
                &metrics,
            );

            stats.record_interaction_result(
                OperationFeedback {
                    authority_name: validator,
                    display_name: validator.concise().to_string(),
                    operation: OperationType::Submit,
                    result: Err(()), // failure
                },
                &metrics,
            );

            let committee = Committee::new_for_testing_with_normalized_voting_power(
                0,
                validators.iter().map(|v| (*v, 1)).collect(),
            );

            // Get latencies for both configurations
            let latencies = stats.get_all_validator_stats(&committee, TxType::SingleWriter);
            let latency = *latencies.get(&validator).unwrap();
            assert!((latency.as_secs_f64() - 3.433).abs() < 0.001);
        }

        println!("Case 2: Test with reliability_weight = 0.0, should have no penalty");
        {
            let config_zero_weight = ValidatorClientMonitorConfig {
                reliability_weight: 0.0,
                ..Default::default()
            };
            let mut stats = ClientObservedStats::new(config_zero_weight);

            stats.record_interaction_result(
                OperationFeedback {
                    authority_name: validator,
                    display_name: validator.concise().to_string(),
                    operation: OperationType::FastPath,
                    result: Ok(Duration::from_millis(100)),
                },
                &metrics,
            );

            stats.record_interaction_result(
                OperationFeedback {
                    authority_name: validator,
                    display_name: validator.concise().to_string(),
                    operation: OperationType::Submit,
                    result: Err(()), // failure
                },
                &metrics,
            );

            let committee = Committee::new_for_testing_with_normalized_voting_power(
                0,
                validators.iter().map(|v| (*v, 1)).collect(),
            );

            let latencies = stats.get_all_validator_stats(&committee, TxType::SingleWriter);
            let latency = *latencies.get(&validator).unwrap();
            assert_eq!(latency, Duration::from_millis(100));
        }
    }
}

#[cfg(test)]
mod client_monitor_tests {
    use sui_types::messages_grpc::TxType;

    use crate::{
        authority_aggregator::{AuthorityAggregator, AuthorityAggregatorBuilder},
        test_authority_clients::MockAuthorityApi,
    };

    use super::*;
    use std::collections::HashSet;

    fn get_authority_aggregator(
        committee_size: usize,
    ) -> Arc<AuthorityAggregator<MockAuthorityApi>> {
        Arc::new(
            AuthorityAggregatorBuilder::from_committee_size(committee_size)
                .build_mock_authority_aggregator(),
        )
    }

    #[tokio::test]
    async fn test_validator_selection_top_k_basic() {
        let auth_agg = get_authority_aggregator(4);
        let monitor = ValidatorClientMonitor::new_for_test(auth_agg.clone());

        let committee = auth_agg.committee.clone();
        let validators = committee.names().cloned().collect::<Vec<_>>();

        // Record different performance for each validator
        for (i, validator) in validators.iter().enumerate() {
            monitor.record_interaction_result(OperationFeedback {
                authority_name: *validator,
                display_name: auth_agg.get_display_name(validator),
                operation: OperationType::FastPath,
                result: Ok(Duration::from_millis((i as u64 + 1) * 50)),
            });
        }

        // Force update cached latencies (in production this happens in the health check loop)
        monitor.force_update_cached_latencies(&auth_agg);

        // Select validators with delta = 100% (50, 100)
        let selected =
            monitor.select_shuffled_preferred_validators(&committee, TxType::SingleWriter, 1.0);
        assert_eq!(selected.len(), 4); // Should return all 4 validators from committee

        // The first 2 positions should contain the best two validators (but shuffled)
        let top_2_positions: HashSet<_> = selected.iter().take(2).cloned().collect();
        assert!(top_2_positions.contains(&validators[0])); // Best performer
        assert!(top_2_positions.contains(&validators[1])); // Second best

        // The third position should be validator[2] (third best)
        assert_eq!(selected[2], validators[2]);

        // The fourth position should be validator[3] (no stats recorded)
        assert_eq!(selected[3], validators[3]);
    }

    #[tokio::test]
    async fn test_validator_selection_with_failures() {
        let auth_agg = get_authority_aggregator(5);
        let monitor = ValidatorClientMonitor::new_for_test(auth_agg.clone());

        let committee = auth_agg.committee.clone();
        let validators = committee.names().cloned().collect::<Vec<_>>();

        // Record different performance for each validator
        for (i, validator) in validators.iter().enumerate() {
            monitor.record_interaction_result(OperationFeedback {
                authority_name: *validator,
                display_name: auth_agg.get_display_name(validator),
                operation: OperationType::FastPath,
                result: if i < 2 {
                    Ok(Duration::from_millis((i as u64 + 1) * 50))
                } else {
                    Err(())
                }, // First 2 validators succeed, others fail
            });
        }

        // Force update cached latencies (in production this happens in the health check loop)
        monitor.force_update_cached_latencies(&auth_agg);

        // Select validators with delta = 200% (50, 100, 150)
        let selected =
            monitor.select_shuffled_preferred_validators(&committee, TxType::SingleWriter, 2.0);

        // Should return all 5 validators
        assert_eq!(selected.len(), 5);

        // The first 3 positions should contain:
        // - validators[0] and validators[1] (successful, better latency)
        // - One of the failed validators (shuffled in top k)
        let top_3_positions: HashSet<_> = selected.iter().take(3).cloned().collect();
        assert!(top_3_positions.contains(&validators[0])); // Best performer with success
        assert!(top_3_positions.contains(&validators[1])); // Second best with success

        // Remaining positions should have the failed validators in latency order
        // Since they all have 0 reliability, they'll be ordered by latency
    }

    #[tokio::test]
    async fn test_validator_selection_empty_committee() {
        let auth_agg = get_authority_aggregator(2);
        let monitor = ValidatorClientMonitor::new_for_test(auth_agg.clone());

        let committee = auth_agg.committee.clone();
        let validators = committee.names().cloned().collect::<Vec<_>>();

        // Create a different committee that doesn't include our tracked validators
        let other_validators: Vec<AuthorityName> = (0..3)
            .map(|_| {
                let (_, key_pair): (_, AuthorityKeyPair) = get_key_pair();
                key_pair.public().into()
            })
            .collect();

        let other_committee = Committee::new_for_testing_with_normalized_voting_power(
            0,
            other_validators.iter().map(|v| (*v, 1)).collect(),
        );

        // Record performance for our original validators (not in committee)
        for validator in &validators {
            for op in [
                OperationType::Submit,
                OperationType::Effects,
                OperationType::HealthCheck,
            ] {
                monitor.record_interaction_result(OperationFeedback {
                    authority_name: *validator,
                    display_name: auth_agg.get_display_name(validator),
                    operation: op,
                    result: Ok(Duration::from_millis(100)),
                });
            }
        }

        // Force update cached latencies (in production this happens in the health check loop)
        monitor.force_update_cached_latencies(&auth_agg);

        // Should still select validators from the provided committee
        let selected = monitor.select_shuffled_preferred_validators(
            &other_committee,
            TxType::SingleWriter,
            1.0,
        );
        assert_eq!(selected.len(), 3); // Should return all 3 validators from other_committee
        for validator in &selected {
            assert!(other_committee.authority_exists(validator));
        }
    }

    #[tokio::test]
    async fn test_validator_selection_more_k_than_validators() {
        let auth_agg = get_authority_aggregator(2);
        let monitor = ValidatorClientMonitor::new_for_test(auth_agg.clone());

        let committee = auth_agg.committee.clone();
        let validators = committee.names().cloned().collect::<Vec<_>>();

        // Record performance for validators
        for validator in &validators {
            for op in [
                OperationType::Submit,
                OperationType::Effects,
                OperationType::HealthCheck,
            ] {
                monitor.record_interaction_result(OperationFeedback {
                    authority_name: *validator,
                    display_name: auth_agg.get_display_name(validator),
                    operation: op,
                    result: Ok(Duration::from_millis(100)),
                });
            }
        }

        // Force update cached latencies (in production this happens in the health check loop)
        monitor.force_update_cached_latencies(&auth_agg);

        // Request higher delta than actual values.
        let selected =
            monitor.select_shuffled_preferred_validators(&committee, TxType::SingleWriter, 1000.0);
        // Should return all available validators
        assert_eq!(selected.len(), 2);
        assert!(selected.contains(&validators[0]));
        assert!(selected.contains(&validators[1]));
    }

    // Testing the select_shuffled_preferred_validators both for the single writer and shared object tx types.
    #[tokio::test]
    async fn test_validator_selection_shared_object_tx_type() {
        let auth_agg = get_authority_aggregator(4);
        let monitor = ValidatorClientMonitor::new_for_test(auth_agg.clone());

        let committee = auth_agg.committee.clone();
        let validators = committee.names().cloned().collect::<Vec<_>>();

        // Record different performance per operation type for each validator
        for (i, validator) in validators.iter().enumerate() {
            monitor.record_interaction_result(OperationFeedback {
                authority_name: *validator,
                display_name: auth_agg.get_display_name(validator),
                operation: OperationType::FastPath,
                result: Ok(Duration::from_millis((i as u64 + 1) * 50)),
            });
        }

        for (i, validator) in validators.iter().rev().enumerate() {
            monitor.record_interaction_result(OperationFeedback {
                authority_name: *validator,
                display_name: auth_agg.get_display_name(validator),
                operation: OperationType::Consensus,
                result: Ok(Duration::from_millis((i as u64 + 1) * 50)),
            });
        }

        // Force update cached latencies (in production this happens in the health check loop)
        monitor.force_update_cached_latencies(&auth_agg);

        // Select validators with delta = 100% for the shared object tx type
        let selected =
            monitor.select_shuffled_preferred_validators(&committee, TxType::SingleWriter, 1.0);
        assert_eq!(selected.len(), 4); // Should return all 4 validators from committee

        // The first 2 positions should contain the best two validators (but shuffled)
        let top_2_positions: HashSet<_> = selected.iter().take(2).cloned().collect();
        assert!(top_2_positions.contains(&validators[0])); // Best performer
        assert!(top_2_positions.contains(&validators[1])); // Second best

        // Select the validators with delta = 100% for the single writer tx type
        let selected =
            monitor.select_shuffled_preferred_validators(&committee, TxType::SharedObject, 1.0);
        assert_eq!(selected.len(), 4); // Should return all 4 validators from committee

        // The first 2 positions should contain the best two validators (but shuffled)
        let top_2_positions: HashSet<_> = selected.iter().take(2).cloned().collect();
        assert!(top_2_positions.contains(&validators[2])); // Best performer
        assert!(top_2_positions.contains(&validators[3])); // Second best
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_stats_cleanup_on_authority_aggregator_change() {
        use arc_swap::ArcSwap;
        use prometheus::Registry;
        use tokio::time::{sleep, timeout};

        // Create initial committee with 3 validators
        let initial_auth_agg = get_authority_aggregator(3);
        let initial_validators: Vec<_> = initial_auth_agg.committee.names().cloned().collect();

        // Create monitor with shorter health check interval for testing
        let config = ValidatorClientMonitorConfig {
            health_check_interval: Duration::from_millis(50),
            ..Default::default()
        };

        let metrics = Arc::new(ValidatorClientMetrics::new(&Registry::default()));
        let auth_agg_swap = Arc::new(ArcSwap::new(initial_auth_agg.clone()));
        let monitor = ValidatorClientMonitor::new(config, metrics, auth_agg_swap.clone());

        // Record stats for all initial validators
        for validator in &initial_validators {
            monitor.record_interaction_result(OperationFeedback {
                authority_name: *validator,
                display_name: initial_auth_agg.get_display_name(validator),
                operation: OperationType::Submit,
                result: Ok(Duration::from_millis(100)),
            });
        }

        // Verify initial stats exist
        assert_eq!(monitor.get_client_stats_len(), 3);
        for validator in &initial_validators {
            assert!(monitor.has_validator_stats(validator));
        }

        // Create new committee with only 2 of the original validators
        let new_auth_agg = Arc::new(
            AuthorityAggregatorBuilder::from_committee(
                Committee::new_for_testing_with_normalized_voting_power(
                    0, // Same epoch
                    initial_validators.iter().take(2).map(|v| (*v, 1)).collect(),
                ),
            )
            .build_mock_authority_aggregator(),
        );

        // Update the authority aggregator
        auth_agg_swap.store(new_auth_agg.clone());

        // Wait for at least one health check cycle to complete
        // The monitor should clean up stats for validators not in the new committee
        let check_result = timeout(Duration::from_secs(2), async {
            loop {
                sleep(Duration::from_millis(100)).await;
                if monitor.get_client_stats_len() == 2 {
                    break;
                }
            }
        })
        .await;

        assert!(
            check_result.is_ok(),
            "Stats cleanup did not happen within timeout"
        );

        // Verify cleanup happened correctly
        assert_eq!(monitor.get_client_stats_len(), 2);
        assert!(monitor.has_validator_stats(&initial_validators[0]));
        assert!(monitor.has_validator_stats(&initial_validators[1]));
        assert!(!monitor.has_validator_stats(&initial_validators[2]));

        // Calculate the latencies for the validators and ensure this is successful
        monitor.force_update_cached_latencies(&initial_auth_agg);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_new_validators_added_after_committee_change() {
        use arc_swap::ArcSwap;
        use prometheus::Registry;
        use tokio::time::{sleep, timeout};

        // Create initial committee with 2 validators
        let initial_auth_agg = get_authority_aggregator(2);
        let initial_validators: Vec<_> = initial_auth_agg.committee.names().cloned().collect();

        // Create monitor with shorter health check interval for testing
        let config = ValidatorClientMonitorConfig {
            health_check_interval: Duration::from_millis(50),
            ..Default::default()
        };

        let metrics = Arc::new(ValidatorClientMetrics::new(&Registry::default()));
        let auth_agg_swap = Arc::new(ArcSwap::new(initial_auth_agg.clone()));
        let monitor = ValidatorClientMonitor::new(config, metrics, auth_agg_swap.clone());

        // Record stats for initial validators
        for validator in &initial_validators {
            monitor.record_interaction_result(OperationFeedback {
                authority_name: *validator,
                display_name: initial_auth_agg.get_display_name(validator),
                operation: OperationType::HealthCheck,
                result: Ok(Duration::from_millis(100)),
            });
        }

        // Create new validator that will be added
        let (_, new_key_pair): (_, AuthorityKeyPair) = get_key_pair();
        let new_validator: AuthorityName = new_key_pair.public().into();

        // Create new committee with all 3 validators (2 old + 1 new)
        let mut new_committee_members = initial_validators
            .iter()
            .map(|v| (*v, 1))
            .collect::<Vec<_>>();
        new_committee_members.push((new_validator, 1));

        let new_committee = Committee::new_for_testing_with_normalized_voting_power(
            0, // Same epoch
            new_committee_members.into_iter().collect(),
        );

        // Create new authority aggregator
        let new_auth_agg = Arc::new(
            AuthorityAggregatorBuilder::from_committee(new_committee)
                .build_mock_authority_aggregator(),
        );

        // Update the authority aggregator
        auth_agg_swap.store(new_auth_agg.clone());

        // Wait for health check to run and record stats for new validator
        let check_result = timeout(Duration::from_secs(2), async {
            loop {
                sleep(Duration::from_millis(100)).await;
                // Check if we have stats for all 3 validators including the new one
                if monitor.get_client_stats_len() == 3
                    && monitor.has_validator_stats(&new_validator)
                {
                    break;
                }
            }
        })
        .await;

        assert!(
            check_result.is_ok(),
            "New validator stats were not added within timeout"
        );

        // Verify all validators have stats
        assert_eq!(monitor.get_client_stats_len(), 3);
        for validator in &initial_validators {
            assert!(monitor.has_validator_stats(validator));
        }
        assert!(monitor.has_validator_stats(&new_validator));
    }
}
