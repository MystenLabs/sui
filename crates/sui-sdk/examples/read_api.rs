// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod utils;

use sui_sdk::rpc_types::{SuiGetPastObjectRequest, SuiObjectDataOptions};
use sui_sdk::types::base_types::ObjectID;
use sui_sdk::SuiClientBuilder;
use utils::sui_address_for_examples;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let sui = SuiClientBuilder::default().build_testnet().await?; // testnet Sui network
    println!("Sui testnet version{:?}", sui.api_version());
    let active_address = sui_address_for_examples().await?; // get a random address for examples. Check utils module if you want to use a local wallet

    // ************ READ API ************ //

    println!("// ************ READ API ************ //\n");
    // Owned Objects
    let owned_objects = sui
        .read_api()
        .get_owned_objects(active_address, None, None, Some(5))
        .await?;
    println!(" *** Owned Objects ***");
    println!("{:?}", owned_objects);
    println!(" *** Owned Objects ***\n");

    // Dynamic Fields
    let dynamic_fields = sui
        .read_api()
        .get_dynamic_fields(ObjectID::from_address(active_address.into()), None, None)
        .await?;
    println!(" *** Dynamic Fields ***");
    println!("{:?}", dynamic_fields);
    println!(" *** Dynamic Fields ***\n");

    let object = owned_objects
        .data
        .get(0)
        .unwrap_or_else(|| panic!("No owned objects for this address {}", active_address));
    let object_data = object
        .data
        .as_ref()
        .unwrap_or_else(|| panic!("No object data for this SuiObjectResponse {:?}", object));
    let object_id = object_data.object_id;
    let version = object_data.version;

    let past_object = sui
        .read_api()
        .try_get_parsed_past_object(
            object_id,
            version,
            SuiObjectDataOptions {
                show_type: true,
                show_owner: true,
                show_previous_transaction: true,
                show_display: true,
                show_content: true,
                show_bcs: true,
                show_storage_rebate: true,
            },
        )
        .await?;
    println!(" *** Past Object *** ");
    println!("{:?}", past_object);
    println!(" *** Past Object ***\n");

    // try_multi_get_parsed_past_object
    let sui_get_past_object_request = past_object.clone().into_object()?;
    let multi_past_object = sui
        .read_api()
        .try_multi_get_parsed_past_object(
            vec![SuiGetPastObjectRequest {
                object_id: sui_get_past_object_request.object_id,
                version: sui_get_past_object_request.version,
            }],
            SuiObjectDataOptions {
                show_type: true,
                show_owner: true,
                show_previous_transaction: true,
                show_display: true,
                show_content: true,
                show_bcs: true,
                show_storage_rebate: true,
            },
        )
        .await?;
    println!(" *** Multi Past Object *** ");
    println!("{:?}", multi_past_object);
    println!(" *** Multi Past Object ***\n");

    // Object with options
    let object_with_options = sui
        .read_api()
        .get_object_with_options(
            sui_get_past_object_request.object_id,
            SuiObjectDataOptions {
                show_type: true,
                show_owner: true,
                show_previous_transaction: true,
                show_display: true,
                show_content: true,
                show_bcs: true,
                show_storage_rebate: true,
            },
        )
        .await?;

    println!(" *** Object with Options *** ");
    println!("{:?}", object_with_options);
    println!(" *** Object with Options ***\n");

    println!(" *** Chain identifier *** ");
    println!("{:?}", sui.read_api().get_chain_identifier().await?);
    println!(" *** Chain identifier ***\n ");

    println!(" *** Protocol Config *** ");
    println!("{:?}", sui.read_api().get_protocol_config(None).await?);
    println!(" *** Protocol Config ***\n ");
    // ************ END OF READ API ************ //

    Ok(())
}
