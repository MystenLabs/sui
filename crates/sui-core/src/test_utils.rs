// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use crate::{
    authority::AuthorityState,
    authority_aggregator::{AuthAggMetrics, AuthorityAggregator},
    authority_client::{NetworkAuthorityClient, NetworkAuthorityClientMetrics},
    epoch::committee_store::CommitteeStore,
    safe_client::SafeClientMetrics,
};

use sui_config::{NetworkConfig, ValidatorInfo};
use sui_types::{
    base_types::{dbg_addr, ObjectID, TransactionDigest},
    batch::UpdateItem,
    committee::Committee,
    crypto::{get_key_pair, AccountKeyPair, Signature},
    messages::{BatchInfoRequest, BatchInfoResponseItem, Transaction, TransactionData},
    object::Object,
};

use futures::StreamExt;
use tokio::time::sleep;
use tracing::info;

/// Create a test authority aggregator.
/// (duplicated from test-utils/src/authority.rs - that function can't be used
/// in sui-core because of type name conflicts (sui_core::safe_client::SafeClient vs
/// safe_client::SafeClient).
pub fn test_authority_aggregator(
    config: &NetworkConfig,
) -> AuthorityAggregator<NetworkAuthorityClient> {
    let validators_info = config.validator_set();
    let committee = Committee::new(0, ValidatorInfo::voting_rights(validators_info)).unwrap();
    let committee_store = Arc::new(CommitteeStore::new_for_testing(&committee));
    let clients: BTreeMap<_, _> = validators_info
        .iter()
        .map(|config| {
            (
                config.protocol_key(),
                NetworkAuthorityClient::connect_lazy(
                    config.network_address(),
                    Arc::new(NetworkAuthorityClientMetrics::new_for_tests()),
                )
                .unwrap(),
            )
        })
        .collect();
    let registry = prometheus::Registry::new();
    AuthorityAggregator::new(
        committee,
        committee_store,
        clients,
        AuthAggMetrics::new(&registry),
        SafeClientMetrics::new(&registry),
    )
}

pub async fn wait_for_tx(wait_digest: TransactionDigest, state: Arc<AuthorityState>) {
    wait_for_all_txes(vec![wait_digest], state).await
}

pub async fn wait_for_all_txes(wait_digests: Vec<TransactionDigest>, state: Arc<AuthorityState>) {
    let mut wait_digests: HashSet<_> = wait_digests.iter().collect();

    let mut timeout = Box::pin(sleep(Duration::from_millis(15_000)));

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
                        max_seq = Some(batch.data().next_sequence_number);
                        info!(?max_seq, "Received Batch");
                    }
                    // Upon receiving a transaction digest we store it, if it is not processed already.
                    Some(Ok(BatchInfoResponseItem(UpdateItem::Transaction((_seq, digest))))) => {
                        info!(?digest, "Received Transaction");
                        if wait_digests.remove(&digest.transaction) {
                            info!(?digest, "Digest found");
                        }
                        if wait_digests.is_empty() {
                            info!(?digest, "all digests found");
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

// Creates a fake sender-signed transaction for testing. This transaction will
// not actually work.
pub fn create_fake_transaction() -> Transaction {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let recipient = dbg_addr(2);
    let object_id = ObjectID::random();
    let object = Object::immutable_with_id_for_testing(object_id);
    let data = TransactionData::new_transfer_sui(
        recipient,
        sender,
        None,
        object.compute_object_reference(),
        10000,
    );
    let signature = Signature::new(&data, &sender_key);
    Transaction::new(data, signature)
}
