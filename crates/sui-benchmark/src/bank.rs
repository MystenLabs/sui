// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::util::{make_pay_tx, UpdatedAndNewlyMintedGasCoins};
use crate::workloads::payload::Payload;
use crate::workloads::workload::{Workload, WorkloadBuilder, MAX_BUDGET};
use crate::workloads::{Gas, GasCoinConfig};
use crate::ValidatorProxy;
use anyhow::{Error, Result};
use itertools::Itertools;
use move_core_types::language_storage::TypeTag;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use sui_types::base_types::{ObjectRef, SuiAddress};
use sui_types::crypto::AccountKeyPair;
use sui_types::gas_coin::GAS;
use sui_types::messages::{CallArg, ObjectArg, TransactionData, VerifiedTransaction};
use sui_types::utils::to_sender_signed_transaction;
use sui_types::{coin, SUI_FRAMEWORK_OBJECT_ID};
use tracing::{debug, error};

/// Bank is used for generating gas for running the benchmark. It is initialized with two gas coins i.e.
/// `pay_coin` which is split into smaller gas coins and `primary_gas` which is the gas coin used
/// for executing coin split transactions
#[derive(Clone)]
pub struct BenchmarkBank {
    pub proxy: Arc<dyn ValidatorProxy + Send + Sync>,
    // Gas to use for execution of gas generation transaction
    pub primary_gas: Gas,
    // Coin to use for splitting and generating small gas coins. Can accept
    // multiple coins in case we no longer have one large coin that will support
    // generating all coins for the workloads.
    pub pay_coins: Vec<Gas>,
}

impl BenchmarkBank {
    pub fn new(
        proxy: Arc<dyn ValidatorProxy + Send + Sync>,
        primary_gas: Gas,
        pay_coins: Vec<Gas>,
    ) -> Self {
        BenchmarkBank {
            proxy,
            primary_gas,
            pay_coins,
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
        debug!("Number of gas requests = {}", chunked_coin_configs.len());
        for chunk in chunked_coin_configs {
            let gas_coins = self.split_coin_and_pay(chunk, gas_price).await?;
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

    fn make_split_coin_tx(
        &self,
        split_amounts: Vec<u64>,
        gas_price: u64,
        keypair: &AccountKeyPair,
        pay_coin: &Gas,
    ) -> Result<VerifiedTransaction> {
        let split_coin = TransactionData::new_move_call(
            self.primary_gas.1,
            SUI_FRAMEWORK_OBJECT_ID,
            coin::PAY_MODULE_NAME.to_owned(),
            coin::PAY_SPLIT_VEC_FUNC_NAME.to_owned(),
            vec![TypeTag::Struct(Box::new(GAS::type_()))],
            self.primary_gas.0,
            vec![
                CallArg::Object(ObjectArg::ImmOrOwnedObject(pay_coin.0)),
                CallArg::Pure(bcs::to_bytes(&split_amounts).unwrap()),
            ],
            MAX_BUDGET,
            gas_price,
        )?;
        let verified_tx = to_sender_signed_transaction(split_coin, keypair);
        Ok(verified_tx)
    }

    async fn split_coin_and_pay(
        &mut self,
        coin_configs: &[GasCoinConfig],
        gas_price: u64,
    ) -> Result<UpdatedAndNewlyMintedGasCoins> {
        // split one coin into smaller coins of different amounts and send them to recipients
        let split_amounts: Vec<u64> = coin_configs.iter().map(|c| c.amount).collect();
        // TODO: Instead of splitting the coin and then using pay tx to transfer it to recipients,
        // we can do both in one tx with pay_sui which will split the coin out for us before
        // transferring it to recipients
        let mut updated_pay_coins = Vec::new();
        let mut transferred_coins: Result<Vec<Gas>> = Err(Error::msg("Failed to split coin"));
        debug!(
            "Splitting {} coin(s) into {} coin(s) of balance {}",
            self.pay_coins.len(),
            split_amounts.len(),
            split_amounts[0],
        );
        for (idx, pay_coin) in self.pay_coins.iter().enumerate() {
            debug!("Attempting split of coin#{idx}");
            let verified_tx = self.make_split_coin_tx(
                split_amounts.clone(),
                gas_price,
                &self.primary_gas.2,
                pay_coin,
            )?;
            let effects = self
                .proxy
                .execute_transaction_block(verified_tx.into())
                .await;

            let effects = match effects {
                Ok(effects) => {
                    if !effects.is_ok() {
                        error!("Failed to split coin: {:?}", effects);
                        // TODO: check effects and make decision on what coin to use
                        // next based on errors.
                        let updated_gas = effects
                            .mutated()
                            .into_iter()
                            .find(|(k, _)| k.0 == self.primary_gas.0 .0)
                            .ok_or("Input gas missing in the effects")
                            .map_err(Error::msg);

                        match updated_gas {
                            Ok(updated_gas) => {
                                self.primary_gas = (
                                    updated_gas.0,
                                    updated_gas.1.get_owner_address()?,
                                    self.primary_gas.2.clone(),
                                );
                            }
                            Err(e) => {
                                error!("Failed to get mutated gas: {:?}", e);
                                continue;
                            }
                        };

                        let updated_coin = effects
                            .mutated()
                            .into_iter()
                            .find(|(k, _)| k.0 == pay_coin.0 .0)
                            .ok_or("Pay coin missing in the effects")
                            .map_err(Error::msg);

                        match updated_coin {
                            Ok(updated_coin) => {
                                updated_pay_coins.push((
                                    updated_coin.0,
                                    updated_coin.1.get_owner_address()?,
                                    self.primary_gas.2.clone(),
                                ));
                                continue;
                            }
                            Err(e) => {
                                error!("Failed to mutated pay coi: {:?}", e);
                                continue;
                            }
                        };
                    }
                    effects
                }
                Err(e) => {
                    error!("Failed to split coin: {:?}", e);
                    continue;
                }
            };

            let updated_gas = effects
                .mutated()
                .into_iter()
                .find(|(k, _)| k.0 == self.primary_gas.0 .0)
                .ok_or("Input gas missing in the effects")
                .map_err(Error::msg)?;
            let created_coins: Vec<ObjectRef> =
                effects.created().into_iter().map(|c| c.0).collect();
            assert_eq!(created_coins.len(), split_amounts.len());
            let updated_coin = effects
                .mutated()
                .into_iter()
                .find(|(k, _)| k.0 == pay_coin.0 .0)
                .ok_or("Input gas missing in the effects")
                .map_err(Error::msg)?;

            updated_pay_coins.push((
                updated_coin.0,
                updated_coin.1.get_owner_address()?,
                self.primary_gas.2.clone(),
            ));

            let recipient_addresses: Vec<SuiAddress> =
                coin_configs.iter().map(|g| g.address).collect();
            let verified_tx = make_pay_tx(
                created_coins,
                self.primary_gas.1,
                recipient_addresses,
                split_amounts,
                updated_gas.0,
                &self.primary_gas.2,
                gas_price,
            )?;
            let effects = self
                .proxy
                .execute_transaction_block(verified_tx.into())
                .await?;
            let address_map: HashMap<SuiAddress, Arc<AccountKeyPair>> = coin_configs
                .iter()
                .map(|c| (c.address, c.keypair.clone()))
                .collect();
            transferred_coins = effects
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
            let updated_gas = effects
                .mutated()
                .into_iter()
                .find(|(k, _)| k.0 == self.primary_gas.0 .0)
                .ok_or("Input gas missing in the effects")
                .map_err(Error::msg)?;

            self.primary_gas = (
                updated_gas.0,
                updated_gas.1.get_owner_address()?,
                self.primary_gas.2.clone(),
            );

            let (_, right) = self.pay_coins.split_at_mut(idx + 1);
            let remaining_pay_coins = right.to_vec();
            self.pay_coins = updated_pay_coins;
            self.pay_coins.extend(remaining_pay_coins);
            break;
        }

        transferred_coins
    }
}
