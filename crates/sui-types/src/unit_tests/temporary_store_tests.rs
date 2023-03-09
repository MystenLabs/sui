// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::messages::ExecutionFailureStatus::EffectsTooLarge;
use crate::{in_memory_storage::InMemoryStorage, messages::InputObjectKind};

use super::*;

#[test]
fn check_effects_limits_enforced() {
    let mut store = InMemoryStorage::new(Vec::new());
    let protocol_config = ProtocolConfig::get_for_max_version();
    let max_effects_size = protocol_config.max_serialized_tx_effects_size();
    let max_input_objects = protocol_config.max_mutable_inputs();
    let amount_to_deduct = 1_000_000;
    let rebate = 10;

    assert!(max_input_objects > 0);

    let sender = SuiAddress::random_for_testing_only();

    // Create lots of input objects
    let immutable_input_objects: Vec<_> = (0..max_input_objects / 10)
        .map(|_| Object::immutable_with_id_for_testing(ObjectID::random()))
        .collect();
    let mutable_input_objects: Vec<_> = (0..(max_input_objects - 10))
        .map(|r| {
            Object::with_id_owner_version_for_testing(
                ObjectID::random(),
                SequenceNumber::from_u64(r * 10 + r + 7),
                sender,
            )
        })
        .collect();
    let gas_objects: Vec<_> = (0..10)
        .map(|r| {
            Object::with_id_owner_version_for_testing(
                ObjectID::random(),
                SequenceNumber::from_u64(r),
                sender,
            )
        })
        .collect();

    let mut all_input_objects = Vec::new();
    all_input_objects.extend_from_slice(&immutable_input_objects);
    all_input_objects.extend_from_slice(&mutable_input_objects);
    all_input_objects.extend_from_slice(&gas_objects);

    let all_input_objects: Vec<_> = all_input_objects
        .iter()
        .map(|q| {
            (
                InputObjectKind::ImmOrOwnedMoveObject(q.compute_object_reference()),
                q.clone(),
            )
        })
        .collect();

    let mut temporary_store = TemporaryStore::new(
        &mut store,
        InputObjects::new(all_input_objects.clone()),
        TransactionDigest::random(),
        &ProtocolConfig::get_for_max_version(),
    );
    let gas_obj_refs: Vec<_> = gas_objects
        .iter()
        .map(|q| q.compute_object_reference())
        .collect();
    let mut gas_object = gas_objects[0].clone();
    temporary_store.smash_gas(sender, &gas_obj_refs).unwrap();
    temporary_store.ensure_active_inputs_mutated(sender, &gas_obj_refs[0].0);

    gas::deduct_gas(&mut gas_object, amount_to_deduct, rebate);
    temporary_store.write_object(&SingleTxContext::gas(sender), gas_object, WriteKind::Mutate);

    // Perform some writes on the temp store
    mutable_input_objects.iter().for_each(|q| {
        temporary_store.write_object(
            &SingleTxContext::transfer_object(sender),
            q.clone(),
            WriteKind::Mutate,
        )
    });

    let mut objects_to_write = Vec::new();
    (0..(max_input_objects * 300)).for_each(|_| {
        let obj = Object::new_gas_for_testing();
        objects_to_write.push(obj.clone());
        temporary_store.write_object(
            &SingleTxContext::transfer_object(sender),
            obj,
            WriteKind::Mutate,
        )
    });

    // For maximum effects size, assume all are shared
    let dummy_shared_obj_refs = all_input_objects
        .iter()
        .map(|q| q.1.compute_object_reference())
        .collect();

    let (inner_temp_store, effects, status) = temporary_store.to_effects(
        dummy_shared_obj_refs,
        &TransactionDigest::random(),
        all_input_objects
            .iter()
            .map(|q| q.1.previous_transaction)
            .collect(),
        GasCostSummary {
            computation_cost: 0,
            storage_cost: 0,
            storage_rebate: 0,
        },
        ExecutionStatus::Success,
        &gas_obj_refs,
        123,
        true,
        &protocol_config,
        &sender,
    );

    let serialized_size = bcs::serialized_size(&effects).unwrap();

    assert!(serialized_size <= max_effects_size);

    // This must err because effects too large
    let err = status.unwrap_err();
    assert!(matches!(
        err.to_execution_status().0,
        EffectsTooLarge { .. }
    ));

    // Check that the effects contains only the mutable inputs objects with their versions bumped, and the gas objects updated
    #[allow(unreachable_patterns)]
    match effects {
        TransactionEffects::V1(inner_effects) => {
            // Status must be failure
            assert!(matches!(
                inner_effects.status,
                ExecutionStatus::Failure { .. }
            ));

            let mut all_mutable_inputs = mutable_input_objects.clone();
            all_mutable_inputs.extend_from_slice(&gas_objects);

            // `modified_at_versions` vector must be of same length
            assert!(inner_effects.modified_at_versions.len() == all_mutable_inputs.len());

            // modified_at_versions` vector must be same as mutable inputs before mutation
            let mut mod_ver: BTreeMap<_, _> =
                inner_effects.modified_at_versions.iter().copied().collect();
            all_mutable_inputs.iter().for_each(|q| {
                let obj_ref = q.compute_object_reference();
                assert!(mod_ver.remove(&obj_ref.0).unwrap() == obj_ref.1);
            });
            assert!(mod_ver.is_empty());

            // Nothing must be created
            assert!(inner_effects.created.is_empty());

            // All input objects including the primary gas object, but excluding the smashed gas objects must be mutated
            let total_gas_objs = gas_objects.len();

            assert!(inner_effects.mutated.len() == all_mutable_inputs.len() - total_gas_objs + 1);
            assert!(
                inner_temp_store.written.len() == all_mutable_inputs.len() - total_gas_objs + 1
            );

            // Ensure that the mutated objects have the highest sequence number + 1
            let highest_seq = mutable_input_objects
                .last()
                .unwrap()
                .compute_object_reference()
                .1;
            // This is what all the gas was smashed into
            let primary_gas = gas_objects[0].compute_object_reference();
            // These are the gas objects which got smashed into primary_gas
            let smashed_gas = gas_objects[1..].to_vec();

            let smashed_gas_set: BTreeSet<_> = smashed_gas
                .iter()
                .map(|q| q.compute_object_reference().0)
                .collect();

            let mut primary_gas_in_mutated = false;
            inner_effects.mutated.iter().for_each(|q| {
                // Ensure smashed gas not in mutated
                assert!(!smashed_gas_set.contains(&q.0 .0));

                // Ensure Lamport timestamp condition holds
                assert!(u64::from(q.0 .1) == u64::from(highest_seq) + 1);

                if q.0 .0 == primary_gas.0 {
                    // This must not already be set
                    assert!(!primary_gas_in_mutated);
                    primary_gas_in_mutated = true;

                    assert!(inner_temp_store.written.contains_key(&q.0 .0));
                }

                // Ensure the objects in the temp store are proper
                let obj = inner_temp_store.written.get(&q.0 .0).unwrap();
                assert!(obj.0 == q.0);
            });

            // We must've seen it at this point
            assert!(primary_gas_in_mutated);

            // Only smashed gas must be deleted
            assert!(inner_effects.deleted.len() == smashed_gas_set.len());
            assert!(inner_temp_store.deleted.len() == inner_effects.deleted.len());

            inner_effects.deleted.iter().for_each(|q| {
                // Ensure smashed gas in deleted field
                assert!(smashed_gas_set.contains(&q.0));
            });
            inner_temp_store.deleted.iter().for_each(|q| {
                // Ensure smashed gas in deleted field
                assert!(smashed_gas_set.contains(q.0));
            });

            // Nothing must be wrapped or unwrapped or unwrapped_then_deleted
            assert!(inner_effects.wrapped.is_empty());
            assert!(inner_effects.unwrapped.is_empty());
            assert!(inner_effects.unwrapped_then_deleted.is_empty());

            // All events must be coin balance change
            assert!(inner_temp_store.events.digest() == inner_effects.events_digest.unwrap());
            inner_temp_store
                .events
                .data
                .iter()
                .for_each(|q| assert!(*q.balance_change_type().unwrap() == BalanceChangeType::Pay));
        }
        _ => unimplemented!("Test not implemented for effects other than V1"),
    }
}
