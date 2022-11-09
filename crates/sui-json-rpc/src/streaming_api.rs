// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::api::TransactionStreamingApiServer;
use crate::SuiRpcModule;
use async_trait::async_trait;
use futures::{StreamExt, TryStream};
use jsonrpsee::types::SubscriptionResult;
use jsonrpsee_core::error::SubscriptionClosed;
use jsonrpsee_core::server::rpc_module::RpcModule;
use jsonrpsee_core::server::rpc_module::SubscriptionSink;
use serde::Serialize;
use std::fmt::Display;
use std::sync::Arc;
use sui_core::authority::AuthorityState;
use sui_core::transaction_streamer::TransactionStreamer;
use sui_json_rpc_types::SuiCertifiedTransaction;
use sui_json_rpc_types::SuiTransactionEffects;
use sui_json_rpc_types::SuiTransactionFilter;
use sui_json_rpc_types::SuiTransactionResponse;
use sui_metrics::spawn_monitored_task;
use sui_open_rpc::Module;
use sui_types::filter::TransactionFilter;
use tracing::warn;

pub struct TransactionStreamingApiImpl {
    state: Arc<AuthorityState>,
    transaction_streamer: Arc<TransactionStreamer>,
}

impl TransactionStreamingApiImpl {
    pub fn new(state: Arc<AuthorityState>, transaction_streamer: Arc<TransactionStreamer>) -> Self {
        Self {
            state,
            transaction_streamer,
        }
    }
}

#[async_trait]
impl TransactionStreamingApiServer for TransactionStreamingApiImpl {
    fn subscribe_transaction(
        &self,
        sink: SubscriptionSink,
        filter: SuiTransactionFilter,
    ) -> SubscriptionResult {
        let filter: TransactionFilter = filter.into();

        let state = self.state.clone();
        let stream = self.transaction_streamer.subscribe(filter);
        let stream = stream.then(move |(tx_cert, signed_effects)| {
            let state_clone = state.clone();
            async move {
                let sui_tx_cert = SuiCertifiedTransaction::try_from(tx_cert)?;
                let sui_tx_effects = SuiTransactionEffects::try_from(
                    signed_effects.into_data(),
                    state_clone.module_cache.as_ref(),
                )?;
                let digest = sui_tx_cert.transaction_digest;
                let ts = state_clone.get_timestamp_ms(&digest).await.unwrap_or(None);
                Ok::<SuiTransactionResponse, anyhow::Error>(SuiTransactionResponse {
                    certificate: sui_tx_cert,
                    effects: sui_tx_effects,
                    timestamp_ms: ts,
                    parsed_data: None,
                })
            }
        });
        spawn_subscription(sink, Box::pin(stream));

        Ok(())
    }
}

impl SuiRpcModule for TransactionStreamingApiImpl {
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        crate::api::TransactionStreamingApiOpenRpc::module_doc()
    }
}

pub fn spawn_subscription<S, T, E>(mut sink: SubscriptionSink, rx: S)
where
    S: TryStream<Ok = T, Error = E> + Unpin + Send + 'static,
    T: Serialize,
    E: Display,
{
    spawn_monitored_task!(async move {
        match sink.pipe_from_try_stream(rx).await {
            SubscriptionClosed::Success => {
                sink.close(SubscriptionClosed::Success);
            }
            SubscriptionClosed::RemotePeerAborted => (),
            SubscriptionClosed::Failed(err) => {
                warn!(error = ?err, "Event subscription closed.");
                sink.close(err);
            }
        };
    });
}
