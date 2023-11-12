// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::HashMap,
    fs,
    io::BufRead,
    path::{Path, PathBuf},
    time::Duration,
};

use prettytable::{row, Table};
use prometheus_parse::Scrape;
use serde::{Deserialize, Serialize};

use crate::{
    benchmark::{BenchmarkParameters, BenchmarkType},
    display,
    protocol::ProtocolMetrics,
    settings::Settings,
};

/// The identifier of prometheus latency buckets.
type BucketId = String;
/// The identifier of a measurement type.
type Label = String;

/// A snapshot measurement at a given time.
#[derive(Serialize, Deserialize, Default, Clone, Debug)]
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
    /// Make new measurements from the text exposed by prometheus. Every measurement is identified by a unique label.
    pub fn from_prometheus<M: ProtocolMetrics>(text: &str) -> HashMap<Label, Self> {
        let br = std::io::BufReader::new(text.as_bytes());
        let parsed = Scrape::parse(br.lines()).unwrap();

        let mut measurements = HashMap::new();
        for sample in &parsed.samples {
            let label = sample
                .labels
                .values()
                .cloned()
                .collect::<Vec<_>>()
                .join(",");

            if sample.metric == M::LATENCY_BUCKETS {
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
            } else if sample.metric == M::LATENCY_SUM {
                let measurement = measurements.entry(label).or_insert_with(Self::default);
                measurement.sum = match sample.value {
                    prometheus_parse::Value::Untyped(value) => Duration::from_secs_f64(value),
                    _ => panic!("Unexpected scraped value"),
                };
            } else if sample.metric == M::TOTAL_TRANSACTIONS {
                let measurement = measurements.entry(label).or_insert_with(Self::default);
                measurement.count = match sample.value {
                    prometheus_parse::Value::Untyped(value) => value as usize,
                    _ => panic!("Unexpected scraped value"),
                };
            } else if sample.metric == M::LATENCY_SQUARED_SUM {
                let measurement = measurements.entry(label).or_insert_with(Self::default);
                measurement.squared_sum = match sample.value {
                    prometheus_parse::Value::Counter(value) => Duration::from_secs_f64(value),
                    _ => panic!("Unexpected scraped value"),
                };
            }
        }

        // Apply the same timestamp to all measurements.
        let timestamp = parsed
            .samples
            .iter()
            .find(|x| x.metric == M::BENCHMARK_DURATION)
            .map(|x| match x.value {
                prometheus_parse::Value::Counter(value) => Duration::from_secs(value as u64),
                _ => panic!("Unexpected scraped value"),
            })
            .unwrap_or_default();
        for sample in measurements.values_mut() {
            sample.timestamp = timestamp;
        }

        measurements
    }

    /// Compute the tps.
    pub fn tps(&self, duration: &Duration) -> u64 {
        let tps = self.count.checked_div(duration.as_secs() as usize);
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

    #[cfg(test)]
    pub fn new_for_test() -> (Label, Self) {
        (
            "owned".to_string(),
            Self {
                timestamp: Duration::from_secs(30),
                buckets: HashMap::new(),
                sum: Duration::from_secs(1265),
                count: 1860,
                squared_sum: Duration::from_secs(952),
            },
        )
    }
}

/// The identifier of the scrapers collecting the prometheus metrics.
type ScraperId = usize;

#[derive(Serialize, Deserialize, Clone)]
pub struct MeasurementsCollection<T> {
    /// The machine / instance type.
    pub machine_specs: String,
    /// The commit of the codebase.
    pub commit: String,
    /// The benchmark parameters of the current run.
    pub parameters: BenchmarkParameters<T>,
    /// The data collected by each scraper.
    pub data: HashMap<Label, HashMap<ScraperId, Vec<Measurement>>>,
}

impl<T: BenchmarkType> MeasurementsCollection<T> {
    /// Create a new (empty) collection of measurements.
    pub fn new(settings: &Settings, parameters: BenchmarkParameters<T>) -> Self {
        Self {
            machine_specs: settings.specs.clone(),
            commit: settings.repository.commit.clone(),
            parameters,
            data: HashMap::new(),
        }
    }

    /// Load a collection of measurement from a json file.
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, std::io::Error> {
        let data = fs::read(path)?;
        let measurements: Self = serde_json::from_slice(data.as_slice())?;
        Ok(measurements)
    }

    /// Add a new measurement to the collection.
    pub fn add(&mut self, scraper_id: ScraperId, label: String, measurement: Measurement) {
        self.data
            .entry(label)
            .or_insert_with(HashMap::new)
            .entry(scraper_id)
            .or_insert_with(Vec::new)
            .push(measurement);
    }

    /// Get all measurements associated with the specified label.
    pub fn all_measurements(&self, label: &Label) -> Vec<Vec<Measurement>> {
        self.data
            .get(label)
            .map(|data| data.values().cloned().collect())
            .unwrap_or_default()
    }

    /// Get all labels.
    pub fn labels(&self) -> impl Iterator<Item = &Label> {
        self.data.keys()
    }

    /// Return the transaction (input) load of the benchmark.
    pub fn transaction_load(&self) -> usize {
        self.parameters.load
    }

    /// Aggregate the benchmark duration of multiple data points by taking the max.
    pub fn benchmark_duration(&self) -> Duration {
        self.labels()
            .map(|label| {
                self.all_measurements(label)
                    .iter()
                    .filter_map(|x| x.last())
                    .map(|x| x.timestamp)
                    .max()
                    .unwrap_or_default()
            })
            .max()
            .unwrap_or_default()
    }

    /// Aggregate the tps of multiple data points.
    pub fn aggregate_tps(&self, label: &Label) -> u64 {
        let duration = self
            .all_measurements(label)
            .iter()
            .filter_map(|x| x.last())
            .map(|x| x.timestamp)
            .max()
            .unwrap_or_default();
        self.all_measurements(label)
            .iter()
            .filter_map(|x| x.last())
            .map(|x| x.tps(&duration))
            .max()
            .unwrap_or_default()
    }

    /// Aggregate the average latency of multiple data points by taking the average.
    pub fn aggregate_average_latency(&self, label: &Label) -> Duration {
        let all_measurements = self.all_measurements(label);
        let last_data_points: Vec<_> = all_measurements.iter().filter_map(|x| x.last()).collect();
        last_data_points
            .iter()
            .map(|x| x.average_latency())
            .sum::<Duration>()
            .checked_div(last_data_points.len() as u32)
            .unwrap_or_default()
    }

    /// Aggregate the stdev latency of multiple data points by taking the max.
    pub fn aggregate_stdev_latency(&self, label: &Label) -> Duration {
        self.all_measurements(label)
            .iter()
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
        let mut table = Table::new();
        table.set_format(display::default_table_format());

        let duration = self.benchmark_duration();

        table.set_titles(row![bH2->"Benchmark Summary"]);
        table.add_row(row![b->"Benchmark type:", self.parameters.benchmark_type]);
        table.add_row(row![bH2->""]);
        table.add_row(row![b->"Nodes:", self.parameters.nodes]);
        table.add_row(row![b->"Faults:", self.parameters.faults]);
        table.add_row(row![b->"Load:", format!("{} tx/s", self.parameters.load)]);
        table.add_row(row![b->"Duration:", format!("{} s", duration.as_secs())]);

        let mut labels: Vec<_> = self.labels().collect();
        labels.sort();
        for label in labels {
            let total_tps = self.aggregate_tps(label);
            let average_latency = self.aggregate_average_latency(label);
            let stdev_latency = self.aggregate_stdev_latency(label);

            table.add_row(row![bH2->""]);
            table.add_row(row![b->"Workload:", label]);
            table.add_row(row![b->"TPS:", format!("{total_tps} tx/s")]);
            table.add_row(row![b->"Latency (avg):", format!("{} ms", average_latency.as_millis())]);
            table.add_row(row![b->"Latency (stdev):", format!("{} ms", stdev_latency.as_millis())]);
        }

        display::newline();
        table.printstd();
        display::newline();
    }
}

#[cfg(test)]
mod test {
    use std::{collections::HashMap, time::Duration};

    use crate::{
        benchmark::test::TestBenchmarkType, protocol::test_protocol_metrics::TestProtocolMetrics,
        settings::Settings,
    };

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
    fn prometheus_parse() {
        let report = r#"
            # HELP benchmark_duration Duration of the benchmark
            # TYPE benchmark_duration counter
            benchmark_duration 30
            # HELP latency_s Total time in seconds to return a response
            # TYPE latency_s histogram
            latency_s_bucket{workload=owned,le=0.1} 0
            latency_s_bucket{workload=owned,le=0.25} 0
            latency_s_bucket{workload=owned,le=0.5} 506
            latency_s_bucket{workload=owned,le=0.75} 1282
            latency_s_bucket{workload=owned,le=1} 1693
            latency_s_bucket{workload="owned",le="1.25"} 1816
            latency_s_bucket{workload="owned",le="1.5"} 1860
            latency_s_bucket{workload="owned",le="1.75"} 1860
            latency_s_bucket{workload="owned",le="2"} 1860
            latency_s_bucket{workload=owned,le=2.5} 1860
            latency_s_bucket{workload=owned,le=5} 1860
            latency_s_bucket{workload=owned,le=10} 1860
            latency_s_bucket{workload=owned,le=20} 1860
            latency_s_bucket{workload=owned,le=30} 1860
            latency_s_bucket{workload=owned,le=60} 1860
            latency_s_bucket{workload=owned,le=90} 1860
            latency_s_bucket{workload=owned,le=+Inf} 1860
            latency_s_sum{workload=owned} 1265.287933130998
            latency_s_count{workload=owned} 1860
            latency_s_bucket{workload="shared",le="0.1"} 42380
            latency_s_bucket{workload="shared",le="0.25"} 104320
            latency_s_bucket{workload="shared",le="0.5"} 110720
            latency_s_bucket{workload="shared",le="0.75"} 112780
            latency_s_bucket{workload="shared",le="1"} 112780
            latency_s_bucket{workload="shared",le="1.25"} 112780
            latency_s_bucket{workload="shared",le="1.5"} 112780
            latency_s_bucket{workload="shared",le="1.75"} 112780
            latency_s_bucket{workload="shared",le="2"} 112780
            latency_s_bucket{workload="shared",le="2.5"} 112780
            latency_s_bucket{workload="shared",le="5"} 112780
            latency_s_bucket{workload="shared",le="10"} 112780
            latency_s_bucket{workload="shared",le="20"} 112780
            latency_s_bucket{workload="shared",le="30"} 112780
            latency_s_bucket{workload="shared",le="60"} 112780
            latency_s_bucket{workload="shared",le="90"} 112780
            latency_s_bucket{workload="shared",le="+Inf"} 112780
            latency_s_sum{workload="shared"} 15452.286558500084
            latency_s_count{workload="shared"} 112780
            # HELP latency_squared_s Square of total time in seconds to return a response
            # TYPE latency_squared_s counter
            latency_squared_s{workload="owned"} 952.8160642745289
        "#;

        let measurements = Measurement::from_prometheus::<TestProtocolMetrics>(report);
        let settings = Settings::new_for_test();
        let mut aggregator = MeasurementsCollection::<TestBenchmarkType>::new(
            &settings,
            BenchmarkParameters::default(),
        );
        let scraper_id = 1;
        for (label, measurement) in measurements {
            aggregator.add(scraper_id, label, measurement);
        }

        assert_eq!(aggregator.data.len(), 2);
        for label in &["owned".to_string(), "shared".to_string()] {
            let data_points = aggregator
                .data
                .get(label)
                .expect("Unable to find label")
                .get(&scraper_id)
                .unwrap();
            assert_eq!(data_points.len(), 1);

            if label == "owned" {
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
                assert_eq!(data.sum.as_secs(), 1265);
                assert_eq!(data.count, 1860);
                assert_eq!(data.timestamp.as_secs(), 30);
                assert_eq!(data.squared_sum.as_secs(), 952);
            }
        }
    }
}
