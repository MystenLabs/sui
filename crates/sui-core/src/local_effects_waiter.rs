// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `sui-core` implementation of [`LocalEffectsWaiter`], backing the node-local "wait for executed
//! effects" RPC. It resolves the moment this node has locally executed a transaction, using the
//! execution cache's notify-read — well before the transaction is checkpointed or indexed.

use async_trait::async_trait;

use sui_types::base_types::TransactionDigest;
use sui_types::error::SuiError;
use sui_types::local_execution::{LocalEffects, LocalEffectsWaiter};
use sui_types::message_envelope::Message;

use crate::authority::AuthorityState;

#[async_trait]
impl LocalEffectsWaiter for AuthorityState {
    async fn wait_for_local_effects(
        &self,
        digest: TransactionDigest,
        include_details: bool,
    ) -> Result<LocalEffects, SuiError> {
        let effects = self
            .get_transaction_cache_reader()
            .notify_read_executed_effects_may_fail("wait_for_local_effects", &[digest])
            .await?
            .pop()
            .expect("notify_read_executed_effects_may_fail returns one entry per requested digest");

        Ok(LocalEffects {
            effects_digest: effects.digest(),
            effects: include_details.then_some(effects),
        })
    }
}
