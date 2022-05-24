// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use consensus::{ConsensusOutput, ConsensusSyncRequest};
use crypto::traits::VerifyingKey;
use tokio::sync::mpsc::{Receiver, Sender};
use tracing::warn;
use types::{Certificate, SequenceNumber};

pub struct MockSequencer<PublicKey: VerifyingKey> {
    rx_sequence: Receiver<Certificate<PublicKey>>,
    rx_client: Receiver<ConsensusSyncRequest>,
    tx_client: Sender<ConsensusOutput<PublicKey>>,
    consensus_index: SequenceNumber,
    sequence: Vec<ConsensusOutput<PublicKey>>,
}

impl<PublicKey: VerifyingKey> MockSequencer<PublicKey> {
    pub fn spawn(
        rx_sequence: Receiver<Certificate<PublicKey>>,
        rx_client: Receiver<ConsensusSyncRequest>,
        tx_client: Sender<ConsensusOutput<PublicKey>>,
    ) {
        tokio::spawn(async move {
            Self {
                rx_sequence,
                rx_client,
                tx_client,
                consensus_index: SequenceNumber::default(),
                sequence: Vec::new(),
            }
            .run()
            .await;
        });
    }

    async fn synchronize(&mut self, request: ConsensusSyncRequest) {
        for i in request.missing {
            let message = self.sequence[i as usize].clone();
            if self.tx_client.send(message).await.is_err() {
                warn!("Failed to deliver sequenced message to client");
                break;
            }
        }
    }

    async fn run(&mut self) {
        loop {
            tokio::select! {
                // Update the subscriber every time a message is sequenced.
                Some(certificate) = self.rx_sequence.recv() => {
                    let message = ConsensusOutput {
                        certificate,
                        consensus_index: self.consensus_index
                    };

                    self.consensus_index += 1;
                    self.sequence.push(message.clone());

                    if  self.tx_client.send(message).await.is_err() {
                        warn!("Failed to deliver sequenced message to client");
                    }
                },

                // Receive sync requests form the subscriber.
                Some(request) = self.rx_client.recv() => self.synchronize(request).await,
            }
        }
    }
}
