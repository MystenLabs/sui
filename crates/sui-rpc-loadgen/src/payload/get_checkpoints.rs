// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::payload::validation::check_transactions;
use crate::payload::{GetCheckpoints, ProcessPayload, RpcCommandProcessor, SignerInfo};
use anyhow::Result;
use async_trait::async_trait;
use dashmap::DashSet;
use futures::future::join_all;
use itertools::Itertools;
use std::sync::Arc;

use crate::payload::checkpoint_utils::get_latest_checkpoint_stats;
use sui_json_rpc_types::CheckpointId;
use sui_types::base_types::TransactionDigest;
use tokio::sync::Mutex;
use tracing::log::warn;
use tracing::{debug, error, info};

#[async_trait]
impl<'a> ProcessPayload<'a, &'a GetCheckpoints> for RpcCommandProcessor {
    async fn process(
        &'a self,
        op: &'a GetCheckpoints,
        _signer_info: &Option<SignerInfo>,
    ) -> Result<()> {
        let clients = self.get_clients().await?;

        let checkpoint_stats = get_latest_checkpoint_stats(&clients, op.end).await;
        let max_checkpoint = checkpoint_stats.max_latest_checkpoint();
        debug!("GetCheckpoints({}, {:?})", op.start, max_checkpoint,);

        // TODO(chris): read `cross_validate` from config
        let cross_validate = true;

        for seq in op.start..=max_checkpoint {
            let transaction_digests: Arc<Mutex<DashSet<TransactionDigest>>> =
                Arc::new(Mutex::new(DashSet::new()));
            let checkpoints = join_all(clients.iter().enumerate().map(|(i, client)| {
                let transaction_digests = transaction_digests.clone();
                let end_checkpoint_for_clients = checkpoint_stats.latest_checkpoints.clone();
                async move {
                    if end_checkpoint_for_clients[i] < seq {
                        // TODO(chris) log actual url
                        warn!(
                            "The RPC server corresponding to the {i}th url has a outdated checkpoint number {}.\
                            The latest checkpoint number is {seq}",
                            end_checkpoint_for_clients[i]
                        );
                        return None;
                    }

                    match client
                        .read_api()
                        .get_checkpoint(CheckpointId::SequenceNumber(seq))
                        .await {
                        Ok(t) => {
                            if t.sequence_number != seq {
                                error!("The RPC server corresponding to the {i}th url has unexpected checkpoint sequence number {}, expected {seq}", t.sequence_number,);
                            }
                            for digest in t.transactions.iter() {
                                transaction_digests.lock().await.insert(*digest);
                            }
                            Some(t)
                        },
                        Err(err) => {
                            error!("Failed to fetch checkpoint {seq} on the {i}th url: {err}");
                            None
                        }
                    }
                }
            }))
                .await;

            let transaction_digests = transaction_digests
                .lock()
                .await
                .iter()
                .map(|digest| *digest)
                .collect::<Vec<_>>();

            if op.verify_transactions {
                let transaction_responses = check_transactions(
                    &clients,
                    &transaction_digests,
                    cross_validate,
                    op.verify_objects,
                )
                .await
                .into_iter()
                .concat();

                if op.record {
                    debug!("adding addresses and object ids from response");
                    self.add_addresses_from_response(&transaction_responses);
                    self.add_object_ids_from_response(&transaction_responses);
                };
            }

            if op.record {
                debug!("adding transaction digests from response");
                self.add_transaction_digests(transaction_digests);
            };

            if cross_validate {
                let valid_checkpoint = checkpoints.iter().enumerate().find_map(|(i, x)| {
                    if x.is_some() {
                        Some((i, x.clone().unwrap()))
                    } else {
                        None
                    }
                });

                if valid_checkpoint.is_none() {
                    error!("none of the urls are returning valid checkpoint for seq {seq}");
                    continue;
                }
                // safe to unwrap because we check some above
                let (valid_checkpoint_idx, valid_checkpoint) = valid_checkpoint.unwrap();
                for (i, x) in checkpoints.iter().enumerate() {
                    if i == valid_checkpoint_idx {
                        continue;
                    }
                    // ignore the None value because it's warned above
                    let eq = x.is_none() || x.as_ref().unwrap() == &valid_checkpoint;
                    if !eq {
                        error!("getCheckpoint {seq} has a different result between the {valid_checkpoint_idx}th and {i}th URL {:?} {:?}", x, checkpoints[valid_checkpoint_idx])
                    }
                }
            }

            if seq % 10000 == 0 {
                info!("Finished processing checkpoint {seq}");
            }
        }

        Ok(())
    }
}
