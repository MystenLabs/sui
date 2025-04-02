// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::AuthorityStore;
use std::sync::Arc;
use sui_types::effects::TransactionEffectsAPI;
use typed_store::traits::Map;

// Temporary migration task that can be removed in a release or two once we've removed the old
// events table and are sure we don't need to revert to using the old events table
pub async fn migrate_events(store: Arc<AuthorityStore>) {
    tracing::info!("Starting events table migration");

    let result = tokio::task::spawn_blocking(move || {
        let span = tracing::info_span!(
            "migrate_events",
            transaction = tracing::field::Empty,
            effects = tracing::field::Empty,
        );
        let _entered = span.enter();

        let mut batch = store.perpetual_tables.events_2.batch();

        for entry in store.perpetual_tables.executed_effects.safe_iter() {
            let (txn_digest, effects_digest) = entry?;
            span.record("transaction", tracing::field::debug(&txn_digest));
            span.record("effects", tracing::field::debug(&effects_digest));

            // If there's already an entry for this transaction in the new table we can skip it
            if store.perpetual_tables.events_2.contains_key(&txn_digest)? {
                continue;
            }

            let effects = if let Some(effects) = store.get_effects(&effects_digest)? {
                effects
            } else {
                // Skip this one if we can't find the effects
                continue;
            };

            let events_digest = if let Some(events_digest) = effects.events_digest() {
                events_digest
            } else {
                // There are no events so we can continue to the next entry
                continue;
            };

            let events = if let Some(events) = store.get_events_by_events_digest(events_digest)? {
                // Check that the events we're loading do match the expected events digest for this
                // transaction
                let fetched_events_digest = events.digest();
                if &fetched_events_digest != events_digest {
                    tracing::warn!(
                        expected_events_digest =? events_digest,
                        fetched_events_digest =? fetched_events_digest,
                        "fetched events don't matched expected digest; skipping",
                    );
                    continue;
                }
                events
            } else {
                // Skip this one if we can't find the events. This means they were liked already
                // pruned
                continue;
            };

            batch.insert_batch(&store.perpetual_tables.events_2, [(&txn_digest, &events)])?;

            // If the batch size grows to greater that 128MB then write out to the DB so that the
            // data we need to hold in memory doesn't grown unbounded.
            if batch.size_in_bytes() >= 1 << 27 {
                std::mem::replace(&mut batch, store.perpetual_tables.events_2.batch()).write()?;
            }
        }

        batch.write()?;

        Ok::<_, anyhow::Error>(())
    })
    .await
    .unwrap();

    if let Err(e) = result {
        tracing::warn!("Error encountered while Finished events table migration: {e}");
    }

    tracing::info!("Finished events table migration");
}
