// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui::{
    config::SUI_NETWORK_CONFIG,
    sui_full_node::SuiFullNode,
    wallet_commands::WalletCommands,
};

use tracing_test::traced_test;
use test_utils::network::setup_network_and_wallet_in_working_dir;

#[traced_test]
#[tokio::test]
async fn test_full_node_follows_txes() -> Result<(), anyhow::Error> {
    let working_dir = tempfile::tempdir()?;

    let (_network, mut context, _) = setup_network_and_wallet_in_working_dir(&working_dir).await?;

    let node = SuiFullNode::start_with_genesis(
        &working_dir.path().join(SUI_NETWORK_CONFIG),
        working_dir.path(),
    )
    .await?;

    let sender = context.config.accounts.get(0).cloned().unwrap();
    let receiver = context.config.accounts.get(1).cloned().unwrap();

    let object_refs = node.client.get_owned_objects(sender).await?;
    let gas_object = object_refs.get(0).unwrap().0;
    let object_to_send = object_refs.get(1).unwrap().0;

    // Send an object
    WalletCommands::Transfer {
        to: receiver,
        coin_object_id: object_to_send,
        gas: Some(gas_object),
        gas_budget: 50000,
    }
    .execute(&mut context)
    .await?;

    // verify that the node has seen the transfer
    let object_info = node.client.get_object_info(object_to_send).await?;
    let object = object_info.into_object()?;

    assert_eq!(object.owner.get_owner_address().unwrap(), receiver);

    Ok(())
}
