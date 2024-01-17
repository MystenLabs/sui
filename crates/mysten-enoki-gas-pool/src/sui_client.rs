// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::types::GasCoin;
use crate::{retry_forever, retry_with_max_delay};
use std::time::Duration;
use sui_json_rpc_types::SuiTransactionBlockEffectsAPI;
use sui_json_rpc_types::{
    SuiData, SuiObjectDataOptions, SuiObjectResponse, SuiTransactionBlockEffects,
    SuiTransactionBlockResponseOptions,
};
use sui_sdk::SuiClientBuilder;
use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::coin::{PAY_MODULE_NAME, PAY_SPLIT_N_FUNC_NAME};
use sui_types::gas_coin::GAS;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{ObjectArg, Transaction, TransactionKind};
use sui_types::SUI_FRAMEWORK_PACKAGE_ID;
use tap::TapFallible;
use tokio_retry::strategy::ExponentialBackoff;
#[cfg(not(test))]
use tokio_retry::strategy::FixedInterval;
use tokio_retry::Retry;
use tracing::{debug, info};

#[derive(Debug, Default)]
pub struct UpdatedGasCoins {
    pub live_gas_coins: Vec<GasCoin>,
    pub deleted_gas_coins: Vec<ObjectID>,
}

#[derive(Clone)]
pub struct SuiClient {
    sui_client: sui_sdk::SuiClient,
}

impl SuiClient {
    pub async fn new(fullnode_url: &str) -> Self {
        let sui_client = SuiClientBuilder::default()
            .build(fullnode_url)
            .await
            .unwrap();
        Self { sui_client }
    }

    pub async fn get_all_owned_sui_coins(&self, address: SuiAddress) -> Vec<GasCoin> {
        info!(
            "Querying all gas coins owned by sponsor address: {}",
            address
        );
        let mut cursor = None;
        let mut coins = Vec::new();
        loop {
            let page = retry_forever!(async {
                self.sui_client
                    .coin_read_api()
                    .get_coins(address, None, cursor, None)
                    .await
                    .tap_err(|err| debug!("Failed to get owned gas coins: {:?}", err))
            })
            .unwrap();
            for coin in page.data {
                coins.push(GasCoin {
                    object_ref: coin.object_ref(),
                    balance: coin.balance,
                });
            }
            if page.has_next_page {
                cursor = page.next_cursor;
            } else {
                break;
            }
        }
        debug!("List of owned coins: {:?}", coins);
        coins
    }

    pub async fn get_reference_gas_price(&self) -> u64 {
        retry_forever!(async {
            self.sui_client
                .governance_api()
                .get_reference_gas_price()
                .await
                .tap_err(|err| debug!("Failed to get reference gas price: {:?}", err))
        })
        .unwrap()
    }

    pub async fn get_latest_gas_objects(&self, object_ids: &[ObjectID]) -> UpdatedGasCoins {
        let mut objects = vec![];
        for chunk in object_ids.chunks(50) {
            objects.extend(
                retry_forever!(async {
                    let result = self
                        .sui_client
                        .read_api()
                        .multi_get_object_with_options(
                            chunk.to_vec(),
                            SuiObjectDataOptions::default().with_bcs(),
                        )
                        .await
                        .map_err(anyhow::Error::from)
                        .tap_err(|err| debug!("Failed to read objects: {:?}", err))?;
                    if result.len() != chunk.len() {
                        anyhow::bail!(
                            "Unable to get all gas coins, got {} out of {}",
                            result.len(),
                            chunk.len()
                        );
                    }
                    Ok(result)
                })
                .unwrap(),
            );
        }
        let mut result = UpdatedGasCoins::default();
        objects.iter().zip(object_ids).for_each(|(o, id)| {
            match Self::try_get_sui_coin_balance(o) {
                Some(coin) => {
                    debug!("Got updated gas coin info: {:?}", coin);
                    result.live_gas_coins.push(coin);
                }
                None => {
                    debug!("Unable to get gas coin info for object {:?}", id);
                    result.deleted_gas_coins.push(*id)
                }
            }
        });
        result
    }

    pub async fn calibrate_gas_cost_per_object(
        &self,
        sponsor_address: SuiAddress,
        gas_coin: &GasCoin,
    ) -> u64 {
        const SPLIT_COUNT: u64 = 500;
        let mut pt_builder = ProgrammableTransactionBuilder::new();
        let object_arg = pt_builder
            .obj(ObjectArg::ImmOrOwnedObject(gas_coin.object_ref))
            .unwrap();
        let pure_arg = pt_builder.pure(SPLIT_COUNT).unwrap();
        pt_builder.programmable_move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            PAY_MODULE_NAME.into(),
            PAY_SPLIT_N_FUNC_NAME.into(),
            vec![GAS::type_tag()],
            vec![object_arg, pure_arg],
        );
        let pt = pt_builder.finish();
        let response = retry_forever!(async {
            self.sui_client
                .read_api()
                .dev_inspect_transaction_block(
                    sponsor_address,
                    TransactionKind::ProgrammableTransaction(pt.clone()),
                    None,
                    None,
                    None,
                )
                .await
        })
        .unwrap();
        let gas_used = response.effects.gas_cost_summary().gas_used();
        // Multiply by 2 to be conservative and resilient to precision loss.
        gas_used / SPLIT_COUNT * 2
    }

    pub async fn execute_transaction(
        &self,
        tx: Transaction,
        max_delay: Duration,
    ) -> anyhow::Result<SuiTransactionBlockEffects> {
        debug!("Executing transaction: {:?}, digest={:?}", tx, tx.digest());
        let response = retry_with_max_delay!(
            async {
                self.sui_client
                    .quorum_driver_api()
                    .execute_transaction_block(
                        tx.clone(),
                        SuiTransactionBlockResponseOptions::full_content(),
                        None,
                    )
                    .await
                    .map_err(anyhow::Error::from)
                    .and_then(|r| r.effects.ok_or_else(|| anyhow::anyhow!("No effects")))
            },
            max_delay
        );
        debug!("Transaction execution response: {:?}", response);
        response
    }

    fn try_get_sui_coin_balance(object: &SuiObjectResponse) -> Option<GasCoin> {
        let data = object.data.as_ref()?;
        let object_ref = data.object_ref();
        let move_obj = data.bcs.as_ref()?.try_as_move()?;
        if move_obj.type_ != sui_types::gas_coin::GasCoin::type_() {
            return None;
        }
        let gas_coin: sui_types::gas_coin::GasCoin = bcs::from_bytes(&move_obj.bcs_bytes).ok()?;
        Some(GasCoin {
            object_ref,
            balance: gas_coin.value(),
        })
    }
}
