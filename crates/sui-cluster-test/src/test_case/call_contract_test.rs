// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::helper::ObjectChecker;
use crate::{TestCaseImpl, TestContext};
use anyhow::bail;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;
use sui::client_commands::{call_move, EXAMPLE_NFT_DESCRIPTION, EXAMPLE_NFT_NAME, EXAMPLE_NFT_URL};
use sui_json::SuiJsonValue;
use sui_json_rpc_types::SuiEvent;
use sui_types::base_types::SequenceNumber;
use sui_types::id::ID;
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    object::Owner,
    SUI_FRAMEWORK_ADDRESS, SUI_FRAMEWORK_OBJECT_ID,
};
use tracing::info;

pub struct CallContractTest;

#[async_trait]
impl TestCaseImpl for CallContractTest {
    fn name(&self) -> &'static str {
        "CallContract"
    }

    fn description(&self) -> &'static str {
        "Test calling move contract"
    }

    async fn run(&self, ctx: &mut TestContext) -> Result<(), anyhow::Error> {
        info!("Testing calling move contract.");
        let signer = ctx.get_wallet_address();
        let mut sui_objs = ctx.get_sui_from_faucet(Some(1)).await;
        let gas_obj = sui_objs.swap_remove(0);

        let wallet_context = ctx.get_wallet_mut();
        let args_json = json!([EXAMPLE_NFT_NAME, EXAMPLE_NFT_DESCRIPTION, EXAMPLE_NFT_URL,]);
        let mut args = vec![];
        for a in args_json.as_array().unwrap() {
            args.push(SuiJsonValue::new(a.clone()).unwrap());
        }
        let (_, effects) = call_move(
            ObjectID::from(SUI_FRAMEWORK_ADDRESS),
            "devnet_nft",
            "mint",
            vec![],
            Some(*gas_obj.id()),
            5000,
            args,
            wallet_context,
        )
        .await
        .or_else(|e| bail!("Failed to call move contract: {e}"))?;

        // Retrieve created nft
        let nft_id = effects
            .created
            .first()
            .expect("Failed to create NFT")
            .reference
            .object_id;

        // Examine effects
        let events = &effects.events;
        assert_eq!(
            events.len(),
            2,
            "Expect one event emitted, but got {}",
            events.len()
        );

        events
            .iter()
            .find(|e| {
                matches!(e, SuiEvent::NewObject {
                    package_id,
                    transaction_module,
                    sender, recipient, object_id
                } if
                    package_id == &SUI_FRAMEWORK_OBJECT_ID
                    && transaction_module == &String::from("devnet_nft")
                    && sender == &signer
                    && recipient == &Owner::AddressOwner(signer)
                    && object_id == &nft_id
                )
            })
            .unwrap_or_else(|| panic!("Expect such a NewObject in events {:?}", events));

        events.iter().find(|e| matches!(e, SuiEvent::MoveEvent{
            package_id,
            transaction_module,
            sender,
            type_,
            fields: _,
            bcs
        } if
            package_id == &SUI_FRAMEWORK_OBJECT_ID
            && transaction_module == &String::from("devnet_nft")
            && sender == &signer
            && type_ == &String::from("0x2::devnet_nft::MintNFTEvent")
            && bcs::from_bytes::<MintNFTEvent>(bcs).unwrap() == MintNFTEvent {object_id: ID {bytes: nft_id}, creator: signer, name: EXAMPLE_NFT_NAME.into()}
        )).unwrap_or_else(|| panic!("Expect such a MoveEvent in events {:?}", events));

        // Verify fullnode observes the txn
        ctx.let_fullnode_sync().await;

        let sui_object = ObjectChecker::new(nft_id)
            .owner(Owner::AddressOwner(signer))
            .check_into_sui_object(ctx.get_fullnode())
            .await;

        assert_eq!(
            sui_object.reference.version,
            SequenceNumber::from_u64(1),
            "Expect sequence number to be 1"
        );

        Ok(())
    }
}

#[derive(Deserialize, Debug, PartialEq)]
struct MintNFTEvent {
    // The Object ID of the NFT
    object_id: ID,
    // The creator of the NFT
    creator: SuiAddress,
    // The name of the NFT
    name: String,
}
