#[path = "generated/narwhal.rs"]
#[rustfmt::skip]
mod narwhal;

use bytes::Bytes;
pub use narwhal::{
    primary_to_primary_client::PrimaryToPrimaryClient,
    primary_to_primary_server::{PrimaryToPrimary, PrimaryToPrimaryServer},
    primary_to_worker_client::PrimaryToWorkerClient,
    primary_to_worker_server::{PrimaryToWorker, PrimaryToWorkerServer},
    transactions_client::TransactionsClient,
    transactions_server::{Transactions, TransactionsServer},
    worker_to_primary_client::WorkerToPrimaryClient,
    worker_to_primary_server::{WorkerToPrimary, WorkerToPrimaryServer},
    worker_to_worker_client::WorkerToWorkerClient,
    worker_to_worker_server::{WorkerToWorker, WorkerToWorkerServer},
    BincodeEncodedPayload, Empty, Transaction as TransactionProto,
};

impl BincodeEncodedPayload {
    pub fn deserialize<T: serde::de::DeserializeOwned>(&self) -> Result<T, bincode::Error> {
        bincode::deserialize(self.payload.as_ref())
    }

    pub fn try_from<T: serde::Serialize>(value: &T) -> Result<Self, bincode::Error> {
        let payload = bincode::serialize(value)?.into();
        Ok(Self { payload })
    }
}

impl From<Bytes> for BincodeEncodedPayload {
    fn from(payload: Bytes) -> Self {
        Self { payload }
    }
}
