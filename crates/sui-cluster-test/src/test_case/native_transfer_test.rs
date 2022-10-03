// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    helper::{ObjectChecker, TransferObjectEventChecker},
    TestCaseImpl, TestContext,
};
use anyhow::bail;
use async_trait::async_trait;
use sui_json_rpc_types::SuiExecutionStatus;
use sui_types::{
    crypto::{get_key_pair, AccountKeyPair},
    event::TransferType,
    object::Owner,
    SUI_FRAMEWORK_OBJECT_ID,
};
use tracing::info;
pub struct NativeTransferTest;

#[async_trait]
impl TestCaseImpl for NativeTransferTest {
    fn name(&self) -> &'static str {
        "NativeTransfer"
    }

    fn description(&self) -> &'static str {
        "Test tranferring SUI coins natively"
    }

    async fn run(&self, ctx: &mut TestContext) -> Result<(), anyhow::Error> {
        info!("Testing gas coin transfer");
        let mut sui_objs = ctx.get_sui_from_faucet(Some(2)).await;
        let gas_obj = sui_objs.swap_remove(0);
        let obj_to_transfer = sui_objs.swap_remove(0);
        let signer = ctx.get_wallet_address();
        let (recipient_addr, _): (_, AccountKeyPair) = get_key_pair();
        let data = ctx
            .get_gateway()
            .transaction_builder()
            .transfer_object(
                signer,
                *obj_to_transfer.id(),
                Some(*gas_obj.id()),
                5000,
                recipient_addr,
            )
            .await
            .expect("Failed to get transaction data for transfer.");

        let response = ctx.sign_and_execute(data, "coin transfer").await;

        let mut effects = response.effects;
        if !matches!(effects.status, SuiExecutionStatus::Success { .. }) {
            bail!(
                "Failed to execute transfer tranasction: {:?}",
                effects.status
            )
        }

        // Examine effects
        let events = &mut effects.events;
        assert_eq!(
            events.len(),
            1,
            "Expect one event emitted, but got {}",
            events.len()
        );
        let event = events.remove(0);

        TransferObjectEventChecker::new()
            .package_id(SUI_FRAMEWORK_OBJECT_ID)
            .transaction_module("native".into())
            .sender(signer)
            .recipient(Owner::AddressOwner(recipient_addr))
            .object_id(*obj_to_transfer.id())
            .type_(TransferType::Coin)
            .check(&event);

        // Verify fullnode observes the txn
        ctx.let_fullnode_sync(vec![response.certificate.transaction_digest], 5)
            .await;

        let _ = ObjectChecker::new(*obj_to_transfer.id())
            .owner(Owner::AddressOwner(recipient_addr))
            .check(ctx.get_fullnode())
            .await;

        Ok(())
    }
}
