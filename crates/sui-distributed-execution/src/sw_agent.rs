use super::agents::*;
use crate::{seqn_worker, types::*};
use async_trait::async_trait;
use tokio::sync::mpsc;

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
        let mut sw_state = seqn_worker::SequenceWorkerState::new(0, my_attrs).await;
        println!("Download watermark: {:?}", sw_state.download);
        println!("Execute watermark: {:?}", sw_state.execute);

        // Run Sequence Worker asynchronously
        sw_state
            .run(&mut self.in_channel, &self.out_channel, ew_ids)
            .await;

        // Await for workers (EWs and SW) to finish.
        // sw_handler.await.expect("sw failed");
    }
}
