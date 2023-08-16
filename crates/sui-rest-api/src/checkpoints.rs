// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashMap, sync::Arc};

use anyhow::Result;
use axum::{
    extract::{Path, State},
    Json, TypedHeader,
};
use serde::{Deserialize, Serialize};
use sui_core::authority::AuthorityState;
use sui_types::{
    effects::{TransactionEffects, TransactionEffectsAPI, TransactionEvents},
    messages_checkpoint::{
        CertifiedCheckpointSummary, CheckpointContents, CheckpointSequenceNumber,
    },
    object::Object,
    storage::ObjectKey,
    transaction::Transaction,
};

use crate::{headers::Accept, AppError, Bcs};

pub const GET_LATEST_CHECKPOINT_PATH: &str = "/checkpoints";
pub const GET_CHECKPOINT_PATH: &str = "/checkpoints/:checkpoint";
pub const GET_FULL_CHECKPOINT_PATH: &str = "/checkpoints/:checkpoint/full";

pub async fn get_full_checkpoint(
    //TODO support digest as well as sequence number
    Path(checkpoint_id): Path<CheckpointSequenceNumber>,
    TypedHeader(accept): TypedHeader<Accept>,
    State(state): State<Arc<AuthorityState>>,
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
        .database
        .multi_get_transaction_blocks(&transaction_digests)?
        .into_iter()
        .map(|maybe_transaction| {
            maybe_transaction.ok_or_else(|| anyhow::anyhow!("missing transaction"))
        })
        .collect::<Result<Vec<_>>>()?;

    let effects = state
        .database
        .multi_get_executed_effects(&transaction_digests)?
        .into_iter()
        .map(|maybe_effects| maybe_effects.ok_or_else(|| anyhow::anyhow!("missing effects")))
        .collect::<Result<Vec<_>>>()?;

    let event_digests = effects
        .iter()
        .flat_map(|fx| fx.events_digest().copied())
        .collect::<Vec<_>>();

    let events = state
        .database
        .multi_get_events(&event_digests)?
        .into_iter()
        .map(|maybe_event| maybe_event.ok_or_else(|| anyhow::anyhow!("missing event")))
        .collect::<Result<Vec<_>>>()?;

    let mut events = event_digests
        .into_iter()
        .zip(events)
        .collect::<HashMap<_, _>>();

    let object_keys = effects
        .iter()
        .flat_map(|fx| fx.all_changed_objects())
        .map(|(object_ref, _owner, _kind)| ObjectKey::from(object_ref))
        .collect::<Vec<_>>();

    let objects = state
        .database
        .multi_get_object_by_key(&object_keys)?
        .into_iter()
        .map(|maybe_object| maybe_object.ok_or_else(|| anyhow::anyhow!("missing object")))
        .collect::<Result<Vec<_>>>()?;

    let mut transactions_effects_and_events = Vec::with_capacity(transactions.len());
    for (tx, fx) in transactions.into_iter().zip(effects) {
        let events = fx.events_digest().map(|event_digest| {
            events
                .remove(event_digest)
                .expect("event was already checked to be present")
        });

        transactions_effects_and_events.push((tx.into(), fx, events));
    }

    Ok(Bcs(CheckpointData {
        checkpoint_summary: verified_summary.into(),
        checkpoint_contents,
        transactions: transactions_effects_and_events,
        objects,
    }))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckpointData {
    pub checkpoint_summary: CertifiedCheckpointSummary,
    pub checkpoint_contents: CheckpointContents,
    pub transactions: Vec<(Transaction, TransactionEffects, Option<TransactionEvents>)>,
    pub objects: Vec<Object>,
}

pub async fn get_latest_checkpoint(
    State(state): State<Arc<AuthorityState>>,
) -> Result<Json<CertifiedCheckpointSummary>, AppError> {
    let latest_checkpoint_sequence_number = state.get_latest_checkpoint_sequence_number()?;
    let verified_summary =
        state.get_verified_checkpoint_by_sequence_number(latest_checkpoint_sequence_number)?;
    Ok(Json(verified_summary.into()))
}

pub async fn get_checkpoint(
    //TODO support digest as well as sequence number
    Path(checkpoint_id): Path<CheckpointSequenceNumber>,
    State(state): State<Arc<AuthorityState>>,
) -> Result<Json<CertifiedCheckpointSummary>, AppError> {
    let verified_summary = state.get_verified_checkpoint_by_sequence_number(checkpoint_id)?;
    Ok(Json(verified_summary.into()))
}
