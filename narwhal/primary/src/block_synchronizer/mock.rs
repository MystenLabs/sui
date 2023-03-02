// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::block_synchronizer::{BlockHeader, BlockSynchronizeResult, Command};
use fastcrypto::hash::Hash;
use std::collections::HashMap;
use tokio::sync::{mpsc, oneshot};
use types::CertificateDigest;

#[derive(Debug)]
enum Core {
    SynchronizeBlockHeaders {
        digests: Vec<CertificateDigest>,
        times: u32,
        result: Vec<BlockSynchronizeResult<BlockHeader>>,
        ready: oneshot::Sender<()>,
    },
    SynchronizeBlockPayload {
        digests: Vec<CertificateDigest>,
        times: u32,
        result: Vec<BlockSynchronizeResult<BlockHeader>>,
        ready: oneshot::Sender<()>,
    },
    AssertExpectations {
        ready: oneshot::Sender<()>,
    },
}

struct MockBlockSynchronizerCore {
    /// A map that holds the expected requests for sync block headers and their
    /// stubbed response.
    block_headers_expected_requests:
        HashMap<Vec<CertificateDigest>, (u32, Vec<BlockSynchronizeResult<BlockHeader>>)>,

    /// A map that holds the expected requests for sync block payload and their
    /// stubbed response.
    block_payload_expected_requests:
        HashMap<Vec<CertificateDigest>, (u32, Vec<BlockSynchronizeResult<BlockHeader>>)>,

    /// Channel to receive the messages that are supposed to be sent to the
    /// block synchronizer.
    rx_commands: mpsc::Receiver<Command>,

    /// Channel to receive the commands to mock the requests.
    rx_core: mpsc::Receiver<Core>,
}

impl MockBlockSynchronizerCore {
    async fn run(&mut self) {
        loop {
            tokio::select! {
                Some(command) = self.rx_commands.recv() => {
                    match command {
                        Command::SynchronizeBlockHeaders { digests, respond_to } => {
                            let (times, results) = self
                                .block_headers_expected_requests
                                .remove(&digests)
                                .unwrap_or_else(||panic!("{}", format!("Unexpected call received for SynchronizeBlockHeaders, {:?}", digests).as_str()))
                                .to_owned();

                            if times > 1 {
                                self.block_headers_expected_requests.insert(digests, (times - 1, results.clone()));
                            }

                            for result in results {
                                respond_to.send(result).await.expect("Couldn't send message");
                            }
                        }
                        Command::SynchronizeBlockPayload { certificates, respond_to } => {
                            let digests = certificates.into_iter().map(|c|c.digest()).collect();
                            let (times, results) = self
                                .block_payload_expected_requests
                                .remove(&digests)
                                .unwrap_or_else(||panic!("{}", format!("Unexpected call received for SynchronizeBlockPayload, {:?}", digests).as_str()))
                                .to_owned();

                            if times > 1 {
                                self.block_payload_expected_requests.insert(digests, (times - 1, results.clone()));
                            }

                            for result in results {
                                respond_to.send(result).await.expect("Couldn't send message");
                            }
                        }
                    }
                }
                Some(command) = self.rx_core.recv() => {
                    match command {
                        Core::SynchronizeBlockHeaders {
                            digests,
                            times,
                            result,
                            ready,
                        } => {
                            self.block_headers_expected_requests.insert(digests, (times, result));
                            ready.send(()).expect("Failed to send ready message");
                        },
                        Core::SynchronizeBlockPayload {
                            digests,
                            times,
                            result,
                            ready,
                        } => {
                            self.block_payload_expected_requests.insert(digests, (times, result));
                            ready.send(()).expect("Failed to send ready message");
                        },
                        Core::AssertExpectations {ready} => {
                            self.assert_expectations();
                            ready.send(()).expect("Failed to send ready message");
                        }
                    }
                }
            }
        }
    }

    fn assert_expectations(&self) {
        let mut result: String = "".to_string();

        for (digests, results) in &self.block_headers_expected_requests {
            result.push_str(
                format!(
                    "SynchronizeBlockHeaders, digests={:?}, results={:?}",
                    digests, results
                )
                .as_str(),
            );
        }

        for (digests, results) in &self.block_payload_expected_requests {
            result.push_str(
                format!(
                    "SynchronizeBlockPayload, digests={:?}, results={:?}",
                    digests, results
                )
                .as_str(),
            );
        }

        if !result.is_empty() {
            panic!(
                "There are expected calls that haven't been fulfilled \n\n {}",
                result
            );
        }
    }
}

/// A mock helper for the BlockSynchronizer to help us mock the responses
/// eliminating the need to wire in the actual BlockSynchronizer when needed
/// for other components.
pub struct MockBlockSynchronizer {
    tx_core: mpsc::Sender<Core>,
}

impl MockBlockSynchronizer {
    pub fn new(rx_commands: mpsc::Receiver<Command>) -> Self {
        let (tx_core, rx_core) = mpsc::channel(1);

        let mut core = MockBlockSynchronizerCore {
            block_headers_expected_requests: HashMap::new(),
            block_payload_expected_requests: HashMap::new(),
            rx_commands,
            rx_core,
        };

        tokio::spawn(async move {
            core.run().await;
        });

        Self { tx_core }
    }

    /// A simple method that allow us to mock responses for the
    /// SynchronizeBlockHeaders requests.
    /// `digests`: The digests we expect
    /// `results`: The results we would like to respond with
    /// `times`: How many times we should expect to be called.
    pub async fn expect_synchronize_block_headers(
        &self,
        digests: Vec<CertificateDigest>,
        result: Vec<BlockSynchronizeResult<BlockHeader>>,
        times: u32,
    ) {
        let (tx, rx) = oneshot::channel();
        self.tx_core
            .send(Core::SynchronizeBlockHeaders {
                digests,
                times,
                result,
                ready: tx,
            })
            .await
            .expect("Failed to send mock expectation");

        Self::await_channel(rx).await;
    }

    /// A method that allow us to mock responses for the
    /// SynchronizeBlockPayload requests. It has to be noted that we use
    /// the digests as a way to identify the expected certificates for
    /// the request since that on its own suffice to identify them.
    /// `digests`: The digests we expect
    /// `results`: The results we would like to respond with
    /// `times`: How many times we should expect to be called.
    pub async fn expect_synchronize_block_payload(
        &self,
        digests: Vec<CertificateDigest>,
        result: Vec<BlockSynchronizeResult<BlockHeader>>,
        times: u32,
    ) {
        let (tx, rx) = oneshot::channel();
        self.tx_core
            .send(Core::SynchronizeBlockPayload {
                digests,
                times,
                result,
                ready: tx,
            })
            .await
            .expect("Failed to send mock expectation");

        Self::await_channel(rx).await;
    }

    /// Asserts that all the expectations have been fulfilled and no
    /// expectation has been left without having been called.
    pub async fn assert_expectations(&self) {
        let (tx, rx) = oneshot::channel();
        self.tx_core
            .send(Core::AssertExpectations { ready: tx })
            .await
            .expect("Failed to assert expectations");

        Self::await_channel(rx).await;
    }

    /// Helper method to wait on a oneshot receiver channel
    /// and avoid printing the error. We expect when the
    /// MockBlockSynchronizerCore panics to violently close
    /// the provided oneshot channel. To ensure that the
    /// current thread will panic, we are handling the error
    /// case and we also print an empty message to avoid
    /// printing the receive error.
    async fn await_channel(rx: oneshot::Receiver<()>) {
        if rx.await.is_err() {
            panic!("");
        }
    }
}
