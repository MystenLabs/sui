// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::metrics::WatchdogMetrics;
use crate::pagerduty::{Body, CreateIncident, Incident, Pagerduty, Service};
use crate::query_runner::{QueryRunner, SnowflakeQueryRunner};
use crate::SecurityWatchdogConfig;
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

const MIST_PER_SUI: i128 = 1_000_000_000;

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
        sf_password: String,
    ) -> anyhow::Result<Self> {
        let scheduler = JobScheduler::new().await?;
        Ok(Self {
            scheduler,
            query_runner: Arc::new(SnowflakeQueryRunner::from_config(config, sf_password)?),
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
                    error!("Failed to run wallet monitoring job with err: {}", err);
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
        let WalletMonitoringEntry { sql_query, .. } = entry;
        let rows = query_runner.run(sql_query).await?;
        for row in rows {
            let wallet_id = row
                .get("WALLET_ID")
                .ok_or_else(|| anyhow!("Missing wallet_id"))?
                .downcast_ref::<String>()
                .ok_or(anyhow!("Failed to downcast wallet_id"))?
                .clone();
            let current_balance = Self::extract_i128(
                row.get("CURRENT_BALANCE")
                    .ok_or_else(|| anyhow!("Missing current_balance"))?,
            )
            .ok_or(anyhow!("Failed to downcast current_balance"))?;
            let lower_bound = Self::extract_i128(
                row.get("LOWER_BOUND")
                    .ok_or_else(|| anyhow!("Missing lower_bound"))?,
            )
            .ok_or(anyhow!("Failed to downcast lower_bound"))?;
            Self::create_wallet_monitoring_incident(
                pagerduty,
                &wallet_id,
                current_balance,
                lower_bound,
                service_id,
            )
            .await?;
        }
        Ok(())
    }

    async fn create_wallet_monitoring_incident(
        pagerduty: &Pagerduty,
        wallet_id: &str,
        current_balance: i128,
        lower_bound: i128,
        service_id: &str,
    ) -> anyhow::Result<()> {
        let service = Service {
            id: service_id.to_string(),
            ..Default::default()
        };
        let incident_body = Body {
            details: format!(
                "Current balance: {} SUI, Lower bound: {} SUI",
                current_balance / MIST_PER_SUI,
                lower_bound / MIST_PER_SUI
            ),
            ..Default::default()
        };
        let incident = Incident {
            title: format!("Wallet: {} is out of compliance", wallet_id),
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

    fn extract_i128(value: &Box<dyn Any + Send>) -> Option<i128> {
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
