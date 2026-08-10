// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::SecurityWatchdogConfig;
use anyhow::{Context, anyhow};
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;
use std::any::Any;
use std::collections::HashMap;
use std::time::Duration;
use tracing::info;

pub type Row = HashMap<String, Box<dyn Any + Send>>;

#[async_trait::async_trait]
pub trait QueryRunner: Send + Sync + 'static {
    /// Asynchronously runs the given SQL query and returns the result as a floating-point number.
    /// Only the first row and first column in returned, so it is important that users of this trait
    /// use it for a query which returns only a single floating point result
    async fn run_single_entry(&self, query: &str) -> anyhow::Result<f64>;

    /// Asynchronously runs the given SQL query and returns the result as a vector of rows.
    async fn run(&self, query: &str) -> anyhow::Result<Vec<Row>>;
}

/// Column descriptor from the `meta` section of a ClickHouse `JSON` response. The declared type
/// is what tells us how to interpret the JSON value, which can be a number or a string for the
/// same column depending on width.
#[derive(Deserialize)]
struct ColumnMeta {
    name: String,
    #[serde(rename = "type")]
    ty: String,
}

#[derive(Deserialize)]
struct QueryResponse {
    meta: Vec<ColumnMeta>,
    data: Vec<HashMap<String, Value>>,
}

pub struct ClickHouseQueryRunner {
    client: Client,
    url: String,
    database: String,
    user: String,
    passwd: String,
}

impl ClickHouseQueryRunner {
    /// Creates a new `ClickHouseQueryRunner` targeting the HTTP interface.
    ///
    /// # Arguments
    /// * `host` - ClickHouse host name.
    /// * `port` - Port of the HTTP interface (8443 for TLS, 8123 for plain HTTP).
    /// * `database` - The database to query against.
    /// * `user` - Username for authentication.
    /// * `passwd` - Password for authentication.
    /// * `no_tls` - Connect over plain HTTP instead of HTTPS.
    /// * `query_timeout_secs` - Per-request timeout.
    pub fn new(
        host: &str,
        port: u16,
        database: &str,
        user: &str,
        passwd: &str,
        no_tls: bool,
        query_timeout_secs: u64,
    ) -> anyhow::Result<Self> {
        let scheme = if no_tls { "http" } else { "https" };
        let client = Client::builder()
            .timeout(Duration::from_secs(query_timeout_secs))
            .build()?;
        Ok(Self {
            client,
            url: format!("{}://{}:{}/", scheme, host, port),
            database: database.to_string(),
            user: user.to_string(),
            passwd: passwd.to_string(),
        })
    }

    pub fn from_config(
        config: &SecurityWatchdogConfig,
        ch_password: String,
    ) -> anyhow::Result<Self> {
        Self::new(
            &config.ch_host,
            config.ch_port,
            &config.ch_database,
            &config.ch_user,
            &ch_password,
            config.ch_no_tls,
            config.ch_query_timeout_secs,
        )
    }

    async fn exec(&self, query: &str) -> anyhow::Result<QueryResponse> {
        let response = self
            .client
            .post(&self.url)
            .query(&[
                ("database", self.database.as_str()),
                ("default_format", "JSON"),
            ])
            .basic_auth(&self.user, Some(&self.passwd))
            .body(query.to_string())
            .send()
            .await?;
        // ClickHouse reports query errors in the body, so surface it rather than just the status.
        let status = response.status();
        let body = response.text().await?;
        if !status.is_success() {
            return Err(anyhow!(
                "ClickHouse query failed with status {}: {}",
                status,
                body.trim()
            ));
        }
        serde_json::from_str(&body)
            .with_context(|| format!("Failed to parse ClickHouse response: {}", body.trim()))
    }
}

/// Unwraps the type modifiers that do not change how a value is encoded in JSON.
fn base_type(ty: &str) -> &str {
    let mut ty = ty.trim();
    loop {
        let unwrapped = ["Nullable(", "LowCardinality("].iter().find_map(|prefix| {
            ty.strip_prefix(prefix)
                .and_then(|inner| inner.strip_suffix(')'))
        });
        match unwrapped {
            Some(inner) => ty = inner.trim(),
            None => return ty,
        }
    }
}

/// ClickHouse encodes integers wider than 32 bits as JSON strings to avoid the precision loss of
/// a double, so accept both encodings. Decimals arrive with a fractional part and are truncated.
fn json_to_i128(value: &Value) -> Option<i128> {
    match value {
        Value::Number(number) => number
            .as_i64()
            .map(|value| value as i128)
            .or_else(|| number.as_f64().map(|value| value as i128)),
        Value::String(string) => string
            .parse::<i128>()
            .ok()
            .or_else(|| string.parse::<f64>().ok().map(|value| value as i128)),
        _ => None,
    }
}

fn json_to_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        // `inf`/`nan` and wide integers are quoted.
        Value::String(string) => string.parse::<f64>().ok(),
        _ => None,
    }
}

/// Boxes a JSON value as the Rust type matching its declared ClickHouse type. Integers are
/// widened to `i128` so a single downcast covers every integer column. Returns `None` for SQL
/// NULL, which is left out of the row entirely.
fn to_any(ty: &str, value: &Value) -> Option<Box<dyn Any + Send>> {
    if value.is_null() {
        return None;
    }
    let ty = base_type(ty);
    if ty.starts_with("Int") || ty.starts_with("UInt") || ty.starts_with("Decimal") {
        return json_to_i128(value).map(|value| Box::new(value) as Box<dyn Any + Send>);
    }
    if ty.starts_with("Float") {
        return json_to_f64(value).map(|value| Box::new(value) as Box<dyn Any + Send>);
    }
    // String, FixedString, UUID, Date and DateTime all arrive as JSON strings. Anything left
    // (arrays, tuples, maps) keeps its JSON encoding rather than being dropped.
    match value {
        Value::String(string) => Some(Box::new(string.clone()) as Box<dyn Any + Send>),
        other => Some(Box::new(other.to_string()) as Box<dyn Any + Send>),
    }
}

#[async_trait::async_trait]
impl QueryRunner for ClickHouseQueryRunner {
    async fn run_single_entry(&self, query: &str) -> anyhow::Result<f64> {
        let response = self.exec(query).await?;
        let column = response
            .meta
            .first()
            .ok_or_else(|| anyhow!("No columns found in query result"))?;
        let value = response
            .data
            .first()
            .ok_or_else(|| anyhow!("No rows found in query result"))?
            .get(&column.name)
            .ok_or_else(|| anyhow!("Missing column {} in query result", column.name))?;
        json_to_f64(value)
            .ok_or_else(|| anyhow!("Column {} is not a number: {}", column.name, value))
    }

    async fn run(&self, query: &str) -> anyhow::Result<Vec<Row>> {
        info!("Running query: {}", query);
        let response = self.exec(query).await?;
        let rows: Vec<Row> = response
            .data
            .iter()
            .map(|record| {
                response
                    .meta
                    .iter()
                    .filter_map(|column| {
                        let value = record.get(&column.name)?;
                        Some((column.name.clone(), to_any(&column.ty, value)?))
                    })
                    .collect()
            })
            .collect();
        info!("Found {} rows", rows.len());
        Ok(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base_type_unwraps_modifiers() {
        assert_eq!(base_type("Int64"), "Int64");
        assert_eq!(base_type("Nullable(Float64)"), "Float64");
        assert_eq!(base_type("LowCardinality(Nullable(String))"), "String");
    }

    #[test]
    fn test_json_to_i128_accepts_both_encodings() {
        assert_eq!(json_to_i128(&serde_json::json!(42)), Some(42));
        // Wide integers arrive quoted; going through f64 here would lose precision.
        assert_eq!(
            json_to_i128(&serde_json::json!("87500000000000001")),
            Some(87500000000000001)
        );
        assert_eq!(json_to_i128(&serde_json::json!("123.9")), Some(123));
        assert_eq!(json_to_i128(&serde_json::json!(null)), None);
    }

    /// Exercises the full path against a live ClickHouse using a wallet-monitoring shaped query.
    /// Ignored by default since it needs credentials and network:
    ///   CH_HOST=.. CH_USER=.. CH_PASSWORD=.. cargo test -p sui-security-watchdog -- --ignored
    #[tokio::test]
    #[ignore]
    async fn test_live_wallet_monitoring_query() {
        let (Ok(host), Ok(user), Ok(passwd)) = (
            std::env::var("CH_HOST"),
            std::env::var("CH_USER"),
            std::env::var("CH_PASSWORD"),
        ) else {
            panic!("CH_HOST, CH_USER and CH_PASSWORD must be set");
        };
        let runner =
            ClickHouseQueryRunner::new(&host, 8443, "ds_prod", &user, &passwd, false, 60).unwrap();

        // Same shape as the configured jobs, with the breach predicate inverted so rows come back
        // on a healthy dataset.
        let rows = runner
            .run(
                "SELECT locked_address AS wallet_id, \
                 toInt64(round(current_balance * 1e6)) AS current_balance, \
                 toInt64(round(coalesce(locked_min_amount, 0) * 1e6)) AS lower_bound \
                 FROM ds_prod.locked_wallet_balances_gold FINAL \
                 WHERE cohort = 'DEEP' AND current_locked_difference >= 0 LIMIT 5",
            )
            .await
            .unwrap();
        assert!(!rows.is_empty(), "expected rows from the gold table");
        for row in &rows {
            assert!(
                row.get("wallet_id")
                    .unwrap()
                    .downcast_ref::<String>()
                    .is_some()
            );
            assert!(
                row.get("current_balance")
                    .unwrap()
                    .downcast_ref::<i128>()
                    .is_some()
            );
            assert!(
                row.get("lower_bound")
                    .unwrap()
                    .downcast_ref::<i128>()
                    .is_some()
            );
        }

        // The freshness guard's query must come back as a plain number.
        let age = runner
            .run_single_entry(
                "SELECT toFloat64(dateDiff('second', max(run_ts), now('UTC'))) \
                 FROM ds_prod.locked_wallet_balances_gold",
            )
            .await
            .unwrap();
        assert!(age >= 0.0, "data age should not be negative, got {}", age);

        // A bad query must surface ClickHouse's error rather than silently yielding no rows.
        assert!(
            runner
                .run("SELECT * FROM ds_prod.no_such_table")
                .await
                .is_err()
        );
    }

    #[test]
    fn test_to_any_maps_declared_types() {
        let wallet = to_any("String", &serde_json::json!("0xabc")).unwrap();
        assert_eq!(wallet.downcast_ref::<String>().unwrap(), "0xabc");

        // Every integer width lands on i128 so callers need only one downcast.
        let balance = to_any("Int64", &serde_json::json!("87500000000000000")).unwrap();
        assert_eq!(balance.downcast_ref::<i128>().unwrap(), &87500000000000000);

        let difference = to_any("Nullable(Float64)", &serde_json::json!(-1.5)).unwrap();
        assert_eq!(difference.downcast_ref::<f64>().unwrap(), &-1.5);

        assert!(to_any("Nullable(Int64)", &serde_json::json!(null)).is_none());
    }
}
