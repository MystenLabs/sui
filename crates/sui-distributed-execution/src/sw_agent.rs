use std::{
    collections::HashMap,
    io::BufRead,
    sync::Arc,
    time::{Duration, Instant},
};

use super::agents::*;
use crate::{
    metrics::{Metrics, LATENCY_S},
    seqn_worker::{self, SequenceWorkerState},
    types::*,
};
use async_trait::async_trait;
use prometheus_parse::Scrape;
use tokio::{sync::mpsc, time::sleep};

pub struct SWAgent {
    id: UniqueId,
    in_channel: mpsc::Receiver<NetworkMessage>,
    out_channel: mpsc::Sender<NetworkMessage>,
    attrs: GlobalConfig,
}

#[async_trait]
impl Agent<SailfishMessage> for SWAgent {
    fn new(
        id: UniqueId,
        in_channel: mpsc::Receiver<NetworkMessage>,
        out_channel: mpsc::Sender<NetworkMessage>,
        attrs: GlobalConfig,
        _metrics: Arc<Metrics>,
    ) -> Self {
        SWAgent {
            id,
            in_channel,
            out_channel,
            attrs,
        }
    }

    async fn run(&mut self) {
        println!("Starting SW agent {}", self.id);
        // extract list of all EWs
        let mut ew_ids: Vec<UniqueId> = Vec::new();
        for (id, entry) in &self.attrs {
            if entry.kind == "EW" {
                ew_ids.push(*id);
            }
        }

        // extract my attrs from the global config
        let my_attrs = &self.attrs.get(&self.id).unwrap().attrs;
        if my_attrs["mode"] == "channel" {
            // Run Sequence Worker asynchronously
            let tx_count = my_attrs["tx_count"].parse::<u64>().unwrap();
            let duration_secs = my_attrs["duration"].parse::<u64>().unwrap();
            let duration = Duration::from_secs(duration_secs);
            let start_time = SequenceWorkerState::run_with_channel(
                &self.out_channel,
                ew_ids,
                tx_count,
                duration,
            )
            .await;
            println!("SW finished");
            self.print_metrics(start_time)
                .await
                .expect("Failed to print metrics");
            loop {
                sleep(Duration::from_millis(1_000)).await;
            }
        } else {
            let mut sw_state = seqn_worker::SequenceWorkerState::new(0, my_attrs).await;
            println!("Download watermark: {:?}", sw_state.download);
            println!("Execute watermark: {:?}", sw_state.execute);

            // Run Sequence Worker asynchronously
            sw_state
                .run(&mut self.in_channel, &self.out_channel, ew_ids)
                .await;
        }
    }
}

impl SWAgent {
    async fn print_metrics(&self, start_time: Instant) -> Result<(), reqwest::Error> {
        let end_time = start_time.elapsed();
        for (id, entry) in &self.attrs {
            if entry.kind == "EW" {
                let route = crate::prometheus::METRICS_ROUTE;
                let address = entry.metrics_address;
                let res = reqwest::get(format! {"http://{address}{route}"}).await?;
                let string = res.text().await?;
                let measurements = Measurement::from_prometheus(&string);
                let measurement = measurements.get("default").unwrap();
                println!("[EW{id}] TPS: {}", measurement.tps(&end_time));
                println!(
                    "[EW{id}] Latency (avg): {:?}",
                    measurement.average_latency()
                );
            }
        }
        Ok(())
    }
}

#[derive(Default, Debug)]
pub struct Measurement {
    pub buckets: HashMap<String, usize>,
    pub sum: Duration,
    pub count: usize,
}

impl Measurement {
    fn from_prometheus(text: &str) -> HashMap<String, Self> {
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
            }
        }
        measurements
    }

    pub fn tps(&self, duration: &Duration) -> u64 {
        let tps = self.count.checked_div(duration.as_secs() as usize);
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
    latency_s_count{workload="default"} 30001"#;

    #[test]
    fn parse_metrics() {
        let measurements = Measurement::from_prometheus(METRICS);
        println!("Measurements: {:?}", measurements);
        assert_eq!(measurements.len(), 1);
        let measurement = measurements.get("default").unwrap();
        assert_eq!(measurement.buckets.len(), 9);
        assert_eq!(measurement.sum.as_secs_f64(), 486.526);
        assert_eq!(measurement.count, 30001);
    }
}
