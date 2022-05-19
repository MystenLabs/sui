// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui::{
    config::SUI_NETWORK_CONFIG,
    sui_full_node::SuiFullNode,
    wallet_commands::{WalletCommandResult, WalletCommands, WalletContext},
};

use sui_types::base_types::{ObjectID, SuiAddress, TransactionDigest};
use test_utils::network::setup_network_and_wallet_in_working_dir;

async fn transfer_coin(
    node: &SuiFullNode,
    context: &mut WalletContext,
) -> Result<(ObjectID, SuiAddress, SuiAddress, TransactionDigest), anyhow::Error> {
    let sender = context.config.accounts.get(0).cloned().unwrap();
    let receiver = context.config.accounts.get(1).cloned().unwrap();

    let object_refs = node.client.get_owned_objects(sender).await?;
    let gas_object = object_refs.get(0).unwrap().0;
    let object_to_send = object_refs.get(1).unwrap().0;

    // Send an object
    let res = WalletCommands::Transfer {
        to: receiver,
        coin_object_id: object_to_send,
        gas: Some(gas_object),
        gas_budget: 50000,
    }
    .execute(context)
    .await?;

    let digest = if let WalletCommandResult::Transfer(_, cert, _) = res {
        cert.transaction_digest
    } else {
        panic!("transfer command did not return WalletCommandResult::Transfer");
    };

    Ok((object_to_send, sender, receiver, digest))
}

#[tokio::test]
async fn test_full_node_follows_txes() -> Result<(), anyhow::Error> {
    let working_dir = tempfile::tempdir()?;

    let (_network, mut context, _) = setup_network_and_wallet_in_working_dir(&working_dir).await?;

    let node = SuiFullNode::start_with_genesis(
        &working_dir.path().join(SUI_NETWORK_CONFIG),
        working_dir.path(),
    )
    .await?;

    let (transfered_object, _, receiver, _) = transfer_coin(&node, &mut context).await?;

    // verify that the node has seen the transfer
    let object_info = node.client.get_object_info(transfered_object).await?;
    let object = object_info.into_object()?;

    assert_eq!(object.owner.get_owner_address().unwrap(), receiver);

    Ok(())
}

#[tokio::test]
async fn test_full_node_indexes() -> Result<(), anyhow::Error> {
    let working_dir = tempfile::tempdir()?;

    let (_network, mut context, _) = setup_network_and_wallet_in_working_dir(&working_dir).await?;

    let node = SuiFullNode::start_with_genesis(
        &working_dir.path().join(SUI_NETWORK_CONFIG),
        working_dir.path(),
    )
    .await?;

    let (transfered_object, sender, receiver, digest) = transfer_coin(&node, &mut context).await?;

    node.client.state.wait_for_cert(digest).await?;

    let txes = node
        .client
        .state
        .get_transactions_by_input_object(transfered_object)
        .await?;
    assert_eq!(txes[0].1, digest);

    let txes = node
        .client
        .state
        .get_transactions_by_mutated_object(transfered_object)
        .await?;
    assert_eq!(txes[0].1, digest);

    let txes = node.client.state.get_transactions_from_addr(sender).await?;
    assert_eq!(txes[0].1, digest);

    let txes = node.client.state.get_transactions_to_addr(receiver).await?;
    assert_eq!(txes[0].1, digest);

    // Note that this is also considered a tx to the sender, because it mutated
    // one or more of the sender's objects.
    let txes = node.client.state.get_transactions_to_addr(sender).await?;
    assert_eq!(txes[0].1, digest);

    // No transactions have originated from the receiver
    let txes = node
        .client
        .state
        .get_transactions_from_addr(receiver)
        .await?;
    assert_eq!(txes.len(), 0);

    Ok(())
}
