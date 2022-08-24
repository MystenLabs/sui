// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::api::EventReadApiServer;
use crate::api::EventStreamingApiServer;
use crate::streaming_api::spawn_subscription;
use crate::SuiRpcModule;
use async_trait::async_trait;
use futures::StreamExt;
use jsonrpsee::core::RpcResult;
use jsonrpsee::types::SubscriptionResult;
use jsonrpsee_core::server::rpc_module::RpcModule;
use jsonrpsee_core::server::rpc_module::SubscriptionSink;
use move_core_types::account_address::AccountAddress;
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::ModuleId;
use std::str::FromStr;
use std::sync::Arc;
use sui_core::authority::AuthorityState;
use sui_core::event_handler::EventHandler;
use sui_json_rpc_types::{SuiEvent, SuiEventEnvelope, SuiEventFilter};
use sui_open_rpc::Module;
use sui_types::base_types::{ObjectID, SuiAddress, TransactionDigest};
use sui_types::object::Owner;
use tracing::warn;

pub struct EventStreamingApiImpl {
    state: Arc<AuthorityState>,
    event_handler: Arc<EventHandler>,
}

impl EventStreamingApiImpl {
    pub fn new(state: Arc<AuthorityState>, event_handler: Arc<EventHandler>) -> Self {
        Self {
            state,
            event_handler,
        }
    }
}

#[async_trait]
impl EventStreamingApiServer for EventStreamingApiImpl {
    fn subscribe_event(
        &self,
        mut sink: SubscriptionSink,
        filter: SuiEventFilter,
    ) -> SubscriptionResult {
        let filter = match filter.try_into() {
            Ok(filter) => filter,
            Err(e) => {
                let e = jsonrpsee_core::Error::from(e);
                warn!(error = ?e, "Rejecting subscription request.");
                return Ok(sink.reject(e)?);
            }
        };

        let state = self.state.clone();
        let stream = self.event_handler.subscribe(filter);
        let stream = stream.map(move |e| {
            let event = SuiEvent::try_from(e.event, state.module_cache.as_ref());
            event.map(|event| SuiEventEnvelope {
                timestamp: e.timestamp,
                tx_digest: e.tx_digest,
                event,
            })
        });
        spawn_subscription(sink, stream);

        Ok(())
    }
}

impl SuiRpcModule for EventStreamingApiImpl {
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        crate::api::EventStreamingApiOpenRpc::module_doc()
    }
}

#[allow(unused)]
pub struct EventReadApiImpl {
    state: Arc<AuthorityState>,
    event_handler: Arc<EventHandler>,
}

impl EventReadApiImpl {
    pub fn new(state: Arc<AuthorityState>, event_handler: Arc<EventHandler>) -> Self {
        Self {
            state,
            event_handler,
        }
    }
}

#[allow(unused)]
#[async_trait]
impl EventReadApiServer for EventReadApiImpl {
    async fn get_events_by_transaction(
        &self,
        digest: TransactionDigest,
        count: usize,
    ) -> RpcResult<Vec<SuiEventEnvelope>> {
        let events = self.state.get_events_by_transaction(digest, count).await?;
        Ok(events)
    }

    async fn get_events_by_transaction_module(
        &self,
        package: ObjectID,
        module: String,
        count: usize,
        start_time: u64,
        end_time: u64,
    ) -> RpcResult<Vec<SuiEventEnvelope>> {
        let module_id = ModuleId::new(
            AccountAddress::from(package),
            Identifier::from_str(&module)?,
        );

        let events = self
            .state
            .get_events_by_transaction_module(&module_id, start_time, end_time, count)
            .await?;
        Ok(events)
    }

    async fn get_events_by_move_event_struct_name(
        &self,
        move_event_struct_name: String,
        count: usize,
        start_time: u64,
        end_time: u64,
    ) -> RpcResult<Vec<SuiEventEnvelope>> {
        let events = self
            .state
            .get_events_by_move_event_struct_name(
                &move_event_struct_name,
                start_time,
                end_time,
                count,
            )
            .await?;
        Ok(events)
    }

    async fn get_events_by_sender(
        &self,
        sender: SuiAddress,
        count: usize,
        start_time: u64,
        end_time: u64,
    ) -> RpcResult<Vec<SuiEventEnvelope>> {
        let events = self
            .state
            .get_events_by_sender(&sender, start_time, end_time, count)
            .await?;
        Ok(events)
    }

    async fn get_events_by_recipient(
        &self,
        recipient: Owner,
        count: usize,
        start_time: u64,
        end_time: u64,
    ) -> RpcResult<Vec<SuiEventEnvelope>> {
        let events = self
            .state
            .get_events_by_recipient(&recipient, start_time, end_time, count)
            .await?;
        Ok(events)
    }

    async fn get_events_by_object(
        &self,
        object: ObjectID,
        count: usize,
        start_time: u64,
        end_time: u64,
    ) -> RpcResult<Vec<SuiEventEnvelope>> {
        let events = self
            .state
            .get_events_by_object(&object, start_time, end_time, count)
            .await?;
        Ok(events)
    }

    async fn get_events_by_timerange(
        &self,
        count: usize,
        start_time: u64,
        end_time: u64,
    ) -> RpcResult<Vec<SuiEventEnvelope>> {
        let events = self
            .state
            .get_events_by_timerange(start_time, end_time, count)
            .await?;
        Ok(events)
    }
}

impl SuiRpcModule for EventReadApiImpl {
    fn rpc(self) -> RpcModule<Self> {
        self.into_rpc()
    }

    fn rpc_doc_module() -> Module {
        crate::api::EventReadApiOpenRpc::module_doc()
    }
}
