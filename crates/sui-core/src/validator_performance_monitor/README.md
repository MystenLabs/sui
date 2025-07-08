# Validator Performance Monitor

## Overview

The Validator Performance Monitor tracks validator performance through a combination of periodic health checks and real-time feedback from TransactionDriver operations. It enables intelligent validator selection to improve transaction submission success rates and reduce latency.

## Key Metrics for Performance Tuning

### Primary Metrics

1. **Transaction Success Rate** (`validator_operation_success_total` / `validator_operation_failure_total`)
   - **Target**: > 95%
   - **Description**: Percentage of successful transaction submissions and effects retrievals
   - **Tuning**: Validators with consistently low success rates should be deprioritized

2. **Operation Latency** (`validator_operation_latency`)
   - **Target**: p50 < 100ms, p99 < 1s for submit; p50 < 500ms, p99 < 5s for effects
   - **Description**: Time taken for submit_transaction and wait_for_effects operations
   - **Tuning**: Use latency percentiles to identify consistently slow validators

3. **Validator Selection Distribution** (`transaction_driver_validator_selections`)
   - **Target**: Relatively balanced across healthy validators (within 2x range)
   - **Description**: Number of times each validator is selected
   - **Tuning**: Ensure no single validator is overloaded; adjust selection temperature

### Health Check Metrics

4. **Pending Certificates** (`validator_pending_certificates`)
   - **Target**: < 1000
   - **Description**: Number of certificates waiting to be executed
   - **Tuning**: High values indicate validator overload

5. **Consensus Lag** (`validator_consensus_round` compared to max)
   - **Target**: Within 10 rounds of the highest validator
   - **Description**: How far behind a validator is in consensus
   - **Tuning**: Large lags indicate consensus participation issues

6. **Transaction Queue Size** (`validator_tx_queue_size`)
   - **Target**: < 10,000
   - **Description**: Number of transactions in execution queue
   - **Tuning**: High values indicate execution bottlenecks

### Performance Score Components

7. **Overall Performance Score** (`validator_performance_score`)
   - **Target**: > 0.7
   - **Description**: Weighted combination of all performance factors
   - **Components**:
     - Latency score (30% weight)
     - Success rate score (40% weight)
     - Pending certificates score (10% weight)
     - Consensus lag score (10% weight)
     - Queue size score (5% weight)
     - Resource usage score (5% weight)

### Failure Tracking

8. **Consecutive Failures** (`validator_consecutive_failures`)
   - **Target**: < 3
   - **Description**: Number of consecutive operation failures
   - **Tuning**: Validators exceeding max_consecutive_failures are temporarily excluded

9. **Time Since Last Success** (`validator_time_since_last_success`)
   - **Target**: < 30s
   - **Description**: Duration since last successful operation
   - **Tuning**: Long durations indicate persistent issues

## Configuration Tuning Guide

### Selection Strategy Parameters

- **WeightedRandom.temperature**: Controls randomness (0.5-2.0)
  - Lower = more deterministic (favor best validators)
  - Higher = more uniform distribution

- **TopK.k**: Number of top validators to consider (3-10)
  - Lower = better performance but less distribution
  - Higher = more distribution but may include weaker validators

- **EpsilonGreedy.epsilon**: Exploration rate (0.05-0.2)
  - Lower = exploit best validators
  - Higher = explore more validators

### Timing Parameters

- **health_check_interval**: How often to query validator health (5-30s)
- **health_check_timeout**: Timeout for health queries (1-5s)
- **metrics_window**: Rolling window for metrics (1-10 minutes)
- **failure_cooldown**: Exclusion period after failures (10-60s)

### Score Weights

Adjust based on your priorities:
- Increase `success_rate` weight for reliability
- Increase `latency` weight for speed
- Increase `pending_certificates` weight if execution lag is critical

## Monitoring Dashboard Queries

```promql
# Success rate by validator
rate(validator_operation_success_total[5m]) /
(rate(validator_operation_success_total[5m]) + rate(validator_operation_failure_total[5m]))

# Latency percentiles
histogram_quantile(0.99, rate(validator_operation_latency_bucket[5m]))

# Selection bias
max(validator_selections_total) / min(validator_selections_total)

# Unhealthy validators
validator_performance_score < 0.5
```

## Experimental Features

- **Adaptive Scoring**: Automatically adjusts scores based on recent recovery
- **Resource-based Scoring**: Incorporates CPU/memory usage when available
- **Multi-factor Selection**: Combines multiple selection strategies

## Future Enhancements

1. **Predictive Scoring**: Use ML to predict future performance
2. **Network Topology Awareness**: Consider network distance in selection
3. **Load-based Adjustment**: Dynamic weight adjustment based on network load
4. **Cross-validator Correlation**: Detect correlated failures