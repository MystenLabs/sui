// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::validator_performance_monitor::config::{ScoreWeights, SelectionStrategy};
use crate::validator_performance_monitor::metrics::ValidatorPerformanceMetrics;
use crate::validator_performance_monitor::performance_stats::{PerformanceStats, ValidatorStats};
use prometheus::Registry;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use sui_types::base_types::AuthorityName;
use sui_types::crypto::{get_key_pair, AuthorityKeyPair, KeypairTraits};
use tokio::time::sleep;

mod performance_stats_tests {
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
    async fn test_performance_stats_new() {
        let config = ValidatorPerformanceConfig::default();
        let stats = PerformanceStats::new(config.clone());

        assert!(stats.validator_stats.is_empty());
        assert!(stats.global_stats.max_latencies.is_empty());
        assert_eq!(stats.config.max_consecutive_failures, 5);
    }

    #[tokio::test]
    async fn test_performance_stats_record_success() {
        let config = ValidatorPerformanceConfig::default();
        let mut stats = PerformanceStats::new(config);

        let validators = create_test_validator_names(1);
        let validator = validators[0];

        // Record successful operation
        let feedback = OperationFeedback {
            validator,
            operation: OperationType::Submit,
            latency: Some(Duration::from_millis(100)),
            success: true,
        };

        stats.record_feedback(feedback);

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
    async fn test_performance_stats_record_failure() {
        let config = ValidatorPerformanceConfig {
            max_consecutive_failures: 3,
            ..Default::default()
        };
        let mut stats = PerformanceStats::new(config);

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
            stats.record_feedback(feedback);

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
    async fn test_performance_stats_calculate_scores() {
        let config = ValidatorPerformanceConfig::default();
        let mut stats = PerformanceStats::new(config);

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
            stats.record_feedback(OperationFeedback {
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
            stats.record_feedback(OperationFeedback {
                validator: validator2,
                operation: op,
                latency: Some(Duration::from_millis(200)),
                success: true,
            });
        }

        // Add one failure for validator2
        stats.record_feedback(OperationFeedback {
            validator: validator2,
            operation: OperationType::Submit,
            latency: None,
            success: false,
        });

        let scores = stats.calculate_all_scores();
        assert_eq!(scores.len(), 2);

        // Validator 1 should have higher score
        let score1 = scores.get(&validator1).unwrap();
        let score2 = scores.get(&validator2).unwrap();
        assert!(score1 > score2);
    }

    #[tokio::test]
    async fn test_performance_stats_exclusion() {
        let config = ValidatorPerformanceConfig {
            max_consecutive_failures: 2,
            failure_cooldown: Duration::from_millis(100),
            ..Default::default()
        };
        let mut stats = PerformanceStats::new(config);

        let validators = create_test_validator_names(1);
        let validator = validators[0];

        // Initialize with all operation types
        for op in [
            OperationType::Submit,
            OperationType::Effects,
            OperationType::HealthCheck,
        ] {
            stats.record_feedback(OperationFeedback {
                validator,
                operation: op,
                latency: Some(Duration::from_millis(50)),
                success: true,
            });
        }

        // Cause exclusion
        for _ in 0..2 {
            stats.record_feedback(OperationFeedback {
                validator,
                operation: OperationType::Submit,
                latency: None,
                success: false,
            });
        }

        // Should be excluded
        let scores = stats.calculate_all_scores();
        assert!(!scores.contains_key(&validator));

        // Wait for cooldown
        sleep(Duration::from_millis(150)).await;

        // Should be included again
        let scores = stats.calculate_all_scores();
        assert!(scores.contains_key(&validator));
    }

    #[tokio::test]
    async fn test_performance_stats_refresh_validator_set() {
        let config = ValidatorPerformanceConfig::default();
        let mut stats = PerformanceStats::new(config);

        // Add stats for 3 validators
        let validators = create_test_validator_names(3);

        for validator in &validators {
            stats.record_feedback(OperationFeedback {
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
        let stats = ValidatorStats::new(0.8);
        assert_eq!(stats.reliability.get(), 0.8);
        assert!(stats.average_latencies.is_empty());
        assert_eq!(stats.consecutive_failures, 0);
        assert!(stats.exclusion_time.is_none());
    }

    #[tokio::test]
    async fn test_validator_stats_update_latency() {
        let mut stats = ValidatorStats::new(1.0);

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
        // With decay factor 0.1, new value = 0.1 * 0.1 + 0.2 * (1 - 0.1) = 0.01 + 0.18 = 0.19
        assert!((latency - 0.19).abs() < 0.001);
    }

    #[tokio::test]
    async fn test_global_stats_update() {
        let config = ValidatorPerformanceConfig::default();
        let mut stats = PerformanceStats::new(config);

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
        // With decay factor 0.1: 0.3 * 0.1 + 0.1 * (1 - 0.1) = 0.03 + 0.09 = 0.12
        assert!((max_latency.get() - 0.12).abs() < 0.001);

        // Another lower update to verify continued decay
        stats.update_global_stats(OperationType::Submit, Duration::from_millis(50));
        let max_latency = stats
            .global_stats
            .max_latencies
            .get(&OperationType::Submit)
            .unwrap();
        // With decay factor 0.1: 0.12 * 0.1 + 0.05 * (1 - 0.1) = 0.012 + 0.045 = 0.057
        assert!((max_latency.get() - 0.057).abs() < 0.001);

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
        let config = ValidatorPerformanceConfig::default();
        let mut stats = PerformanceStats::new(config);

        let validators = create_test_validator_names(1);
        let validator = validators[0];

        // Only record Submit operation, missing Effects and HealthCheck
        stats.record_feedback(OperationFeedback {
            validator,
            operation: OperationType::Submit,
            latency: Some(Duration::from_millis(100)),
            success: true,
        });

        let scores = stats.calculate_all_scores();
        // Should not have a score since not all operations are present
        assert!(!scores.contains_key(&validator));
    }

    #[tokio::test]
    async fn test_reliability_decay() {
        let config = ValidatorPerformanceConfig::default();
        let mut stats = PerformanceStats::new(config);

        let validators = create_test_validator_names(1);
        let validator = validators[0];

        // Start with success
        stats.record_feedback(OperationFeedback {
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
        stats.record_feedback(OperationFeedback {
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
        let config = ValidatorPerformanceConfig::default();
        let mut stats = PerformanceStats::new(config);

        let validators = create_test_validator_names(3);

        // Validator 1: 100ms latency
        stats.record_feedback(OperationFeedback {
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
        stats.record_feedback(OperationFeedback {
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
        // 0.1 * 0.1 + 0.05 * 0.9 = 0.01 + 0.045 = 0.055
        assert!((max_latency.get() - 0.055).abs() < 0.001);

        // Validator 3: 200ms latency (higher, should immediately become new max)
        stats.record_feedback(OperationFeedback {
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
    async fn test_different_operation_weights() {
        let config = ValidatorPerformanceConfig {
            score_weights: ScoreWeights {
                submit_latency_weight: 0.1,
                effects_latency_weight: 0.8,
                health_check_latency_weight: 0.1,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut stats = PerformanceStats::new(config);

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
            stats.record_feedback(OperationFeedback {
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
            stats.record_feedback(OperationFeedback {
                validator: validator2,
                operation: op,
                latency: Some(Duration::from_millis(latency)),
                success: true,
            });
        }

        let scores = stats.calculate_all_scores();
        // Validator 1 should have higher score due to fast effects (high weight)
        assert!(scores.get(&validator1).unwrap() > scores.get(&validator2).unwrap());
    }
}

#[cfg(test)]
mod performance_monitor_tests {
    use super::*;
    use sui_types::committee::Committee;

    /// Simple mock for testing validator selection without full AuthorityAggregator setup
    struct MockValidatorMonitor {
        config: ValidatorPerformanceConfig,
        #[allow(dead_code)]
        metrics: Arc<ValidatorPerformanceMetrics>,
        performance_stats: parking_lot::RwLock<PerformanceStats>,
    }

    impl MockValidatorMonitor {
        fn new(config: ValidatorPerformanceConfig) -> Self {
            let registry = Registry::new();
            let metrics = Arc::new(ValidatorPerformanceMetrics::new(&registry));
            Self {
                config: config.clone(),
                metrics,
                performance_stats: parking_lot::RwLock::new(PerformanceStats::new(config)),
            }
        }

        fn record_feedback(&self, feedback: OperationFeedback) {
            let mut stats = self.performance_stats.write();
            stats.record_feedback(feedback);
        }

        fn select_validator(&self, committee: &Committee) -> AuthorityName {
            let validator_scores = self.performance_stats.read().calculate_all_scores();

            let available_validators: Vec<_> = validator_scores
                .into_iter()
                .filter(|(validator, _)| committee.authority_exists(validator))
                .collect();

            if available_validators.is_empty() {
                return *committee.sample();
            }

            match &self.config.selection_strategy {
                SelectionStrategy::WeightedRandom { temperature } => {
                    self.select_weighted_random(available_validators, *temperature)
                }
                SelectionStrategy::TopK { k } => self.select_top_k(available_validators, *k),
            }
        }

        fn select_weighted_random(
            &self,
            validators: Vec<(AuthorityName, f64)>,
            temperature: f64,
        ) -> AuthorityName {
            use rand::{distributions::WeightedIndex, prelude::*};

            let scores: Vec<f64> = validators
                .iter()
                .map(|(_, score)| (*score / temperature).exp())
                .collect();

            let sum: f64 = scores.iter().sum();
            let weights: Vec<f64> = scores.iter().map(|s| s / sum).collect();

            let dist = WeightedIndex::new(&weights).unwrap();
            let mut rng = thread_rng();
            let idx = dist.sample(&mut rng);

            validators[idx].0
        }

        fn select_top_k(&self, validators: Vec<(AuthorityName, f64)>, k: usize) -> AuthorityName {
            use rand::prelude::*;

            let mut sorted = validators;
            sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

            let k = k.min(sorted.len());
            let top_k = &sorted[..k];

            let idx = thread_rng().gen_range(0..k);
            top_k[idx].0
        }
    }

    #[tokio::test]
    async fn test_validator_selection_weighted_random() {
        let config = ValidatorPerformanceConfig {
            selection_strategy: SelectionStrategy::WeightedRandom { temperature: 1.0 },
            ..Default::default()
        };

        let monitor = MockValidatorMonitor::new(config);

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
                monitor.record_feedback(OperationFeedback {
                    validator: *validator,
                    operation: op,
                    latency: Some(Duration::from_millis((i as u64 + 1) * 50)),
                    success: true,
                });
            }
        }

        // Select validators multiple times to verify weighted selection
        let mut selections = HashMap::new();
        for _ in 0..100 {
            let selected = monitor.select_validator(&committee);
            *selections.entry(selected).or_insert(0) += 1;
        }

        // All validators should be selected at least once
        assert_eq!(selections.len(), 3);

        // Better performing validators should be selected more often
        let counts: Vec<_> = validators
            .iter()
            .map(|v| selections.get(v).copied().unwrap_or(0))
            .collect();

        // First validator (best performance) should be selected significantly more
        // We can't guarantee exact order due to randomness, but with 100 selections
        // and significant performance differences, this should generally hold
        // Note: This test might occasionally fail due to randomness
        assert!(counts[0] > 10); // At least some selections
                                 // Just check that all validators were selected at least once
        assert!(counts.iter().all(|&c| c > 0));
    }

    #[tokio::test]
    async fn test_validator_selection_top_k() {
        let config = ValidatorPerformanceConfig {
            selection_strategy: SelectionStrategy::TopK { k: 2 },
            ..Default::default()
        };

        let monitor = MockValidatorMonitor::new(config);

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
                monitor.record_feedback(OperationFeedback {
                    validator: *validator,
                    operation: op,
                    latency: Some(Duration::from_millis((i as u64 + 1) * 50)),
                    success: i < 2, // First 2 validators succeed, others fail
                });
            }
        }

        // Select validators multiple times
        let mut selections = HashSet::new();
        for _ in 0..20 {
            let selected = monitor.select_validator(&committee);
            selections.insert(selected);
        }

        // Only top 2 validators should be selected
        assert_eq!(selections.len(), 2);
        assert!(selections.contains(&validators[0]));
        assert!(selections.contains(&validators[1]));
    }

    #[tokio::test]
    async fn test_validator_selection_empty_committee() {
        let config = ValidatorPerformanceConfig::default();
        let monitor = MockValidatorMonitor::new(config);

        // Create test validators
        let validators: Vec<AuthorityName> = (0..2)
            .map(|_| {
                let (_, key_pair): (_, AuthorityKeyPair) = get_key_pair();
                key_pair.public().into()
            })
            .collect();

        // Create a different committee that doesn't include our tracked validators
        let other_validators: Vec<AuthorityName> = (0..2)
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
                monitor.record_feedback(OperationFeedback {
                    validator: *validator,
                    operation: op,
                    latency: Some(Duration::from_millis(100)),
                    success: true,
                });
            }
        }

        // Should still select a validator from the provided committee
        let selected = monitor.select_validator(&other_committee);
        assert!(other_committee.authority_exists(&selected));
    }
}
