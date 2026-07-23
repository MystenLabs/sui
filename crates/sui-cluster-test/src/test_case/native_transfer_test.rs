// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use tracing::info;

use sui_rpc_api::client::ExecutedTransaction;
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    crypto::{AccountKeyPair, get_key_pair},
    object::Owner,
};

use crate::{
    TestCaseImpl, TestContext,
    helper::{BalanceChangeChecker, ObjectChecker},
};

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
        let mut sui_objs = ctx.get_sui_from_faucet(Some(1)).await;
        let gas_obj = ctx.get_sui_from_faucet(Some(1)).await.swap_remove(0);

        let signer = ctx.get_wallet_address();
        let (recipient_addr, _): (_, AccountKeyPair) = get_key_pair();
        let gas_budget = 2_000_000;

        // Test transfer object: move a whole SUI coin object to the recipient,
        // paying for gas with a separate, explicitly-supplied gas coin (from the
        // faucet response) so transaction construction is deterministic.
        let obj_to_transfer: ObjectID = *sui_objs.swap_remove(0).id();
        let gas_ref = ctx.current_object_ref(*gas_obj.id()).await;
        let builder = ctx.get_grpc_client().transaction_builder();
        let data = builder
            .transfer_object(
                signer,
                obj_to_transfer,
                Some(gas_ref.0),
                gas_budget,
                recipient_addr,
            )
            .await?;
        let response = ctx.sign_and_execute(data, "coin transfer").await;
        Self::examine_response(ctx, &response, signer, recipient_addr, obj_to_transfer).await;

        // Test transfer of a second, distinct SUI coin object.
        let mut sui_objs_2 = ctx.get_sui_from_faucet(Some(1)).await;
        let obj_to_transfer_2 = *sui_objs_2.swap_remove(0).id();
        // Refresh the gas ref: its version/digest changed after the first tx.
        let gas_ref = ctx.current_object_ref(*gas_obj.id()).await;
        let builder = ctx.get_grpc_client().transaction_builder();
        let data = builder
            .transfer_object(
                signer,
                obj_to_transfer_2,
                Some(gas_ref.0),
                gas_budget,
                recipient_addr,
            )
            .await?;
        let response = ctx.sign_and_execute(data, "coin transfer").await;

        // Verify the SECOND transferred object (previously this asserted the
        // first object's ID, which was already a no-op bug).
        Self::examine_response(ctx, &response, signer, recipient_addr, obj_to_transfer_2).await;
        Ok(())
    }
}

impl NativeTransferTest {
    async fn examine_response(
        ctx: &TestContext,
        response: &ExecutedTransaction,
        signer: SuiAddress,
        recipient: SuiAddress,
        obj_to_transfer_id: ObjectID,
    ) {
        let mut balance_changes = response.balance_changes.clone();
        // for transfer we only expect 2 balance changes, one for sender and one
        // for recipient.
        assert_eq!(
            balance_changes.len(),
            2,
            "Expect 2 balance changes emitted, but got {}",
            balance_changes.len()
        );
        // Order of balance change is not fixed so need to check whose balance
        // comes first. This makes sure the recipient always comes first.
        let signer_sdk: sui_sdk_types::Address = signer.into();
        if balance_changes[0].address == signer_sdk {
            balance_changes.reverse()
        }
        BalanceChangeChecker::new()
            .address(recipient)
            .coin_type("0x2::sui::SUI")
            .check(&balance_changes.remove(0));
        BalanceChangeChecker::new()
            .address(signer)
            .coin_type("0x2::sui::SUI")
            .check(&balance_changes.remove(0));

        // The executed transaction is already checkpointed; read the transferred
        // object by ID and confirm the new owner (`LedgerService`). A failed
        // read must fail the test — it is the ownership assertion itself.
        ObjectChecker::new(obj_to_transfer_id)
            .owner(Owner::AddressOwner(recipient))
            .check(&ctx.get_grpc_client())
            .await
            .expect("transferred object must be readable and owned by the recipient");
    }
}
