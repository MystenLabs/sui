// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::fmt::Debug;
use std::time::Duration;

use futures::stream::StreamExt;
use log::info;
use rxrust::observer::Observer;
use serde::Deserialize;

use sui_sdk::rpc_types::{EventFilter, SuiEvent};
use sui_sdk::types::base_types::SuiAddress;
use sui_sdk::types::parse_sui_struct_tag;
use sui_sdk::{SuiClient, SuiClientBuilder};

use crate::listeners::Subject;

pub struct SuiListener {
    client: SuiClient,
    axelar_gateway: SuiAddress,
}

impl SuiListener {
    pub async fn new(config: SuiNetworkConfig) -> Result<Self, anyhow::Error> {
        Ok(SuiListener {
            client: SuiClientBuilder::default()
                .ws_url(config.ws_url)
                .ws_ping_interval(Duration::from_secs(20))
                .build(config.rpc_url)
                .await?,
            axelar_gateway: config.axelar_gateway,
        })
    }
    pub async fn listen<T: Clone + SuiAxelarEvent>(self, mut subject: Subject<T>) {
        let mut events = self
            .client
            .event_api()
            .subscribe_event(EventFilter::All(vec![EventFilter::MoveEventType(
                parse_sui_struct_tag(&format!(
                    "{}::{}::{}",
                    self.axelar_gateway,
                    T::EVENT_MODULE,
                    T::EVENT_TYPE
                ))
                .unwrap(),
            )]))
            .await
            .expect("Cannot subscribe to Sui events.");

        info!(
            "Start listening to Sui events: [{}]",
            std::any::type_name::<T>()
        );

        while let Some(Ok(ev)) = events.next().await {
            match T::parse_event(ev) {
                Ok(ev) => subject.next(ev),
                Err(e) => println!("Error: {e}"),
            }
        }
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

pub struct SuiNetworkConfig {
    rpc_url: String,
    ws_url: String,
    private_key: String,
    axelar_gateway: SuiAddress,
}

impl Default for SuiNetworkConfig {
    fn default() -> Self {
        SuiNetworkConfig {
            rpc_url: "https://rpc.testnet.sui.io:443".to_string(),
            ws_url: "wss://rpc.testnet.sui.io:443".to_string(),
            private_key: "".to_string(),
            axelar_gateway: Default::default(),
        }
    }
}
