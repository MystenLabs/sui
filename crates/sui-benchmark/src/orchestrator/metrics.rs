use std::{collections::HashMap, fs, hash::Hash, io::BufRead, path::PathBuf, time::Duration};

use chrono::{DateTime, Utc};
use prettytable::{format, row, Table};
use prometheus_parse::Scrape;
use serde::Serialize;

use super::testbed::BenchmarkParameters;

type BucketId = String;

#[derive(Serialize, Default)]
pub struct DataPoint {
    /// Duration since the beginning of the benchmark.
    timestamp: Duration,
    /// Latency buckets.
    buckets: HashMap<BucketId, usize>,
    /// Sum of the latencies of all finalized transactions.
    sum: Duration,
    /// Total number of finalized transactions
    count: usize,
}

impl DataPoint {
    pub fn new(
        timestamp: Duration,
        buckets: HashMap<BucketId, usize>,
        sum: Duration,
        count: usize,
    ) -> Self {
        Self {
            timestamp,
            buckets,
            sum,
            count,
        }
    }

    pub fn tps(&self) -> u64 {
        let tps = self.count.checked_div(self.timestamp.as_secs() as usize);
        tps.unwrap_or_default() as u64
    }

    pub fn average_latency(&self) -> Duration {
        let latency_in_millis = self.sum.as_millis().checked_div(self.count as u128);
        Duration::from_millis(latency_in_millis.unwrap_or_default() as u64)
    }
}

#[derive(Serialize)]
pub struct MetricsAggregator<ScraperId: Serialize> {
    #[serde(skip_serializing)]
    start: DateTime<Utc>,
    scrapers: HashMap<ScraperId, Vec<DataPoint>>,
}

impl<ScraperId> MetricsAggregator<ScraperId>
where
    ScraperId: Eq + Hash + Serialize,
{
    pub fn new() -> Self {
        Self {
            start: Utc::now(),
            scrapers: HashMap::new(),
        }
    }

    pub fn collect(&mut self, scraper_id: ScraperId, text: &str) {
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
                prometheus_parse::Value::Untyped(value) => Duration::from_secs(value as u64),
                _ => panic!("Unexpected scraped value"),
            })
            .unwrap_or_default();

        let (count, timestamp) = parsed
            .samples
            .iter()
            .find(|x| x.metric == "latency_s_count")
            .map(|x| {
                let count = match x.value {
                    prometheus_parse::Value::Untyped(value) => value as usize,
                    _ => panic!("Unexpected scraped value"),
                };
                let timestamp = x.timestamp;
                (count, timestamp)
            })
            .unwrap_or_default();

        let duration = (timestamp - self.start.clone()).to_std().unwrap();

        self.scrapers
            .entry(scraper_id)
            .or_insert_with(Vec::new)
            .push(DataPoint::new(duration, buckets, sum, count));
    }

    pub fn save(&self) {
        let json = serde_json::to_string(self).expect("Cannot serialize metrics");
        let path = PathBuf::from("results.json");
        fs::write(path, json).unwrap();
    }

    pub fn print_summary(&self, parameters: &BenchmarkParameters) {
        let last_data_points: Vec<_> = self.scrapers.values().filter_map(|x| x.last()).collect();
        let duration = last_data_points
            .iter()
            .map(|x| x.timestamp)
            .max()
            .unwrap_or_default();
        let total_tps: u64 = last_data_points.iter().map(|x| x.tps()).sum();
        let average_latency = last_data_points
            .iter()
            .map(|x| x.average_latency().as_millis())
            .sum::<u128>()
            .checked_div(last_data_points.len() as u128)
            .unwrap_or_default();

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

        println!();
        table.set_titles(row![bH2->"Summary"]);
        table.add_row(row![b->"Nodes:", parameters.nodes]);
        table.add_row(row![b->"Faults:", parameters.faults]);
        table.add_row(row![b->"Load:", parameters.load]);
        table.add_row(row![b->"Duration:", duration.as_secs()]);
        table.add_row(row![bH2->""]);
        table.add_row(row![b->"TPS:", total_tps]);
        table.add_row(row![b->"Latency (avg):", average_latency]);
        table.printstd();
        println!();
    }
}

#[cfg(test)]
mod test {
    use std::time::Duration;

    use crate::orchestrator::testbed::BenchmarkParameters;

    use super::MetricsAggregator;

    const EXAMPLE: &'static str = r#"
        # HELP current_requests_in_flight Current number of requests being processed in QuorumDriver
        # TYPE current_requests_in_flight gauge
        current_requests_in_flight 0
        # HELP latency_s Total time in seconds to return a response
        # TYPE latency_s histogram
        latency_s_bucket{workload=transfer_object,le=0.01} 0
        latency_s_bucket{workload=transfer_object,le=0.05} 0
        latency_s_bucket{workload=transfer_object,le=0.1} 0
        latency_s_bucket{workload=transfer_object,le=0.25} 5871
        latency_s_bucket{workload=transfer_object,le=0.5} 10269
        latency_s_bucket{workload=transfer_object,le=1} 11968
        latency_s_bucket{workload=transfer_object,le=2.5} 12000
        latency_s_bucket{workload=transfer_object,le=5} 12000
        latency_s_bucket{workload=transfer_object,le=10} 12000
        latency_s_bucket{workload=transfer_object,le=20} 12000
        latency_s_bucket{workload=transfer_object,le=30} 12000
        latency_s_bucket{workload=transfer_object,le=60} 12000
        latency_s_bucket{workload=transfer_object,le=90} 12000
        latency_s_bucket{workload=transfer_object,le=+Inf} 12000
        latency_s_sum{workload=transfer_object} 3633.637998717
        latency_s_count{workload=transfer_object} 12000
    "#;

    #[test]
    fn collect() {
        let mut aggregator = MetricsAggregator::new();

        let scraper_id = 1u8;
        aggregator.collect(scraper_id, EXAMPLE);
        aggregator.print_summary(&BenchmarkParameters::default());

        assert_eq!(aggregator.scrapers.len(), 1);
        let data_points = aggregator.scrapers.get(&scraper_id).unwrap();
        assert_eq!(data_points.len(), 1);

        let data = &data_points[0];
        assert_eq!(
            data.buckets,
            ([
                ("10".into(), 12000),
                ("60".into(), 12000),
                ("0.1".into(), 0),
                ("0.05".into(), 0),
                ("0.5".into(), 10269),
                ("2.5".into(), 12000),
                ("90".into(), 12000),
                ("0.01".into(), 0),
                ("0.25".into(), 5871),
                ("5".into(), 12000),
                ("20".into(), 12000),
                ("30".into(), 12000),
                ("1".into(), 11968),
                ("inf".into(), 12000)
            ])
            .iter()
            .cloned()
            .collect()
        );
        assert_eq!(data.sum, Duration::from_secs(3633));
        assert_eq!(data.count, 12000);

        assert_eq!(data.average_latency(), Duration::from_millis(302));
        assert_eq!(data.tps(), 0);
    }
}
