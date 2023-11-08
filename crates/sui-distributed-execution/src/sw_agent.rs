use std::{sync::Arc, time::Duration};

use super::agents::*;
use crate::{
    metrics::{Measurement, Metrics},
    seqn_worker::{self, SequenceWorkerState},
    types::*,
};
use async_trait::async_trait;
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
            SequenceWorkerState::run_with_channel(&self.out_channel, ew_ids, tx_count, duration)
                .await;
            println!("SW finished");
            self.print_metrics().await.expect("Failed to print metrics");
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
    async fn print_metrics(&self) -> Result<(), reqwest::Error> {
        let mut summary = Vec::new();
        for (id, entry) in &self.attrs {
            if entry.kind == "EW" {
                let route = crate::prometheus::METRICS_ROUTE;
                let address = entry.metrics_address;
                let res = reqwest::get(format! {"http://{address}{route}"}).await?;
                let string = res.text().await?;
                let measurements = Measurement::from_prometheus(&string);
                let measurement = measurements.get("default").unwrap();
                summary.push(format!(
                    "[SW{id}] TPS: {}\tLatency (avg): {:?}",
                    measurement.tps(),
                    measurement.average_latency()
                ));
            }
        }
        println!("{}\n", summary.join("\n"));
        Ok(())
    }
}
