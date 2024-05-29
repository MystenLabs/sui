// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/*
* 20240504
* Implement grpc server for listening consensus request and sends transaction into consensus layer
*/
use anyhow::anyhow;
use consensus_common::proto::{ConsensusApi, ExternalTransaction, RequestEcho, ResponseEcho};
use fastcrypto::error::FastCryptoError;
use narwhal_worker::LazyNarwhalClient;
use prometheus::Registry;
use std::{pin::Pin, sync::Arc};
use sui_config::NodeConfig;
use sui_types::{
    crypto::{AccountKeyPair, AuthorityKeyPair, ToFromBytes},
    error::{SuiError, SuiResult},
};
use tap::TapFallible;
use tokio::sync::mpsc::{self};
use tokio_stream::{wrappers::UnboundedReceiverStream, Stream, StreamExt};
use tonic::Response;
use tracing::{error, info, warn};

use crate::ConsensusTransactionWrapper;

use super::{
    types::{
        CommitedTransactionsResultSender, ConsensusServiceResult, ConsensusStreamItem,
        NsTransaction,
    },
    CONSENSUS_LISTENER,
};

pub type ResponseStream = Pin<Box<dyn Stream<Item = ConsensusStreamItem> + Send>>;

#[async_trait::async_trait]
trait SubmitNsTransaction {
    async fn submit_ns_transaction(&self, transaction: NsTransaction) -> SuiResult;
}
/*
* 20240504
* Scalaris: Extend LazyNarwhalClient with a method handles submit namespace transaction
*/
#[async_trait::async_trait]
impl SubmitNsTransaction for LazyNarwhalClient {
    async fn submit_ns_transaction(&self, transaction: NsTransaction) -> SuiResult {
        let client = {
            let c = self.client.load();
            if c.is_some() {
                c
            } else {
                self.client.store(Some(self.get().await));
                self.client.load()
            }
        };
        let wrapper = ConsensusTransactionWrapper::Namespace(transaction);
        let client = client.as_ref().unwrap().load();
        let tx_bytes = bcs::to_bytes(&wrapper).expect("Serialization should not fail.");
        client
            .submit_transaction(tx_bytes)
            .await
            .map_err(|e| SuiError::FailedToSubmitToConsensus(format!("{:?}", e)))
            .tap_err(|r| {
                // Will be logged by caller as well.
                warn!("Submit transaction failed with: {:?}", r);
            })?;
        Ok(())
    }
}
#[derive(Clone)]
pub struct ConsensusServiceMetrics {
    // pub transaction_counter: Histogram,
}

impl ConsensusServiceMetrics {
    pub fn new(_registry: &Registry) -> Self {
        Self {
            // transaction_counter: Histogram::new_in_registry(
            //     "scalar_consensus_transaction_counter",
            //     "The input limit for transaction_counter, after applying the cap",
            //     registry,
            // ),
        }
    }
}
#[derive(Clone)]
pub struct ConsensusService {
    validator_keypair: Arc<AccountKeyPair>,
    narwhal_client: Arc<LazyNarwhalClient>,
    _metrics: Arc<ConsensusServiceMetrics>,
}
fn get_ed25519_from_bls12381(
    authority: &AuthorityKeyPair,
) -> Result<AccountKeyPair, FastCryptoError> {
    let authority_keypair = authority.as_bytes();
    assert!(authority_keypair.len() >= 32);
    AccountKeyPair::from_bytes(&authority_keypair[0..32])
}
/*
* 20240504
* Scalaris: current version create a narwhal client from consensus config
*/
impl ConsensusService {
    pub fn new(config: &NodeConfig, prometheus_registry: &Registry) -> anyhow::Result<Self> {
        let consensus_config = config
            .consensus_config
            .as_ref()
            .ok_or_else(|| anyhow!("Validator is missing consensus config"))?;
        let authority_keypair = config.protocol_key_pair();
        let validator_keypair = get_ed25519_from_bls12381(authority_keypair)?;
        let narwhal_client = Arc::new(LazyNarwhalClient::new(
            consensus_config.address().to_owned(),
        ));
        Ok(Self {
            validator_keypair: Arc::new(validator_keypair),
            narwhal_client,
            _metrics: Arc::new(ConsensusServiceMetrics::new(prometheus_registry)),
        })
    }
    pub async fn add_consensus_listener(&self, listener: CommitedTransactionsResultSender) {
        CONSENSUS_LISTENER.add_listener(listener).await;
    }
    pub async fn handle_consensus_transaction(
        &self,
        transaction_in: ExternalTransaction,
    ) -> anyhow::Result<()> {
        info!(
            "gRpc service handle consensus_transaction {:?}",
            &transaction_in
        );
        let ns_transaction = NsTransaction::from(transaction_in);
        //Send transaction to the consensus's worker
        self.narwhal_client
            .submit_ns_transaction(ns_transaction)
            .await
            .map_err(|err| anyhow!(err.to_string()))
    }
}

#[tonic::async_trait]
impl ConsensusApi for ConsensusService {
    async fn echo(
        &self,
        request: tonic::Request<RequestEcho>,
    ) -> ConsensusServiceResult<ResponseEcho> {
        info!("ConsensusServiceServer::echo");
        let echo_message = request.into_inner().message;

        Ok(Response::new(ResponseEcho {
            message: echo_message,
        }))
    }

    type InitTransactionStream = ResponseStream;
    /*
     * Consensus client init a duplex streaming connection to send external transaction
     * and to receives consensus output.
     * External trasaction contains a namespace field and a content in byte array
     */
    async fn init_transaction(
        &self,
        request: tonic::Request<tonic::Streaming<ExternalTransaction>>,
    ) -> ConsensusServiceResult<Self::InitTransactionStream> {
        info!("ConsensusServiceServer::init_transaction_streams");
        let mut in_stream = request.into_inner();
        /*
         * 20240504
         * Mỗi consensus client khi kết nối tới consensus server sẽ được map với 1 sender channel để nhận kết quả trả ra từ consensus layer
         * Todo: optimize listeners collections để chỉ gửi đúng các dữ liệu mà client quan tâm (ví dụ theo namespace)
         */
        let (tx_consensus, rx_consensus) = mpsc::unbounded_channel();
        self.add_consensus_listener(tx_consensus).await;
        let service = self.clone();
        let _handle = tokio::spawn(async move {
            //let service = consensus_service;
            while let Some(client_message) = in_stream.next().await {
                match client_message {
                    Ok(transaction_in) => {
                        let _handle_res =
                            service.handle_consensus_transaction(transaction_in).await;
                    }
                    Err(err) => {
                        error!("{:?}", err);
                    }
                }
            }
        });
        let out_stream = UnboundedReceiverStream::new(rx_consensus);

        Ok(Response::new(
            Box::pin(out_stream) as Self::InitTransactionStream
        ))
    }
}

#[cfg(test)]
mod tests {
    use sui_config::node::AuthorityKeyPairWithPath;

    #[test]
    fn test_get_ed25519_from_bls12381() {
        let authority_keypair = AuthorityKeyPairWithPath::new(
            get_key_pair_from_rng::<AuthorityKeyPair, _>(&mut OsRng).1,
        )
        .authority_keypair();
        let ed25519_keypair = super::get_ed25519_from_bls12381(authority_keypair);
        assert!(ed25519_keypair.is_ok())
    }
}
