// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::convert::Infallible;
use std::fmt::Debug;

use futures::stream::StreamExt;
use rxrust::observer::Observer;
use rxrust::subject::SubjectThreads;
use serde::Deserialize;
use tracing::{error, info};

use sui_sdk::rpc_types::{EventFilter, SuiEvent};
use sui_sdk::types::base_types::ObjectID;
use sui_sdk::types::parse_sui_struct_tag;
use sui_sdk::SuiClient;

pub type Subject<T> = SubjectThreads<T, Infallible>;
pub struct SuiListener {
    client: SuiClient,
    gateway: ObjectID,
}

impl SuiListener {
    pub fn new(sui_client: SuiClient, gateway: ObjectID) -> Self {
        SuiListener {
            client: sui_client,
            gateway,
        }
    }
    pub async fn listen<T: Clone + SuiAxelarEvent>(self, mut subject: Subject<T>) {
        // todo: use event query api instead of ws subscription for replay support.
        let event_type = format!("{}::{}::{}", self.gateway, T::EVENT_MODULE, T::EVENT_TYPE);
        let mut events = self
            .client
            .event_api()
            .subscribe_event(EventFilter::All(vec![EventFilter::MoveEventType(
                parse_sui_struct_tag(&event_type).unwrap(),
            )]))
            .await
            .expect("Cannot subscribe to Sui events.");

        info!("Start listening to Sui events: {event_type}");

        while let Some(ev) = events.next().await {
            match T::parse_event(ev.expect("Subscription erred.")) {
                Ok(ev) => subject.next(ev),
                Err(e) => error!("Error: {e}"),
            }
        }

        // todo: reconnect
        panic!("Subscription to event '{event_type}' stopped.")
    }
}

pub trait SuiAxelarEvent {
    const EVENT_MODULE: &'static str;
    const EVENT_TYPE: &'static str;
    fn parse_event(event: SuiEvent) -> Result<Self, anyhow::Error>
    where
        Self: Sized;
}

#[derive(Deserialize, Debug, Clone)]
pub struct ContractCall {
    source: Vec<u8>,
    destination: Vec<u8>,
    destination_address: Vec<u8>,
    payload: Vec<u8>,
}

impl SuiAxelarEvent for ContractCall {
    const EVENT_MODULE: &'static str = "gateway";
    const EVENT_TYPE: &'static str = "ContractCall";
    fn parse_event(event: SuiEvent) -> Result<ContractCall, anyhow::Error> {
        // TODO: extra check for event type
        Ok(bcs::from_bytes(&event.bcs)?)
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct OperatorshipTransferred {
    epoch: u64,
    new_operators_hash: Vec<u8>,
}

impl SuiAxelarEvent for OperatorshipTransferred {
    const EVENT_MODULE: &'static str = "validators";
    const EVENT_TYPE: &'static str = "OperatorshipTransferred";
    fn parse_event(event: SuiEvent) -> Result<OperatorshipTransferred, anyhow::Error> {
        // TODO: extra check for event type
        Ok(bcs::from_bytes(&event.bcs)?)
    }
}
