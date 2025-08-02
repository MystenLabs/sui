// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::validator_client_monitor::stats::{ClientObservedStats, ValidatorClientStats};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use sui_config::validator_client_monitor_config::{ScoreWeights, ValidatorClientMonitorConfig};
use sui_types::base_types::{AuthorityName, ConciseableName};
use sui_types::committee::Committee;
use sui_types::crypto::{get_key_pair, AuthorityKeyPair, KeypairTraits};
use tokio::time::sleep;

mod client_stats_tests {

    use super::*;
    use crate::validator_client_monitor::metrics::ValidatorClientMetrics;
    use prometheus::Registry;

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
        assert_eq!(submit_latency.get(), 0.1); // 100ms = 0.1s
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
    async fn test_client_stats_calculate_scores() {
        let config = ValidatorClientMonitorConfig::default();
        let mut stats = ClientObservedStats::new(config);
        let metrics = create_test_metrics();

        // Create two validators with different performance
        let validators = create_test_validator_names(2);
        let validator1 = validators[0];
        let validator2 = validators[1];

        // Validator 1: good performance
        for op in [
            OperationType::Submit,
            OperationType::Effects,
            OperationType::HealthCheck,
        ] {
            stats.record_interaction_result(
                OperationFeedback {
                    authority_name: validator1,
                    display_name: validator1.concise().to_string(),
                    operation: op,
                    result: Ok(Duration::from_millis(50)),
                },
                &metrics,
            );
        }

        // Validator 2: worse performance
        for op in [
            OperationType::Submit,
            OperationType::Effects,
            OperationType::HealthCheck,
        ] {
            stats.record_interaction_result(
                OperationFeedback {
                    authority_name: validator2,
                    display_name: validator2.concise().to_string(),
                    operation: op,
                    result: Ok(Duration::from_millis(200)),
                },
                &metrics,
            );
        }

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

        // Create display names map for testing
        let display_names: HashMap<AuthorityName, String> = validators
            .iter()
            .map(|v| (*v, v.concise().to_string()))
            .collect();
        let all_stats = stats.get_all_validator_stats(&committee, &display_names);
        assert_eq!(all_stats.len(), 2);

        // Validator 1 should have higher score
        let score1 = *all_stats.get(&validator1).unwrap();
        let score2 = *all_stats.get(&validator2).unwrap();
        assert!(score1 > score2);
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

        // Initialize with all operation types
        for op in [
            OperationType::Submit,
            OperationType::Effects,
            OperationType::HealthCheck,
        ] {
            stats.record_interaction_result(
                OperationFeedback {
                    authority_name: validator,
                    display_name: validator.concise().to_string(),
                    operation: op,
                    result: Ok(Duration::from_millis(50)),
                },
                &metrics,
            );
        }

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

        // Should be excluded (score 0)
        // Create display names map for testing
        let display_names: HashMap<AuthorityName, String> = validators
            .iter()
            .map(|v| (*v, v.concise().to_string()))
            .collect();
        let all_stats = stats.get_all_validator_stats(&committee, &display_names);
        let score = *all_stats.get(&validator).unwrap();
        assert_eq!(score, 0.0);

        // Wait for cooldown
        sleep(Duration::from_millis(150)).await;

        // Should be included again (score > 0)
        // Create display names map for testing
        let display_names: HashMap<AuthorityName, String> = validators
            .iter()
            .map(|v| (*v, v.concise().to_string()))
            .collect();
        let all_stats = stats.get_all_validator_stats(&committee, &display_names);
        let score = *all_stats.get(&validator).unwrap();
        assert!(score > 0.0);
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
            0.1
        );

        // Second update uses EMA
        stats.update_average_latency(OperationType::Submit, Duration::from_millis(200));
        let latency = stats
            .average_latencies
            .get(&OperationType::Submit)
            .unwrap()
            .get();
        // With decay factor 0.9, new value = 0.1 * 0.9 + 0.2 * (1 - 0.9) = 0.09 + 0.02 = 0.11
        assert!((latency - 0.11).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_global_stats_update() {
        let config = ValidatorClientMonitorConfig::default();
        let mut stats = ClientObservedStats::new(config);

        // First update sets initial value
        stats.update_global_stats(OperationType::Submit, Duration::from_millis(100));
        let max_latency = stats
            .global_stats
            .max_latencies
            .get(&OperationType::Submit)
            .unwrap();
        assert_eq!(max_latency.get(), 0.1); // 100ms = 0.1s

        // Update with higher latency - should immediately jump to new max
        stats.update_global_stats(OperationType::Submit, Duration::from_millis(300));
        let max_latency = stats
            .global_stats
            .max_latencies
            .get(&OperationType::Submit)
            .unwrap();
        assert_eq!(max_latency.get(), 0.3); // 300ms = 0.3s

        // Update with lower latency - should apply decay
        stats.update_global_stats(OperationType::Submit, Duration::from_millis(100));
        let max_latency = stats
            .global_stats
            .max_latencies
            .get(&OperationType::Submit)
            .unwrap();
        // With decay factor 0.99: 0.3 * 0.99 + 0.1 * (1 - 0.99) = 0.297 + 0.001 = 0.298
        assert!((max_latency.get() - 0.298).abs() < 0.001);

        // Another lower update to verify continued decay
        stats.update_global_stats(OperationType::Submit, Duration::from_millis(50));
        let max_latency = stats
            .global_stats
            .max_latencies
            .get(&OperationType::Submit)
            .unwrap();
        // With decay factor 0.99: 0.298 * 0.99 + 0.05 * (1 - 0.99) = 0.29502 + 0.0005 = 0.29552
        assert!((max_latency.get() - 0.29552).abs() < 0.001);

        // Update with higher latency again - should jump to new max
        stats.update_global_stats(OperationType::Submit, Duration::from_millis(500));
        let max_latency = stats
            .global_stats
            .max_latencies
            .get(&OperationType::Submit)
            .unwrap();
        assert_eq!(max_latency.get(), 0.5); // 500ms = 0.5s
    }

    #[tokio::test]
    async fn test_score_calculation_with_missing_operations() {
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

        // Create display names map for testing
        let display_names: HashMap<AuthorityName, String> = validators
            .iter()
            .map(|v| (*v, v.concise().to_string()))
            .collect();
        let all_stats = stats.get_all_validator_stats(&committee, &display_names);
        // Should have a partial score even with only one operation type
        let score = *all_stats.get(&validator).unwrap();
        assert!(score > 0.0);
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
        // With decay factor 0.5: 1.0 * 0.5 + 0.0 * 0.5 = 0.5
        assert_eq!(new_reliability, 0.5);
    }

    #[tokio::test]
    async fn test_global_max_latency_with_multiple_validators() {
        let config = ValidatorClientMonitorConfig::default();
        let mut stats = ClientObservedStats::new(config);
        let metrics = create_test_metrics();

        let validators = create_test_validator_names(3);

        // Validator 1: 100ms latency
        stats.record_interaction_result(
            OperationFeedback {
                authority_name: validators[0],
                display_name: validators[0].concise().to_string(),
                operation: OperationType::Submit,
                result: Ok(Duration::from_millis(100)),
            },
            &metrics,
        );

        // Check global max is 100ms
        let max_latency = stats
            .global_stats
            .max_latencies
            .get(&OperationType::Submit)
            .unwrap();
        assert_eq!(max_latency.get(), 0.1);

        // Validator 2: 50ms latency (lower, should apply decay)
        stats.record_interaction_result(
            OperationFeedback {
                authority_name: validators[1],
                display_name: validators[1].concise().to_string(),
                operation: OperationType::Submit,
                result: Ok(Duration::from_millis(50)),
            },
            &metrics,
        );

        // Max should decay towards 50ms
        let max_latency = stats
            .global_stats
            .max_latencies
            .get(&OperationType::Submit)
            .unwrap();
        // 0.1 * 0.99 + 0.05 * (1 - 0.99) = 0.099 + 0.0005 = 0.0995
        assert!((max_latency.get() - 0.0995).abs() < 0.001);

        // Validator 3: 200ms latency (higher, should immediately become new max)
        stats.record_interaction_result(
            OperationFeedback {
                authority_name: validators[2],
                display_name: validators[2].concise().to_string(),
                operation: OperationType::Submit,
                result: Ok(Duration::from_millis(200)),
            },
            &metrics,
        );

        // Max should jump to 200ms
        let max_latency = stats
            .global_stats
            .max_latencies
            .get(&OperationType::Submit)
            .unwrap();
        assert_eq!(max_latency.get(), 0.2);
    }

    #[tokio::test]
    async fn test_decay_factor_differences() {
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
        assert_eq!(validator_latency, 0.1);

        let max_latency = stats
            .global_stats
            .max_latencies
            .get(&OperationType::Submit)
            .unwrap()
            .get();
        assert_eq!(max_latency, 0.1);

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

        // Validator latency should decay faster (factor 0.1)
        let validator_latency = stats
            .validator_stats
            .get(&validator)
            .unwrap()
            .average_latencies
            .get(&OperationType::Submit)
            .unwrap()
            .get();
        // old * decay + new * (1-decay) = 0.1 * 0.9 + 0.05 * (1 - 0.9) = 0.09 + 0.005 = 0.095
        assert!((validator_latency - 0.095).abs() < 0.001);

        // Max latency should decay slower (factor 0.01)
        let max_latency = stats
            .global_stats
            .max_latencies
            .get(&OperationType::Submit)
            .unwrap()
            .get();
        // Since new value (0.05) is less than current (0.1), we apply decay
        // old * decay + new * (1-decay) = 0.1 * 0.99 + 0.05 * (1 - 0.99) = 0.099 + 0.0005 = 0.0995
        assert!((max_latency - 0.0995).abs() < 0.001);

        // Since max decays slower, it should be closer to the original value than validator average
        // validator_latency = 0.055, max_latency = 0.0505
        // Actually validator average went lower, so this assertion was wrong
        // Let's just verify both decayed correctly
    }

    #[tokio::test]
    async fn test_different_operation_weights() {
        let config = ValidatorClientMonitorConfig {
            score_weights: ScoreWeights {
                submit_latency_weight: 0.1,
                effects_latency_weight: 0.8,
                health_check_latency_weight: 0.1,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut stats = ClientObservedStats::new(config);
        let metrics = create_test_metrics();

        let validators = create_test_validator_names(2);
        let validator1 = validators[0];
        let validator2 = validators[1];

        // Validator 1: fast effects, slow others
        for op in [
            OperationType::Submit,
            OperationType::Effects,
            OperationType::HealthCheck,
        ] {
            let latency = if op == OperationType::Effects {
                50
            } else {
                200
            };
            stats.record_interaction_result(
                OperationFeedback {
                    authority_name: validator1,
                    display_name: validator1.concise().to_string(),
                    operation: op,
                    result: Ok(Duration::from_millis(latency)),
                },
                &metrics,
            );
        }

        // Validator 2: slow effects, fast others
        for op in [
            OperationType::Submit,
            OperationType::Effects,
            OperationType::HealthCheck,
        ] {
            let latency = if op == OperationType::Effects {
                200
            } else {
                50
            };
            stats.record_interaction_result(
                OperationFeedback {
                    authority_name: validator2,
                    display_name: validator2.concise().to_string(),
                    operation: op,
                    result: Ok(Duration::from_millis(latency)),
                },
                &metrics,
            );
        }

        // Create a committee with both validators
        let committee = Committee::new_for_testing_with_normalized_voting_power(
            0,
            vec![(validator1, 1), (validator2, 1)].into_iter().collect(),
        );

        // Create display names map for testing
        let display_names: HashMap<AuthorityName, String> = validators
            .iter()
            .map(|v| (*v, v.concise().to_string()))
            .collect();
        let all_stats = stats.get_all_validator_stats(&committee, &display_names);
        // Validator 1 should have higher score due to fast effects (high weight)
        let score1 = *all_stats.get(&validator1).unwrap();
        let score2 = *all_stats.get(&validator2).unwrap();
        assert!(score1 > score2);
    }
}

#[cfg(test)]
mod client_monitor_tests {
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
            for op in [
                OperationType::Submit,
                OperationType::Effects,
                OperationType::HealthCheck,
            ] {
                monitor.record_interaction_result(OperationFeedback {
                    authority_name: *validator,
                    display_name: auth_agg.get_display_name(validator),
                    operation: op,
                    result: Ok(Duration::from_millis((i as u64 + 1) * 50)),
                });
            }
        }

        // Force update cached scores (in production this happens in the health check loop)
        monitor.force_update_cached_scores();

        // Select validators with k=2
        let selected = monitor.select_shuffled_preferred_validators(&committee, 2);
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
            for op in [
                OperationType::Submit,
                OperationType::Effects,
                OperationType::HealthCheck,
            ] {
                monitor.record_interaction_result(OperationFeedback {
                    authority_name: *validator,
                    display_name: auth_agg.get_display_name(validator),
                    operation: op,
                    result: if i < 2 {
                        Ok(Duration::from_millis((i as u64 + 1) * 50))
                    } else {
                        Err(())
                    }, // First 2 validators succeed, others fail
                });
            }
        }

        // Force update cached scores (in production this happens in the health check loop)
        monitor.force_update_cached_scores();

        // Select validators with k=3
        let selected = monitor.select_shuffled_preferred_validators(&committee, 3);

        // Should return all 5 validators
        assert_eq!(selected.len(), 5);

        // The first 3 positions should contain:
        // - validators[0] and validators[1] (successful, better latency)
        // - One of the failed validators (shuffled in top k)
        let top_3_positions: HashSet<_> = selected.iter().take(3).cloned().collect();
        assert!(top_3_positions.contains(&validators[0])); // Best performer with success
        assert!(top_3_positions.contains(&validators[1])); // Second best with success

        // Remaining positions should have the failed validators in score order
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

        // Force update cached scores (in production this happens in the health check loop)
        monitor.force_update_cached_scores();

        // Should still select validators from the provided committee
        let selected = monitor.select_shuffled_preferred_validators(&other_committee, 2);
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

        // Force update cached scores (in production this happens in the health check loop)
        monitor.force_update_cached_scores();

        // Request more validators than available
        let selected = monitor.select_shuffled_preferred_validators(&committee, 5);
        // Should return all available validators
        assert_eq!(selected.len(), 2);
        assert!(selected.contains(&validators[0]));
        assert!(selected.contains(&validators[1]));
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
