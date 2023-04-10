// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{TestCaseImpl, TestContext};
use async_trait::async_trait;
use sui_json_rpc_types::{Balance, SuiTransactionBlockResponseOptions};
use sui_types::gas_coin::GAS;
use sui_types::messages::ExecuteTransactionRequestType;
use sui_types::object::Owner;
// use test_utils::messages::make_staking_transaction_with_wallet_context;
// use tracing::info;

pub struct CoinIndexTest;

#[async_trait]
impl TestCaseImpl for CoinIndexTest {
    fn name(&self) -> &'static str {
        "CoinIndex"
    }

    fn description(&self) -> &'static str {
        "Test executing coin index"
    }

    async fn run(&self, ctx: &mut TestContext) -> Result<(), anyhow::Error> {
        let gas_price = ctx.get_reference_gas_price().await;
        let account = ctx.get_wallet_address();
        let client = ctx.clone_fullnode_client();

        ctx.get_sui_from_faucet(None).await;
        let Balance {
            coin_object_count: mut old_coin_object_count,
            total_balance: mut old_total_balance,
            ..
        } = client.coin_read_api().get_balance(account, None).await?;

        let txn = ctx
            .make_transactions(1, 2_000_000 * gas_price)
            .await
            .swap_remove(0);

        let response = client
            .quorum_driver()
            .execute_transaction_block(
                txn,
                SuiTransactionBlockResponseOptions::new()
                    .with_effects()
                    .with_balance_changes(),
                Some(ExecuteTransactionRequestType::WaitForLocalExecution),
            )
            .await?;

        let balance_change = response.balance_changes.unwrap();
        let owner_balance = balance_change
            .iter()
            .find(|b| b.owner == Owner::AddressOwner(account))
            .unwrap();
        let recipient_balance = balance_change
            .iter()
            .find(|b| b.owner != Owner::AddressOwner(account))
            .unwrap();
        let Balance {
            coin_object_count,
            total_balance,
            coin_type,
            ..
        } = client.coin_read_api().get_balance(account, None).await?;
        assert_eq!(coin_type, GAS::type_().to_string());

        assert_eq!(coin_object_count, old_coin_object_count);
        assert_eq!(
            total_balance,
            (old_total_balance as i128 + owner_balance.amount) as u128
        );
        old_coin_object_count = coin_object_count;
        old_total_balance = total_balance;

        let Balance {
            coin_object_count,
            total_balance,
            ..
        } = client
            .coin_read_api()
            .get_balance(recipient_balance.owner.get_owner_address().unwrap(), None)
            .await?;
        assert_eq!(coin_object_count, 1);
        assert!(recipient_balance.amount > 0);
        assert_eq!(total_balance, recipient_balance.amount as u128);

        // // Staking
        // let validator_addr = ctx
        //     .get_latest_sui_system_state()
        //     .await
        //     .active_validators
        //     .get(0)
        //     .unwrap()
        //     .sui_address;
        // let txn =
        //     make_staking_transaction_with_wallet_context(ctx.get_wallet_mut(), validator_addr)
        //         .await;

        // let response = client
        //     .quorum_driver()
        //     .execute_transaction_block(
        //         txn,
        //         SuiTransactionBlockResponseOptions::new()
        //             .with_effects()
        //             .with_balance_changes(),
        //         Some(ExecuteTransactionRequestType::WaitForLocalExecution),
        //     )
        //     .await?;

        // info!("response: {:?}", response);
        // let balance_change = &response.balance_changes.unwrap()[0];
        // assert_eq!(balance_change.owner, Owner::AddressOwner(account));

        // let Balance {
        //     coin_object_count,
        //     total_balance,
        //     ..
        // } = client.coin_read_api().get_balance(account, None).await?;
        // assert_eq!(coin_object_count, old_coin_object_count - 1); // an object is staked
        // assert_eq!(
        //     total_balance,
        //     (old_total_balance as i128 + balance_change.amount) as u128
        // );
        // old_coin_object_count = coin_object_count;
        // old_total_balance = total_balance;

        // // let obj = response.effects.unwrap().gas_object().reference.object_id;
        // let mut objs = client
        //     .coin_read_api()
        //     .get_coins(account, None, None, None)
        //     .await?
        //     .data;
        // let primary_coin = objs.swap_remove(0);
        // let coin_to_merge = objs.swap_remove(0);

        Ok(())
    }
}
