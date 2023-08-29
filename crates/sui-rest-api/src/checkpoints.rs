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

        let input_object_keys = fx
            .input_shared_objects()
            .into_iter()
            .map(|(object_ref, _kind)| ObjectKey::from(object_ref))
            .chain(
                fx.modified_at_versions()
                    .into_iter()
                    .map(|(object_id, version)| ObjectKey(object_id, version)),
            )
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();

        let input_objects = state
            .database
            .multi_get_object_by_key(&input_object_keys)?
            .into_iter()
            .map(|maybe_object| maybe_object.ok_or_else(|| anyhow::anyhow!("missing object")))
            .collect::<Result<Vec<_>>>()?;

        let output_object_keys = fx
            .all_changed_objects()
            .into_iter()
            .map(|(object_ref, _owner, _kind)| ObjectKey::from(object_ref))
            .collect::<Vec<_>>();

        let output_objects = state
            .database
            .multi_get_object_by_key(&output_object_keys)?
            .into_iter()
            .map(|maybe_object| maybe_object.ok_or_else(|| anyhow::anyhow!("missing object")))
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckpointData {
    pub checkpoint_summary: CertifiedCheckpointSummary,
    pub checkpoint_contents: CheckpointContents,
    pub transactions: Vec<CheckpointTransaction>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckpointTransaction {
    pub transaction: Transaction,
    pub effects: TransactionEffects,
    pub events: Option<TransactionEvents>,
    pub input_objects: Vec<Object>,
    pub output_objects: Vec<Object>,
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
