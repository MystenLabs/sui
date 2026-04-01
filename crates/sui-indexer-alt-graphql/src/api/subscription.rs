// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::Context;

use crate::api::types::checkpoint::Checkpoint;
use crate::error::RpcError;

#[derive(Default)]
pub struct Subscription;

#[async_graphql::Subscription]
impl Subscription {
    /// Subscribe to checkpoints as they are finalized, starting from the current tip.
    ///
    /// This subscription is not yet available for use.
    async fn checkpoints(
        &self,
        _ctx: &Context<'_>,
    ) -> Result<impl futures::Stream<Item = Result<Checkpoint, RpcError>>, RpcError> {
        Err::<futures::stream::Empty<_>, _>(RpcError::FeatureUnavailable {
            what: "Checkpoint Subscriptions",
        })
    }
}
