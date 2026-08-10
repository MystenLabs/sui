# Security Watchdog Service

## Overview
The Analytics Watchdog Service is designed to monitor and analyze data changes over time. It achieves this by parsing a configuration file for SQL queries and executing those queries on a set schedule. Results either update Prometheus metrics or raise PagerDuty incidents, depending on the entry type.

Queries run against ClickHouse over its HTTP interface. The deployed configuration lives in the
[security-watchdog](https://github.com/MystenLabs/security-watchdog) repository, and the tables it
reads are built by the `locked_wallets` dbt models in
[data-science](https://github.com/MystenLabs/data-science).

## Running the Service
Execute the compiled binary to start the service:
```shell
cargo run --release -p sui-security-watchdog
```
Connection settings are passed as flags (`--help` for the list), with `CH_PASSWORD` and `PD_API_KEY`
read from the environment. Deployed values live in the
[security-watchdog](https://github.com/MystenLabs/security-watchdog) Pulumi stack.

## Usage
The service parses the configuration file at startup and schedules the SQL queries as specified. Metrics will be updated in Prometheus according to the results of these queries.
The config file allows setting up time based schedule for expected results. For example, when periodically checking total sui in the network we want it to be an exact value i.e 10B whereas when periodically checking balance of an account
which has time based token unlocks, we want it to compare against a lower bound (balance of the account should never drop below a certain number before a given date), etc.

```json lines
[
  {
    "name": "sui_10B",
    "cron_schedule": "0 0 0 * * *",  // Every day at midnight (UTC)
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
    "cron_schedule": "0 15 15 * * *",  // Every day at 3:15 PM (UTC)
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

## Wallet monitoring entries
A `WalletMonitoringEntry` raises a PagerDuty incident per row its query returns, so the query should
select only wallets that are out of compliance. It must project three columns, matched
case-insensitively: `wallet_id`, and `current_balance` / `lower_bound` in the coin's base units.

```json lines
[
  {
    "type": "WalletMonitoringEntry",
    "name": "DEEP Wallet Balance Check",
    "cron_schedule": "0 0 */4 * * *",  // Every 4 hours
    // Decimals of the coin the balances are denominated in, used to render incident text in
    // whole tokens. SUI and WAL are 9, DEEP and NS are 6. Defaults to 9.
    "decimals": 6,
    "sql_query": "SELECT locked_address AS wallet_id, ... WHERE current_locked_difference <= -1",
    // Optional staleness guard, set as a pair. Returns the age in seconds of the data behind
    // `sql_query`; the job fails if that exceeds `max_data_age_secs`. Balances come from a daily
    // snapshot, and a stale snapshot returns no breaching rows -- which is indistinguishable from
    // a healthy run -- so without this a stalled pipeline silently disables the monitor.
    "freshness_sql": "SELECT toFloat64(dateDiff('second', max(run_ts), now('UTC'))) FROM ...",
    "max_data_age_secs": 345600
  }
]
```