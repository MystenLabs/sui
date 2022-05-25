// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::StreamExt;
use std::sync::Arc;
use sui::{
    config::SUI_NETWORK_CONFIG,
    wallet_commands::{WalletCommandResult, WalletCommands, WalletContext},
};
use sui_config::{NetworkConfig, PersistedConfig};
use sui_core::authority::AuthorityState;
use sui_node::SuiNode;

use sui_types::object::Owner;
use sui_types::{
    base_types::{ObjectID, SuiAddress, TransactionDigest},
    batch::UpdateItem,
    messages::{BatchInfoRequest, BatchInfoResponseItem},
};
use test_utils::network::setup_network_and_wallet_in_working_dir;
use tokio::time::{sleep, Duration};
use tracing::info;

async fn transfer_coin(
    node: &SuiNode,
    context: &mut WalletContext,
) -> Result<(ObjectID, SuiAddress, SuiAddress, TransactionDigest), anyhow::Error> {
    let sender = context.config.accounts.get(0).cloned().unwrap();
    let receiver = context.config.accounts.get(1).cloned().unwrap();

    let object_refs = node
        .state()
        .get_owner_objects(Owner::AddressOwner(sender))?;
    let gas_object = object_refs.get(0).unwrap().object_id;
    let object_to_send = object_refs.get(1).unwrap().object_id;

    // Send an object
    info!(
        "transferring coin {:?} from {:?} -> {:?}",
        object_to_send, sender, receiver
    );
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

async fn wait_for_tx(wait_digest: TransactionDigest, state: Arc<AuthorityState>) {
    let mut timeout = Box::pin(sleep(Duration::from_millis(5000)));

    let mut max_seq = Some(0);

    let mut stream = Box::pin(
        state
            .handle_batch_streaming(BatchInfoRequest {
                start: max_seq,
                length: 1000,
            })
            .await
            .unwrap(),
    );

    loop {
        tokio::select! {
            _ = &mut timeout => panic!("wait_for_tx timed out"),

            items = &mut stream.next() => {
                match items {
                    // Upon receiving a batch
                    Some(Ok(BatchInfoResponseItem(UpdateItem::Batch(batch)) )) => {
                        max_seq = Some(batch.batch.next_sequence_number);
                        info!(?max_seq, "Received Batch");
                    }
                    // Upon receiving a transaction digest we store it, if it is not processed already.
                    Some(Ok(BatchInfoResponseItem(UpdateItem::Transaction((_seq, digest))))) => {
                        info!(?digest, "Received Transaction");
                        if wait_digest == digest {
                            info!(?digest, "Digest found");
                            break;
                        }
                    },

                    Some(Err( err )) => panic!("{}", err),
                    None => {
                        info!(?max_seq, "Restarting Batch");
                        stream = Box::pin(
                                state
                                    .handle_batch_streaming(BatchInfoRequest {
                                        start: max_seq,
                                        length: 1000,
                                    })
                                    .await
                                    .unwrap(),
                            );

                    }
                }
            },
        }
    }
}

#[tokio::test]
async fn test_full_node_follows_txes() -> Result<(), anyhow::Error> {
    let working_dir = tempfile::tempdir()?;

    let (_network, mut context, _) = setup_network_and_wallet_in_working_dir(&working_dir).await?;

    let network_config_path = working_dir.path().join(SUI_NETWORK_CONFIG);
    let config: NetworkConfig = PersistedConfig::read(&network_config_path)?;
    let config = config.generate_fullnode_config();

    let node = SuiNode::start(&config).await?;

    let (transfered_object, _, receiver, digest) = transfer_coin(&node, &mut context).await?;
    wait_for_tx(digest, node.state().clone()).await;

    // verify that the node has seen the transfer
    let object_read = node.state().get_object_read(&transfered_object).await?;
    let object = object_read.into_object()?;

    assert_eq!(object.owner.get_owner_address().unwrap(), receiver);

    Ok(())
}

#[tokio::test]
async fn test_full_node_indexes() -> Result<(), anyhow::Error> {
    let subscriber = ::tracing_subscriber::FmtSubscriber::builder()
        .with_test_writer()
        .with_env_filter(::tracing_subscriber::EnvFilter::from_default_env())
        .finish();
    let _ = ::tracing::subscriber::set_global_default(subscriber);

    let working_dir = tempfile::tempdir()?;

    let (_network, mut context, _) = setup_network_and_wallet_in_working_dir(&working_dir).await?;

    let network_config_path = working_dir.path().join(SUI_NETWORK_CONFIG);
    let config: NetworkConfig = PersistedConfig::read(&network_config_path)?;
    let config = config.generate_fullnode_config();

    let node = SuiNode::start(&config).await?;

    let (transfered_object, sender, receiver, digest) = transfer_coin(&node, &mut context).await?;

    wait_for_tx(digest, node.state().clone()).await;

    let txes = node
        .state()
        .get_transactions_by_input_object(transfered_object)
        .await?;
    assert_eq!(txes[0].1, digest);

    let txes = node
        .state()
        .get_transactions_by_mutated_object(transfered_object)
        .await?;
    assert_eq!(txes[0].1, digest);

    let txes = node.state().get_transactions_from_addr(sender).await?;
    assert_eq!(txes[0].1, digest);

    let txes = node.state().get_transactions_to_addr(receiver).await?;
    assert_eq!(txes[0].1, digest);

    // Note that this is also considered a tx to the sender, because it mutated
    // one or more of the sender's objects.
    let txes = node.state().get_transactions_to_addr(sender).await?;
    assert_eq!(txes[0].1, digest);

    // No transactions have originated from the receiver
    let txes = node.state().get_transactions_from_addr(receiver).await?;
    assert_eq!(txes.len(), 0);

    Ok(())
}
