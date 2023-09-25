// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::future::join_all;
use rxrust::observable::ObservableItem;

use crate::listeners::evm_listener::{ContractCallWithToken, EvmListener, EvmNetworkConfig};
use crate::listeners::sui_listener::{ContractCall, SuiListener, SuiNetworkConfig};
use crate::listeners::Subject;

mod event_handlers;
mod listeners;

#[derive(Default)]
pub struct SuiAxelarRelayer;

impl SuiAxelarRelayer {
    pub async fn start() -> Result<(), anyhow::Error> {
        let sui_listener = SuiListener::new(SuiNetworkConfig::default()).await?;
        let evm_listener = EvmListener::new(EvmNetworkConfig::default()).await?;

        let sui_contract_call = Subject::<ContractCall>::default();
        sui_contract_call.clone().subscribe(|call| {
            // todo: pass to axelar
            println!("{call:?}")
        });

        let evm_contract_call_with_token = Subject::<ContractCallWithToken>::default();
        evm_contract_call_with_token.clone().subscribe(|call| {
            // todo: pass to Sui
            //handle_evm_contract_call()
        });

        join_all(vec![
            tokio::spawn(sui_listener.listen(sui_contract_call)),
            tokio::spawn(evm_listener.listen(evm_contract_call_with_token)),
        ])
        .await;

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    SuiAxelarRelayer::start().await
}
