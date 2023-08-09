The `sui-metric-checker` crate is used for querying prometheus metrics and validating the results. It will primarily be used to check for performance regressions in nightly deployments. Requires `api_key`, `api_user` & prometheus `url` which can be found in `sui-ops` repo or by asking the PE team.

## Guide

### Example Usage

```
RUST_LOG=debug cargo run --package sui-metric-checker --bin sui-metric-checker  -- --api-key xxxxxxxx --api-user xxxx_metrics --config checks.yaml --url https://xxxx.sui.io/prometheus
```

### Example Config

[example/config.yaml](example/config.yaml#L1-L32)

### Example Error Output

```
Error: Following queries failed to meet threshold conditions: [
    "After 3 retry attempts - Did not get expected response from server for histogram_quantile(0.5, sum by(le) (rate(latency_s_bucket{network=\"private-testnet\"}[15m])))",
    "After 3 retry attempts - Did not get expected response from server for histogram_quantile(0.95, sum by(le) (rate(latency_s_bucket{network=\"private-testnet\"}[15m])))",
    "After 3 retry attempts - Did not get expected response from server for sum(rate(num_success{network=\"private-testnet\"}[5m]))",
    "Query \"histogram_quantile(0.50, sum by(le) (rate(batch_execution_latency_bucket{network=\"private-testnet\"}[15m])))\" returned value of 3.112150385622982 which is Greater 3",
    "Query \"avg(rate(total_transaction_effects{network=\"private-testnet\"}[5m]))\" returned value of 1.081275647819765 which is Less 5500",
    "Query \"avg (rate(batch_size_sum{network=\"private-testnet\"}[5m]))\" returned value of 0.24698238962944846 which is Less 5500",
]
```
