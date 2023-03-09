// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::HashMap,
    fs,
    io::BufRead,
    path::{Path, PathBuf},
    time::Duration,
};

use prettytable::{format, row, Table};
use prometheus_parse::Scrape;
use serde::{Deserialize, Serialize};

use crate::{benchmark::BenchmarkParameters, settings::Settings};

/// The identifier of prometheus latency buckets.
type BucketId = String;

/// A snapshot measurement at a given time.
#[derive(Serialize, Deserialize, Default, Clone)]
pub struct Measurement {
    /// Duration since the beginning of the benchmark.
    timestamp: Duration,
    /// Latency buckets.
    buckets: HashMap<BucketId, usize>,
    /// Sum of the latencies of all finalized transactions.
    sum: Duration,
    /// Total number of finalized transactions
    count: usize,
    /// Square of the latencies of all finalized transactions.
    squared_sum: Duration,
}

impl Measurement {
    // Make a new measurement from the text exposed by prometheus.
    pub fn from_prometheus(text: &str) -> Self {
        let br = std::io::BufReader::new(text.as_bytes());
        let parsed = Scrape::parse(br.lines()).unwrap();

        let buckets: HashMap<_, _> = parsed
            .samples
            .iter()
            .find(|x| x.metric == "latency_s")
            .map(|x| match &x.value {
                prometheus_parse::Value::Histogram(values) => values
                    .iter()
                    .map(|x| {
                        let bucket_id = x.less_than.to_string();
                        let count = x.count as usize;
                        (bucket_id, count)
                    })
                    .collect(),
                _ => panic!("Unexpected scraped value"),
            })
            .unwrap_or_default();

        let sum = parsed
            .samples
            .iter()
            .find(|x| x.metric == "latency_s_sum")
            .map(|x| match x.value {
                prometheus_parse::Value::Untyped(value) => Duration::from_secs_f64(value),
                _ => panic!("Unexpected scraped value"),
            })
            .unwrap_or_default();

        let count = parsed
            .samples
            .iter()
            .find(|x| x.metric == "latency_s_count")
            .map(|x| match x.value {
                prometheus_parse::Value::Untyped(value) => value as usize,
                _ => panic!("Unexpected scraped value"),
            })
            .unwrap_or_default();

        let squared_sum = parsed
            .samples
            .iter()
            .find(|x| x.metric == "latency_squared_s")
            .map(|x| match x.value {
                prometheus_parse::Value::Counter(value) => Duration::from_secs_f64(value),
                _ => panic!("Unexpected scraped value"),
            })
            .unwrap_or_default();

        let timestamp = parsed
            .samples
            .iter()
            .find(|x| x.metric == "benchmark_duration")
            .map(|x| match x.value {
                prometheus_parse::Value::Counter(value) => Duration::from_secs(value as u64),
                _ => panic!("Unexpected scraped value"),
            })
            .unwrap_or_default();

        Self {
            timestamp,
            buckets,
            sum,
            count,
            squared_sum,
        }
    }

    /// Compute the tps.
    pub fn tps(&self) -> u64 {
        let tps = self.count.checked_div(self.timestamp.as_secs() as usize);
        tps.unwrap_or_default() as u64
    }

    /// Compute the average latency.
    pub fn average_latency(&self) -> Duration {
        self.sum.checked_div(self.count as u32).unwrap_or_default()
    }

    /// Compute the standard deviation from the sum of squared latencies:
    /// `stdev = sqrt( squared_sum / count - avg^2 )`
    pub fn stdev_latency(&self) -> Duration {
        // Compute `squared_sum / count`.
        let first_term = if self.count == 0 {
            0.0
        } else {
            self.squared_sum.as_secs_f64() / self.count as f64
        };

        // Compute `avg^2`.
        let squared_avg = self.average_latency().as_secs_f64().powf(2.0);

        // Compute `squared_sum / count - avg^2`.
        let variance = if squared_avg > first_term {
            0.0
        } else {
            first_term - squared_avg
        };

        // Compute `sqrt( squared_sum / count - avg^2 )`.
        let stdev = variance.sqrt();
        Duration::from_secs_f64(stdev)
    }
}

/// The identifier of the scrapers collecting the prometheus metrics.
type ScraperId = usize;

#[derive(Serialize, Deserialize, Clone)]
pub struct MeasurementsCollection {
    /// The machine / instance type.
    pub machine_specs: String,
    /// The commit of the codebase.
    pub commit: String,
    /// The benchmark parameters of the current run.
    pub parameters: BenchmarkParameters,
    /// The data collected by each scraper.
    scrapers: HashMap<ScraperId, Vec<Measurement>>,
}

impl MeasurementsCollection {
    /// Create a new (empty) collection of measurements.
    pub fn new(settings: &Settings, parameters: BenchmarkParameters) -> Self {
        Self {
            machine_specs: settings.specs.clone(),
            commit: settings.repository.commit.clone(),
            parameters,
            scrapers: HashMap::new(),
        }
    }

    /// Load a collection of measurement from a json file.
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, std::io::Error> {
        let data = fs::read(path)?;
        let measurements: Self = serde_json::from_slice(data.as_slice())?;
        Ok(measurements)
    }

    /// Add a new measurement to the collection.
    pub fn add(&mut self, scraper_id: ScraperId, measurement: Measurement) {
        self.scrapers
            .entry(scraper_id)
            .or_insert_with(Vec::new)
            .push(measurement);
    }

    /// Return the transaction (input) load of the benchmark.
    pub fn transaction_load(&self) -> usize {
        self.parameters.load
    }

    /// Aggregate the benchmark duration of multiple data points by taking the max.
    pub fn benchmark_duration(&self) -> Duration {
        self.scrapers
            .values()
            .filter_map(|x| x.last())
            .map(|x| x.timestamp)
            .max()
            .unwrap_or_default()
    }

    /// Aggregate the tps of multiple data points by taking the sum.
    pub fn aggregate_tps(&self) -> u64 {
        self.scrapers
            .values()
            .filter_map(|x| x.last())
            .map(|x| x.tps())
            .sum()
    }

    /// Aggregate the average latency of multiple data points by taking the average.
    pub fn aggregate_average_latency(&self) -> Duration {
        let last_data_points: Vec<_> = self.scrapers.values().filter_map(|x| x.last()).collect();
        last_data_points
            .iter()
            .map(|x| x.average_latency())
            .sum::<Duration>()
            .checked_div(last_data_points.len() as u32)
            .unwrap_or_default()
    }

    /// Aggregate the stdev latency of multiple data points by taking the max.
    pub fn aggregate_stdev_latency(&self) -> Duration {
        self.scrapers
            .values()
            .filter_map(|x| x.last())
            .map(|x| x.stdev_latency())
            .max()
            .unwrap_or_default()
    }

    /// Save the collection of measurements as a json file.
    pub fn save<P: AsRef<Path>>(&self, path: P) {
        let json = serde_json::to_string_pretty(self).expect("Cannot serialize metrics");
        let mut file = PathBuf::from(path.as_ref());
        file.push(format!("measurements-{:?}.json", self.parameters));
        fs::write(file, json).unwrap();
    }

    /// Display a summary of the measurements.
    pub fn display_summary(&self) {
        let duration = self.benchmark_duration();
        let total_tps = self.aggregate_tps();
        let average_latency = self.aggregate_average_latency();
        let stdev_latency = self.aggregate_stdev_latency();

        let mut table = Table::new();
        let format = format::FormatBuilder::new()
            .separators(
                &[
                    format::LinePosition::Top,
                    format::LinePosition::Bottom,
                    format::LinePosition::Title,
                ],
                format::LineSeparator::new('-', '-', '-', '-'),
            )
            .padding(1, 1)
            .build();
        table.set_format(format);

        table.set_titles(row![bH2->"Benchmark Summary"]);
        table.add_row(
            row![b->"Shared objects:", format!("{}%",self.parameters.shared_objects_ratio)],
        );
        table.add_row(row![bH2->""]);
        table.add_row(row![b->"Nodes:", self.parameters.nodes]);
        table.add_row(row![b->"Faults:", self.parameters.faults]);
        table.add_row(row![b->"Load:", format!("{} tx/s", self.parameters.load)]);
        table.add_row(row![b->"Duration:", format!("{} s", duration.as_secs())]);
        table.add_row(row![bH2->""]);
        table.add_row(row![b->"TPS:", format!("{total_tps} tx/s")]);
        table.add_row(row![b->"Latency (avg):", format!("{} ms", average_latency.as_millis())]);
        table.add_row(row![b->"Latency (stdev):", format!("{} ms", stdev_latency.as_millis())]);

        println!();
        table.printstd();
        println!();
    }
}

#[cfg(test)]
mod test {
    use std::{collections::HashMap, time::Duration};

    use crate::settings::Settings;

    use super::{BenchmarkParameters, Measurement, MeasurementsCollection};

    #[test]
    fn average_latency() {
        let data = Measurement {
            timestamp: Duration::from_secs(10),
            buckets: HashMap::new(),
            sum: Duration::from_secs(2),
            count: 100,
            squared_sum: Duration::from_secs(0),
        };

        assert_eq!(data.average_latency(), Duration::from_millis(20));
    }

    #[test]
    fn stdev_latency() {
        let data = Measurement {
            timestamp: Duration::from_secs(10),
            buckets: HashMap::new(),
            sum: Duration::from_secs(50),
            count: 100,
            squared_sum: Duration::from_secs(75),
        };

        // squared_sum / count
        assert_eq!(
            data.squared_sum.checked_div(data.count as u32),
            Some(Duration::from_secs_f64(0.75))
        );
        // avg^2
        assert_eq!(data.average_latency().as_secs_f64().powf(2.0), 0.25);
        // sqrt( squared_sum / count - avg^2 )
        let stdev = data.stdev_latency();
        assert_eq!((stdev.as_secs_f64() * 10.0).round(), 7.0);
    }

    #[test]
    fn collect() {
        let report = r#"
            # HELP benchmark_duration Duration of the benchmark
            # TYPE benchmark_duration counter
            benchmark_duration 30
            # HELP latency_s Total time in seconds to return a response
            # TYPE latency_s histogram
            latency_s_bucket{workload=transfer_object,le=0.1} 0
            latency_s_bucket{workload=transfer_object,le=0.25} 0
            latency_s_bucket{workload=transfer_object,le=0.5} 506
            latency_s_bucket{workload=transfer_object,le=0.75} 1282
            latency_s_bucket{workload=transfer_object,le=1} 1693
            latency_s_bucket{workload="transfer_object",le="1.25"} 1816
            latency_s_bucket{workload="transfer_object",le="1.5"} 1860
            latency_s_bucket{workload="transfer_object",le="1.75"} 1860
            latency_s_bucket{workload="transfer_object",le="2"} 1860
            latency_s_bucket{workload=transfer_object,le=2.5} 1860
            latency_s_bucket{workload=transfer_object,le=5} 1860
            latency_s_bucket{workload=transfer_object,le=10} 1860
            latency_s_bucket{workload=transfer_object,le=20} 1860
            latency_s_bucket{workload=transfer_object,le=30} 1860
            latency_s_bucket{workload=transfer_object,le=60} 1860
            latency_s_bucket{workload=transfer_object,le=90} 1860
            latency_s_bucket{workload=transfer_object,le=+Inf} 1860
            latency_s_sum{workload=transfer_object} 1265.287933130998
            latency_s_count{workload=transfer_object} 1860
            # HELP latency_squared_s Square of total time in seconds to return a response
            # TYPE latency_squared_s counter
            latency_squared_s{workload="transfer_object"} 952.8160642745289
        "#;

        let measurement = Measurement::from_prometheus(report);
        let settings = Settings::new_for_test();
        let mut aggregator = MeasurementsCollection::new(&settings, BenchmarkParameters::default());
        let scraper_id = 1;
        aggregator.add(scraper_id, measurement);

        assert_eq!(aggregator.scrapers.len(), 1);
        let data_points = aggregator.scrapers.get(&scraper_id).unwrap();
        assert_eq!(data_points.len(), 1);

        let data = &data_points[0];
        assert_eq!(
            data.buckets,
            ([
                ("0.1".into(), 0),
                ("0.25".into(), 0),
                ("0.5".into(), 506),
                ("0.75".into(), 1282),
                ("1".into(), 1693),
                ("1.25".into(), 1816),
                ("1.5".into(), 1860),
                ("1.75".into(), 1860),
                ("2".into(), 1860),
                ("2.5".into(), 1860),
                ("5".into(), 1860),
                ("10".into(), 1860),
                ("20".into(), 1860),
                ("30".into(), 1860),
                ("60".into(), 1860),
                ("90".into(), 1860),
                ("inf".into(), 1860)
            ])
            .iter()
            .cloned()
            .collect()
        );
        assert_eq!(data.sum, Duration::from_secs(1265));
        assert_eq!(data.count, 1860);
        assert_eq!(data.timestamp, Duration::from_secs(30));
        assert_eq!(data.squared_sum, Duration::from_secs(952));
    }
}
