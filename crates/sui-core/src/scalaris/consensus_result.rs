/*
* 20240504
* Listener for handling consensus output result
* ConsensusListener holds all Sender for all grpc clients,
* which established connection to the consensus service
*/
use consensus_common::proto::{CommitedTransactions, ExternalTransaction};
use std::sync::Arc;
use tokio::sync::RwLock;

use super::{types::CommitedTransactionsResultSender, NsTransaction};

pub struct ConsensusListener {
    senders: Arc<RwLock<Vec<CommitedTransactionsResultSender>>>,
}
impl Default for ConsensusListener {
    fn default() -> Self {
        Self {
            senders: Default::default(),
        }
    }
}
impl ConsensusListener {
    // Add listener for new client connection
    pub async fn add_listener(&self, sender: CommitedTransactionsResultSender) {
        let mut guard = self.senders.write().await;
        guard.push(sender);
    }
    // Send consensus result to all client
    pub async fn notify(&self, ns_transactions: Vec<NsTransaction>) {
        let transactions = ns_transactions
            .into_iter()
            .map(|ns_tx| ns_tx.into())
            .collect::<Vec<ExternalTransaction>>();
        let commited_transactions = CommitedTransactions { transactions };
        let senders = self.senders.read().await;
        let mut handles = vec![];
        //Loop throw all channel sender
        for sender in senders.iter() {
            let send_transactions = commited_transactions.clone();
            let clone_sender = sender.clone();
            let handle = tokio::spawn(async move { clone_sender.send(Ok(send_transactions)) });
            handles.push(handle);
        }
    }
}
