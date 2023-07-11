// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
mod narwhal {
    #![allow(clippy::derive_partial_eq_without_eq)]
    tonic::include_proto!("narwhal");

    include!(concat!(env!("OUT_DIR"), "/narwhal.PrimaryToPrimary.rs"));
    include!(concat!(env!("OUT_DIR"), "/narwhal.PrimaryToWorker.rs"));
    include!(concat!(env!("OUT_DIR"), "/narwhal.WorkerToPrimary.rs"));
    include!(concat!(env!("OUT_DIR"), "/narwhal.WorkerToWorker.rs"));
}

use crate::Transaction;
use bytes::Bytes;

pub use narwhal::{
    primary_to_primary_client::PrimaryToPrimaryClient,
    primary_to_primary_server::{MockPrimaryToPrimary, PrimaryToPrimary, PrimaryToPrimaryServer},
    primary_to_worker_client::PrimaryToWorkerClient,
    primary_to_worker_server::{MockPrimaryToWorker, PrimaryToWorker, PrimaryToWorkerServer},
    transactions_client::TransactionsClient,
    transactions_server::{Transactions, TransactionsServer},
    worker_to_primary_client::WorkerToPrimaryClient,
    worker_to_primary_server::{MockWorkerToPrimary, WorkerToPrimary, WorkerToPrimaryServer},
    worker_to_worker_client::WorkerToWorkerClient,
    worker_to_worker_server::{MockWorkerToWorker, WorkerToWorker, WorkerToWorkerServer},
    Empty, Transaction as TransactionProto,
};

impl From<Transaction> for TransactionProto {
    fn from(transaction: Transaction) -> Self {
        TransactionProto {
            transaction: Bytes::from(transaction),
        }
    }
}

impl From<TransactionProto> for Transaction {
    fn from(transaction: TransactionProto) -> Self {
        transaction.transaction.to_vec()
    }
}
