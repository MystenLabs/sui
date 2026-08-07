// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! User-facing response projection for local checkpoint dry-runs.
//!
//! Local execution retains normalized transaction data, raw events, and exact pre/post object
//! bodies. This module enriches those values into the response type used by ordinary dry-run
//! formatting without changing the underlying execution or artifact representation.

use super::LocalDryRunExecution;
use anyhow::{Context, Result};
use serde_json::json;
use std::collections::BTreeMap;
use sui_json_rpc_types::{
    BalanceChange, BcsEvent, DryRunTransactionBlockResponse, ObjectChange, SuiEvent,
    SuiTransactionBlockData, SuiTransactionBlockEffects, SuiTransactionBlockEvents,
};
use sui_types::{
    balance_change::derive_balance_changes_2,
    effects::{ObjectRemoveKind, TransactionEffectsAPI},
    event::{Event, EventID},
    full_checkpoint_content::ObjectSet,
    in_memory_storage::InMemoryStorage,
    inner_temporary_store::PackageStoreWithFallback,
    object::Owner,
    storage::ObjectKey,
    supported_protocol_versions::{ProtocolConfig, ProtocolVersion},
    transaction::{TransactionData, TransactionDataAPI},
};

/// Project local execution state into the response shape used by ordinary dry-run output.
///
/// `submitted_transaction` is kept separate from `execution.transaction`: ordinary dry-run shows
/// the transaction supplied by the user, while execution may normalize it by injecting mock gas.
/// Effects and event IDs use the normalized execution digest. Object and balance changes are
/// enriched from the exact object versions retained by local execution. Suggested congestion gas
/// pricing is left unset because it requires target-fullnode state.
pub fn build_local_dry_run_response(
    execution: &LocalDryRunExecution,
    submitted_transaction: TransactionData,
) -> Result<DryRunTransactionBlockResponse> {
    let objects = object_set(execution);
    let object_changes = object_changes(execution, submitted_transaction.sender(), &objects)?;
    let balance_changes = derive_balance_changes_2(&execution.effects, &objects)
        .into_iter()
        .map(|change| BalanceChange {
            owner: Owner::AddressOwner(change.address),
            coin_type: change.coin_type,
            amount: change.amount,
        })
        .collect();
    // Match ordinary dry-run input rendering, which does not resolve Move-call argument layouts.
    let input = SuiTransactionBlockData::try_from_with_module_cache(
        submitted_transaction,
        &InMemoryStorage::default(),
    )?;
    let execution_error_source = execution
        .transaction_result
        .as_ref()
        .err()
        .and_then(|error| error.source().as_ref().map(ToString::to_string));

    Ok(DryRunTransactionBlockResponse {
        effects: SuiTransactionBlockEffects::try_from(execution.effects.clone())?,
        events: events(execution)?,
        object_changes,
        balance_changes,
        input,
        execution_error_source,
        suggested_gas_price: None,
    })
}

/// Merge every object version needed to derive user-facing input and output changes.
///
/// The replay cache contains checkpoint and synthetic objects, while the temporary store contains
/// exact execution inputs and all writes. No individual source is complete on its own.
fn object_set(execution: &LocalDryRunExecution) -> ObjectSet {
    let mut objects = ObjectSet::default();
    for object in execution
        .object_cache
        .values()
        .flat_map(|versions| versions.values())
        .chain(execution.inner_store.input_objects.values())
        .chain(execution.inner_store.written.values())
    {
        objects.insert(object.clone());
    }
    objects
}

/// Convert raw execution events using the package view after this local execution.
///
/// A later PTB `MoveCall` cannot target a package published earlier in the same transaction:
/// package targets are literal IDs resolved before command execution. Publication does, however,
/// stage the package before running its module initializers, and an initializer can emit an event
/// type defined in that package. New-module upgrade initializers can do the same when enabled by
/// the protocol. Transaction-written packages therefore take precedence over checkpoint packages
/// during layout resolution. If resolution fails, the event is still returned with its BCS payload
/// and an empty parsed JSON value.
fn events(execution: &LocalDryRunExecution) -> Result<SuiTransactionBlockEvents> {
    let checkpoint_store = InMemoryStorage::new(
        execution
            .object_cache
            .values()
            .filter_map(|versions| versions.values().next_back().cloned())
            .collect(),
    );
    let protocol_config = ProtocolConfig::get_for_version(
        ProtocolVersion::new(execution.snapshot.epoch.protocol_version),
        execution.snapshot.chain_identifier.chain(),
    );
    let executor = sui_execution::executor(&protocol_config, true)?;
    let package_store = PackageStoreWithFallback::new(&execution.inner_store, checkpoint_store);
    let mut resolver = executor.type_layout_resolver(&protocol_config, Box::new(package_store));
    let transaction_digest = *execution.effects.transaction_digest();

    let data = execution
        .inner_store
        .events
        .data
        .iter()
        .cloned()
        .enumerate()
        .map(|(event_seq, event)| {
            let event_seq = event_seq as u64;
            resolver
                .get_annotated_layout(&event.type_)
                .and_then(|layout| {
                    SuiEvent::try_from(event.clone(), transaction_digest, event_seq, None, layout)
                })
                .unwrap_or_else(|_| raw_event(event, transaction_digest, event_seq))
        })
        .collect();

    Ok(SuiTransactionBlockEvents { data })
}

/// Construct a standard response event from raw BCS when layout resolution or conversion fails.
fn raw_event(
    event: Event,
    transaction_digest: sui_types::digests::TransactionDigest,
    event_seq: u64,
) -> SuiEvent {
    let Event {
        package_id,
        transaction_module,
        sender,
        type_,
        contents,
    } = event;
    SuiEvent {
        id: EventID {
            tx_digest: transaction_digest,
            event_seq,
        },
        package_id,
        transaction_module,
        sender,
        type_,
        parsed_json: json!({}),
        bcs: BcsEvent::new(contents),
        timestamp_ms: None,
    }
}

/// Enrich low-level effects changes with owners, object types, versions, and package modules.
///
/// Output bodies describe created and mutated objects. Removed objects require their pre-state
/// bodies for type information and the effects' tombstone references for their output versions.
fn object_changes(
    execution: &LocalDryRunExecution,
    sender: sui_types::base_types::SuiAddress,
    objects: &ObjectSet,
) -> Result<Vec<ObjectChange>> {
    let removed = execution
        .effects
        .all_removed_objects()
        .into_iter()
        .map(|(object_ref, kind)| (object_ref.0, (object_ref, kind)))
        .collect::<BTreeMap<_, _>>();
    let mut result = Vec::new();

    for change in execution.effects.object_changes() {
        if let (Some(version), Some(digest)) = (change.output_version, change.output_digest) {
            let object = objects
                .get(&ObjectKey(change.id, version))
                .with_context(|| {
                    format!("missing output object {} at version {}", change.id, version)
                })?;
            if object.is_package() {
                let modules = object
                    .data
                    .try_as_package()
                    .context("package output is missing its package data")?
                    .serialized_module_map()
                    .keys()
                    .cloned()
                    .collect();
                result.push(ObjectChange::Published {
                    package_id: change.id,
                    version,
                    digest,
                    modules,
                });
                continue;
            }

            let object_type = object
                .type_()
                .context("non-package output is missing its object type")?
                .clone()
                .into();
            if change.id_operation == sui_types::effects::IDOperation::Created {
                result.push(ObjectChange::Created {
                    sender,
                    owner: object.owner().clone(),
                    object_type,
                    object_id: change.id,
                    version,
                    digest,
                });
            } else {
                // Match ordinary dry-run, which renders every non-created write as a mutation.
                result.push(ObjectChange::Mutated {
                    sender,
                    owner: object.owner().clone(),
                    object_type,
                    object_id: change.id,
                    version,
                    previous_version: change.input_version.unwrap_or_default(),
                    digest,
                });
            }
            continue;
        }

        let Some(input_version) = change.input_version else {
            continue;
        };
        let Some((object_ref, remove_kind)) = removed.get(&change.id) else {
            continue;
        };
        let object_type = objects
            .get(&ObjectKey(change.id, input_version))
            .with_context(|| {
                format!(
                    "missing removed object {} at version {}",
                    change.id, input_version
                )
            })?
            .type_()
            .context("removed object is missing its object type")?
            .clone()
            .into();
        result.push(match remove_kind {
            ObjectRemoveKind::Delete => ObjectChange::Deleted {
                sender,
                object_type,
                object_id: change.id,
                version: object_ref.1,
            },
            ObjectRemoveKind::Wrap => ObjectChange::Wrapped {
                sender,
                object_type,
                object_id: change.id,
                version: object_ref.1,
            },
        });
    }

    Ok(result)
}
