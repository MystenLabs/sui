use core::panic;

use tokio::{sync::mpsc, time::Duration};

use super::types::*;

/*****************************************************************************************
 *                                    MockConsensus Worker                               *
 *****************************************************************************************/
const DURATION: u64 = 10000;

pub async fn mock_consensus_worker_run(
    in_channel: &mut mpsc::UnboundedReceiver<RemoraMessage>,
    out_channel: &mpsc::UnboundedSender<Vec<TransactionWithEffects>>,
    _my_id: u16,
) {
    let mut consensus_interval = tokio::time::interval(Duration::from_millis(DURATION));
    let mut tx_vec: Vec<TransactionWithEffects> = Vec::new();

    loop {
        tokio::select! {
            Some(msg) = in_channel.recv() => {
                println!("Consensus receive a txn");
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
