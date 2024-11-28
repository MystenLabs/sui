// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use async_trait::async_trait;
use enum_dispatch::enum_dispatch;
use futures::{future, StreamExt};
use serde::{Deserialize, Serialize};

use sui_json_rpc_types::Coin;
use sui_sdk::SuiClient;
use sui_types::base_types::{ObjectRef, SuiAddress};
use sui_types::transaction::{ProgrammableTransaction, TransactionData};

use crate::errors::Error;
use crate::types::ConstructionMetadata;
use pay_coin::pay_coin_pt;
pub use pay_coin::PayCoin;
use pay_sui::pay_sui_pt;
pub use pay_sui::PaySui;
use stake::stake_pt;
pub use stake::Stake;
use withdraw_stake::withdraw_stake_pt;
pub use withdraw_stake::WithdrawStake;

mod pay_coin;
mod pay_sui;
mod stake;
mod withdraw_stake;

pub const MAX_GAS_COINS: usize = 255;
const MAX_COMMAND_ARGS: usize = 511;
const MAX_GAS_BUDGET: u64 = 50_000_000_000;
/// Minimum gas-units a tx might need
const START_GAS_UNITS: u64 = 1_000;

pub struct TransactionAndObjectData {
    pub gas_coins: Vec<ObjectRef>,
    pub extra_gas_coins: Vec<ObjectRef>,
    pub objects: Vec<ObjectRef>,
    pub pt: ProgrammableTransaction,
    /// Refers to the sum of the `Coin<SUI>` balance of the coins participating in the transaction;
    /// either as gas or as objects.
    pub total_sui_balance: i128,
    pub budget: u64,
}

#[async_trait]
#[enum_dispatch]
pub trait TryConstructTransaction {
    async fn try_fetch_needed_objects(
        self,
        client: &SuiClient,
        gas_price: Option<u64>,
        budget: Option<u64>,
    ) -> Result<TransactionAndObjectData, Error>;
}

#[enum_dispatch(TryConstructTransaction)]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum InternalOperation {
    PaySui(PaySui),
    PayCoin(PayCoin),
    Stake(Stake),
    WithdrawStake(WithdrawStake),
}

impl InternalOperation {
    pub fn sender(&self) -> SuiAddress {
        match self {
            InternalOperation::PaySui(PaySui { sender, .. })
            | InternalOperation::PayCoin(PayCoin { sender, .. })
            | InternalOperation::Stake(Stake { sender, .. })
            | InternalOperation::WithdrawStake(WithdrawStake { sender, .. }) => *sender,
        }
    }

    /// Combine with ConstructionMetadata to form the TransactionData
    pub fn try_into_data(self, metadata: ConstructionMetadata) -> Result<TransactionData, Error> {
        let pt = match self {
            Self::PaySui(PaySui {
                recipients,
                amounts,
                ..
            }) => pay_sui_pt(recipients, amounts, &metadata.extra_gas_coins)?,
            Self::PayCoin(PayCoin {
                recipients,
                amounts,
                ..
            }) => {
                let currency = &metadata
                    .currency
                    .ok_or(anyhow!("metadata.coin_type is needed to PayCoin"))?;
                pay_coin_pt(recipients, amounts, &metadata.objects, currency)?
            }
            InternalOperation::Stake(Stake {
                validator, amount, ..
            }) => {
                let (stake_all, amount) = match amount {
                    Some(amount) => (false, amount),
                    None => {
                        if (metadata.total_coin_value - metadata.budget as i128) < 0 {
                            return Err(anyhow!(
                                "ConstructionMetadata malformed. total_coin_value - budget < 0"
                            )
                            .into());
                        }
                        (true, metadata.total_coin_value as u64 - metadata.budget)
                    }
                };
                stake_pt(validator, amount, stake_all, &metadata.extra_gas_coins)?
            }
            InternalOperation::WithdrawStake(WithdrawStake { stake_ids, .. }) => {
                let withdraw_all = stake_ids.is_empty();
                withdraw_stake_pt(metadata.objects, withdraw_all)?
            }
        };

        Ok(TransactionData::new_programmable(
            metadata.sender,
            metadata.gas_coins,
            pt,
            metadata.budget,
            metadata.gas_price,
        ))
    }
}

/// When an address is spammed with a lot of dust gas-coins, we cannot merge the coins before
/// paying for gas. The only thing we can do is try to choose a subset of 255 coins which sum up to
/// more than the budget.
/// This function iterates over the coin-stream and it populates a vector of coins sorted by
/// balance descending. The iteration ends when the first 255 coins in this vector sum up to more
/// than the amount passed, or the coin-stream ends.
// TODO: test
async fn gather_coins_in_balance_reverse_order(
    client: &SuiClient,
    owner: SuiAddress,
    amount: u64,
) -> anyhow::Result<Vec<Coin>> {
    let mut sum_largest_255 = 0;
    let mut gathered_coins_reverse_sorted = vec![];
    client
        .coin_read_api()
        .get_coins_stream(owner, None)
        .take_while(|coin: &Coin| {
            if gathered_coins_reverse_sorted.is_empty() {
                sum_largest_255 += coin.balance;
                gathered_coins_reverse_sorted.push(coin.clone());
            } else {
                let pos = insert_in_reverse_order(&mut gathered_coins_reverse_sorted, coin.clone());
                if pos < MAX_GAS_COINS {
                    sum_largest_255 += coin.balance;
                    if gathered_coins_reverse_sorted.len() > MAX_GAS_COINS {
                        sum_largest_255 -= gathered_coins_reverse_sorted[MAX_GAS_COINS].balance;
                    }
                }
            }
            future::ready(amount < sum_largest_255)
        })
        .collect::<Vec<_>>()
        .await;
    Ok(gathered_coins_reverse_sorted)
}

fn insert_in_reverse_order(vec: &mut Vec<Coin>, coin: Coin) -> usize {
    match vec
        .iter()
        .enumerate()
        .find(|(_pos, existing)| existing.balance < coin.balance)
    {
        Some((pos, _)) => {
            vec.insert(pos, coin);
            pos
        }
        None => {
            vec.push(coin);
            vec.len() - 1
        }
    }
}

#[cfg(test)]
mod internal_operation_tests {
    use super::insert_in_reverse_order;
    use rand::rngs::StdRng;
    use rand::seq::SliceRandom;
    use rand::{Rng, SeedableRng};
    use sui_json_rpc_types::Coin;
    use sui_sdk::SUI_COIN_TYPE;
    use sui_types::base_types::random_object_ref;
    use sui_types::digests::TransactionDigest;

    #[test]
    fn test_insert_in_reverse_order() {
        const N_COINS: u64 = 10;
        let mut coins = (0..N_COINS)
            .map(|i| {
                let obj_ref = random_object_ref();
                Coin {
                    coin_type: SUI_COIN_TYPE.to_string(),
                    coin_object_id: obj_ref.0,
                    version: obj_ref.1,
                    digest: obj_ref.2,
                    balance: i,
                    previous_transaction: TransactionDigest::default(),
                }
            })
            .collect::<Vec<_>>();

        // Specify a seed for reproducible tests
        let mut rng = StdRng::from_seed([42; 32]);
        for _ in 0..5 {
            coins.shuffle(&mut rng);
            // Check the test-cases if necessary
            // println!("[");
            // coins.iter().for_each(|c| print!("{}, ", c.balance));
            // println!("");
            // println!("]");

            let mut ordered = Vec::with_capacity(N_COINS as usize);
            for coin in coins {
                insert_in_reverse_order(&mut ordered, coin);
            }

            let mut expected_balance = N_COINS;
            ordered.iter().for_each(|coin| {
                expected_balance -= 1;
                assert!(coin.balance == expected_balance);
            });
            coins = ordered;
        }

        // try adding some duplicate values
        let mut dupls: Vec<u64> = (0..5).map(|_| rng.gen::<u64>() % 10).collect();
        for &balance in &dupls {
            let obj_ref = random_object_ref();
            coins.push(Coin {
                coin_type: SUI_COIN_TYPE.to_string(),
                coin_object_id: obj_ref.0,
                version: obj_ref.1,
                digest: obj_ref.2,
                balance,
                previous_transaction: TransactionDigest::default(),
            });
        }
        // push duplicates to the edges too
        let obj_ref = random_object_ref();
        coins.push(Coin {
            coin_type: SUI_COIN_TYPE.to_string(),
            coin_object_id: obj_ref.0,
            version: obj_ref.1,
            digest: obj_ref.2,
            balance: 9,
            previous_transaction: TransactionDigest::default(),
        });
        dupls.push(9);
        let obj_ref = random_object_ref();
        coins.push(Coin {
            coin_type: SUI_COIN_TYPE.to_string(),
            coin_object_id: obj_ref.0,
            version: obj_ref.1,
            digest: obj_ref.2,
            balance: 0,
            previous_transaction: TransactionDigest::default(),
        });
        dupls.push(0);
        let mut dupls_to_find = dupls.clone();
        for _ in 0..5 {
            coins.shuffle(&mut rng);
            // Check the test-cases if necessary
            // println!("[");
            // coins.iter().for_each(|c| print!("{}, ", c.balance));
            // println!("");
            // println!("]");

            let mut ordered = Vec::with_capacity(N_COINS as usize + 5);
            for coin in coins {
                insert_in_reverse_order(&mut ordered, coin);
            }

            let mut expected_balance = N_COINS;
            ordered.iter().for_each(|coin| {
                if let Some(pos) = dupls_to_find.iter().position(|&x| x == expected_balance) {
                    dupls_to_find.remove(pos);
                } else {
                    expected_balance -= 1;
                };
                assert!(coin.balance == expected_balance);
            });
            coins = ordered;
            dupls_to_find = dupls.clone();
        }
    }
}
