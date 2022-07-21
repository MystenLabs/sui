// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{helper::verify_transfer_object_event, TestCaseImpl, TestContext};
use anyhow::bail;
use async_trait::async_trait;
use sui_json_rpc_types::{GetObjectDataResponse, SuiExecutionStatus};
use sui_types::{
    crypto::get_key_pair, event::TransferType, object::Owner, SUI_FRAMEWORK_OBJECT_ID,
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
        let (recipient_addr, _) = get_key_pair();
        let data = ctx
            .get_gateway()
            .public_transfer_object(
                signer,
                *obj_to_transfer.id(),
                Some(*gas_obj.id()),
                5000,
                recipient_addr,
            )
            .await
            .expect("Failed to get transaction data for transfer.");

        let response = ctx
            .sign_and_execute(data, "coin transfer")
            .await
            .to_effect_response()
            .unwrap();

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

        verify_transfer_object_event(
            &event,
            Some(SUI_FRAMEWORK_OBJECT_ID),
            Some("native".into()),
            Some(signer),
            Some(Owner::AddressOwner(recipient_addr)),
            Some(*obj_to_transfer.id()),
            None,
            Some(TransferType::Coin),
        )?;

        // Verify fullnode observes the txn
        // Let fullnode sync
        ctx.let_fullnode_sync().await;
        let object_read = ctx
            .get_fullnode()
            .get_object(*obj_to_transfer.id())
            .await
            .or_else(|e| bail!("Failed to get created NFT object: {e}"))?;

        if let GetObjectDataResponse::Exists(sui_object) = object_read {
            assert_eq!(
                sui_object.owner,
                Owner::AddressOwner(recipient_addr),
                "Expect new owner to be the recipient_addr, but got {:?}",
                sui_object.owner
            );
            Ok(())
        } else {
            bail!(
                "Object {} does not exist or was deleted",
                *obj_to_transfer.id()
            );
        }
    }
}
