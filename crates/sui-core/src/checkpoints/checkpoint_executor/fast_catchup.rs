// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::AuthorityStore;
use crate::checkpoints::CheckpointStore;
use std::collections::HashMap;
use std::sync::Arc;
use sui_types::base_types::{ObjectID, SequenceNumber};
use sui_types::error::{SuiError, SuiResult};
use sui_types::fp_ensure;
use sui_types::messages::{
    InputObjectKind, TransactionDataAPI, TransactionEffects, TransactionEffectsAPI,
    VerifiedTransaction,
};
use sui_types::messages_checkpoint::{CheckpointContents, CheckpointSequenceNumber};
use sui_types::object::Owner;
use tracing::info;

#[derive(PartialEq, Eq, Clone, Hash)]
struct DepObjectKey {
    object_id: ObjectID,
    version: Option<SequenceNumber>,
    write: bool,
}

pub async fn catch_up(
    checkpoint_store: &Arc<CheckpointStore>,
    authority_store: &Arc<AuthorityStore>,
    starting_checkpoint: CheckpointSequenceNumber,
    ending_checkpoint: CheckpointSequenceNumber,
) -> SuiResult {
    let checkpoint_contents =
        load_checkpoints(checkpoint_store, starting_checkpoint, ending_checkpoint)?;
    let object_dependencies = create_dependency_graph(authority_store, checkpoint_contents)?;
    analyze_dependency_graph(object_dependencies);
    Ok(())
}

fn load_checkpoints(
    checkpoint_store: &Arc<CheckpointStore>,
    starting_checkpoint: CheckpointSequenceNumber,
    ending_checkpoint: CheckpointSequenceNumber,
) -> SuiResult<Vec<CheckpointContents>> {
    info!(
        "Catching up from checkpoint {} to checkpoint {}. Number of checkpoints: {}",
        starting_checkpoint,
        ending_checkpoint,
        ending_checkpoint - starting_checkpoint + 1,
    );
    let checkpoints = checkpoint_store
        .multi_get_checkpoint_by_sequence_number_range(starting_checkpoint, ending_checkpoint)?;
    fp_ensure!(
        checkpoints.len() as u64 + starting_checkpoint - 1 == ending_checkpoint,
        SuiError::from("Missing checkpoints")
    );
    let init_network_total_transactions = if starting_checkpoint == 0 {
        0
    } else {
        checkpoint_store
            .get_checkpoint_by_sequence_number(starting_checkpoint - 1)?
            .unwrap()
            .network_total_transactions
    };
    info!(
        "Total number of transactions: {}",
        checkpoints[checkpoints.len() - 1].network_total_transactions
            - init_network_total_transactions
    );
    let contents: SuiResult<Vec<_>> = checkpoint_store
        .multi_get_checkpoint_contents(checkpoints.iter().map(|c| &c.content_digest))?
        .into_iter()
        .map(|c| c.ok_or(SuiError::from("Missing checkpoint content")))
        .collect();
    info!("Read all checkpoint contents from db");
    contents
}

/// Returns a dependency graph, where each key is a output object of a transaction, and the value is
/// the parent object that has longest dependency chain, and the depth of the dependency chain.
fn create_dependency_graph(
    authority_store: &Arc<AuthorityStore>,
    checkpoint_contents: Vec<CheckpointContents>,
) -> SuiResult<HashMap<DepObjectKey, (Option<DepObjectKey>, u64)>> {
    let mut object_dependencies = HashMap::new();
    for content in checkpoint_contents {
        let exec_digests = content.into_inner();
        let tx_digests: Vec<_> = exec_digests.iter().map(|d| d.transaction).collect();
        let transactions: SuiResult<Vec<_>> = authority_store
            .multi_get_transaction_blocks(&tx_digests)?
            .into_iter()
            .map(|c| c.ok_or(SuiError::from("Missing transaction")))
            .collect();
        let all_effects: SuiResult<Vec<_>> = authority_store
            .multi_get_effects(exec_digests.iter().map(|d| &d.effects))?
            .into_iter()
            .map(|c| c.ok_or(SuiError::from("Missing effects")))
            .collect();
        for (tx, effects) in transactions?.into_iter().zip(all_effects?) {
            let dependencies = get_input_dependencies(&tx, &effects);
            let mut max_parent = None;
            let mut max_depth = 0;
            for dependency in dependencies {
                if let Some((_parent, depth)) = object_dependencies.get(&dependency) {
                    if *depth > max_depth {
                        max_depth = *depth;
                        max_parent = Some(dependency.clone());
                    }
                }
            }
            let cur_depth = max_depth + 1;
            for dependency in get_output_dependencies(&tx, &effects) {
                object_dependencies.insert(dependency, (max_parent.clone(), cur_depth));
            }
        }
    }
    Ok(object_dependencies)
}

fn analyze_dependency_graph(graph: HashMap<DepObjectKey, (Option<DepObjectKey>, u64)>) {
    let mut global_max: Option<(DepObjectKey, u64)> = None;
    for (key, (_, depth)) in graph.iter() {
        if global_max.is_none() || *depth > global_max.as_ref().unwrap().1 {
            global_max = Some((key.clone(), *depth));
        }
    }
    let Some(global_max) = global_max else {
        return;
    };
    info!("Depth of longest critical path: {}", global_max.1);
    let mut dep_stats: HashMap<ObjectID, u64> = HashMap::new();
    let mut cur = global_max;
    loop {
        let (parent, depth) = graph.get(&cur.0).unwrap();
        assert_eq!(depth, &cur.1);
        let Some(parent) = parent else {
            assert_eq!(*depth, 1);
            break;
        };
        let entry = dep_stats.entry(parent.object_id).or_default();
        *entry += 1;
        cur = (parent.clone(), depth - 1);
    }
    info!("Object dependency stats: {:?}", dep_stats);
}

fn get_input_dependencies(
    tx: &VerifiedTransaction,
    effects: &TransactionEffects,
) -> Vec<DepObjectKey> {
    let mut deps = vec![];
    for input in tx.data().transaction_data().input_objects().unwrap() {
        match input {
            InputObjectKind::MovePackage(object_id) => {
                deps.push(DepObjectKey {
                    object_id,
                    version: None,
                    write: true,
                });
            }
            InputObjectKind::ImmOrOwnedMoveObject(obj_ref) => {
                deps.push(DepObjectKey {
                    object_id: obj_ref.0,
                    version: Some(obj_ref.1),
                    write: true,
                });
            }
            InputObjectKind::SharedMoveObject {
                id,
                initial_shared_version: _,
                mutable,
            } => {
                let version = effects
                    .shared_objects()
                    .iter()
                    .find(|obj_ref| obj_ref.0 == id)
                    .unwrap()
                    .1;
                deps.push(DepObjectKey {
                    object_id: id,
                    version: Some(version),
                    write: true,
                });
                if mutable {
                    deps.push(DepObjectKey {
                        object_id: id,
                        version: Some(version),
                        write: false,
                    });
                }
            }
        }
    }
    deps
}

fn get_output_dependencies(
    tx: &VerifiedTransaction,
    effects: &TransactionEffects,
) -> Vec<DepObjectKey> {
    let mut objects = vec![];
    for (obj_ref, owner, _) in effects.all_changed_objects() {
        if owner == &Owner::Immutable {
            objects.extend(vec![
                DepObjectKey {
                    object_id: obj_ref.0,
                    version: None,
                    write: true,
                },
                DepObjectKey {
                    object_id: obj_ref.0,
                    version: Some(obj_ref.1),
                    write: true,
                },
            ]);
        } else {
            objects.push(DepObjectKey {
                object_id: obj_ref.0,
                version: Some(obj_ref.1),
                write: true,
            });
        }
    }
    for input in tx.data().transaction_data().input_objects().unwrap() {
        if let InputObjectKind::SharedMoveObject {
            id,
            initial_shared_version: _,
            mutable: false,
        } = input
        {
            let version = effects
                .shared_objects()
                .iter()
                .find(|obj_ref| obj_ref.0 == id)
                .unwrap()
                .1;
            objects.push(DepObjectKey {
                object_id: id,
                version: Some(version),
                write: false,
            });
        }
    }
    objects
}
