// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use anyhow::Result;
use axum::{
    extract::{Path, State},
    Json, TypedHeader,
};
use sui_types::full_checkpoint_content::{CheckpointData, CheckpointTransaction};
use sui_types::{
    effects::TransactionEffectsAPI,
    messages_checkpoint::{CertifiedCheckpointSummary, CheckpointSequenceNumber},
    storage::ObjectKey,
};

use crate::{headers::Accept, node_state_getter::NodeStateGetter, AppError, Bcs};

pub const GET_LATEST_CHECKPOINT_PATH: &str = "/checkpoints";
pub const GET_CHECKPOINT_PATH: &str = "/checkpoints/:checkpoint";
pub const GET_FULL_CHECKPOINT_PATH: &str = "/checkpoints/:checkpoint/full";

pub async fn get_full_checkpoint(
    //TODO support digest as well as sequence number
    Path(checkpoint_id): Path<CheckpointSequenceNumber>,
    TypedHeader(accept): TypedHeader<Accept>,
    State(state): State<Arc<dyn NodeStateGetter>>,
) -> Result<Bcs<CheckpointData>, AppError> {
    if accept.as_str() != crate::APPLICATION_BCS {
        return Err(AppError(anyhow::anyhow!("invalid accept type")));
    }

    let verified_summary = state.get_verified_checkpoint_by_sequence_number(checkpoint_id)?;
    let checkpoint_contents = state.get_checkpoint_contents(verified_summary.content_digest)?;

    let transaction_digests = checkpoint_contents
        .iter()
        .map(|execution_digests| execution_digests.transaction)
        .collect::<Vec<_>>();

    let transactions = state
        .multi_get_transaction_blocks(&transaction_digests)?
        .into_iter()
        .map(|maybe_transaction| {
            maybe_transaction.ok_or_else(|| anyhow::anyhow!("missing transaction"))
        })
        .collect::<Result<Vec<_>>>()?;

    let effects = state
        .multi_get_executed_effects(&transaction_digests)?
        .into_iter()
        .map(|maybe_effects| maybe_effects.ok_or_else(|| anyhow::anyhow!("missing effects")))
        .collect::<Result<Vec<_>>>()?;

    let event_digests = effects
        .iter()
        .flat_map(|fx| fx.events_digest().copied())
        .collect::<Vec<_>>();

    let events = state
        .multi_get_events(&event_digests)?
        .into_iter()
        .map(|maybe_event| maybe_event.ok_or_else(|| anyhow::anyhow!("missing event")))
        .collect::<Result<Vec<_>>>()?;

    let events = event_digests
        .into_iter()
        .zip(events)
        .collect::<HashMap<_, _>>();

    let mut full_transactions = Vec::with_capacity(transactions.len());
    for (tx, fx) in transactions.into_iter().zip(effects) {
        let events = fx.events_digest().map(|event_digest| {
            events
                .get(event_digest)
                .cloned()
                .expect("event was already checked to be present")
        });
        // Note unwrapped_then_deleted contains **updated** versions.
        let unwrapped_then_deleted_obj_ids = fx
            .unwrapped_then_deleted()
            .into_iter()
            .map(|k| k.0)
            .collect::<HashSet<_>>();

        let input_object_keys = fx
            .input_shared_objects()
            .into_iter()
            .map(|kind| {
                let (id, version) = kind.id_and_version();
                ObjectKey(id, version)
            })
            .chain(
                fx.modified_at_versions()
                    .into_iter()
                    .map(|(object_id, version)| ObjectKey(object_id, version)),
            )
            .collect::<HashSet<_>>()
            .into_iter()
            // Unwrapped-then-deleted objects are not stored in state before the tx, so we have nothing to fetch.
            .filter(|key| !unwrapped_then_deleted_obj_ids.contains(&key.0))
            .collect::<Vec<_>>();

        let input_objects = state
            .multi_get_object_by_key(&input_object_keys)?
            .into_iter()
            .enumerate()
            .map(|(idx, maybe_object)| {
                maybe_object.ok_or_else(|| {
                    anyhow::anyhow!(
                        "missing input object key {:?} from tx {}",
                        input_object_keys[idx],
                        tx.digest()
                    )
                })
            })
            .collect::<Result<Vec<_>>>()?;

        let output_object_keys = fx
            .all_changed_objects()
            .into_iter()
            .map(|(object_ref, _owner, _kind)| ObjectKey::from(object_ref))
            .collect::<Vec<_>>();

        let output_objects = state
            .multi_get_object_by_key(&output_object_keys)?
            .into_iter()
            .enumerate()
            .map(|(idx, maybe_object)| {
                maybe_object.ok_or_else(|| {
                    anyhow::anyhow!(
                        "missing output object key {:?} from tx {}",
                        output_object_keys[idx],
                        tx.digest()
                    )
                })
            })
            .collect::<Result<Vec<_>>>()?;

        let full_transaction = CheckpointTransaction {
            transaction: tx.into(),
            effects: fx,
            events,
            input_objects,
            output_objects,
        };

        full_transactions.push(full_transaction);
    }

    Ok(Bcs(CheckpointData {
        checkpoint_summary: verified_summary.into(),
        checkpoint_contents,
        transactions: full_transactions,
    }))
}

pub async fn get_latest_checkpoint(
    State(state): State<Arc<dyn NodeStateGetter>>,
) -> Result<Json<CertifiedCheckpointSummary>, AppError> {
    let latest_checkpoint_sequence_number = state.get_latest_checkpoint_sequence_number()?;
    let verified_summary =
        state.get_verified_checkpoint_by_sequence_number(latest_checkpoint_sequence_number)?;
    Ok(Json(verified_summary.into()))
}

pub async fn get_checkpoint(
    //TODO support digest as well as sequence number
    Path(checkpoint_id): Path<CheckpointSequenceNumber>,
    State(state): State<Arc<dyn NodeStateGetter>>,
) -> Result<Json<CertifiedCheckpointSummary>, AppError> {
    let verified_summary = state.get_verified_checkpoint_by_sequence_number(checkpoint_id)?;
    Ok(Json(verified_summary.into()))
}
