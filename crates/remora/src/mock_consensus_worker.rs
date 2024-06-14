use core::panic;

use tokio::sync::mpsc;

use tokio::time::Duration;

use super::types::*;

/*****************************************************************************************
 *                                    MockConsensus Worker                                   *
 *****************************************************************************************/

pub struct MockConsensusWorkerState {}

impl MockConsensusWorkerState {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn run(
        &mut self,
        in_channel: &mut mpsc::UnboundedReceiver<RemoraMessage>,
        out_channel: &mpsc::UnboundedSender<Vec<TransactionWithEffects>>,
        my_id: u16,
    ) {
        let mut counter = 0;
        let mut consensus_interval = tokio::time::interval(Duration::from_millis(300));
        let mut tx_vec: Vec<TransactionWithEffects> = Vec::new();

        loop {
            tokio::select! {
                Some(msg) = in_channel.recv() => {
                    println!("{} receive a txn", my_id);
                    counter += 1;
                    if let RemoraMessage::ProposeExec(full_tx) = msg {
                        tx_vec.push(full_tx);
                    } else {
                        eprintln!("PRI consensus received unexpected message from: {:?}", msg);
                        panic!("unexpected message");
                    };
                },

                // forward to the primary executor
                _ = consensus_interval.tick() => {
                    if !tx_vec.is_empty() {
                        println!("Consensus engine sending {} transactions", tx_vec.len());
                        if let Err(e) = out_channel.send(tx_vec.clone()) {
                            eprintln!("Consensus engine failed to forward to executor: {:?}", e);
                        }
                        tx_vec.clear();
                    }
                }
            }
        }
    }
}
