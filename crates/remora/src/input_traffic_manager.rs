use super::types::*;
use core::panic;
use tokio::sync::mpsc;

/*****************************************************************************************
 *                              Input Traffic Manager in Primary                         *
 *****************************************************************************************/

pub async fn input_traffic_manager_run(
    in_channel: &mut mpsc::Receiver<NetworkMessage>,
    out_consensus: &mpsc::UnboundedSender<RemoraMessage>,
    out_executor: &mpsc::UnboundedSender<RemoraMessage>,
    my_id: u16,
) {
    let _counter = 0;
    loop {
        tokio::select! {
            Some(msg) = in_channel.recv() => {
                println!("{} receive a msg", my_id);
                let msg = msg.payload;
                if let RemoraMessage::ProposeExec(ref _full_tx) = msg {
                    if let Err(e) = out_consensus.send(msg) {
                        eprintln!("Failed to forward to consensus engine: {:?}", e);
                    };
                } else if let RemoraMessage::PreExecResult(ref _full_tx) = msg {
                    if let Err(e) = out_executor.send(msg) {
                        eprintln!("Failed to forward to executor engine: {:?}", e);
                    };
                } else {
                    eprintln!("PRI {} received unexpected message from: {:?}", my_id, msg);
                    panic!("unexpected message");
                };
            },
        }
    }
}
