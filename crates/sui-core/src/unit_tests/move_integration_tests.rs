// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::{
    authority::authority_tests::{
        call_move, call_move_with_shared, init_state_with_ids, send_and_confirm_transaction,
        TestCallArg,
    },
    test_utils::to_sender_signed_transaction,
};

use move_core_types::{
    language_storage::TypeTag,
    value::{MoveStruct, MoveValue},
};
use sui_framework_build::compiled_package::BuildConfig;
use sui_types::{
    crypto::{get_key_pair, AccountKeyPair},
    error::SuiError,
    event::{Event, EventType},
    messages::ExecutionStatus,
    object::OBJECT_START_VERSION,
};

use std::path::PathBuf;
use std::{env, str::FromStr};

const MAX_GAS: u64 = 10000;

fn run_tokio_test_with_big_stack<F>(fut: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_time()
        .thread_stack_size(128 * 1024 * 1024)
        .build()
        .unwrap();
    runtime.block_on(async move {
        // spawn future on a worker thread with a big stack
        let handle = tokio::task::spawn(fut);
        handle.await.unwrap()
    })
}

#[test]
#[cfg_attr(msim, ignore)]
fn test_object_wrapping_unwrapping() {
    run_tokio_test_with_big_stack(async move {
        let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
        let gas = ObjectID::random();
        let authority = init_state_with_ids(vec![(sender, gas)]).await;

        let package = build_and_publish_test_package(
            &authority,
            &sender,
            &sender_key,
            &gas,
            "object_wrapping",
        )
        .await;

        // Create a Child object.
        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "object_wrapping",
            "create_child",
            vec![],
            vec![],
        )
        .await
        .unwrap();
        assert!(
            matches!(effects.status, ExecutionStatus::Success { .. }),
            "{:?}",
            effects.status
        );
        let child_object_ref = effects.created[0].0;
        assert_eq!(child_object_ref.1, OBJECT_START_VERSION);

        // Create a Parent object, by wrapping the child object.
        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "object_wrapping",
            "create_parent",
            vec![],
            vec![TestCallArg::Object(child_object_ref.0)],
        )
        .await
        .unwrap();
        assert!(
            matches!(effects.status, ExecutionStatus::Success { .. }),
            "{:?}",
            effects.status
        );
        // Child object is wrapped, Parent object is created.
        assert_eq!(
            (
                effects.created.len(),
                effects.deleted.len(),
                effects.wrapped.len()
            ),
            (1, 0, 1)
        );
        let new_child_object_ref = effects.wrapped[0];
        let expected_child_object_ref = (
            child_object_ref.0,
            child_object_ref.1.increment(),
            ObjectDigest::OBJECT_DIGEST_WRAPPED,
        );
        // Make sure that the child's version gets increased after wrapped.
        assert_eq!(new_child_object_ref, expected_child_object_ref);
        check_latest_object_ref(&authority, &expected_child_object_ref).await;
        let child_object_ref = new_child_object_ref;

        let parent_object_ref = effects.created[0].0;
        assert_eq!(parent_object_ref.1, OBJECT_START_VERSION);

        // Extract the child out of the parent.
        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "object_wrapping",
            "extract_child",
            vec![],
            vec![TestCallArg::Object(parent_object_ref.0)],
        )
        .await
        .unwrap();
        assert!(
            matches!(effects.status, ExecutionStatus::Success { .. }),
            "{:?}",
            effects.status
        );
        // Check that the child shows up in unwrapped, not created.
        // mutated contains parent and gas.
        assert_eq!(
            (
                effects.mutated.len(),
                effects.created.len(),
                effects.unwrapped.len()
            ),
            (2, 0, 1)
        );
        // Make sure that version increments again when unwrapped.
        assert_eq!(effects.unwrapped[0].0 .1, child_object_ref.1.increment());
        check_latest_object_ref(&authority, &effects.unwrapped[0].0).await;
        let child_object_ref = effects.unwrapped[0].0;

        // Wrap the child to the parent again.
        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "object_wrapping",
            "set_child",
            vec![],
            vec![
                TestCallArg::Object(parent_object_ref.0),
                TestCallArg::Object(child_object_ref.0),
            ],
        )
        .await
        .unwrap();
        assert!(
            matches!(effects.status, ExecutionStatus::Success { .. }),
            "{:?}",
            effects.status
        );
        // Check that child object showed up in wrapped.
        // mutated contains parent and gas.
        assert_eq!((effects.mutated.len(), effects.wrapped.len()), (2, 1));
        let expected_child_object_ref = (
            child_object_ref.0,
            child_object_ref.1.increment(),
            ObjectDigest::OBJECT_DIGEST_WRAPPED,
        );
        assert_eq!(effects.wrapped[0], expected_child_object_ref);
        check_latest_object_ref(&authority, &expected_child_object_ref).await;
        let child_object_ref = effects.wrapped[0];
        let parent_object_ref = effects.mutated_excluding_gas().next().unwrap().0;

        // Now delete the parent object, which will in turn delete the child object.
        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "object_wrapping",
            "delete_parent",
            vec![],
            vec![TestCallArg::Object(parent_object_ref.0)],
        )
        .await
        .unwrap();
        assert!(
            matches!(effects.status, ExecutionStatus::Success { .. }),
            "{:?}",
            effects.status
        );
        assert_eq!(effects.deleted.len(), 2);
        // Check that both objects are marked as wrapped in the authority.
        let expected_child_object_ref = (
            child_object_ref.0,
            child_object_ref.1.increment(),
            ObjectDigest::OBJECT_DIGEST_DELETED,
        );
        assert!(effects.deleted.contains(&expected_child_object_ref));
        check_latest_object_ref(&authority, &expected_child_object_ref).await;
        let expected_parent_object_ref = (
            parent_object_ref.0,
            parent_object_ref.1.increment(),
            ObjectDigest::OBJECT_DIGEST_DELETED,
        );
        assert!(effects.deleted.contains(&expected_parent_object_ref));
        check_latest_object_ref(&authority, &expected_parent_object_ref).await;
    })
}

#[test]
#[cfg_attr(msim, ignore)]
fn test_object_owning_another_object() {
    run_tokio_test_with_big_stack(async move {
        let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
        let gas = ObjectID::random();
        let authority = init_state_with_ids(vec![(sender, gas)]).await;

        let package =
            build_and_publish_test_package(&authority, &sender, &sender_key, &gas, "object_owner")
                .await;

        // Create a parent.
        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "object_owner",
            "create_parent",
            vec![],
            vec![],
        )
        .await
        .unwrap();
        assert!(effects.status.is_ok());
        assert_eq!(effects.events.len(), 2);
        assert_eq!(effects.events[0].event_type(), EventType::CoinBalanceChange);
        assert_eq!(effects.events[1].event_type(), EventType::NewObject);
        let parent = effects.created[0].0;
        assert_eq!(effects.events[1].object_id(), Some(parent.0));

        // Create a child.
        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "object_owner",
            "create_child",
            vec![],
            vec![],
        )
        .await
        .unwrap();
        assert!(effects.status.is_ok());
        assert_eq!(effects.events.len(), 2);
        assert_eq!(effects.events[0].event_type(), EventType::CoinBalanceChange);
        assert_eq!(effects.events[1].event_type(), EventType::NewObject);
        let child = effects.created[0].0;

        // Mutate the child directly should work fine.
        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "object_owner",
            "mutate_child",
            vec![],
            vec![TestCallArg::Object(child.0)],
        )
        .await
        .unwrap();
        assert!(effects.status.is_ok());

        // Add the child to the parent.
        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "object_owner",
            "add_child",
            vec![],
            vec![TestCallArg::Object(parent.0), TestCallArg::Object(child.0)],
        )
        .await
        .unwrap();
        effects.status.unwrap();
        let child_effect = effects
            .mutated
            .iter()
            .find(|((id, _, _), _)| id == &child.0)
            .unwrap();
        // Check that the child is now owned by the parent.
        let field_id = match child_effect.1 {
            Owner::ObjectOwner(field_id) => field_id.into(),
            Owner::Shared { .. } | Owner::Immutable | Owner::AddressOwner(_) => panic!(),
        };
        let field_object = authority.get_object(&field_id).await.unwrap().unwrap();
        assert_eq!(field_object.owner, parent.0);

        // Mutate the child directly will now fail because we need the parent to authenticate.
        let result = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "object_owner",
            "mutate_child",
            vec![],
            vec![TestCallArg::Object(child.0)],
        )
        .await;
        assert!(result.is_err());

        // Mutate the child with the parent will not succeed.
        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "object_owner",
            "mutate_child_with_parent",
            vec![],
            vec![TestCallArg::Object(child.0), TestCallArg::Object(parent.0)],
        )
        .await;
        assert!(effects.is_err());
        assert!(format!("{effects:?}")
            .contains("TransactionInputObjectsErrors { errors: [InvalidChildObjectArgument"));

        // Create another parent.
        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "object_owner",
            "create_parent",
            vec![],
            vec![],
        )
        .await
        .unwrap();
        assert!(effects.status.is_ok());
        assert_eq!(effects.events.len(), 2);
        assert_eq!(effects.events[0].event_type(), EventType::CoinBalanceChange);
        assert_eq!(effects.events[1].event_type(), EventType::NewObject);
        let new_parent = effects.created[0].0;

        // Transfer the child to the new_parent.
        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "object_owner",
            "transfer_child",
            vec![],
            vec![
                TestCallArg::Object(parent.0),
                TestCallArg::Object(new_parent.0),
            ],
        )
        .await
        .unwrap();
        assert!(effects.status.is_ok());
        assert_eq!(effects.events.len(), 6);
        let num_transfers = effects
            .events
            .iter()
            .filter(|e| matches!(e.event_type(), EventType::TransferObject { .. }))
            .count();
        assert_eq!(num_transfers, 2);
        let num_created = effects
            .events
            .iter()
            .filter(|e| matches!(e.event_type(), EventType::NewObject { .. }))
            .count();
        assert_eq!(num_created, 1);
        let child_event = effects
            .events
            .iter()
            .find(|e| e.object_id() == Some(child.0))
            .unwrap();
        let (recipient, object_type) = match child_event {
            Event::TransferObject {
                recipient,
                object_type,
                ..
            } => (recipient, object_type),
            _ => panic!("Unexpected event type: {:?}", child_event),
        };
        assert_eq!(object_type, &format!("{}::object_owner::Child", package.0));
        let new_field_id = match recipient {
            Owner::ObjectOwner(field_id) => (*field_id).into(),
            Owner::Shared { .. } | Owner::Immutable | Owner::AddressOwner(_) => panic!(),
        };
        let new_field_object = authority.get_object(&new_field_id).await.unwrap().unwrap();
        assert_eq!(
            new_field_object.owner,
            Owner::ObjectOwner(new_parent.0.into())
        );

        let child_effect = effects
            .mutated
            .iter()
            .find(|((id, _, _), _)| id == &child.0)
            .unwrap();
        // Check that the child is now owned by the new parent.
        assert_eq!(child_effect.1, new_field_id);

        // Delete the child. This should fail as the child cannot be used as a transaction argument
        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "object_owner",
            "delete_child",
            vec![],
            vec![TestCallArg::Object(child.0)],
        )
        .await;
        assert!(effects.is_err());
    })
}

#[test]
#[cfg_attr(msim, ignore)]
fn test_create_then_delete_parent_child() {
    run_tokio_test_with_big_stack(async move {
        let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
        let gas = ObjectID::random();
        let authority = init_state_with_ids(vec![(sender, gas)]).await;

        let package =
            build_and_publish_test_package(&authority, &sender, &sender_key, &gas, "object_owner")
                .await;

        // Create a parent and a child together
        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "object_owner",
            "create_parent_and_child",
            vec![],
            vec![],
        )
        .await
        .unwrap();
        assert!(effects.status.is_ok());
        // Creates 3 objects, the parent, a field, and the child
        assert_eq!(effects.created.len(), 3);
        // Creates 4 events, gas charge, child, parent and wrapped object
        assert_eq!(effects.events.len(), 4);
        let parent = effects
            .created
            .iter()
            .find(|(_, owner)| matches!(owner, Owner::AddressOwner(_)))
            .unwrap()
            .0;

        // Delete the parent and child altogether.
        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "object_owner",
            "delete_parent_and_child",
            vec![],
            vec![TestCallArg::Object(parent.0)],
        )
        .await
        .unwrap();
        assert!(effects.status.is_ok());
        // Check that both objects were deleted.
        // TODO field object should be deleted too
        assert_eq!(effects.deleted.len(), 2);
        assert_eq!(effects.events.len(), 4);
    })
}

#[test]
#[cfg_attr(msim, ignore)]
fn test_create_then_delete_parent_child_wrap() {
    run_tokio_test_with_big_stack(async move {
        let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
        let gas = ObjectID::random();
        let authority = init_state_with_ids(vec![(sender, gas)]).await;

        let package =
            build_and_publish_test_package(&authority, &sender, &sender_key, &gas, "object_owner")
                .await;

        // Create a parent and a child together
        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "object_owner",
            "create_parent_and_child_wrapped",
            vec![],
            vec![],
        )
        .await
        .unwrap();
        assert!(effects.status.is_ok());
        // Creates 3 objects, the parent, a field, and the child
        assert_eq!(effects.created.len(), 2);
        // not wrapped as it wasn't first created
        assert_eq!(effects.wrapped.len(), 0);
        assert_eq!(effects.events.len(), 3);

        let parent = effects
            .created
            .iter()
            .find(|(_, owner)| matches!(owner, Owner::AddressOwner(_)))
            .unwrap()
            .0;

        // Delete the parent and child altogether.
        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "object_owner",
            "delete_parent_and_child_wrapped",
            vec![],
            vec![TestCallArg::Object(parent.0)],
        )
        .await
        .unwrap();
        assert!(effects.status.is_ok());
        // Check that both objects were deleted.
        // TODO field object should be deleted too
        assert_eq!(effects.deleted.len(), 2);
        assert_eq!(effects.events.len(), 4);
    })
}

#[test]
#[cfg_attr(msim, ignore)]
fn test_create_then_delete_parent_child_wrap_separate() {
    run_tokio_test_with_big_stack(async move {
        let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
        let gas = ObjectID::random();
        let authority = init_state_with_ids(vec![(sender, gas)]).await;

        let package =
            build_and_publish_test_package(&authority, &sender, &sender_key, &gas, "object_owner")
                .await;

        // Create a parent.
        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "object_owner",
            "create_parent",
            vec![],
            vec![],
        )
        .await
        .unwrap();
        assert!(effects.status.is_ok());
        assert_eq!(effects.events.len(), 2);
        assert_eq!(effects.events[0].event_type(), EventType::CoinBalanceChange);
        assert_eq!(effects.events[1].event_type(), EventType::NewObject);
        let parent = effects.created[0].0;
        assert_eq!(effects.events[1].object_id(), Some(parent.0));

        // Create a child.
        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "object_owner",
            "create_child",
            vec![],
            vec![],
        )
        .await
        .unwrap();
        assert!(effects.status.is_ok());
        assert_eq!(effects.events.len(), 2);
        assert_eq!(effects.events[0].event_type(), EventType::CoinBalanceChange);
        assert_eq!(effects.events[1].event_type(), EventType::NewObject);
        let child = effects.created[0].0;

        // Add the child to the parent.
        println!("add_child_wrapped");
        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "object_owner",
            "add_child_wrapped",
            vec![],
            vec![TestCallArg::Object(parent.0), TestCallArg::Object(child.0)],
        )
        .await
        .unwrap();
        assert!(effects.status.is_ok());
        assert_eq!(effects.created.len(), 1);
        assert_eq!(effects.wrapped.len(), 1);
        assert_eq!(effects.events.len(), 4);

        // Delete the parent and child altogether.
        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "object_owner",
            "delete_parent_and_child_wrapped",
            vec![],
            vec![TestCallArg::Object(parent.0)],
        )
        .await
        .unwrap();
        assert!(effects.status.is_ok());
        // Check that both objects were deleted.
        // TODO field object should be deleted too
        assert_eq!(effects.deleted.len(), 2);
        assert_eq!(effects.events.len(), 4);
    })
}

#[test]
#[cfg_attr(msim, ignore)]
fn test_entry_point_vector_empty() {
    run_tokio_test_with_big_stack(async move {
        let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
        let gas = ObjectID::random();
        let authority = init_state_with_ids(vec![(sender, gas)]).await;

        let package = build_and_publish_test_package(
            &authority,
            &sender,
            &sender_key,
            &gas,
            "entry_point_vector",
        )
        .await;

        // call a function with an empty vector
        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "entry_point_vector",
            "obj_vec_empty",
            vec![],
            vec![TestCallArg::ObjVec(vec![])],
        )
        .await
        .unwrap();
        assert!(
            matches!(effects.status, ExecutionStatus::Success { .. }),
            "{:?}",
            effects.status
        );

        // call a function with an empty vector whose type is generic
        let type_tag =
            TypeTag::from_str(format!("{}::entry_point_vector::Obj", package.0).as_str()).unwrap();
        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "entry_point_vector",
            "type_param_vec_empty",
            vec![type_tag],
            vec![TestCallArg::ObjVec(vec![])],
        )
        .await
        .unwrap();
        assert!(
            matches!(effects.status, ExecutionStatus::Success { .. }),
            "{:?}",
            effects.status
        );
    })
}

#[test]
#[cfg_attr(msim, ignore)]
fn test_entry_point_vector_primitive() {
    run_tokio_test_with_big_stack(async move {
        let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
        let gas = ObjectID::random();
        let authority = init_state_with_ids(vec![(sender, gas)]).await;

        let package = build_and_publish_test_package(
            &authority,
            &sender,
            &sender_key,
            &gas,
            "entry_point_vector",
        )
        .await;

        // just a test call with vector of 2 primitive values and check its length in the entry function
        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "entry_point_vector",
            "prim_vec_len",
            vec![],
            vec![TestCallArg::Pure(
                bcs::to_bytes(&vec![7_u64, 42_u64]).unwrap(),
            )],
        )
        .await
        .unwrap();
        assert!(
            matches!(effects.status, ExecutionStatus::Success { .. }),
            "{:?}",
            effects.status
        );
    })
}

#[test]
#[cfg_attr(msim, ignore)]
fn test_entry_point_vector() {
    run_tokio_test_with_big_stack(async move {
        let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
        let gas = ObjectID::random();
        let authority = init_state_with_ids(vec![(sender, gas)]).await;

        let package = build_and_publish_test_package(
            &authority,
            &sender,
            &sender_key,
            &gas,
            "entry_point_vector",
        )
        .await;

        // mint an owned object
        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "entry_point_vector",
            "mint",
            vec![],
            vec![TestCallArg::Pure(bcs::to_bytes(&(42_u64)).unwrap())],
        )
        .await
        .unwrap();
        assert!(
            matches!(effects.status, ExecutionStatus::Success { .. }),
            "{:?}",
            effects.status
        );
        let (obj_id, _, _) = effects.created[0].0;
        // call a function with a vector containing one owned object
        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "entry_point_vector",
            "obj_vec_destroy",
            vec![],
            vec![TestCallArg::ObjVec(vec![obj_id])],
        )
        .await
        .unwrap();
        assert!(
            matches!(effects.status, ExecutionStatus::Success { .. }),
            "{:?}",
            effects.status
        );

        // mint a parent object and a child object and make sure that parent stored in the vector
        // authenticates the child passed by-value
        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "entry_point_vector",
            "mint",
            vec![],
            vec![TestCallArg::Pure(bcs::to_bytes(&(42_u64)).unwrap())],
        )
        .await
        .unwrap();
        assert!(
            matches!(effects.status, ExecutionStatus::Success { .. }),
            "{:?}",
            effects.status
        );
        let (parent_id, _, _) = effects.created[0].0;
        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "entry_point_vector",
            "mint_child",
            vec![],
            vec![
                TestCallArg::Pure(bcs::to_bytes(&(42_u64)).unwrap()),
                TestCallArg::Object(parent_id),
            ],
        )
        .await
        .unwrap();
        assert!(
            matches!(effects.status, ExecutionStatus::Success { .. }),
            "{:?}",
            effects.status
        );
        let (child_id, _, _) = effects.created[0].0;
        // call a function with a vector containing the same owned object as another one passed as
        // a reference argument
        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "entry_point_vector",
            "child_access",
            vec![],
            vec![
                TestCallArg::Object(child_id),
                TestCallArg::ObjVec(vec![parent_id]),
            ],
        )
        .await;
        assert!(effects.is_err());
        assert!(format!("{effects:?}")
            .contains("TransactionInputObjectsErrors { errors: [InvalidChildObjectArgument"));
    })
}

#[test]
#[cfg_attr(msim, ignore)]
fn test_entry_point_vector_error() {
    run_tokio_test_with_big_stack(async move {
        let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
        let gas = ObjectID::random();
        let authority = init_state_with_ids(vec![(sender, gas)]).await;

        let package = build_and_publish_test_package(
            &authority,
            &sender,
            &sender_key,
            &gas,
            "entry_point_vector",
        )
        .await;

        // mint an owned object of a wrong type
        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "entry_point_vector",
            "mint_another",
            vec![],
            vec![TestCallArg::Pure(bcs::to_bytes(&(42_u64)).unwrap())],
        )
        .await
        .unwrap();
        assert!(
            matches!(effects.status, ExecutionStatus::Success { .. }),
            "{:?}",
            effects.status
        );
        let (obj_id, _, _) = effects.created[0].0;
        // call a function with a vector containing one owned object
        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "entry_point_vector",
            "obj_vec_destroy",
            vec![],
            vec![TestCallArg::ObjVec(vec![obj_id])],
        )
        .await
        .unwrap();
        // should fail as we passed object of the wrong type
        assert!(
            matches!(effects.status, ExecutionStatus::Failure { .. }),
            "{:?}",
            effects.status
        );

        // mint two objects - one of a wrong type and one of the correct type
        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "entry_point_vector",
            "mint_another",
            vec![],
            vec![TestCallArg::Pure(bcs::to_bytes(&(42_u64)).unwrap())],
        )
        .await
        .unwrap();
        assert!(
            matches!(effects.status, ExecutionStatus::Success { .. }),
            "{:?}",
            effects.status
        );
        let (wrong_obj_id, _, _) = effects.created[0].0;
        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "entry_point_vector",
            "mint",
            vec![],
            vec![TestCallArg::Pure(bcs::to_bytes(&(42_u64)).unwrap())],
        )
        .await
        .unwrap();
        assert!(
            matches!(effects.status, ExecutionStatus::Success { .. }),
            "{:?}",
            effects.status
        );
        let (correct_obj_id, _, _) = effects.created[0].0;
        // call a function with a vector containing one owned object
        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "entry_point_vector",
            "two_obj_vec_destroy",
            vec![],
            vec![TestCallArg::ObjVec(vec![wrong_obj_id, correct_obj_id])],
        )
        .await
        .unwrap();
        // should fail as we passed object of the wrong type as the first element of the vector
        assert!(
            matches!(effects.status, ExecutionStatus::Failure { .. }),
            "{:?}",
            effects.status
        );

        // mint a shared object
        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "entry_point_vector",
            "mint_shared",
            vec![],
            vec![TestCallArg::Pure(bcs::to_bytes(&(42_u64)).unwrap())],
        )
        .await
        .unwrap();
        assert!(
            matches!(effects.status, ExecutionStatus::Success { .. }),
            "{:?}",
            effects.status
        );
        let (shared_obj_id, _, _) = effects.created[0].0;
        // call a function with a vector containing one shared object
        let effects = call_move_with_shared(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "entry_point_vector",
            "obj_vec_destroy",
            vec![],
            vec![TestCallArg::ObjVec(vec![shared_obj_id])],
            true, // shared object in arguments
        )
        .await
        .unwrap();
        // should fail as we do not support shared objects in vectors
        assert!(
            matches!(effects.status, ExecutionStatus::Failure { .. }),
            "{:?}",
            effects.status
        );

        // mint an owned object
        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "entry_point_vector",
            "mint",
            vec![],
            vec![TestCallArg::Pure(bcs::to_bytes(&(42_u64)).unwrap())],
        )
        .await
        .unwrap();
        assert!(
            matches!(effects.status, ExecutionStatus::Success { .. }),
            "{:?}",
            effects.status
        );
        let (obj_id, _, _) = effects.created[0].0;
        // call a function with a vector containing the same owned object as another one passed as
        // argument
        let result = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "entry_point_vector",
            "same_objects",
            vec![],
            vec![
                TestCallArg::Object(obj_id),
                TestCallArg::ObjVec(vec![obj_id]),
            ],
        )
        .await;
        // should fail as we have the same object passed in vector and as a separate by-value argument
        assert!(
            matches!(
                result.clone().err().unwrap(),
                SuiError::DuplicateObjectRefInput { .. }
            ),
            "{:?}",
            result
        );

        // mint an owned object
        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "entry_point_vector",
            "mint",
            vec![],
            vec![TestCallArg::Pure(bcs::to_bytes(&(42_u64)).unwrap())],
        )
        .await
        .unwrap();
        assert!(
            matches!(effects.status, ExecutionStatus::Success { .. }),
            "{:?}",
            effects.status
        );
        let (obj_id, _, _) = effects.created[0].0;
        // call a function with a vector containing the same owned object as another one passed as
        // a reference argument
        let result = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "entry_point_vector",
            "same_objects_ref",
            vec![],
            vec![
                TestCallArg::Object(obj_id),
                TestCallArg::ObjVec(vec![obj_id]),
            ],
        )
        .await;
        // should fail as we have the same object passed in vector and as a separate by-reference argument
        assert!(
            matches!(
                result.clone().err().unwrap(),
                SuiError::DuplicateObjectRefInput { .. }
            ),
            "{:?}",
            result
        );
    })
}

#[test]
#[cfg_attr(msim, ignore)]
fn test_entry_point_vector_any() {
    run_tokio_test_with_big_stack(async move {
        let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
        let gas = ObjectID::random();
        let authority = init_state_with_ids(vec![(sender, gas)]).await;

        let package = build_and_publish_test_package(
            &authority,
            &sender,
            &sender_key,
            &gas,
            "entry_point_vector",
        )
        .await;

        let any_type_tag =
            TypeTag::from_str(format!("{}::entry_point_vector::Any", package.0).as_str()).unwrap();

        // mint an owned object
        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "entry_point_vector",
            "mint_any",
            vec![any_type_tag.clone()],
            vec![TestCallArg::Pure(bcs::to_bytes(&(42_u64)).unwrap())],
        )
        .await
        .unwrap();
        assert!(
            matches!(effects.status, ExecutionStatus::Success { .. }),
            "{:?}",
            effects.status
        );
        let (obj_id, _, _) = effects.created[0].0;
        // call a function with a vector containing one owned object
        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "entry_point_vector",
            "obj_vec_destroy_any",
            vec![any_type_tag.clone()],
            vec![TestCallArg::ObjVec(vec![obj_id])],
        )
        .await
        .unwrap();
        assert!(
            matches!(effects.status, ExecutionStatus::Success { .. }),
            "{:?}",
            effects.status
        );

        // mint a parent object and a child object and make sure that parent stored in the vector
        // authenticates the child passed by-value
        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "entry_point_vector",
            "mint_any",
            vec![any_type_tag.clone()],
            vec![TestCallArg::Pure(bcs::to_bytes(&(42_u64)).unwrap())],
        )
        .await
        .unwrap();
        assert!(
            matches!(effects.status, ExecutionStatus::Success { .. }),
            "{:?}",
            effects.status
        );
        let (parent_id, _, _) = effects.created[0].0;
        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "entry_point_vector",
            "mint_child_any",
            vec![any_type_tag.clone()],
            vec![
                TestCallArg::Pure(bcs::to_bytes(&(42_u64)).unwrap()),
                TestCallArg::Object(parent_id),
            ],
        )
        .await
        .unwrap();
        assert!(
            matches!(effects.status, ExecutionStatus::Success { .. }),
            "{:?}",
            effects.status
        );
        let (child_id, _, _) = effects.created[0].0;
        // call a function with a vector containing the same owned object as another one passed as
        // a reference argument
        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "entry_point_vector",
            "child_access_any",
            vec![any_type_tag],
            vec![
                TestCallArg::Object(child_id),
                TestCallArg::ObjVec(vec![parent_id]),
            ],
        )
        .await;
        assert!(effects.is_err());
        assert!(format!("{effects:?}")
            .contains("TransactionInputObjectsErrors { errors: [InvalidChildObjectArgument"));
    })
}

#[test]
#[cfg_attr(msim, ignore)]
fn test_entry_point_vector_any_error() {
    run_tokio_test_with_big_stack(async move {
        let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
        let gas = ObjectID::random();
        let authority = init_state_with_ids(vec![(sender, gas)]).await;

        let package = build_and_publish_test_package(
            &authority,
            &sender,
            &sender_key,
            &gas,
            "entry_point_vector",
        )
        .await;

        let any_type_tag =
            TypeTag::from_str(format!("{}::entry_point_vector::Any", package.0).as_str()).unwrap();

        // mint an owned object of a wrong type
        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "entry_point_vector",
            "mint_another_any",
            vec![any_type_tag.clone()],
            vec![TestCallArg::Pure(bcs::to_bytes(&(42_u64)).unwrap())],
        )
        .await
        .unwrap();
        assert!(
            matches!(effects.status, ExecutionStatus::Success { .. }),
            "{:?}",
            effects.status
        );
        let (obj_id, _, _) = effects.created[0].0;
        // call a function with a vector containing one owned object
        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "entry_point_vector",
            "obj_vec_destroy_any",
            vec![any_type_tag.clone()],
            vec![TestCallArg::ObjVec(vec![obj_id])],
        )
        .await
        .unwrap();
        // should fail as we passed object of the wrong type
        assert!(
            matches!(effects.status, ExecutionStatus::Failure { .. }),
            "{:?}",
            effects.status
        );

        // mint two objects - one of a wrong type and one of the correct type
        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "entry_point_vector",
            "mint_another_any",
            vec![any_type_tag.clone()],
            vec![TestCallArg::Pure(bcs::to_bytes(&(42_u64)).unwrap())],
        )
        .await
        .unwrap();
        assert!(
            matches!(effects.status, ExecutionStatus::Success { .. }),
            "{:?}",
            effects.status
        );
        let (wrong_obj_id, _, _) = effects.created[0].0;
        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "entry_point_vector",
            "mint_any",
            vec![any_type_tag.clone()],
            vec![TestCallArg::Pure(bcs::to_bytes(&(42_u64)).unwrap())],
        )
        .await
        .unwrap();
        assert!(
            matches!(effects.status, ExecutionStatus::Success { .. }),
            "{:?}",
            effects.status
        );
        let (correct_obj_id, _, _) = effects.created[0].0;
        // call a function with a vector containing one owned object
        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "entry_point_vector",
            "two_obj_vec_destroy_any",
            vec![any_type_tag.clone()],
            vec![TestCallArg::ObjVec(vec![wrong_obj_id, correct_obj_id])],
        )
        .await
        .unwrap();
        // should fail as we passed object of the wrong type as the first element of the vector
        assert!(
            matches!(effects.status, ExecutionStatus::Failure { .. }),
            "{:?}",
            effects.status
        );

        // mint a shared object
        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "entry_point_vector",
            "mint_shared_any",
            vec![any_type_tag.clone()],
            vec![TestCallArg::Pure(bcs::to_bytes(&(42_u64)).unwrap())],
        )
        .await
        .unwrap();
        assert!(
            matches!(effects.status, ExecutionStatus::Success { .. }),
            "{:?}",
            effects.status
        );
        let (shared_obj_id, _, _) = effects.created[0].0;
        // call a function with a vector containing one shared object
        let effects = call_move_with_shared(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "entry_point_vector",
            "obj_vec_destroy_any",
            vec![any_type_tag.clone()],
            vec![TestCallArg::ObjVec(vec![shared_obj_id])],
            true, // shared object in arguments
        )
        .await
        .unwrap();
        // should fail as we do not support shared objects in vectors
        assert!(
            matches!(effects.status, ExecutionStatus::Failure { .. }),
            "{:?}",
            effects.status
        );

        // mint an owned object
        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "entry_point_vector",
            "mint_any",
            vec![any_type_tag.clone()],
            vec![TestCallArg::Pure(bcs::to_bytes(&(42_u64)).unwrap())],
        )
        .await
        .unwrap();
        assert!(
            matches!(effects.status, ExecutionStatus::Success { .. }),
            "{:?}",
            effects.status
        );
        let (obj_id, _, _) = effects.created[0].0;
        // call a function with a vector containing the same owned object as another one passed as
        // argument
        let result = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "entry_point_vector",
            "same_objects_any",
            vec![any_type_tag.clone()],
            vec![
                TestCallArg::Object(obj_id),
                TestCallArg::ObjVec(vec![obj_id]),
            ],
        )
        .await;
        // should fail as we have the same object passed in vector and as a separate by-value argument
        assert!(
            matches!(
                result.clone().err().unwrap(),
                SuiError::DuplicateObjectRefInput { .. }
            ),
            "{:?}",
            result
        );

        // mint an owned object
        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "entry_point_vector",
            "mint_any",
            vec![any_type_tag.clone()],
            vec![TestCallArg::Pure(bcs::to_bytes(&(42_u64)).unwrap())],
        )
        .await
        .unwrap();
        assert!(
            matches!(effects.status, ExecutionStatus::Success { .. }),
            "{:?}",
            effects.status
        );
        let (obj_id, _, _) = effects.created[0].0;
        // call a function with a vector containing the same owned object as another one passed as
        // a reference argument
        let result = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "entry_point_vector",
            "same_objects_ref_any",
            vec![any_type_tag.clone()],
            vec![
                TestCallArg::Object(obj_id),
                TestCallArg::ObjVec(vec![obj_id]),
            ],
        )
        .await;
        assert!(
            matches!(
                result.clone().err().unwrap(),
                SuiError::DuplicateObjectRefInput { .. }
            ),
            "{:?}",
            result
        );
    })
}

#[test]
#[cfg_attr(msim, ignore)]
fn test_entry_point_string() {
    run_tokio_test_with_big_stack(async move {
        let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
        let gas = ObjectID::random();
        let authority = init_state_with_ids(vec![(sender, gas)]).await;

        let package = build_and_publish_test_package(
            &authority,
            &sender,
            &sender_key,
            &gas,
            "entry_point_string",
        )
        .await;

        // pass a valid ascii string
        let ascii_str = "SomeString";
        let ascii_str_bcs = MoveValue::Struct(MoveStruct::Runtime(vec![MoveValue::Vector(
            ascii_str
                .as_bytes()
                .iter()
                .map(|c| MoveValue::U8(*c))
                .collect(),
        )]))
        .simple_serialize()
        .unwrap();

        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "entry_point_string",
            "ascii_arg",
            vec![],
            vec![TestCallArg::Pure(ascii_str_bcs)],
        )
        .await
        .unwrap();
        assert!(
            matches!(effects.status, ExecutionStatus::Success { .. }),
            "{:?}",
            effects.status
        );

        // pass a valid utf8 string
        let utf8_str = "çå∞≠¢õß∂ƒ∫";
        let utf_str_bcs = MoveValue::Struct(MoveStruct::Runtime(vec![MoveValue::Vector(
            utf8_str
                .as_bytes()
                .iter()
                .map(|c| MoveValue::U8(*c))
                .collect(),
        )]))
        .simple_serialize()
        .unwrap();

        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "entry_point_string",
            "utf8_arg",
            vec![],
            vec![TestCallArg::Pure(utf_str_bcs)],
        )
        .await
        .unwrap();
        assert!(
            matches!(effects.status, ExecutionStatus::Success { .. }),
            "{:?}",
            effects.status
        );
    })
}

#[test]
#[cfg_attr(msim, ignore)]
fn test_entry_point_string_vec() {
    run_tokio_test_with_big_stack(async move {
        let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
        let gas = ObjectID::random();
        let authority = init_state_with_ids(vec![(sender, gas)]).await;

        let package = build_and_publish_test_package(
            &authority,
            &sender,
            &sender_key,
            &gas,
            "entry_point_string",
        )
        .await;

        // pass a valid utf8 string vector
        let utf8_str_1 = "çå∞≠¢";
        let utf8_str_2 = "õß∂ƒ∫";
        let utf_str_vec_bcs = MoveValue::Vector(vec![
            MoveValue::Struct(MoveStruct::Runtime(vec![MoveValue::Vector(
                utf8_str_1
                    .as_bytes()
                    .iter()
                    .map(|c| MoveValue::U8(*c))
                    .collect(),
            )])),
            MoveValue::Struct(MoveStruct::Runtime(vec![MoveValue::Vector(
                utf8_str_2
                    .as_bytes()
                    .iter()
                    .map(|c| MoveValue::U8(*c))
                    .collect(),
            )])),
        ])
        .simple_serialize()
        .unwrap();

        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "entry_point_string",
            "utf8_vec_arg",
            vec![],
            vec![TestCallArg::Pure(utf_str_vec_bcs)],
        )
        .await
        .unwrap();
        assert!(
            matches!(effects.status, ExecutionStatus::Success { .. }),
            "{:?}",
            effects.status
        );
    })
}

#[test]
#[cfg_attr(msim, ignore)]
fn test_entry_point_string_error() {
    run_tokio_test_with_big_stack(async move {
        let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
        let gas = ObjectID::random();
        let authority = init_state_with_ids(vec![(sender, gas)]).await;

        let package = build_and_publish_test_package(
            &authority,
            &sender,
            &sender_key,
            &gas,
            "entry_point_string",
        )
        .await;

        // pass a invalid ascii string
        let ascii_str = "SomeString";
        let mut ascii_u8_vec: Vec<MoveValue> = ascii_str
            .as_bytes()
            .iter()
            .map(|c| MoveValue::U8(*c))
            .collect();
        // mess up one element
        ascii_u8_vec[7] = MoveValue::U8(255);

        let ascii_str_bcs =
            MoveValue::Struct(MoveStruct::Runtime(vec![MoveValue::Vector(ascii_u8_vec)]))
                .simple_serialize()
                .unwrap();

        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "entry_point_string",
            "ascii_arg",
            vec![],
            vec![TestCallArg::Pure(ascii_str_bcs)],
        )
        .await
        .unwrap();
        assert!(
            matches!(effects.status, ExecutionStatus::Failure { .. }),
            "{:?}",
            effects.status
        );

        // pass a invalid utf8 string
        let utf8_str = "çå∞≠¢õß∂ƒ∫";
        let mut utf8_u8_vec: Vec<MoveValue> = utf8_str
            .as_bytes()
            .iter()
            .map(|c| MoveValue::U8(*c))
            .collect();
        // mess up one element
        utf8_u8_vec[7] = MoveValue::U8(255);

        let utf8_str_bcs =
            MoveValue::Struct(MoveStruct::Runtime(vec![MoveValue::Vector(utf8_u8_vec)]))
                .simple_serialize()
                .unwrap();

        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "entry_point_string",
            "utf8_arg",
            vec![],
            vec![TestCallArg::Pure(utf8_str_bcs)],
        )
        .await
        .unwrap();
        assert!(
            matches!(effects.status, ExecutionStatus::Failure { .. }),
            "{:?}",
            effects.status
        );
    })
}

#[test]
#[cfg_attr(msim, ignore)]
fn test_entry_point_string_vec_error() {
    run_tokio_test_with_big_stack(async move {
        let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
        let gas = ObjectID::random();
        let authority = init_state_with_ids(vec![(sender, gas)]).await;

        let package = build_and_publish_test_package(
            &authority,
            &sender,
            &sender_key,
            &gas,
            "entry_point_string",
        )
        .await;

        // pass an utf8 string vector with one invalid string
        let utf8_str_1 = "çå∞≠¢";
        let utf8_str_2 = "õß∂ƒ∫";
        let mut utf8_u8_vec_1: Vec<MoveValue> = utf8_str_1
            .as_bytes()
            .iter()
            .map(|c| MoveValue::U8(*c))
            .collect();
        // mess up one element
        utf8_u8_vec_1[7] = MoveValue::U8(255);
        let utf_str_vec_bcs = MoveValue::Vector(vec![
            MoveValue::Struct(MoveStruct::Runtime(vec![MoveValue::Vector(utf8_u8_vec_1)])),
            MoveValue::Struct(MoveStruct::Runtime(vec![MoveValue::Vector(
                utf8_str_2
                    .as_bytes()
                    .iter()
                    .map(|c| MoveValue::U8(*c))
                    .collect(),
            )])),
        ])
        .simple_serialize()
        .unwrap();

        let effects = call_move(
            &authority,
            &gas,
            &sender,
            &sender_key,
            &package,
            "entry_point_string",
            "utf8_vec_arg",
            vec![],
            vec![TestCallArg::Pure(utf_str_vec_bcs)],
        )
        .await
        .unwrap();
        assert!(
            matches!(effects.status, ExecutionStatus::Failure { .. }),
            "{:?}",
            effects.status
        );
    })
}

#[test]
#[cfg_attr(msim, ignore)]
fn test_object_no_id_error() {
    run_tokio_test_with_big_stack(async move {
        let mut build_config = BuildConfig::default();
        build_config.config.test_mode = true;
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        // in this package object struct (NotObject) is defined incorrectly and publishing should
        // fail (it's defined in test-only code hence cannot be checked by transactional testing
        // framework which goes through "normal" publishing path which excludes tests).
        path.push("src/unit_tests/data/object_no_id/");
        let res = sui_framework::build_move_package(&path, build_config);

        matches!(res.err(), Some(SuiError::ExecutionError(err_str)) if
                 err_str.contains("SuiMoveVerificationError")
                 && err_str.contains("First field of struct NotObject must be 'id'"));
    })
}

pub async fn build_and_try_publish_test_package(
    authority: &AuthorityState,
    sender: &SuiAddress,
    sender_key: &AccountKeyPair,
    gas_object_id: &ObjectID,
    test_dir: &str,
    gas_budget: u64,
) -> VerifiedTransactionInfoResponse {
    let build_config = BuildConfig::default();
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("src/unit_tests/data/");
    path.push(test_dir);
    let all_module_bytes = sui_framework::build_move_package(&path, build_config)
        .unwrap()
        .get_package_bytes();

    let gas_object = authority.get_object(gas_object_id).await.unwrap();
    let gas_object_ref = gas_object.unwrap().compute_object_reference();

    let data = TransactionData::new_module(*sender, gas_object_ref, all_module_bytes, gas_budget);
    let transaction = to_sender_signed_transaction(data, sender_key);

    send_and_confirm_transaction(authority, transaction)
        .await
        .unwrap()
}

async fn build_and_publish_test_package(
    authority: &AuthorityState,
    sender: &SuiAddress,
    sender_key: &AccountKeyPair,
    gas_object_id: &ObjectID,
    test_dir: &str,
) -> ObjectRef {
    let effects = build_and_try_publish_test_package(
        authority,
        sender,
        sender_key,
        gas_object_id,
        test_dir,
        MAX_GAS,
    )
    .await
    .signed_effects
    .unwrap()
    .into_data();
    assert!(
        matches!(effects.status, ExecutionStatus::Success { .. }),
        "{:?}",
        effects.status
    );
    effects.created[0].0
}

async fn check_latest_object_ref(authority: &AuthorityState, object_ref: &ObjectRef) {
    let response = authority
        .handle_object_info_request(ObjectInfoRequest {
            object_id: object_ref.0,
            request_kind: ObjectInfoRequestKind::LatestObjectInfo(None),
        })
        .await
        .unwrap();
    assert_eq!(&response.requested_object_reference.unwrap(), object_ref,);
}
