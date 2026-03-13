// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::pin::Pin;

use sui_rpc::field::FieldMaskTree;
use sui_rpc::merge::Merge;
use sui_rpc::proto::sui::rpc::v2::Checkpoint;
use sui_rpc::proto::sui::rpc::v2::subscription_service_server::SubscriptionService;
use sui_rpc::proto::sui::rpc::v2::{SubscribeCheckpointsRequest, SubscribeCheckpointsResponse};
use sui_types::balance_change::derive_balance_changes_2;
use tokio_stream::StreamExt as _;
use tokio_stream::wrappers::ReceiverStream;

/// A SubscriptionService implementation backed by the ForkingStore/Simulacrum.
pub struct ForkingSubscriptionService {
    context: crate::context::Context,
}

impl ForkingSubscriptionService {
    pub fn new(context: crate::context::Context) -> Self {
        Self { context }
    }
}

#[tonic::async_trait]
impl SubscriptionService for ForkingSubscriptionService {
    /// Server streaming response type for the SubscribeCheckpoints method.
    type SubscribeCheckpointsStream = Pin<
        Box<
            dyn tokio_stream::Stream<Item = Result<SubscribeCheckpointsResponse, tonic::Status>>
                + Send,
        >,
    >;

    async fn subscribe_checkpoints(
        &self,
        request: tonic::Request<SubscribeCheckpointsRequest>,
    ) -> Result<tonic::Response<Self::SubscribeCheckpointsStream>, tonic::Status> {
        let read_mask = request.into_inner().read_mask.unwrap_or_default();
        let read_mask = FieldMaskTree::from(read_mask);

        let Some(receiver) = self
            .context
            .subscription_service_handle
            .register_subscription()
            .await
        else {
            return Err(tonic::Status::unavailable(
                "too many existing subscriptions",
            ));
        };

        let response = ReceiverStream::new(receiver).map(move |checkpoint| {
            let cursor = checkpoint.summary.sequence_number;
            let mut checkpoint_message = Checkpoint::merge_from(checkpoint.as_ref(), &read_mask);

            if read_mask.contains("transactions.balance_changes") {
                for (txn, effects) in checkpoint_message
                    .transactions_mut()
                    .iter_mut()
                    .zip(checkpoint.transactions.iter().map(|t| &t.effects))
                {
                    *txn.balance_changes_mut() =
                        derive_balance_changes_2(effects, &checkpoint.object_set)
                            .into_iter()
                            .map(Into::into)
                            .collect();
                }
            }

            let mut response = SubscribeCheckpointsResponse::default();
            response.cursor = Some(cursor);
            response.checkpoint = Some(checkpoint_message);
            Ok(response)
        });

        Ok(tonic::Response::new(Box::pin(response)))
    }
}
