// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::identifier::Identifier;
use std::{collections::BTreeSet, path::PathBuf};
use sui_macros::sim_test;
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    base_types::{ObjectID, ObjectRef, SequenceNumber},
    effects::{TransactionEffects, TransactionEffectsAPI},
    execution_status::{ExecutionErrorKind, ExecutionFailure, ExecutionStatus},
    object::Owner,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{ObjectArg, SharedObjectMutability, TransactionData},
};
use test_cluster::addr_balance_test_env::{TestEnv, TestEnvBuilder};

/// Wrapped objects per holder. With the holder's own ID this is `max_num_new_move_object_ids`.
const WRAPPED_OBJECTS: u64 = 2047;
/// Read-only shared inputs. With the gas coin this stays within `max_input_objects`.
const SHARED_INPUTS: u64 = 2000;
/// Mutable shared inputs. Counting them as unchanged consensus objects would push the estimate
/// over the limit, although their effects are far smaller.
const MUTABLE_SHARED_INPUTS: usize = 1500;
/// Room left under the limit by the estimate that ignores ID-only changes and unchanged
/// consensus objects. It is smaller than the bytes those entries add.
const ESTIMATE_MARGIN: usize = 20_000;
const GAS_BUDGET: u64 = 50_000_000_000;

fn move_test_code_path() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests/move_test_code");
    path
}

fn max_effects_size() -> usize {
    ProtocolConfig::get_for_max_version_UNSAFE().max_serialized_tx_effects_size_bytes() as usize
}

async fn setup() -> (TestEnv, ObjectID) {
    let mut test_env = TestEnvBuilder::new()
        .with_proto_override_cb(Box::new(|_, mut cfg| {
            cfg.set_complete_effects_size_estimate_for_testing(true);
            cfg
        }))
        .with_num_validators(1)
        .build()
        .await;
    let pkg = test_env.setup_test_package(move_test_code_path()).await;
    (test_env, pkg)
}

/// Effects entries that the estimate without `complete_effects_size_estimate` ignores
/// or overcounts.
enum Entries<'a> {
    None,
    /// Deletes the objects wrapped in the holder.
    UnwrappedThenDeleted(ObjectRef),
    /// Creates `WRAPPED_OBJECTS` objects and wraps them in a new holder.
    CreatedThenWrapped,
    /// Takes the shared objects as unused inputs.
    SharedInputs(&'a [(ObjectID, SequenceNumber)], SharedObjectMutability),
}

/// Builds a transaction that produces `entries` and sends `num_writes` wide accumulator writes.
async fn execute(
    test_env: &mut TestEnv,
    pkg: ObjectID,
    entries: Entries<'_>,
    num_writes: u64,
) -> TransactionEffects {
    let (sender, gas) = test_env.get_sender_and_gas(0);
    let mut builder = ProgrammableTransactionBuilder::new();
    match entries {
        Entries::None => {}
        Entries::UnwrappedThenDeleted(holder) => {
            let holder = builder.obj(ObjectArg::ImmOrOwnedObject(holder)).unwrap();
            builder.programmable_move_call(
                pkg,
                Identifier::new("effects_size_gaps").unwrap(),
                Identifier::new("delete_items").unwrap(),
                vec![],
                vec![holder],
            );
        }
        Entries::CreatedThenWrapped => {
            let count = builder.pure(WRAPPED_OBJECTS).unwrap();
            builder.programmable_move_call(
                pkg,
                Identifier::new("effects_size_gaps").unwrap(),
                Identifier::new("create_holder").unwrap(),
                vec![],
                vec![count],
            );
        }
        Entries::SharedInputs(shared, mutability) => {
            for (id, initial_shared_version) in shared {
                builder
                    .obj(ObjectArg::SharedObject {
                        id: *id,
                        initial_shared_version: *initial_shared_version,
                        mutability,
                    })
                    .unwrap();
            }
        }
    }
    let count = builder.pure(num_writes).unwrap();
    builder.programmable_move_call(
        pkg,
        Identifier::new("accumulator_spam").unwrap(),
        Identifier::new("send_wide_to_distinct").unwrap(),
        vec![],
        vec![count],
    );
    let tx = TransactionData::new_programmable(
        sender,
        vec![gas],
        builder.finish(),
        GAS_BUDGET,
        test_env.rgp,
    );
    let (_, effects) = test_env
        .exec_tx_directly(tx)
        .await
        .expect("execution must not panic the node");
    effects
}

async fn call(
    test_env: &mut TestEnv,
    pkg: ObjectID,
    function: &str,
    count: u64,
) -> TransactionEffects {
    let (sender, gas) = test_env.get_sender_and_gas(0);
    let mut builder = ProgrammableTransactionBuilder::new();
    let count = builder.pure(count).unwrap();
    builder.programmable_move_call(
        pkg,
        Identifier::new("effects_size_gaps").unwrap(),
        Identifier::new(function).unwrap(),
        vec![],
        vec![count],
    );
    let tx = TransactionData::new_programmable(
        sender,
        vec![gas],
        builder.finish(),
        GAS_BUDGET,
        test_env.rgp,
    );
    let (_, effects) = test_env.exec_tx_directly(tx).await.unwrap();
    assert!(effects.status().is_ok(), "{:?}", effects.status());
    effects
}

async fn create_holder(test_env: &mut TestEnv, pkg: ObjectID) -> ObjectRef {
    let effects = call(test_env, pkg, "create_holder", WRAPPED_OBJECTS).await;
    let created = effects.created();
    assert_eq!(
        created.len(),
        1,
        "wrapped objects must not appear as created"
    );
    created[0].0
}

async fn create_shared(test_env: &mut TestEnv, pkg: ObjectID) -> Vec<(ObjectID, SequenceNumber)> {
    call(test_env, pkg, "create_markers", SHARED_INPUTS)
        .await
        .created()
        .into_iter()
        .map(|((id, _, _), owner)| match owner {
            Owner::Shared {
                initial_shared_version,
            } => (id, initial_shared_version),
            owner => panic!("unexpected owner {owner:?}"),
        })
        .collect()
}

/// The effects size estimate of one wide accumulator write.
async fn wide_write_estimate(test_env: &mut TestEnv, pkg: ObjectID) -> usize {
    let effects = execute(test_env, pkg, Entries::None, 1).await;
    assert!(effects.status().is_ok(), "{:?}", effects.status());
    let events = effects.accumulator_events();
    assert_eq!(events.len(), 1);
    TransactionEffects::estimate_accumulator_write_size_upperbound(&events[0].write.address.ty)
}

/// The number of wide writes that leaves `ESTIMATE_MARGIN` under the limit once each input
/// object is counted as a dependency.
fn writes_to_fill(write_estimate: usize, num_inputs: usize) -> u64 {
    ((max_effects_size() - 40 * num_inputs - ESTIMATE_MARGIN) / write_estimate) as u64
}

fn assert_effects_too_large(effects: &TransactionEffects) {
    assert!(
        matches!(
            effects.status(),
            ExecutionStatus::Failure(ExecutionFailure {
                error: ExecutionErrorKind::EffectsTooLarge { .. },
                ..
            })
        ),
        "expected EffectsTooLarge, got {:?} with effects size {}",
        effects.status(),
        bcs::serialized_size(effects).unwrap()
    );
}

fn assert_within_limit(effects: &TransactionEffects) {
    let size = bcs::serialized_size(effects).unwrap();
    assert!(
        size <= max_effects_size(),
        "effects size {size} exceeds limit {}",
        max_effects_size()
    );
}

/// Unwrapped-then-deleted objects produce ID-only effects entries.
#[sim_test]
async fn test_unwrapped_then_deleted_count_toward_effects_size_limit() {
    let (mut test_env, pkg) = setup().await;
    let write_estimate = wide_write_estimate(&mut test_env, pkg).await;
    let holder = create_holder(&mut test_env, pkg).await;

    let effects = execute(
        &mut test_env,
        pkg,
        Entries::UnwrappedThenDeleted(holder),
        writes_to_fill(write_estimate, 2),
    )
    .await;
    assert_effects_too_large(&effects);
    assert_within_limit(&effects);

    // The failed transaction reverted the deletions, so the holder still wraps every item.
    let holder = effects
        .mutated()
        .into_iter()
        .find(|(obj_ref, _)| obj_ref.0 == holder.0)
        .unwrap()
        .0;
    let effects = execute(&mut test_env, pkg, Entries::UnwrappedThenDeleted(holder), 1).await;
    assert!(effects.status().is_ok(), "{:?}", effects.status());
    assert_eq!(
        effects.unwrapped_then_deleted().len(),
        WRAPPED_OBJECTS as usize
    );
    assert_within_limit(&effects);
}

/// Read-only shared inputs produce `unchanged_consensus_objects` entries.
#[sim_test]
async fn test_unchanged_consensus_objects_count_toward_effects_size_limit() {
    let (mut test_env, pkg) = setup().await;
    let write_estimate = wide_write_estimate(&mut test_env, pkg).await;
    let shared = create_shared(&mut test_env, pkg).await;

    let effects = execute(
        &mut test_env,
        pkg,
        Entries::SharedInputs(&shared, SharedObjectMutability::Immutable),
        writes_to_fill(write_estimate, shared.len() + 1),
    )
    .await;
    assert_effects_too_large(&effects);
    assert_within_limit(&effects);

    let effects = execute(
        &mut test_env,
        pkg,
        Entries::SharedInputs(&shared, SharedObjectMutability::Immutable),
        1,
    )
    .await;
    assert!(effects.status().is_ok(), "{:?}", effects.status());
    let unchanged: BTreeSet<_> = effects
        .unchanged_consensus_objects()
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    assert!(shared.iter().all(|(id, _)| unchanged.contains(id)));
    assert_within_limit(&effects);
}

/// Objects created then wrapped produce ID-only effects entries.
#[sim_test]
async fn test_created_then_wrapped_count_toward_effects_size_limit() {
    let (mut test_env, pkg) = setup().await;
    let write_estimate = wide_write_estimate(&mut test_env, pkg).await;

    let effects = execute(
        &mut test_env,
        pkg,
        Entries::CreatedThenWrapped,
        writes_to_fill(write_estimate, 1),
    )
    .await;
    assert_effects_too_large(&effects);
    assert_within_limit(&effects);

    let effects = execute(&mut test_env, pkg, Entries::CreatedThenWrapped, 1).await;
    assert!(effects.status().is_ok(), "{:?}", effects.status());
    assert_eq!(effects.created().len(), 1);
    assert_within_limit(&effects);
}

/// Mutable shared inputs are written objects, so they do not count as unchanged consensus
/// objects.
#[sim_test]
async fn test_mutable_shared_inputs_do_not_count_as_unchanged_consensus_objects() {
    let (mut test_env, pkg) = setup().await;
    let shared = create_shared(&mut test_env, pkg).await;

    let effects = execute(
        &mut test_env,
        pkg,
        Entries::SharedInputs(
            &shared[..MUTABLE_SHARED_INPUTS],
            SharedObjectMutability::Mutable,
        ),
        1,
    )
    .await;
    assert!(effects.status().is_ok(), "{:?}", effects.status());
    assert_within_limit(&effects);
}
