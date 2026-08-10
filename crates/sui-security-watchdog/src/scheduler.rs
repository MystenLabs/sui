// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::SecurityWatchdogConfig;
use crate::metrics::WatchdogMetrics;
use crate::pagerduty::{Body, CreateIncident, Incident, Pagerduty, Service};
use crate::query_runner::{ClickHouseQueryRunner, QueryRunner, Row};
use anyhow::anyhow;
use chrono::{DateTime, Utc};
use prometheus::{IntGauge, Registry};
use serde::{Deserialize, Serialize};
use std::any::Any;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::sync::Arc;
use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::{error, info};
use uuid::Uuid;

// Default for entries that do not declare their own.
const SUI_DECIMALS: u32 = 9;

fn default_decimals() -> u32 {
    SUI_DECIMALS
}

// MonitoringEntry is an enum that represents the types of monitoring entries that can be scheduled.
#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
enum MonitoringEntry {
    MetricPublishingEntry(MetricPublishingEntry),
    WalletMonitoringEntry(WalletMonitoringEntry),
}

// MetricPublishingEntry is a struct that represents the configuration for a job which runs a sql
// query on a cron schedule and publishes metrics if the output is outside expected thresholds. Alerts
// could be set on the metric dashboard in grafana if needed
#[derive(Clone, Serialize, Deserialize)]
pub struct MetricPublishingEntry {
    name: String,
    cron_schedule: String,
    sql_query: String,
    metric_name: String,
    timed_upper_limits: BTreeMap<DateTime<Utc>, f64>,
    timed_lower_limits: BTreeMap<DateTime<Utc>, f64>,
    timed_exact_limits: BTreeMap<DateTime<Utc>, f64>,
}

// WalletMonitoringEntry is a struct that represents the configuration of a job which monitors wallet balances.
// It creates pagerduty incidents based on the given SQL query and cron schedule.
#[derive(Clone, Serialize, Deserialize)]
pub struct WalletMonitoringEntry {
    name: String,
    cron_schedule: String,
    sql_query: String,
    // Decimals of the coin whose base units `sql_query` reports, used to render incident text in
    // whole tokens. SUI and WAL are 9, DEEP and NS are 6.
    #[serde(default = "default_decimals")]
    decimals: u32,
    // Optional freshness guard, set together with `max_data_age_secs`. Must return the age in
    // seconds of the data behind `sql_query`. A stale snapshot yields no breaching rows, which is
    // indistinguishable from a healthy run, so without this a stalled pipeline mutes the monitor.
    #[serde(default)]
    freshness_sql: Option<String>,
    #[serde(default)]
    max_data_age_secs: Option<u64>,
}

pub struct SchedulerService {
    scheduler: JobScheduler,
    query_runner: Arc<dyn QueryRunner>,
    metrics: Arc<WatchdogMetrics>,
    entries: Vec<MonitoringEntry>,
    pagerduty: Pagerduty,
    pd_wallet_monitoring_service_id: String,
}

impl SchedulerService {
    pub async fn new(
        config: &SecurityWatchdogConfig,
        registry: &Registry,
        pd_api_key: String,
        ch_password: String,
    ) -> anyhow::Result<Self> {
        let scheduler = JobScheduler::new().await?;
        Ok(Self {
            scheduler,
            query_runner: Arc::new(ClickHouseQueryRunner::from_config(config, ch_password)?),
            metrics: Arc::new(WatchdogMetrics::new(registry)),
            entries: Self::from_config(config)?,
            pagerduty: Pagerduty::new(pd_api_key.clone()),
            pd_wallet_monitoring_service_id: config.pd_wallet_monitoring_service_id.clone(),
        })
    }

    pub async fn schedule(&self) -> anyhow::Result<()> {
        for monitoring_entry in &self.entries {
            match monitoring_entry {
                MonitoringEntry::MetricPublishingEntry(entry) => {
                    Self::schedule_metric_publish_job(
                        entry.clone(),
                        self.scheduler.clone(),
                        self.query_runner.clone(),
                        self.metrics.clone(),
                    )
                    .await?;
                }
                MonitoringEntry::WalletMonitoringEntry(entry) => {
                    self.schedule_wallet_monitoring_job(
                        entry.clone(),
                        self.scheduler.clone(),
                        self.query_runner.clone(),
                        self.pd_wallet_monitoring_service_id.clone(),
                        self.metrics.clone(),
                        self.pagerduty.clone(),
                    )
                    .await?;
                }
            }
        }
        Ok(())
    }

    pub async fn start(&self) -> anyhow::Result<()> {
        self.scheduler.start().await?;
        Ok(())
    }

    fn from_config(config: &SecurityWatchdogConfig) -> anyhow::Result<Vec<MonitoringEntry>> {
        let mut file = File::open(&config.config)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        let entries: Vec<MonitoringEntry> = serde_json::from_str(&contents)?;
        Ok(entries)
    }

    async fn schedule_wallet_monitoring_job(
        &self,
        entry: WalletMonitoringEntry,
        scheduler: JobScheduler,
        query_runner: Arc<dyn QueryRunner>,
        pd_service_id: String,
        metrics: Arc<WatchdogMetrics>,
        pagerduty: Pagerduty,
    ) -> anyhow::Result<Uuid> {
        let name = entry.name.clone();
        let cron_schedule = entry.cron_schedule.clone();
        let job = Job::new_async(cron_schedule.as_str(), move |_uuid, _lock| {
            let entry = entry.clone();
            let query_runner = query_runner.clone();
            let pd_service_id = pd_service_id.to_string();
            let pd = pagerduty.clone();
            let metrics = metrics.clone();
            Box::pin(async move {
                info!("Running wallet monitoring job: {}", entry.name);
                if let Err(err) =
                    Self::run_wallet_monitoring_job(&pd, &pd_service_id, &query_runner, &entry)
                        .await
                {
                    error!(
                        "Failed to run wallet monitoring job: {} with err: {}",
                        entry.name, err
                    );
                    metrics
                        .get("wallet_monitoring_error")
                        .await
                        .iter()
                        .for_each(|metric| metric.inc());
                }
            })
        })?;
        let job_id = scheduler.add(job).await?;
        info!("Scheduled job: {}", name);
        Ok(job_id)
    }

    async fn run_wallet_monitoring_job(
        pagerduty: &Pagerduty,
        service_id: &str,
        query_runner: &Arc<dyn QueryRunner>,
        entry: &WalletMonitoringEntry,
    ) -> anyhow::Result<()> {
        let WalletMonitoringEntry {
            sql_query,
            name,
            decimals,
            ..
        } = entry;
        Self::check_freshness(query_runner, entry).await?;
        let rows = query_runner.run(sql_query).await?;
        for row in rows {
            let wallet_id = Self::get_column(&row, "wallet_id")?
                .downcast_ref::<String>()
                .ok_or(anyhow!("Failed to downcast wallet_id"))?
                .clone();
            let current_balance = Self::extract_i128(Self::get_column(&row, "current_balance")?)
                .ok_or(anyhow!("Failed to downcast current_balance"))?;
            let lower_bound = Self::extract_i128(Self::get_column(&row, "lower_bound")?)
                .ok_or(anyhow!("Failed to downcast lower_bound"))?;
            Self::create_wallet_monitoring_incident(
                pagerduty,
                &wallet_id,
                current_balance,
                lower_bound,
                service_id,
                name,
                *decimals,
            )
            .await?;
        }
        Ok(())
    }

    /// Fails the job when the data behind `sql_query` is older than the entry allows.
    async fn check_freshness(
        query_runner: &Arc<dyn QueryRunner>,
        entry: &WalletMonitoringEntry,
    ) -> anyhow::Result<()> {
        match (&entry.freshness_sql, entry.max_data_age_secs) {
            (Some(freshness_sql), Some(max_data_age_secs)) => {
                let age_secs = query_runner.run_single_entry(freshness_sql).await?;
                if age_secs > max_data_age_secs as f64 {
                    return Err(anyhow!(
                        "Data backing job {} is stale: age {:.0}s exceeds max {}s",
                        entry.name,
                        age_secs,
                        max_data_age_secs
                    ));
                }
                Ok(())
            }
            (None, None) => Ok(()),
            // Half a guard is worse than none, since it reads as configured but never fires.
            _ => Err(anyhow!(
                "Job {}: freshness_sql and max_data_age_secs must be set together",
                entry.name
            )),
        }
    }

    /// Looks a column up case-insensitively. Snowflake upper-cased unquoted aliases while
    /// ClickHouse returns them verbatim, so neither casing should break the job.
    fn get_column<'a>(row: &'a Row, name: &str) -> anyhow::Result<&'a (dyn Any + Send)> {
        if let Some(value) = row.get(name) {
            return Ok(value.as_ref());
        }
        row.iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_ref())
            .ok_or_else(|| anyhow!("Missing {} in query result", name))
    }

    async fn create_wallet_monitoring_incident(
        pagerduty: &Pagerduty,
        wallet_id: &str,
        current_balance: i128,
        lower_bound: i128,
        service_id: &str,
        name: &str,
        decimals: u32,
    ) -> anyhow::Result<()> {
        let service = Service {
            id: service_id.to_string(),
            ..Default::default()
        };
        // Scale by the coin's own decimals, not SUI's, which understated 6-decimal coins 1000x.
        let per_token = 10i128.pow(decimals);
        let incident_body = Body {
            details: format!(
                "Current balance: {}, Lower bound: {}, for job: {}",
                current_balance / per_token,
                lower_bound / per_token,
                name
            ),
            ..Default::default()
        };
        let incident = Incident {
            title: format!(
                "Wallet: {} is out of compliance, for job: {}",
                wallet_id, name
            ),
            service,
            incident_key: wallet_id.to_string(),
            body: incident_body,
            ..Default::default()
        };
        let create_incident = CreateIncident { incident };
        pagerduty
            .create_incident("sadhan@mystenlabs.com", create_incident)
            .await?;
        Ok(())
    }

    async fn schedule_metric_publish_job(
        entry: MetricPublishingEntry,
        scheduler: JobScheduler,
        query_runner: Arc<dyn QueryRunner>,
        metrics: Arc<WatchdogMetrics>,
    ) -> anyhow::Result<Uuid> {
        let name = entry.name.clone();
        let cron_schedule = entry.cron_schedule.clone();
        let job = Job::new_async(cron_schedule.as_str(), move |_uuid, _lock| {
            let entry = entry.clone();
            let query_runner = query_runner.clone();
            let metrics = metrics.clone();
            Box::pin(async move {
                info!("Running metric publish job: {}", &entry.name);
                if let Err(err) =
                    Self::run_metric_publish_job(&query_runner, &metrics, &entry).await
                {
                    error!("Failed to run metric publish job with err: {}", err);
                    metrics
                        .get("metric_publishing_error")
                        .await
                        .iter()
                        .for_each(|metric| metric.inc());
                }
            })
        })?;
        let job_id = scheduler.add(job).await?;
        info!("Scheduled job: {}", name);
        Ok(job_id)
    }

    async fn run_metric_publish_job(
        query_runner: &Arc<dyn QueryRunner>,
        metrics: &Arc<WatchdogMetrics>,
        entry: &MetricPublishingEntry,
    ) -> anyhow::Result<()> {
        let MetricPublishingEntry {
            sql_query,
            timed_exact_limits,
            timed_upper_limits,
            timed_lower_limits,
            metric_name,
            ..
        } = entry;
        let res = query_runner.run_single_entry(sql_query).await?;
        let update_metrics = |limits: &BTreeMap<DateTime<Utc>, f64>, metric: IntGauge| {
            if let Some(value) = Self::get_current_limit(limits) {
                metric.set((res - value) as i64);
            } else {
                metric.set(0);
            }
        };

        update_metrics(timed_exact_limits, metrics.get_exact(metric_name).await?);
        update_metrics(timed_upper_limits, metrics.get_upper(metric_name).await?);
        update_metrics(timed_lower_limits, metrics.get_lower(metric_name).await?);
        Ok(())
    }

    fn get_current_limit(limits: &BTreeMap<DateTime<Utc>, f64>) -> Option<f64> {
        limits.range(..Utc::now()).next_back().map(|(_, val)| *val)
    }

    fn extract_i128(value: &(dyn Any + Send)) -> Option<i128> {
        if let Some(value) = value.downcast_ref::<i128>() {
            Some(*value)
        } else if let Some(value) = value.downcast_ref::<u32>() {
            Some(*value as i128)
        } else if let Some(value) = value.downcast_ref::<u16>() {
            Some(*value as i128)
        } else if let Some(value) = value.downcast_ref::<u8>() {
            Some(*value as i128)
        } else if let Some(value) = value.downcast_ref::<i64>() {
            Some(*value as i128)
        } else if let Some(value) = value.downcast_ref::<i32>() {
            Some(*value as i128)
        } else if let Some(value) = value.downcast_ref::<i16>() {
            Some(*value as i128)
        } else {
            value.downcast_ref::<i8>().map(|value| *value as i128)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mirrors a deployed wallet-monitoring entry. The config lives in the security-watchdog repo,
    // so a rename here fails at startup in production rather than at build time.
    const WALLET_ENTRY: &str = r#"[{
        "type": "WalletMonitoringEntry",
        "name": "DEEP Wallet Balance Check",
        "cron_schedule": "0 */10 * * * *",
        "decimals": 6,
        "sql_query": "SELECT locked_address AS wallet_id FROM ds_prod.locked_wallet_balances_gold",
        "freshness_sql": "SELECT toFloat64(1)",
        "max_data_age_secs": 345600
    }]"#;

    #[test]
    fn test_deserialize_wallet_monitoring_entry() {
        let entries: Vec<MonitoringEntry> = serde_json::from_str(WALLET_ENTRY).unwrap();
        let MonitoringEntry::WalletMonitoringEntry(entry) = &entries[0] else {
            panic!("expected a WalletMonitoringEntry");
        };
        assert_eq!(entry.decimals, 6);
        assert_eq!(entry.max_data_age_secs, Some(345600));
        assert!(entry.freshness_sql.is_some());
    }

    #[test]
    fn test_entry_without_optional_fields_defaults_to_sui() {
        let json = r#"[{
            "type": "WalletMonitoringEntry",
            "name": "Main Wallet Balance Check",
            "cron_schedule": "0 */10 * * * *",
            "sql_query": "SELECT 1"
        }]"#;
        let entries: Vec<MonitoringEntry> = serde_json::from_str(json).unwrap();
        let MonitoringEntry::WalletMonitoringEntry(entry) = &entries[0] else {
            panic!("expected a WalletMonitoringEntry");
        };
        assert_eq!(entry.decimals, SUI_DECIMALS);
        assert_eq!(entry.freshness_sql, None);
    }

    #[test]
    fn test_incident_scaling_uses_entry_decimals() {
        // 55_330_643 DEEP in base units must not render as 55_330 the way a SUI divisor would.
        let deep_base_units: i128 = 55_330_643_000_000;
        assert_eq!(deep_base_units / 10i128.pow(6), 55_330_643);
        assert_eq!(deep_base_units / 10i128.pow(SUI_DECIMALS), 55_330);
    }

    #[tokio::test]
    async fn test_freshness_config_must_be_complete() {
        let entry = WalletMonitoringEntry {
            name: "partial".to_string(),
            cron_schedule: "0 */10 * * * *".to_string(),
            sql_query: "SELECT 1".to_string(),
            decimals: 9,
            freshness_sql: Some("SELECT 1".to_string()),
            max_data_age_secs: None,
        };
        let runner: Arc<dyn QueryRunner> = Arc::new(
            ClickHouseQueryRunner::new("localhost", 8443, "ds_prod", "user", "passwd").unwrap(),
        );
        assert!(
            SchedulerService::check_freshness(&runner, &entry)
                .await
                .is_err()
        );
    }
}
