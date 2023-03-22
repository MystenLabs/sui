use std::sync::Arc;

use arc_swap::ArcSwapOption;
use thiserror::Error;
use types::{metered_channel::Sender, Transaction, TxResponse};

static LOCAL_NARWHAL_CLIENT: ArcSwapOption<LocalNarwhalClient> = ArcSwapOption::const_empty();

/// The maximum allowed size of transactions into Narwhal.
pub const MAX_ALLOWED_TRANSACTION_SIZE: usize = 6 * 1024 * 1024;

/// Errors returned to clients submitting transactions to Narwhal.
#[derive(Clone, Debug, Error)]
pub enum NarwhalError {
    #[error("Failed to include transaction in a header!")]
    TransactionNotIncludedInHeader,

    #[error("Narwhal is shutting down!")]
    ShuttingDown,

    #[error("Transaction is too large: size={0} limit={1}")]
    TransactionTooLarge(usize, usize),
}

/// A client that connects to Narwhal locally.
#[derive(Clone)]
pub struct LocalNarwhalClient {
    tx_batch_maker: Sender<(Transaction, TxResponse)>,
}

impl LocalNarwhalClient {
    pub fn new(tx_batch_maker: Sender<(Transaction, TxResponse)>) -> Arc<Self> {
        Arc::new(Self { tx_batch_maker })
    }

    /// Sets the global instance of LocalNarwhalClient.
    pub fn set(instance: Arc<Self>) {
        LOCAL_NARWHAL_CLIENT.store(Some(instance));
    }

    /// Gets the global instance of LocalNarwhalClient.
    pub fn get() -> Option<Arc<Self>> {
        LOCAL_NARWHAL_CLIENT.load_full()
    }

    /// Submits a transaction to the local Narwhal worker.
    pub async fn submit_transaction(&self, transaction: Transaction) -> Result<(), NarwhalError> {
        if transaction.len() > MAX_ALLOWED_TRANSACTION_SIZE {
            return Err(NarwhalError::TransactionTooLarge(
                transaction.len(),
                MAX_ALLOWED_TRANSACTION_SIZE,
            ));
        }
        // Send the transaction to the batch maker.
        let (notifier, when_done) = tokio::sync::oneshot::channel();
        self.tx_batch_maker
            .send((transaction, notifier))
            .await
            .map_err(|_| NarwhalError::ShuttingDown)?;

        let _digest = when_done
            .await
            .map_err(|_| NarwhalError::TransactionNotIncludedInHeader)?;

        Ok(())
    }
}
