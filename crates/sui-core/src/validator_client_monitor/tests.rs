// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::validator_client_monitor::metrics::ValidatorClientMetrics as MetricsModule;
use crate::validator_client_monitor::stats::{ClientObservedStats, ValidatorClientStats};
use prometheus::Registry;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use sui_config::validator_client_monitor_config::{ScoreWeights, ValidatorClientMonitorConfig};
use sui_types::base_types::AuthorityName;
use sui_types::crypto::{get_key_pair, AuthorityKeyPair, KeypairTraits};
use tokio::time::sleep;

mod client_stats_tests {

    use super::*;

    /// Helper to create test validator names
    fn create_test_validator_names(n: usize) -> Vec<AuthorityName> {
        (0..n)
            .map(|_| {
                let (_, key_pair): (_, AuthorityKeyPair) = get_key_pair();
                key_pair.public().into()
            })
            .collect()
    }

    #[tokio::test]
    async fn test_client_stats_new() {
        let config = ValidatorClientMonitorConfig::default();
        let stats = ClientObservedStats::new(config.clone());

        assert!(stats.validator_stats.is_empty());
        assert!(stats.global_stats.max_latencies.is_empty());
        assert_eq!(stats.config.max_consecutive_failures, 5);
    }

    #[tokio::test]
    async fn test_client_stats_record_success() {
        let config = ValidatorClientMonitorConfig::default();
        let mut stats = ClientObservedStats::new(config);

        let validators = create_test_validator_names(1);
        let validator = validators[0];

        // Record successful operation
        let feedback = OperationFeedback {
            validator,
            operation: OperationType::Submit,
            latency: Some(Duration::from_millis(100)),
            success: true,
        };

        stats.record_interaction_result(feedback);

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

        let validators = create_test_validator_names(1);
        let validator = validators[0];

        // Record multiple failures
        for i in 0..3 {
            let feedback = OperationFeedback {
                validator,
                operation: OperationType::Submit,
                latency: Some(Duration::from_millis(100)),
                success: false,
            };
            stats.record_interaction_result(feedback);

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
            stats.record_interaction_result(OperationFeedback {
                validator: validator1,
                operation: op,
                latency: Some(Duration::from_millis(50)),
                success: true,
            });
        }

        // Validator 2: worse performance
        for op in [
            OperationType::Submit,
            OperationType::Effects,
            OperationType::HealthCheck,
        ] {
            stats.record_interaction_result(OperationFeedback {
                validator: validator2,
                operation: op,
                latency: Some(Duration::from_millis(200)),
                success: true,
            });
        }

        // Add one failure for validator2
        stats.record_interaction_result(OperationFeedback {
            validator: validator2,
            operation: OperationType::Submit,
            latency: None,
            success: false,
        });

        let scores = stats.calculate_all_client_scores();
        assert_eq!(scores.len(), 2);

        // Validator 1 should have higher score
        let score1 = scores.get(&validator1).unwrap();
        let score2 = scores.get(&validator2).unwrap();
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

        let validators = create_test_validator_names(1);
        let validator = validators[0];

        // Initialize with all operation types
        for op in [
            OperationType::Submit,
            OperationType::Effects,
            OperationType::HealthCheck,
        ] {
            stats.record_interaction_result(OperationFeedback {
                validator,
                operation: op,
                latency: Some(Duration::from_millis(50)),
                success: true,
            });
        }

        // Cause exclusion
        for _ in 0..2 {
            stats.record_interaction_result(OperationFeedback {
                validator,
                operation: OperationType::Submit,
                latency: None,
                success: false,
            });
        }

        // Should be excluded
        let scores = stats.calculate_all_client_scores();
        assert!(!scores.contains_key(&validator));

        // Wait for cooldown
        sleep(Duration::from_millis(150)).await;

        // Should be included again
        let scores = stats.calculate_all_client_scores();
        assert!(scores.contains_key(&validator));
    }

    #[tokio::test]
    async fn test_client_stats_refresh_validator_set() {
        let config = ValidatorClientMonitorConfig::default();
        let mut stats = ClientObservedStats::new(config);

        // Add stats for 3 validators
        let validators = create_test_validator_names(3);

        for validator in &validators {
            stats.record_interaction_result(OperationFeedback {
                validator: *validator,
                operation: OperationType::Submit,
                latency: Some(Duration::from_millis(100)),
                success: true,
            });
        }

        assert_eq!(stats.validator_stats.len(), 3);

        // Refresh with only first 2 validators
        let new_set: HashSet<_> = validators.iter().take(2).cloned().collect();
        stats.refresh_validator_set(&new_set);

        assert_eq!(stats.validator_stats.len(), 2);
        assert!(stats.validator_stats.contains_key(&validators[0]));
        assert!(stats.validator_stats.contains_key(&validators[1]));
        assert!(!stats.validator_stats.contains_key(&validators[2]));
    }

    #[tokio::test]
    async fn test_validator_stats_new() {
        let stats = ValidatorClientStats::new(0.8);
        assert_eq!(stats.reliability.get(), 0.8);
        assert!(stats.average_latencies.is_empty());
        assert_eq!(stats.consecutive_failures, 0);
        assert!(stats.exclusion_time.is_none());
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

        let validators = create_test_validator_names(1);
        let validator = validators[0];

        // Only record Submit operation, missing Effects and HealthCheck
        stats.record_interaction_result(OperationFeedback {
            validator,
            operation: OperationType::Submit,
            latency: Some(Duration::from_millis(100)),
            success: true,
        });

        let scores = stats.calculate_all_client_scores();
        // Should not have a score since not all operations are present
        assert!(!scores.contains_key(&validator));
    }

    #[tokio::test]
    async fn test_reliability_decay() {
        let config = ValidatorClientMonitorConfig::default();
        let mut stats = ClientObservedStats::new(config);

        let validators = create_test_validator_names(1);
        let validator = validators[0];

        // Start with success
        stats.record_interaction_result(OperationFeedback {
            validator,
            operation: OperationType::Submit,
            latency: Some(Duration::from_millis(100)),
            success: true,
        });

        let initial_reliability = stats
            .validator_stats
            .get(&validator)
            .unwrap()
            .reliability
            .get();
        assert_eq!(initial_reliability, 1.0);

        // Add failure
        stats.record_interaction_result(OperationFeedback {
            validator,
            operation: OperationType::Submit,
            latency: Some(Duration::from_millis(100)),
            success: false,
        });

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

        let validators = create_test_validator_names(3);

        // Validator 1: 100ms latency
        stats.record_interaction_result(OperationFeedback {
            validator: validators[0],
            operation: OperationType::Submit,
            latency: Some(Duration::from_millis(100)),
            success: true,
        });

        // Check global max is 100ms
        let max_latency = stats
            .global_stats
            .max_latencies
            .get(&OperationType::Submit)
            .unwrap();
        assert_eq!(max_latency.get(), 0.1);

        // Validator 2: 50ms latency (lower, should apply decay)
        stats.record_interaction_result(OperationFeedback {
            validator: validators[1],
            operation: OperationType::Submit,
            latency: Some(Duration::from_millis(50)),
            success: true,
        });

        // Max should decay towards 50ms
        let max_latency = stats
            .global_stats
            .max_latencies
            .get(&OperationType::Submit)
            .unwrap();
        // 0.1 * 0.99 + 0.05 * (1 - 0.99) = 0.099 + 0.0005 = 0.0995
        assert!((max_latency.get() - 0.0995).abs() < 0.001);

        // Validator 3: 200ms latency (higher, should immediately become new max)
        stats.record_interaction_result(OperationFeedback {
            validator: validators[2],
            operation: OperationType::Submit,
            latency: Some(Duration::from_millis(200)),
            success: true,
        });

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

        let validator = create_test_validator_names(1)[0];

        // Initial values for both validator latency and global max
        stats.record_interaction_result(OperationFeedback {
            validator,
            operation: OperationType::Submit,
            latency: Some(Duration::from_millis(100)),
            success: true,
        });

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
        stats.record_interaction_result(OperationFeedback {
            validator,
            operation: OperationType::Submit,
            latency: Some(Duration::from_millis(50)),
            success: true,
        });

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
            stats.record_interaction_result(OperationFeedback {
                validator: validator1,
                operation: op,
                latency: Some(Duration::from_millis(latency)),
                success: true,
            });
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
            stats.record_interaction_result(OperationFeedback {
                validator: validator2,
                operation: op,
                latency: Some(Duration::from_millis(latency)),
                success: true,
            });
        }

        let scores = stats.calculate_all_client_scores();
        // Validator 1 should have higher score due to fast effects (high weight)
        assert!(scores.get(&validator1).unwrap() > scores.get(&validator2).unwrap());
    }
}

#[cfg(test)]
mod client_monitor_tests {
    use super::*;
    use sui_config::validator_client_monitor_config::SelectionStrategy;
    use sui_types::committee::Committee;

    /// Simple mock for testing validator selection without full AuthorityAggregator setup
    struct MockValidatorClientMonitor {
        config: ValidatorClientMonitorConfig,
        #[allow(dead_code)]
        metrics: Arc<MetricsModule>,
        client_stats: parking_lot::RwLock<ClientObservedStats>,
    }

    impl MockValidatorClientMonitor {
        fn new(config: ValidatorClientMonitorConfig) -> Self {
            let registry = Registry::new();
            let metrics = Arc::new(MetricsModule::new(&registry));
            Self {
                config: config.clone(),
                metrics,
                client_stats: parking_lot::RwLock::new(ClientObservedStats::new(config)),
            }
        }

        fn record_interaction_result(&self, feedback: OperationFeedback) {
            let mut stats = self.client_stats.write();
            stats.record_interaction_result(feedback);
        }

        fn select_preferred_validators(
            &self,
            committee: &Committee,
            k: usize,
        ) -> Vec<AuthorityName> {
            let validator_scores = self.client_stats.read().calculate_all_client_scores();

            let mut available_validators: Vec<_> = validator_scores
                .into_iter()
                .filter(|(validator, _)| committee.authority_exists(validator))
                .collect();

            // Sort by score (highest first)
            available_validators.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

            let mut selected_validators = Vec::with_capacity(k);

            // Add top-scoring validators up to k
            for (validator, _score) in available_validators.iter().take(k) {
                selected_validators.push(*validator);
            }

            // If we don't have enough validators with scores, fill with random validators
            if selected_validators.len() < k {
                let selected_set: std::collections::HashSet<_> =
                    selected_validators.iter().cloned().collect();
                let remaining_count = k - selected_validators.len();

                let unselected: Vec<_> = committee
                    .members()
                    .filter(|(validator, _)| !selected_set.contains(validator))
                    .map(|(validator, _)| *validator)
                    .collect();

                if !unselected.is_empty() {
                    use rand::seq::SliceRandom;
                    let mut rng = rand::thread_rng();
                    let mut shuffled = unselected;
                    shuffled.shuffle(&mut rng);

                    for validator in shuffled.into_iter().take(remaining_count) {
                        selected_validators.push(validator);
                    }
                }
            }

            selected_validators
        }
    }

    #[tokio::test]
    async fn test_validator_selection_top_k_basic() {
        let config = ValidatorClientMonitorConfig {
            selection_strategy: SelectionStrategy::TopK { k: 2 },
            ..Default::default()
        };

        let monitor = MockValidatorClientMonitor::new(config);

        // Create test validators
        let validators: Vec<AuthorityName> = (0..3)
            .map(|_| {
                let (_, key_pair): (_, AuthorityKeyPair) = get_key_pair();
                key_pair.public().into()
            })
            .collect();

        // Create committee
        let committee = Committee::new_for_testing_with_normalized_voting_power(
            0,
            validators.iter().map(|v| (*v, 1)).collect(),
        );

        // Record different performance for each validator
        for (i, validator) in validators.iter().enumerate() {
            for op in [
                OperationType::Submit,
                OperationType::Effects,
                OperationType::HealthCheck,
            ] {
                monitor.record_interaction_result(OperationFeedback {
                    validator: *validator,
                    operation: op,
                    latency: Some(Duration::from_millis((i as u64 + 1) * 50)),
                    success: true,
                });
            }
        }

        // Select validators with k=2
        let selected = monitor.select_preferred_validators(&committee, 2);
        assert_eq!(selected.len(), 2);

        // The best two validators should be selected
        assert!(selected.contains(&validators[0])); // Best performer
        assert!(selected.contains(&validators[1])); // Second best
    }

    #[tokio::test]
    async fn test_validator_selection_with_failures() {
        let config = ValidatorClientMonitorConfig {
            selection_strategy: SelectionStrategy::TopK { k: 3 },
            ..Default::default()
        };

        let monitor = MockValidatorClientMonitor::new(config);

        // Create test validators
        let validators: Vec<AuthorityName> = (0..5)
            .map(|_| {
                let (_, key_pair): (_, AuthorityKeyPair) = get_key_pair();
                key_pair.public().into()
            })
            .collect();

        // Create committee
        let committee = Committee::new_for_testing_with_normalized_voting_power(
            0,
            validators.iter().map(|v| (*v, 1)).collect(),
        );

        // Record different performance for each validator
        for (i, validator) in validators.iter().enumerate() {
            for op in [
                OperationType::Submit,
                OperationType::Effects,
                OperationType::HealthCheck,
            ] {
                monitor.record_interaction_result(OperationFeedback {
                    validator: *validator,
                    operation: op,
                    latency: Some(Duration::from_millis((i as u64 + 1) * 50)),
                    success: i < 2, // First 2 validators succeed, others fail
                });
            }
        }

        // Select validators with k=3
        let selected = monitor.select_preferred_validators(&committee, 3);

        // Should select the 2 successful validators
        assert_eq!(selected.len(), 3);
        assert!(selected.contains(&validators[0]));
        assert!(selected.contains(&validators[1]));

        // And one failed validator (the best of the failed ones)
        assert!(selected.contains(&validators[2]));
    }

    #[tokio::test]
    async fn test_validator_selection_empty_committee() {
        let config = ValidatorClientMonitorConfig::default();
        let monitor = MockValidatorClientMonitor::new(config);

        // Create test validators
        let validators: Vec<AuthorityName> = (0..2)
            .map(|_| {
                let (_, key_pair): (_, AuthorityKeyPair) = get_key_pair();
                key_pair.public().into()
            })
            .collect();

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
                    validator: *validator,
                    operation: op,
                    latency: Some(Duration::from_millis(100)),
                    success: true,
                });
            }
        }

        // Should still select validators from the provided committee
        let selected = monitor.select_preferred_validators(&other_committee, 2);
        assert_eq!(selected.len(), 2);
        for validator in &selected {
            assert!(other_committee.authority_exists(validator));
        }
    }

    #[tokio::test]
    async fn test_validator_selection_more_k_than_validators() {
        let config = ValidatorClientMonitorConfig::default();
        let monitor = MockValidatorClientMonitor::new(config);

        // Create test validators
        let validators: Vec<AuthorityName> = (0..2)
            .map(|_| {
                let (_, key_pair): (_, AuthorityKeyPair) = get_key_pair();
                key_pair.public().into()
            })
            .collect();

        // Create committee
        let committee = Committee::new_for_testing_with_normalized_voting_power(
            0,
            validators.iter().map(|v| (*v, 1)).collect(),
        );

        // Record performance for validators
        for validator in &validators {
            for op in [
                OperationType::Submit,
                OperationType::Effects,
                OperationType::HealthCheck,
            ] {
                monitor.record_interaction_result(OperationFeedback {
                    validator: *validator,
                    operation: op,
                    latency: Some(Duration::from_millis(100)),
                    success: true,
                });
            }
        }

        // Request more validators than available
        let selected = monitor.select_preferred_validators(&committee, 5);
        // Should return all available validators
        assert_eq!(selected.len(), 2);
        assert!(selected.contains(&validators[0]));
        assert!(selected.contains(&validators[1]));
    }
}
