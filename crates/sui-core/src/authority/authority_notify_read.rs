// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use sui_types::base_types::TransactionEffectsDigest;
use sui_types::effects::TransactionEffects;
use sui_types::error::SuiResult;
use sui_types::transaction::TransactionKey;

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
        keys: Vec<TransactionKey>,
    ) -> SuiResult<Vec<TransactionEffects>>;

    async fn notify_read_executed_effects_digests(
        &self,
        keys: Vec<TransactionKey>,
    ) -> SuiResult<Vec<TransactionEffectsDigest>>;

    fn multi_get_executed_effects(
        &self,
        keys: &[TransactionKey],
    ) -> SuiResult<Vec<Option<TransactionEffects>>>;
}
