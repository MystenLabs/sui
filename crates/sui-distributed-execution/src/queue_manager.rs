use std::collections::{HashMap, HashSet};

use sui_types::base_types::ObjectID;
use sui_types::digests::TransactionDigest;
use tokio::sync::mpsc;

use super::types::*;

pub const MANAGER_CHANNEL_SIZE: usize = 1_000;

pub struct QueuesManager {
    tx_store: HashMap<TransactionDigest, TransactionWithEffects>,
    writing_tx: HashMap<ObjectID, TransactionDigest>,
    wait_table: HashMap<TransactionDigest, HashSet<TransactionDigest>>,
    reverse_wait_table: HashMap<TransactionDigest, HashSet<TransactionDigest>>,
    new: mpsc::UnboundedReceiver<TransactionWithEffects>,
    ready: mpsc::UnboundedSender<TransactionWithEffects>,
    done: mpsc::UnboundedReceiver<TransactionDigest>,
}

// The methods of the QueuesManager are called from a single thread, so no need for locks
impl QueuesManager {
    pub fn new(
        new_tx_receiver: mpsc::UnboundedReceiver<TransactionWithEffects>,
        ready_tx_sender: mpsc::UnboundedSender<TransactionWithEffects>,
        done_tx_receiver: mpsc::UnboundedReceiver<TransactionDigest>,
    ) -> QueuesManager {
        QueuesManager {
            tx_store: HashMap::new(),
            writing_tx: HashMap::new(),
            wait_table: HashMap::new(),
            reverse_wait_table: HashMap::new(),
            new: new_tx_receiver,
            ready: ready_tx_sender,
            done: done_tx_receiver,
        }
    }

    pub async fn run(&mut self) {
        loop {
            tokio::select! {
                biased;
                //TODO can we make sure we only process new_tx if done_tx is empty?
                Some(done_tx) = self.done.recv() => {
                    self.clean_up(&done_tx).await;
                }
                Some(new_tx) = self.new.recv() => {
                    self.queue_tx(new_tx).await;
                }
                else => {
                    eprintln!("QD error, abort");
                    break
                }

            }
        }
    }

    /// Enqueues a transaction on the manager
    async fn queue_tx(&mut self, full_tx: TransactionWithEffects) {
        let txid = full_tx.tx.digest();

        // Get RW set
        let r_set = full_tx.get_read_set();
        let w_set = full_tx.get_write_set();
        let mut wait_ctr = 0;

        // Add tx to wait lists
        r_set.union(&w_set).for_each(|obj| {
            let prev_write = self.writing_tx.insert(*obj, *txid);
            if let Some(other_txid) = prev_write {
                self.wait_table.entry(*txid).or_default().insert(other_txid);
                self.reverse_wait_table
                    .entry(other_txid)
                    .or_default()
                    .insert(*txid);
                wait_ctr += 1;
            }
        });

        // Set this transaction as the current writer
        w_set.iter().for_each(|obj| {
            self.writing_tx.insert(*obj, *txid);
        });

        // Store tx
        self.tx_store.insert(*txid, full_tx.clone());

        // Set the wait table and check if tx is ready
        if wait_ctr == 0 {
            self.ready.send(full_tx).expect("send failed");
        }
    }

    /// Cleans up after a completed transaction
    async fn clean_up(&mut self, txid: &TransactionDigest) {
        if let Some(completed_tx) = self.tx_store.remove(txid) {
            assert!(self.wait_table.get(txid).is_none());

            // Remove tx itself from objects where it is still marked as their current writer
            for obj in completed_tx.get_read_write_set().iter() {
                if let Some(t) = self.writing_tx.get(obj) {
                    if t == txid {
                        self.writing_tx.remove(obj);
                    }
                }
            }
        }

        if let Some(waiting_txs) = self.reverse_wait_table.remove(txid) {
            for other_txid in waiting_txs {
                if let Some(waiting_tx_set) = self.wait_table.get_mut(&other_txid) {
                    waiting_tx_set.remove(txid);

                    if waiting_tx_set.is_empty() {
                        self.wait_table.remove(&other_txid);
                        let ready_tx = self.get_tx(&other_txid).clone();
                        self.ready.send(ready_tx).expect("send failed");
                    }
                }
            }
        }
    }

    fn get_tx(&self, txid: &TransactionDigest) -> &TransactionWithEffects {
        self.tx_store.get(txid).unwrap()
    }
}
