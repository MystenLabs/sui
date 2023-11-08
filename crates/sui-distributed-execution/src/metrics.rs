// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{cmp::max, collections::HashMap, io::BufRead, time::Duration};

use prometheus::{
    register_histogram_vec_with_registry, register_int_counter_with_registry, HistogramVec,
    IntCounter, Registry,
};
use prometheus_parse::Scrape;

pub const LATENCY_S: &str = "latency_s";
const LATENCY_SEC_BUCKETS: &[f64] = &[0.01, 0.015, 0.025, 0.05, 0.1, 0.25, 0.5, 0.75, 1., 2.0];
pub const START_TIME_S: &str = "start_time_s";
pub const LAST_UPDATE_S: &str = "last_update_s";

#[derive(Clone)]
pub struct Metrics {
    /// End-to-end latency of a workload in seconds.
    pub latency_s: HistogramVec,
    /// Benchmark start time (time since UNIX epoch in seconds).
    pub start_time_s: IntCounter,
    /// Time since last update (time since UNIX epoch in seconds). Technically, this is not needed
    /// as every sample update contains a timestamp.
    pub last_update_s: IntCounter,
}

impl Metrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            latency_s: register_histogram_vec_with_registry!(
                LATENCY_S,
                "Buckets measuring the end-to-end latency of a workload in seconds",
                &["workload"],
                LATENCY_SEC_BUCKETS.to_vec(),
                registry,
            )
            .unwrap(),
            start_time_s: register_int_counter_with_registry!(
                START_TIME_S,
                "Benchmark start time (time since UNIX epoch in seconds)",
                registry
            )
            .unwrap(),
            last_update_s: register_int_counter_with_registry!(
                LAST_UPDATE_S,
                "Time since last update (time since UNIX epoch in seconds)",
                registry
            )
            .unwrap(),
        }
    }

    /// Get the current time since the UNIX epoch in seconds.
    pub fn now() -> Duration {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
    }

    /// Register the start time. Should be called once before any transactions are registered.
    pub fn register_start_time(&self) {
        if self.start_time_s.get() == 0 {
            self.start_time_s.inc_by(Self::now().as_secs());
        }
    }

    /// Register a transaction. The parameter `tx_submission_timestamp` is the time since the UNIX
    /// epoch in seconds when the transaction was submitted.
    pub fn register_transaction(&self, tx_submission_timestamp: f64, workload: &str) {
        let now = Self::now();

        // Record last metrics update.
        self.register_last_update(now);

        // Update latency metrics.
        let elapsed = now.as_secs_f64() - tx_submission_timestamp;
        self.latency_s
            .with_label_values(&[workload])
            .observe(elapsed);
    }

    /// Register the time since the last update. Must be called periodically. The parameter `now`
    /// is the time since the UNIX epoch in seconds.
    fn register_last_update(&self, now: Duration) {
        let last_update = self.last_update_s.get();
        if let Some(delta) = now.as_secs().checked_sub(last_update) {
            self.last_update_s.inc_by(delta);
        }
    }
}

#[derive(Default, Debug)]
pub struct Measurement {
    pub buckets: HashMap<String, usize>,
    pub sum: Duration,
    pub count: usize,
    pub start_time: Duration,
    pub last_update: Duration,
}

impl Measurement {
    pub fn from_prometheus(text: &str) -> HashMap<String, Self> {
        let br = std::io::BufReader::new(text.as_bytes());
        let parsed = Scrape::parse(br.lines()).unwrap();

        let mut measurements = HashMap::new();
        let mut start_time = Duration::default();
        let mut last_update = Duration::default();
        for sample in &parsed.samples {
            let label = sample
                .labels
                .values()
                .cloned()
                .collect::<Vec<_>>()
                .join(",");

            if sample.metric == format!("{LATENCY_S}") {
                let measurement = measurements.entry(label).or_insert_with(Self::default);
                match &sample.value {
                    prometheus_parse::Value::Histogram(values) => {
                        for value in values {
                            let bucket_id = value.less_than.to_string();
                            let count = value.count as usize;
                            measurement.buckets.insert(bucket_id, count);
                        }
                    }
                    _ => panic!("Unexpected scraped value"),
                }
            } else if sample.metric == format!("{LATENCY_S}_sum") {
                let measurement = measurements.entry(label).or_insert_with(Self::default);
                measurement.sum = match sample.value {
                    prometheus_parse::Value::Untyped(value) => Duration::from_secs_f64(value),
                    _ => panic!("Unexpected scraped value"),
                };
            } else if sample.metric == format!("{LATENCY_S}_count") {
                let measurement = measurements.entry(label).or_insert_with(Self::default);
                measurement.count = match sample.value {
                    prometheus_parse::Value::Untyped(value) => value as usize,
                    _ => panic!("Unexpected scraped value"),
                };
            } else if sample.metric == START_TIME_S {
                let time = match sample.value {
                    prometheus_parse::Value::Counter(value) => Duration::from_secs_f64(value),
                    _ => panic!("Unexpected scraped value"),
                };
                start_time = max(start_time, time);
            } else if sample.metric == LAST_UPDATE_S {
                let time = match sample.value {
                    prometheus_parse::Value::Counter(value) => Duration::from_secs_f64(value),
                    _ => panic!("Unexpected scraped value"),
                };
                last_update = max(last_update, time);
            }
        }

        for measurement in measurements.values_mut() {
            measurement.start_time = max(measurement.start_time, start_time);
            measurement.last_update = max(measurement.last_update, last_update);
        }
        measurements
    }

    pub fn benchmark_duration(&self) -> Duration {
        self.last_update
            .checked_sub(self.start_time)
            .unwrap_or_default()
    }

    pub fn tps(&self) -> u64 {
        let duration = self.benchmark_duration().as_secs();
        let tps = self.count.checked_div(duration as usize);
        tps.unwrap_or_default() as u64
    }

    pub fn average_latency(&self) -> Duration {
        self.sum.checked_div(self.count as u32).unwrap_or_default()
    }
}

#[cfg(test)]
mod test {
    use super::Measurement;

    const METRICS: &str = r#"# TYPE latency_s histogram
    latency_s_bucket{workload="default",le="0.025"} 29043
    latency_s_bucket{workload="default",le="0.05"} 29151
    latency_s_bucket{workload="default",le="0.1"} 29201
    latency_s_bucket{workload="default",le="0.25"} 29351
    latency_s_bucket{workload="default",le="0.5"} 29601
    latency_s_bucket{workload="default",le="0.75"} 29851
    latency_s_bucket{workload="default",le="1"} 30001
    latency_s_bucket{workload="default",le="2"} 30001
    latency_s_bucket{workload="default",le="+Inf"} 30001
    latency_s_sum{workload="default"} 486.52599999967885
    latency_s_count{workload="default"} 30001
    # HELP start_time_s Benchmark start time (time since UNIX epoch in seconds)
    # TYPE start_time_s counter
    start_time_s 10
    # HELP last_update_s Time since last update (time since UNIX epoch in seconds)
    # TYPE last_update_s counter
    last_update_s 1699466232"#;

    #[test]
    fn parse_metrics() {
        let measurements = Measurement::from_prometheus(METRICS);
        println!("Measurements: {:?}", measurements);
        assert_eq!(measurements.len(), 1);
        let measurement = measurements.get("default").unwrap();

        assert_eq!(measurement.buckets.len(), 9);
        assert_eq!(measurement.sum.as_secs_f64(), 486.526);
        assert_eq!(measurement.count, 30001);

        assert_eq!(measurement.benchmark_duration().as_secs(), 1699466222);
    }
}
