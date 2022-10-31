// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::Stream;

use sui_types::messages::TxCertAndSignedEffects;

use sui_types::filter::TransactionFilter;

use tracing::error;

use super::streamer::Streamer;

const CHANNEL_SIZE: usize = 1000;

pub struct TransactionStreamer {
    streamer: Streamer<TxCertAndSignedEffects, TransactionFilter>,
}

impl TransactionStreamer {
    pub fn new() -> Self {
        TransactionStreamer {
            streamer: Streamer::spawn(CHANNEL_SIZE),
        }
    }

    pub fn subscribe(
        &self,
        filter: TransactionFilter,
    ) -> impl Stream<Item = TxCertAndSignedEffects> {
        self.streamer.subscribe(filter)
    }

    pub async fn enqueue(&self, tx: TxCertAndSignedEffects) -> bool {
        let tx_digest = *tx.0.digest();
        if let Err(e) = self.streamer.send(tx).await {
            error!(?tx_digest, error =? e, "Failed to send tx to dispatch");
            return false;
        }
        true
    }
}

impl Default for TransactionStreamer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use test_utils::messages::{make_tx_certs_and_signed_effects, test_shared_object_transactions};
    use tokio::time::{timeout, Duration};

    #[tokio::test]
    async fn test_basic() -> Result<(), anyhow::Error> {
        let streamer = TransactionStreamer::new();
        let mut stream = streamer.subscribe(TransactionFilter::Any);
        let tx = test_shared_object_transactions().swap_remove(0);
        let (mut tx_certs, mut signed_effects) = make_tx_certs_and_signed_effects(vec![tx]);
        let tx_cert = tx_certs.swap_remove(0);
        let tx_digest = *tx_cert.digest();
        let signed_effects = signed_effects.swap_remove(0);
        let result = streamer
            .enqueue((tx_cert.into(), signed_effects.clone()))
            .await;

        assert!(result);
        if let Some((cert, effects)) = stream.next().await {
            assert_eq!(cert.digest(), &tx_digest);
            assert_eq!(effects, signed_effects);
        } else {
            panic!("Expect Some value but got None");
        }

        // No more
        assert!(timeout(Duration::from_millis(500), stream.next())
            .await
            .is_err());
        Ok(())
    }
}
