// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

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
    pub chain_id: String,
    #[serde(with = "serde_bytes")]
    pub transaction: Transaction,
}
impl NsTransaction {
    pub fn new(chain_id: String, transaction: Transaction) -> NsTransaction {
        Self {
            chain_id,
            transaction,
        }
    }
}

impl Into<ExternalTransaction> for NsTransaction {
    fn into(self) -> ExternalTransaction {
        let NsTransaction {
            chain_id,
            transaction,
        } = self;
        ExternalTransaction {
            chain_id,
            tx_bytes: transaction,
        }
    }
}

impl From<ExternalTransaction> for NsTransaction {
    fn from(value: ExternalTransaction) -> Self {
        let ExternalTransaction { chain_id, tx_bytes } = value;

        NsTransaction {
            chain_id,
            transaction: tx_bytes,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConsensusTransactionWrapper {
    Namespace(NsTransaction),
    Consensus(ConsensusTransaction),
}
