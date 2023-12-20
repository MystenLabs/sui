// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::types::EthLog;
use ethers::{
    abi::RawLog,
    contract::{abigen, EthLogDecode},
};
use serde::{Deserialize, Serialize};

use crate::types::{BridgeAction, EthToSuiBridgeAction};

// TODO: write a macro to handle variants

// TODO: Dummy placeholder, will be replaced by actual abis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EthBridgeEvent {
    ExampleContract(ExampleContractEvents),
}

abigen!(
    ExampleContract,
    "abi/example.json",
    event_derives(serde::Deserialize, serde::Serialize)
);

impl EthBridgeEvent {
    pub fn try_from_eth_log(log: &EthLog) -> Option<EthBridgeEvent> {
        let raw_log = RawLog {
            topics: log.log.topics.clone(),
            data: log.log.data.to_vec(),
        };
        if let Ok(decoded) = ExampleContractEvents::decode_log(&raw_log) {
            return Some(EthBridgeEvent::ExampleContract(decoded));
        }

        // TODO: try other variants
        None
    }
}

impl EthBridgeEvent {
    pub fn try_into_bridge_action(
        self,
        eth_tx_hash: ethers::types::H256,
        eth_event_index: u16,
    ) -> Option<BridgeAction> {
        match self {
            EthBridgeEvent::ExampleContract(event) => {
                Some(BridgeAction::EthToSuiBridgeAction(EthToSuiBridgeAction {
                    eth_tx_hash,
                    eth_event_index,
                    eth_bridge_event: event.clone(),
                }))
            }
        }
    }
}

#[cfg(test)]
pub mod tests {
    use ethers::types::Address as EthAddress;
    use ethers::{
        abi::{long_signature, ParamType},
        types::{Log, H256},
    };
    use hex_literal::hex;

    use crate::types::{BridgeAction, EthToSuiBridgeAction};

    use super::{ExampleContractEvents, TransferFilter};

    /// Returns a test Log and corresponding BridgeAction
    // Refernece: https://github.com/rust-ethereum/ethabi/blob/master/ethabi/src/event.rs#L192
    pub fn get_test_log_and_action(contract_address: EthAddress) -> (Log, BridgeAction) {
        let log = Log {
            address: contract_address,
            topics: vec![
                long_signature(
                    "Transfer",
                    &[ParamType::Address, ParamType::Address, ParamType::Uint(256)],
                ),
                hex!("ddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef").into(),
                hex!("000000000000000000000000dbf5e9c5206d0db70a90108bf936da60221dc080").into(),
            ],
            data: hex!(
                "
                0000000000000000000000000000000000000000000000000000000000000003
                "
            )
            .into(),
            block_hash: Some(H256::random()),
            block_number: Some(1.into()),
            transaction_hash: Some(H256::random()),
            log_index: Some(0.into()),
            ..Default::default()
        };
        let bridge_action = BridgeAction::EthToSuiBridgeAction(EthToSuiBridgeAction {
            eth_tx_hash: log.transaction_hash.unwrap(),
            eth_event_index: 10,
            eth_bridge_event: ExampleContractEvents::TransferFilter(TransferFilter {
                from: log.topics[1].into(),
                to: log.topics[2].into(),
                amount: 3.into(), // matches `data` in log
            }),
        });
        (log, bridge_action)
    }
}
