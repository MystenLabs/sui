use consensus_common::proto::{CommitedTransactions, ExternalTransaction};
use narwhal_types::Transaction;
use serde::{Deserialize, Serialize};
use sui_types::messages_consensus::ConsensusTransaction;
use tokio::sync::mpsc::UnboundedSender;
use tonic::{Response, Status};

pub type ConsensusStreamItem = Result<CommitedTransactions, Status>;
pub type CommitedTransactionsResultSender = UnboundedSender<ConsensusStreamItem>;
pub type ConsensusServiceResult<T> = Result<Response<T>, Status>;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NsTransaction {
    pub namespace: String,
    #[serde(with = "serde_bytes")]
    pub transaction: Transaction,
}
impl NsTransaction {
    pub fn new(namespace: String, transaction: Transaction) -> NsTransaction {
        Self {
            namespace,
            transaction,
        }
    }
}

impl Into<ExternalTransaction> for NsTransaction {
    fn into(self) -> ExternalTransaction {
        let NsTransaction {
            namespace,
            transaction,
        } = self;
        ExternalTransaction {
            namespace,
            tx_bytes: transaction,
        }
    }
}

impl From<ExternalTransaction> for NsTransaction {
    fn from(value: ExternalTransaction) -> Self {
        let ExternalTransaction {
            namespace,
            tx_bytes,
        } = value;

        NsTransaction {
            namespace,
            transaction: tx_bytes,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConsensusTransactionWrapper {
    Namespace(NsTransaction),
    Consensus(ConsensusTransaction),
}
