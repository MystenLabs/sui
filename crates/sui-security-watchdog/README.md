# Security Watchdog Service

## Overview
The Analytics Watchdog Service is designed to monitor and analyze data changes over time. It achieves this by periodically downloading a specified GitHub repository, parsing configuration files for SQL queries, and executing these queries on a set schedule. Results are then used to update Prometheus metrics, providing real-time insights into data trends.

## Running the Service
Execute the compiled binary to start the service:
```shell
cargo run --release -p sui-security-watchdog
```
## Usage
The service will automatically start downloading the configured GitHub repository, parsing the configuration file, and scheduling SQL queries as specified. Metrics will be updated in Prometheus according to the results of these queries.
The config file allows setting up time based schedule for expected results. For example, when periodically checking total sui in the network we want it to be an exact value i.e 10B whereas when periodically checking balance of an account
which has time based token unlocks, we want it to compare against a lower bound (balance of the account should never drop below a certain number before a given date), etc.

```json lines
[
  {
    "name": "sui_10B",
    "cron_schedule": "0 0 * * *",  // Every day at midnight (UTC)
    "sql_query": "SELECT total_sui FROM total_sui_mainnet ORDER BY epoch DESC LIMIT 1",
    "metric_name": "total_sui_10B",
    "timed_exact_limits": {
      // total sui should always be exact 10B since
      // the dawn of time
      "1970-01-01T00:00:00Z": 10000000000.0
    },
    "timed_lower_limits": {},
    "timed_exact_limits": {}
  },
  {
    "name": "user_x_balance",
    "cron_schedule": "0 15 * * *",  // Every day at 3:15 PM (UTC)
    "sql_query": "SELECT balance FROM user_balances WHERE user_id = 'x' LIMIT 1",
    "metric_name": "user_x_balance",
    "timed_exact_limits": {},
    "timed_upper_limits": {},
    "timed_lower_limits": {
      // user balance should not drop below these numbers on those dates
      // i.e. balance should not drop below 50 SUI before 1/1/2024,
      // balance should not drop below 100SUI before 2/1/2024
      // and it should not drop below 150SUI before 3/1/2024
      "2024-01-01T15:00:00Z": 50.0,
      "2024-02-01T15:00:00Z": 100.0,
      "2024-03-01T15:00:00Z": 150.0,
    }
  }
]
```