// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::AuthorityStore;
use async_trait::async_trait;
use either::Either;
use futures::future::join_all;
use futures::stream::FuturesUnordered;
use futures::StreamExt;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use sui_types::base_types::{TransactionDigest, TransactionEffectsDigest};
use sui_types::effects::TransactionEffects;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::error::SuiResult;
use tracing::trace;

#[async_trait]
pub trait EffectsNotifyRead: Send + Sync + 'static {
    /// This method reads executed transaction effects from database.
    /// If effects are not available immediately (i.e. haven't been executed yet),
    /// the method blocks until they are persisted in the database.
    ///
    /// This method **does not** schedule transactions for execution - it is responsibility of the caller
    /// to schedule transactions for execution before calling this method.
    async fn notify_read_executed_effects(
        &self,
        digests: Vec<TransactionDigest>,
    ) -> SuiResult<Vec<TransactionEffects>>;

    async fn notify_read_executed_effects_digests(
        &self,
        digests: Vec<TransactionDigest>,
    ) -> SuiResult<Vec<TransactionEffectsDigest>>;

    fn multi_get_executed_effects(
        &self,
        digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<TransactionEffects>>>;
}

#[async_trait]
impl EffectsNotifyRead for Arc<AuthorityStore> {
    async fn notify_read_executed_effects(
        &self,
        digests: Vec<TransactionDigest>,
    ) -> SuiResult<Vec<TransactionEffects>> {
        let timer = Instant::now();
        // We need to register waiters _before_ reading from the database to avoid race conditions
        let registrations = self
            .executed_effects_notify_read
            .register_all(digests.clone());
        let effects = self.multi_get_executed_effects(&digests)?;
        let mut needs_wait = false;
        let mut results: FuturesUnordered<_> = effects
            .into_iter()
            .zip(registrations.into_iter())
            .map(|(e, r)| match e {
                // Note that Some() clause also drops registration that is already fulfilled
                Some(ready) => Either::Left(futures::future::ready(ready)),
                None => {
                    needs_wait = true;
                    Either::Right(r)
                }
            })
            .collect();
        let mut effects_map = HashMap::new();
        let mut last_finished = None;
        while let Some(finished) = results.next().await {
            last_finished = Some(*finished.transaction_digest());
            effects_map.insert(*finished.transaction_digest(), finished);
        }
        if needs_wait {
            // Only log the duration if we ended up waiting.
            trace!(duration=?timer.elapsed(), ?last_finished, "Finished notify_read_effects");
        }
        // Map from digests to ensures returned order is the same as order of digests
        Ok(digests
            .iter()
            .map(|d| {
                effects_map
                    .remove(d)
                    .expect("Every effect must have been added after each task finishes above")
            })
            .collect())
    }

    async fn notify_read_executed_effects_digests(
        &self,
        digests: Vec<TransactionDigest>,
    ) -> SuiResult<Vec<TransactionEffectsDigest>> {
        // We need to register waiters _before_ reading from the database to avoid race conditions
        let registrations = self
            .executed_effects_digests_notify_read
            .register_all(digests.clone());

        let effects_digests = self.multi_get_executed_effects_digests(&digests)?;

        let results = effects_digests
            .into_iter()
            .zip(registrations.into_iter())
            .map(|(a, r)| match a {
                // Note that Some() clause also drops registration that is already fulfilled
                Some(ready) => Either::Left(futures::future::ready(ready)),
                None => Either::Right(r),
            });

        Ok(join_all(results).await)
    }

    fn multi_get_executed_effects(
        &self,
        digests: &[TransactionDigest],
    ) -> SuiResult<Vec<Option<TransactionEffects>>> {
        AuthorityStore::multi_get_executed_effects(self, digests)
    }
}
