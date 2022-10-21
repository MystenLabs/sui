// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use jsonrpsee::rpc_params;
use move_core_types::language_storage::TypeTag;
use serde::Deserialize;
use serde_json::json;
use tracing::info;

use sui::client_commands::{EXAMPLE_NFT_DESCRIPTION, EXAMPLE_NFT_NAME, EXAMPLE_NFT_URL};
use sui_json::SuiJsonValue;
use sui_json_rpc_types::SuiEvent;
use sui_types::base_types::SequenceNumber;
use sui_types::id::ID;
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    object::Owner,
    SUI_FRAMEWORK_ADDRESS, SUI_FRAMEWORK_OBJECT_ID,
};

use crate::helper::ObjectChecker;
use crate::{TestCaseImpl, TestContext};

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

        let args_json = json!([EXAMPLE_NFT_NAME, EXAMPLE_NFT_DESCRIPTION, EXAMPLE_NFT_URL,]);
        let mut args = vec![];
        for a in args_json.as_array().unwrap() {
            args.push(SuiJsonValue::new(a.clone()).unwrap());
        }
        let type_args: Vec<TypeTag> = vec![];
        let params = rpc_params![
            signer,
            ObjectID::from(SUI_FRAMEWORK_ADDRESS),
            "devnet_nft",
            "mint",
            type_args,
            args,
            Some(*gas_obj.id()),
            20000
        ];

        let data = ctx
            .build_transaction_remotely("sui_moveCall", params)
            .await?;
        let (_, effects) = ctx.sign_and_execute(data, "call contract").await;

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
            3,
            "Expect three event emitted, but got {}",
            events.len()
        );

        events
            .iter()
            .find(|e| {
                matches!(e, SuiEvent::NewObject {
                    package_id,
                    transaction_module,
                    sender, recipient, object_type, object_id
                } if
                    package_id == &SUI_FRAMEWORK_OBJECT_ID
                    && transaction_module == &String::from("devnet_nft")
                    && sender == &signer
                    && recipient == &Owner::AddressOwner(signer)
                    && object_id == &nft_id
                    && object_type == "0x2::devnet_nft::DevNetNFT"
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
        ctx.let_fullnode_sync(vec![effects.transaction_digest], 5)
            .await;

        let object = ObjectChecker::new(nft_id)
            .owner(Owner::AddressOwner(signer))
            .check_into_object(ctx.get_fullnode_client())
            .await;

        assert_eq!(
            object.reference.version,
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
