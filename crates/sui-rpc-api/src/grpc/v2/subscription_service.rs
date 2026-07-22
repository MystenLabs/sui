// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! v2 `SubscriptionService`: filtered, real-time streams of checkpoints,
//! transactions, and events.
//!
//! Each Subscribe API pairs with the LedgerService List API of the same name:
//! requests take the same DNF filter message and responses use identical cursor
//! semantics, so clients can replay subscription gaps with the paired List API.
//!
//! A subscription behaves like an unbounded ascending scan. Every transaction
//! and event frame carries a `watermark`; its payload is optional. Every
//! checkpoint frame carries a scalar `cursor`; its checkpoint payload is
//! optional, and progress-only checkpoint frames occur only on filtered
//! streams. The first frame on a
//! filtered subscription is a progress-only frame establishing the stream's
//! start position. Further progress-only frames are emitted when a stream
//! advances a configured number of checkpoints without a matching item (see
//! `RpcConfig::subscription_watermark_interval`). Streams have no successful
//! end: when the subscription actor drops a subscriber (lag or backpressure),
//! the stream simply closes and the client reconnects, backfilling via List.

use std::time::Instant;

use mysten_common::ZipDebugEqIteratorExt;
use sui_inverted_index::BitmapQuery;
use sui_rpc::field::FieldMaskTree;
use sui_rpc::merge::Merge;
use sui_rpc::proto::sui::rpc::v2::Checkpoint;
use sui_rpc::proto::sui::rpc::v2::Event as ProtoEvent;
use sui_rpc::proto::sui::rpc::v2::EventFilter;
use sui_rpc::proto::sui::rpc::v2::ExecutedTransaction;
use sui_rpc::proto::sui::rpc::v2::ObjectSet as ProtoObjectSet;
use sui_rpc::proto::sui::rpc::v2::SubscribeCheckpointsRequest;
use sui_rpc::proto::sui::rpc::v2::SubscribeCheckpointsResponse;
use sui_rpc::proto::sui::rpc::v2::SubscribeEventsRequest;
use sui_rpc::proto::sui::rpc::v2::SubscribeEventsResponse;
use sui_rpc::proto::sui::rpc::v2::SubscribeTransactionsRequest;
use sui_rpc::proto::sui::rpc::v2::SubscribeTransactionsResponse;
use sui_rpc::proto::sui::rpc::v2::TransactionFilter;
use sui_rpc::proto::sui::rpc::v2::subscription_service_server::SubscriptionService;
use sui_rpc_cursor::Position;
use sui_types::balance_change::derive_balance_changes_2;
use sui_types::effects::TransactionEffectsAPI;
use tokio::sync::mpsc;
use tonic::codegen::BoxStream;

use crate::RpcError;
use crate::RpcService;
use crate::ledger_history::filter::event_filter_to_query;
use crate::ledger_history::filter::transaction_filter_to_query;
use crate::ledger_history::query_options::QueryOptions;
use crate::ledger_history::watermark::advance_covered_bound_before_checkpoint;
use crate::ledger_history::watermark::boundary_watermark;
use crate::ledger_history::watermark::item_watermark;
use crate::ledger_history::watermark::merge_covered_checkpoint_bound;
use crate::metrics::SubscriptionFrameKind;
use crate::metrics::SubscriptionStreamMetrics;
use crate::read_mask_defaults;
use crate::subscription::SubscriptionKind;
use crate::subscription::SubscriptionSpec;
use crate::subscription::SubscriptionUpdate;

#[tonic::async_trait]
impl SubscriptionService for RpcService {
    async fn subscribe_checkpoints(
        &self,
        request: tonic::Request<SubscribeCheckpointsRequest>,
    ) -> Result<tonic::Response<BoxStream<SubscribeCheckpointsResponse>>, tonic::Status> {
        let request = request.into_inner();
        let read_mask = read_mask_defaults::validate_read_mask::<Checkpoint>(
            request.read_mask,
            read_mask_defaults::CHECKPOINT,
        )?;
        let query = compile_transaction_filter(self, request.filter.as_ref())?;
        let (mut receiver, stream_metrics) = register(
            self,
            SubscriptionSpec {
                kind: SubscriptionKind::Checkpoints,
                query,
            },
        )
        .await?;

        let response = Box::pin(async_stream::stream! {
            while let Some(update) = receiver.recv().await {
                let mut response = SubscribeCheckpointsResponse::default();
                let frame_kind = match update {
                    SubscriptionUpdate::Matched(matched) => {
                        let cp = matched.checkpoint.summary.sequence_number;
                        response.cursor = Some(cp);
                        response.checkpoint =
                            Some(render_checkpoint_message(&matched.checkpoint, &read_mask));
                        SubscriptionFrameKind::Payload
                    }
                    SubscriptionUpdate::WatermarkTick { checkpoint: cp, .. } => {
                        response.cursor = Some(cp);
                        SubscriptionFrameKind::Watermark
                    }
                };
                stream_metrics.observe_frame(&response, frame_kind);
                let yielded_at = Instant::now();
                yield Ok(response);
                stream_metrics.observe_yield_wait(yielded_at.elapsed());
            }
        });

        Ok(tonic::Response::new(response))
    }

    async fn subscribe_transactions(
        &self,
        request: tonic::Request<SubscribeTransactionsRequest>,
    ) -> Result<tonic::Response<BoxStream<SubscribeTransactionsResponse>>, tonic::Status> {
        let request = request.into_inner();
        let read_mask = read_mask_defaults::validate_read_mask::<ExecutedTransaction>(
            request.read_mask,
            read_mask_defaults::TRANSACTION,
        )?;
        let query = compile_transaction_filter(self, request.filter.as_ref())?;
        let (mut receiver, stream_metrics) = register(
            self,
            SubscriptionSpec {
                kind: SubscriptionKind::Transactions,
                query,
            },
        )
        .await?;

        let service = self.clone();
        let options = QueryOptions::subscription();
        let response = Box::pin(async_stream::stream! {
            let mut boundary: Option<u64> = None;
            let mut entry_checkpoint = None;
            while let Some(update) = receiver.recv().await {
                match update {
                    SubscriptionUpdate::Matched(matched) => {
                        let checkpoint = &matched.checkpoint;
                        let Some(indices) = matched
                            .matches
                            .transaction_indices(checkpoint.transactions.len() as u32)
                        else {
                            continue;
                        };
                        let cp = checkpoint.summary.sequence_number;
                        let entry_checkpoint = *entry_checkpoint.get_or_insert(cp);
                        let tx_hi = checkpoint.summary.data().network_total_transactions;
                        let tx_lo = tx_hi - checkpoint.transactions.len() as u64;
                        for i in indices {
                            let tx_seq = tx_lo + i as u64;
                            // An item never proves its own checkpoint
                            // complete (list_transactions semantics).
                            boundary = advance_covered_bound_before_checkpoint(
                                boundary,
                                cp,
                                entry_checkpoint,
                                &options,
                            );
                            let mut response = SubscribeTransactionsResponse::default();
                            response.transaction = Some(render_transaction_message(
                                &service,
                                checkpoint,
                                i as usize,
                                &read_mask,
                            ));
                            response.watermark = Some(item_watermark(
                                Position::Transactions {
                                    checkpoint: cp,
                                    tx_seq,
                                },
                                boundary,
                            ));
                            stream_metrics.observe_frame(
                                &response,
                                SubscriptionFrameKind::Payload,
                            );
                            let yielded_at = Instant::now();
                            yield Ok(response);
                            stream_metrics.observe_yield_wait(yielded_at.elapsed());
                        }
                    }
                    SubscriptionUpdate::WatermarkTick { checkpoint: cp, tx_hi } => {
                        // Checkpoint `cp` is fully delivered; the resume cursor lands
                        // on the first transaction of `cp + 1`. The synthetic start
                        // tick establishes only the cursor.
                        let entry_checkpoint =
                            *entry_checkpoint.get_or_insert(cp.saturating_add(1));
                        if cp >= entry_checkpoint {
                            boundary =
                                merge_covered_checkpoint_bound(boundary, cp, &options);
                        }
                        let mut response = SubscribeTransactionsResponse::default();
                        response.watermark = Some(boundary_watermark(
                            Position::Transactions {
                                checkpoint: cp + 1,
                                tx_seq: tx_hi,
                            },
                            boundary,
                        ));
                        stream_metrics.observe_frame(
                            &response,
                            SubscriptionFrameKind::Watermark,
                        );
                        let yielded_at = Instant::now();
                        yield Ok(response);
                        stream_metrics.observe_yield_wait(yielded_at.elapsed());
                    }
                }
            }
        });

        Ok(tonic::Response::new(response))
    }

    async fn subscribe_events(
        &self,
        request: tonic::Request<SubscribeEventsRequest>,
    ) -> Result<tonic::Response<BoxStream<SubscribeEventsResponse>>, tonic::Status> {
        let request = request.into_inner();
        let read_mask = read_mask_defaults::validate_read_mask::<ProtoEvent>(
            request.read_mask,
            read_mask_defaults::EVENT,
        )?;
        let query = compile_event_filter(self, request.filter.as_ref())?;
        let (mut receiver, stream_metrics) = register(
            self,
            SubscriptionSpec {
                kind: SubscriptionKind::Events,
                query,
            },
        )
        .await?;

        let service = self.clone();
        let options = QueryOptions::subscription();
        let response = Box::pin(async_stream::stream! {
            let mut boundary: Option<u64> = None;
            let mut entry_checkpoint = None;
            while let Some(update) = receiver.recv().await {
                match update {
                    SubscriptionUpdate::Matched(matched) => {
                        let checkpoint = &matched.checkpoint;
                        let Some(pairs) = matched.matches.event_indices(checkpoint) else {
                            continue;
                        };
                        let cp = checkpoint.summary.sequence_number;
                        let entry_checkpoint = *entry_checkpoint.get_or_insert(cp);
                        let tx_hi = checkpoint.summary.data().network_total_transactions;
                        let tx_lo = tx_hi - checkpoint.transactions.len() as u64;
                        for (tx_idx, ev) in pairs {
                            let tx = &checkpoint.transactions[tx_idx as usize];
                            let tx_seq = tx_lo + tx_idx as u64;
                            boundary = advance_covered_bound_before_checkpoint(
                                boundary,
                                cp,
                                entry_checkpoint,
                                &options,
                            );
                            let event = &tx
                                .events
                                .as_ref()
                                .expect("matched event implies events")
                                .data[ev as usize];
                            let mut proto_event = service.render_event_to_proto(
                                event,
                                &read_mask,
                                &checkpoint.object_set,
                            );
                            if read_mask.contains(ProtoEvent::CHECKPOINT_FIELD.name) {
                                proto_event.checkpoint = Some(cp);
                            }
                            if read_mask.contains(ProtoEvent::TRANSACTION_DIGEST_FIELD.name) {
                                proto_event.transaction_digest =
                                    Some(tx.effects.transaction_digest().base58_encode());
                            }
                            if read_mask.contains(ProtoEvent::TRANSACTION_INDEX_FIELD.name) {
                                proto_event.transaction_index = Some(tx_idx as u64);
                            }
                            if read_mask.contains(ProtoEvent::EVENT_INDEX_FIELD.name) {
                                proto_event.event_index = Some(ev);
                            }
                            let mut response = SubscribeEventsResponse::default();
                            response.event = Some(proto_event);
                            response.watermark = Some(item_watermark(
                                Position::Events {
                                    checkpoint: cp,
                                    tx_seq,
                                    event_index: ev,
                                },
                                boundary,
                            ));
                            stream_metrics.observe_frame(
                                &response,
                                SubscriptionFrameKind::Payload,
                            );
                            let yielded_at = Instant::now();
                            yield Ok(response);
                            stream_metrics.observe_yield_wait(yielded_at.elapsed());
                        }
                    }
                    SubscriptionUpdate::WatermarkTick { checkpoint: cp, tx_hi } => {
                        let entry_checkpoint =
                            *entry_checkpoint.get_or_insert(cp.saturating_add(1));
                        if cp >= entry_checkpoint {
                            boundary =
                                merge_covered_checkpoint_bound(boundary, cp, &options);
                        }
                        let mut response = SubscribeEventsResponse::default();
                        response.watermark = Some(boundary_watermark(
                            Position::Events {
                                checkpoint: cp + 1,
                                tx_seq: tx_hi,
                                event_index: 0,
                            },
                            boundary,
                        ));
                        stream_metrics.observe_frame(
                            &response,
                            SubscriptionFrameKind::Watermark,
                        );
                        let yielded_at = Instant::now();
                        yield Ok(response);
                        stream_metrics.observe_yield_wait(yielded_at.elapsed());
                    }
                }
            }
        });

        Ok(tonic::Response::new(response))
    }
}

/// Render a full `Checkpoint` message from live executed-checkpoint data,
/// including the `transactions.balance_changes` special case that
/// `merge_from` cannot fill (it needs the checkpoint's `ObjectSet`).
fn render_checkpoint_message(
    checkpoint: &sui_types::full_checkpoint_content::Checkpoint,
    read_mask: &FieldMaskTree,
) -> Checkpoint {
    let mut checkpoint_message = Checkpoint::merge_from(checkpoint, read_mask);

    if read_mask.contains("transactions.balance_changes") {
        for (txn, effects) in checkpoint_message
            .transactions_mut()
            .iter_mut()
            .zip_debug_eq(checkpoint.transactions.iter().map(|t| &t.effects))
        {
            *txn.balance_changes_mut() = derive_balance_changes_2(effects, &checkpoint.object_set)
                .into_iter()
                .map(Into::into)
                .collect();
        }
    }

    checkpoint_message
}

fn transaction_objects<'a>(
    transaction: &'a sui_types::full_checkpoint_content::ExecutedTransaction,
    checkpoint_objects: &'a sui_types::full_checkpoint_content::ObjectSet,
) -> impl Iterator<Item = &'a sui_types::object::Object> + 'a {
    sui_types::storage::get_transaction_object_set(
        &transaction.transaction,
        &transaction.effects,
        &transaction.unchanged_loaded_runtime_objects,
    )
    .into_iter()
    .filter_map(move |key| checkpoint_objects.get(&key))
}

/// Render one `ExecutedTransaction` message from live executed-checkpoint
/// data. The nested-transaction `merge_from` does not set `checkpoint` /
/// `timestamp` (the checkpoint-level merge does), so set them here, along
/// with `balance_changes` which needs the checkpoint's `ObjectSet`.
fn render_transaction_message(
    service: &RpcService,
    checkpoint: &sui_types::full_checkpoint_content::Checkpoint,
    index: usize,
    read_mask: &FieldMaskTree,
) -> ExecutedTransaction {
    let tx = &checkpoint.transactions[index];
    let mut message = ExecutedTransaction::merge_from(tx, read_mask);

    if let Some(object_mask) = read_mask
        .subtree(ExecutedTransaction::OBJECTS_FIELD)
        .and_then(|submask| submask.subtree(ProtoObjectSet::OBJECTS_FIELD))
    {
        message.objects = Some(
            ProtoObjectSet::default().with_objects(
                transaction_objects(tx, &checkpoint.object_set)
                    .map(|object| {
                        service.render_object_to_proto(object, &object_mask, &checkpoint.object_set)
                    })
                    .collect(),
            ),
        );
    }

    if read_mask.contains(ExecutedTransaction::CHECKPOINT_FIELD) {
        message.checkpoint = Some(checkpoint.summary.sequence_number);
    }
    if read_mask.contains(ExecutedTransaction::TIMESTAMP_FIELD) {
        message.timestamp = Some(sui_rpc::proto::timestamp_ms_to_proto(
            checkpoint.summary.timestamp_ms,
        ));
    }
    if read_mask.contains(ExecutedTransaction::BALANCE_CHANGES_FIELD) {
        message.balance_changes = derive_balance_changes_2(&tx.effects, &checkpoint.object_set)
            .into_iter()
            .map(Into::into)
            .collect();
    }
    if read_mask.contains(ExecutedTransaction::TRANSACTION_INDEX_FIELD) {
        message.transaction_index = Some(index as u64);
    }

    message
}

fn compile_transaction_filter(
    service: &RpcService,
    filter: Option<&TransactionFilter>,
) -> Result<Option<BitmapQuery>, RpcError> {
    let max_literals = service.config.ledger_history().max_bitmap_filter_literals();
    filter
        .map(|filter| transaction_filter_to_query(filter, max_literals))
        .transpose()
}

fn compile_event_filter(
    service: &RpcService,
    filter: Option<&EventFilter>,
) -> Result<Option<BitmapQuery>, RpcError> {
    let max_literals = service.config.ledger_history().max_bitmap_filter_literals();
    filter
        .map(|filter| event_filter_to_query(filter, max_literals))
        .transpose()
}

async fn register(
    service: &RpcService,
    spec: SubscriptionSpec,
) -> Result<
    (
        mpsc::Receiver<SubscriptionUpdate>,
        SubscriptionStreamMetrics,
    ),
    tonic::Status,
> {
    let handle = service
        .subscription_service_handle
        .as_ref()
        .ok_or_else(|| tonic::Status::unimplemented("subscription service not enabled"))?;
    let kind = spec.kind;
    let receiver = handle
        .register_subscription(spec)
        .await
        .ok_or_else(|| tonic::Status::unavailable("too many existing subscriptions"))?;
    let stream_metrics = handle.stream_metrics(kind);
    Ok((receiver, stream_metrics))
}

#[cfg(test)]
mod tests {
    use sui_types::test_checkpoint_data_builder::TestCheckpointBuilder;

    use super::*;

    #[test]
    fn transaction_objects_exclude_checkpoint_siblings() {
        let checkpoint = TestCheckpointBuilder::new(1)
            .start_transaction(0)
            .create_owned_object(10)
            .finish_transaction()
            .start_transaction(1)
            .create_owned_object(20)
            .finish_transaction()
            .build_checkpoint();
        let first_object = TestCheckpointBuilder::derive_object_id(10);
        let second_object = TestCheckpointBuilder::derive_object_id(20);

        let first_ids = transaction_objects(&checkpoint.transactions[0], &checkpoint.object_set)
            .map(|object| object.id())
            .collect::<Vec<_>>();
        let second_ids = transaction_objects(&checkpoint.transactions[1], &checkpoint.object_set)
            .map(|object| object.id())
            .collect::<Vec<_>>();

        assert!(first_ids.contains(&first_object));
        assert!(!first_ids.contains(&second_object));
        assert!(second_ids.contains(&second_object));
        assert!(!second_ids.contains(&first_object));
    }
}
