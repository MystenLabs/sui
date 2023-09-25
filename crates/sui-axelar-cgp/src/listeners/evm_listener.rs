// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use ethcontract::contract;
use ethcontract::errors::EventError;
use ethcontract::prelude::*;
use futures::stream::BoxStream;
use futures::StreamExt;
use log::info;
use rxrust::observer::Observer;
use web3::api::Web3;
use web3::transports::WebSocket;

pub use crate::listeners::evm_listener::gateway::event_data::*;
use crate::listeners::Subject;

contract!(
    "etherscan:0xed9938294acf9ee52d097133ca2caaff0c804f16",
    contract = IAxelarGateway,
    mod = gateway,
    deployments {
        1 => "0x4F4495243837681061C4743b74B3eEdf548D56A5",
        5 => "0xe432150cce91c13a887f7D836923d5597adD8E31",
    },
    event_derives(serde::Deserialize, serde::Serialize),
);
pub struct EvmListener {
    contract: IAxelarGateway,
}

impl EvmListener {
    pub async fn new(config: EvmNetworkConfig) -> Result<Self, anyhow::Error> {
        let ws = WebSocket::new(&config.rpc_url)
            .await
            .expect("transport failed");
        let web3 = Web3::new(ws);
        let contract = IAxelarGateway::deployed(&web3)
            .await
            .expect("locating deployed contract failed");

        Ok(EvmListener { contract })
    }
    pub async fn listen<T: Clone + EvmEvent>(self, mut subject: Subject<T>) {
        let mut events = T::filter_event(&self.contract);

        info!(
            "Start listening to EVM events: [{}]",
            std::any::type_name::<T>()
        );

        while let Some(ev) = events.next().await {
            // TODO: handle errors
            match ev {
                Ok(ev) => subject.next(ev),
                Err(e) => println!("Error: {e}"),
            }
        }
    }
}

pub trait EvmEvent {
    fn filter_event(contract: &IAxelarGateway) -> BoxStream<Result<Self, EventError>>
    where
        Self: Sized;
}

pub struct EvmNetworkConfig {
    id: String,
    name: String,
    rpc_url: String,
    gateway: String,
    finality: u64,
    private_key: String,
}

impl Default for EvmNetworkConfig {
    fn default() -> Self {
        Self {
            id: "".to_string(),
            name: "".to_string(),
            rpc_url: "wss://mainnet.infura.io/ws/v3/a06ac77299dd4addacb0838b3b73e0a0".to_string(),
            gateway: "".to_string(),
            finality: 0,
            private_key: "".to_string(),
        }
    }
}

impl EvmEvent for OperatorshipTransferred {
    fn filter_event(
        contract: &IAxelarGateway,
    ) -> BoxStream<Result<OperatorshipTransferred, EventError>> {
        contract
            .events()
            .operatorship_transferred()
            .stream()
            .map(|ev| {
                ev.map(
                    |ev: Event<EventStatus<OperatorshipTransferred>>| match ev.data {
                        EventStatus::Added(o) => o,
                        EventStatus::Removed(o) => o,
                    },
                )
            })
            .boxed()
    }
}

impl EvmEvent for ContractCallWithToken {
    fn filter_event(
        contract: &IAxelarGateway,
    ) -> BoxStream<Result<ContractCallWithToken, EventError>> {
        contract
            .events()
            .contract_call_with_token()
            .stream()
            .map(|ev| {
                ev.map(
                    |ev: Event<EventStatus<ContractCallWithToken>>| match ev.data {
                        EventStatus::Added(o) => o,
                        EventStatus::Removed(o) => o,
                    },
                )
            })
            .boxed()
    }
}
