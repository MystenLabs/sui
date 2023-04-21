// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::util::UpdatedAndNewlyMintedGasCoins;
use crate::workloads::payload::Payload;
use crate::workloads::workload::{Workload, WorkloadBuilder};
use crate::workloads::{Gas, GasCoinConfig};
use crate::ValidatorProxy;
use anyhow::{Error, Result};
use itertools::Itertools;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use sui_core::test_utils::{make_pay_sui_transaction, make_transfer_sui_transaction};
use sui_types::base_types::SuiAddress;
use sui_types::crypto::AccountKeyPair;
use tracing::{debug, info};

const PAY_SUI_BUDGET: u64 = 4000000;

/// Bank is used for generating gas for running the benchmark.
#[derive(Clone)]
pub struct BenchmarkBank {
    pub proxy: Arc<dyn ValidatorProxy + Send + Sync>,
    // Coin used for paying for gas & splitting into smaller gas coins
    pub primary_coin: Gas,
}

impl BenchmarkBank {
    pub fn new(proxy: Arc<dyn ValidatorProxy + Send + Sync>, primary_coin: Gas) -> Self {
        BenchmarkBank {
            proxy,
            primary_coin,
        }
    }
    pub async fn generate(
        &mut self,
        builders: Vec<Box<dyn WorkloadBuilder<dyn Payload>>>,
        gas_price: u64,
        chunk_size: u64,
    ) -> Result<Vec<Box<dyn Workload<dyn Payload>>>> {
        let mut coin_configs = VecDeque::new();
        for builder in builders.iter() {
            let init_gas_config = builder.generate_coin_config_for_init().await;
            let payload_gas_config = builder.generate_coin_config_for_payloads().await;
            coin_configs.push_back(init_gas_config);
            coin_configs.push_back(payload_gas_config);
        }
        let mut all_coin_configs = vec![];
        coin_configs
            .iter()
            .for_each(|v| all_coin_configs.extend(v.clone()));

        let mut new_gas_coins: Vec<Gas> = vec![];
        let chunked_coin_configs = all_coin_configs.chunks(chunk_size as usize);

        // Split off the initlization coin for this workload, to reduce contention
        // of main gas coin used by other instances of this tool.
        let total_gas_needed: u64 = all_coin_configs.iter().map(|c| c.amount).sum();
        let total_items = chunked_coin_configs
            .clone()
            .flat_map(|chunk| chunk.iter())
            .count();
        let pay_sui_budget = PAY_SUI_BUDGET * total_items as u64;
        let mut init_coin = self
            .create_init_coin(total_gas_needed + pay_sui_budget, gas_price)
            .await?;

        debug!("Number of gas requests = {}", chunked_coin_configs.len());
        for chunk in chunked_coin_configs {
            let gas_coins = self
                .pay_sui(chunk, &mut init_coin, gas_price, pay_sui_budget)
                .await?;
            new_gas_coins.extend(gas_coins);
        }
        let mut workloads = vec![];
        for builder in builders.iter() {
            let init_gas_config = coin_configs.pop_front().unwrap();
            let payload_gas_config = coin_configs.pop_front().unwrap();
            let init_gas: Vec<Gas> = init_gas_config
                .iter()
                .map(|c| {
                    let (index, _) = new_gas_coins
                        .iter()
                        .find_position(|g| g.1 == c.address)
                        .unwrap();
                    new_gas_coins.remove(index)
                })
                .collect();
            let payload_gas: Vec<Gas> = payload_gas_config
                .iter()
                .map(|c| {
                    let (index, _) = new_gas_coins
                        .iter()
                        .find_position(|g| g.1 == c.address)
                        .unwrap();
                    new_gas_coins.remove(index)
                })
                .collect();
            workloads.push(builder.build(init_gas, payload_gas).await);
        }
        Ok(workloads)
    }

    async fn pay_sui(
        &mut self,
        coin_configs: &[GasCoinConfig],
        mut init_coin: &mut Gas,
        gas_price: u64,
        budget: u64,
    ) -> Result<UpdatedAndNewlyMintedGasCoins> {
        let recipient_addresses: Vec<SuiAddress> = coin_configs.iter().map(|g| g.address).collect();
        let amounts: Vec<u64> = coin_configs.iter().map(|c| c.amount).collect();

        info!(
            "Creating {} coin(s) of balance {}...",
            amounts.len(),
            amounts[0],
        );

        let verified_tx = make_pay_sui_transaction(
            init_coin.0,
            vec![],
            recipient_addresses,
            amounts,
            init_coin.1,
            &init_coin.2,
            gas_price,
            budget,
        );

        let effects = self
            .proxy
            .execute_transaction_block(verified_tx.into())
            .await?;

        if !effects.is_ok() {
            effects.print_gas_summary();
            panic!("Could not generate coins for workload...");
        }

        let updated_gas = effects
            .mutated()
            .into_iter()
            .find(|(k, _)| k.0 == init_coin.0 .0)
            .ok_or("Input gas missing in the effects")
            .map_err(Error::msg)?;

        init_coin.0 = updated_gas.0;
        init_coin.1 = updated_gas.1.get_owner_address()?;
        init_coin.2 = self.primary_coin.2.clone();

        let address_map: HashMap<SuiAddress, Arc<AccountKeyPair>> = coin_configs
            .iter()
            .map(|c| (c.address, c.keypair.clone()))
            .collect();

        let transferred_coins: Result<Vec<Gas>> = effects
            .created()
            .into_iter()
            .map(|c| {
                let address = c.1.get_owner_address()?;
                let keypair = address_map
                    .get(&address)
                    .ok_or("Owner address missing in the address map")
                    .map_err(Error::msg)?;
                Ok((c.0, address, keypair.clone()))
            })
            .collect();

        transferred_coins
    }

    async fn create_init_coin(&mut self, amount: u64, gas_price: u64) -> Result<Gas> {
        info!("Creating initilization coin of value {amount}...");

        let verified_tx = make_transfer_sui_transaction(
            self.primary_coin.0,
            self.primary_coin.1,
            Some(amount),
            self.primary_coin.1,
            &self.primary_coin.2,
            gas_price,
        );

        let effects = self
            .proxy
            .execute_transaction_block(verified_tx.into())
            .await?;

        if !effects.is_ok() {
            effects.print_gas_summary();
            panic!("Failed to create initilization coin for workload.");
        }

        let updated_gas = effects
            .mutated()
            .into_iter()
            .find(|(k, _)| k.0 == self.primary_coin.0 .0)
            .ok_or("Input gas missing in the effects")
            .map_err(Error::msg)?;

        self.primary_coin = (
            updated_gas.0,
            updated_gas.1.get_owner_address()?,
            self.primary_coin.2.clone(),
        );

        match effects.created().get(0) {
            Some(created_coin) => Ok((
                created_coin.0,
                created_coin.1.get_owner_address()?,
                self.primary_coin.2.clone(),
            )),
            None => panic!("Failed to create initilization coin for workload."),
        }
    }
}
