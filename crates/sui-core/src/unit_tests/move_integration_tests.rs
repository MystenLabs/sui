// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::authority::authority_tests::{
    call_move, call_move_, init_state_with_ids, send_and_confirm_transaction, TestCallArg,
};
use sui_types::{object::Data, utils::to_sender_signed_transaction};

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
};

use expect_test::expect;
use std::fs::File;
use std::io::Read;
use std::{collections::HashSet, path::PathBuf};
use std::{env, str::FromStr};

const MAX_GAS: u64 = 10000;

#[tokio::test]
#[cfg_attr(msim, ignore)]
async fn test_publishing_with_unpublished_deps() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas = ObjectID::random();
    let authority = init_state_with_ids(vec![(sender, gas)]).await;

    let effects = build_and_try_publish_test_package(
        &authority,
        &sender,
        &sender_key,
        &gas,
        "depends_on_basics",
        MAX_GAS,
        /* with_unpublished_deps */ true,
    )
    .await
    .1
    .into_data();

    assert!(effects.status().is_ok());
    assert_eq!(effects.created().len(), 1);
    let package = effects.created()[0].0;

    let ObjectRead::Exists(read_ref, package_obj, _) = authority
        .get_object_read(&package.0)
        .await
        .unwrap()
    else {
        panic!("Can't read package")
    };

    assert_eq!(package, read_ref);
    let Data::Package(move_package) = package_obj.data else {
        panic!("Not a package")
    };

    // Check that the published package includes its depended upon module.
    assert_eq!(
        move_package
            .serialized_module_map()
            .keys()
            .map(String::as_str)
            .collect::<HashSet<_>>(),
        HashSet::from(["depends_on_basics", "object_basics"]),
    );

    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
        "depends_on_basics",
        "delegate",
        vec![],
        vec![],
    )
    .await
    .unwrap();

    assert!(effects.status().is_ok());
    assert_eq!(effects.created().len(), 1);
    let ((_, v, _), owner) = effects.created()[0];

    // Check that calling the function does what we expect
    assert!(matches!(
        owner,
        Owner::Shared { initial_shared_version: initial } if initial == v
    ));
}

#[tokio::test]
#[cfg_attr(msim, ignore)]
async fn test_object_wrapping_unwrapping() {
    let (sender, sender_key): (_, AccountKeyPair) = get_key_pair();
    let gas = ObjectID::random();
    let authority = init_state_with_ids(vec![(sender, gas)]).await;

    let package =
        build_and_publish_test_package(&authority, &sender, &sender_key, &gas, "object_wrapping")
            .await;

    let gas_version = authority.get_object(&gas).await.unwrap().unwrap().version();
    let create_child_version = SequenceNumber::lamport_increment([gas_version]);

    // Create a Child object.
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
        "object_wrapping",
        "create_child",
        vec![],
        vec![],
    )
    .await
    .unwrap();
    assert!(
        matches!(effects.status(), ExecutionStatus::Success { .. }),
        "{:?}",
        effects.status()
    );
    let child_object_ref = effects.created()[0].0;
    assert_eq!(child_object_ref.1, create_child_version);

    let wrapped_version =
        SequenceNumber::lamport_increment([child_object_ref.1, effects.gas_object().0 .1]);

    // Create a Parent object, by wrapping the child object.
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
        "object_wrapping",
        "create_parent",
        vec![],
        vec![TestCallArg::Object(child_object_ref.0)],
    )
    .await
    .unwrap();
    assert!(
        matches!(effects.status(), ExecutionStatus::Success { .. }),
        "{:?}",
        effects.status()
    );
    // Child object is wrapped, Parent object is created.
    assert_eq!(
        (
            effects.created().len(),
            effects.deleted().len(),
            effects.unwrapped_then_deleted().len(),
            effects.wrapped().len()
        ),
        (1, 0, 0, 1)
    );
    let new_child_object_ref = effects.wrapped()[0];
    let expected_child_object_ref = (
        child_object_ref.0,
        wrapped_version,
        ObjectDigest::OBJECT_DIGEST_WRAPPED,
    );
    // Make sure that the child's version gets increased after wrapped.
    assert_eq!(new_child_object_ref, expected_child_object_ref);
    check_latest_object_ref(&authority, &expected_child_object_ref, true).await;

    let parent_object_ref = effects.created()[0].0;
    assert_eq!(parent_object_ref.1, wrapped_version);

    let unwrapped_version =
        SequenceNumber::lamport_increment([parent_object_ref.1, effects.gas_object().0 .1]);

    // Extract the child out of the parent.
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
        "object_wrapping",
        "extract_child",
        vec![],
        vec![TestCallArg::Object(parent_object_ref.0)],
    )
    .await
    .unwrap();
    assert!(
        matches!(effects.status(), ExecutionStatus::Success { .. }),
        "{:?}",
        effects.status()
    );
    // Check that the child shows up in unwrapped, not created.
    // mutated contains parent and gas.
    assert_eq!(
        (
            effects.mutated().len(),
            effects.created().len(),
            effects.unwrapped().len()
        ),
        (2, 0, 1)
    );
    // Make sure that version increments again when unwrapped.
    assert_eq!(effects.unwrapped()[0].0 .1, unwrapped_version);
    check_latest_object_ref(&authority, &effects.unwrapped()[0].0, false).await;
    let child_object_ref = effects.unwrapped()[0].0;

    let rewrap_version = SequenceNumber::lamport_increment([
        parent_object_ref.1,
        child_object_ref.1,
        effects.gas_object().0 .1,
    ]);

    // Wrap the child to the parent again.
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
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
        matches!(effects.status(), ExecutionStatus::Success { .. }),
        "{:?}",
        effects.status()
    );
    // Check that child object showed up in wrapped.
    // mutated contains parent and gas.
    assert_eq!((effects.mutated().len(), effects.wrapped().len()), (2, 1));
    let expected_child_object_ref = (
        child_object_ref.0,
        rewrap_version,
        ObjectDigest::OBJECT_DIGEST_WRAPPED,
    );
    assert_eq!(effects.wrapped()[0], expected_child_object_ref);
    check_latest_object_ref(&authority, &expected_child_object_ref, true).await;
    let child_object_ref = effects.wrapped()[0];
    let parent_object_ref = effects.mutated_excluding_gas().first().unwrap().0;

    let deleted_version =
        SequenceNumber::lamport_increment([parent_object_ref.1, effects.gas_object().0 .1]);

    // Now delete the parent object, which will in turn delete the child object.
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
        "object_wrapping",
        "delete_parent",
        vec![],
        vec![TestCallArg::Object(parent_object_ref.0)],
    )
    .await
    .unwrap();
    assert!(
        matches!(effects.status(), ExecutionStatus::Success { .. }),
        "{:?}",
        effects.status()
    );
    assert_eq!(effects.deleted().len(), 1);
    assert_eq!(effects.unwrapped_then_deleted().len(), 1);
    // Check that both objects are marked as deleted in the authority.
    let expected_child_object_ref = (
        child_object_ref.0,
        deleted_version,
        ObjectDigest::OBJECT_DIGEST_DELETED,
    );
    assert!(effects
        .unwrapped_then_deleted()
        .contains(&expected_child_object_ref));
    check_latest_object_ref(&authority, &expected_child_object_ref, true).await;
    let expected_parent_object_ref = (
        parent_object_ref.0,
        deleted_version,
        ObjectDigest::OBJECT_DIGEST_DELETED,
    );
    assert!(effects.deleted().contains(&expected_parent_object_ref));
    check_latest_object_ref(&authority, &expected_parent_object_ref, true).await;
}

#[tokio::test]
#[cfg_attr(msim, ignore)]
async fn test_object_owning_another_object() {
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
        &package.0,
        "object_owner",
        "create_parent",
        vec![],
        vec![],
    )
    .await
    .unwrap();
    let events = if let Some(digest) = &effects.events_digest() {
        authority.database.get_events(digest).unwrap().data
    } else {
        vec![]
    };
    assert!(effects.status().is_ok());
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].event_type(), EventType::CoinBalanceChange);
    assert_eq!(events[1].event_type(), EventType::NewObject);
    let parent = effects.created()[0].0;
    assert_eq!(events[1].object_id(), Some(parent.0));

    // Create a child.
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
        "object_owner",
        "create_child",
        vec![],
        vec![],
    )
    .await
    .unwrap();
    let events = if let Some(digest) = &effects.events_digest() {
        authority.database.get_events(digest).unwrap().data
    } else {
        vec![]
    };
    assert!(effects.status().is_ok());
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].event_type(), EventType::CoinBalanceChange);
    assert_eq!(events[1].event_type(), EventType::NewObject);
    let child = effects.created()[0].0;

    // Mutate the child directly should work fine.
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
        "object_owner",
        "mutate_child",
        vec![],
        vec![TestCallArg::Object(child.0)],
    )
    .await
    .unwrap();
    assert!(effects.status().is_ok());

    // Add the child to the parent.
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
        "object_owner",
        "add_child",
        vec![],
        vec![TestCallArg::Object(parent.0), TestCallArg::Object(child.0)],
    )
    .await
    .unwrap();
    effects.status().unwrap();
    let child_effect = effects
        .mutated()
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
        &package.0,
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
        &package.0,
        "object_owner",
        "mutate_child_with_parent",
        vec![],
        vec![TestCallArg::Object(child.0), TestCallArg::Object(parent.0)],
    )
    .await;
    assert!(effects.is_err());
    assert!(format!("{effects:?}").contains("InvalidChildObjectArgument"));

    // Create another parent.
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
        "object_owner",
        "create_parent",
        vec![],
        vec![],
    )
    .await
    .unwrap();
    let events = if let Some(digest) = &effects.events_digest() {
        authority.database.get_events(digest).unwrap().data
    } else {
        vec![]
    };
    assert!(effects.status().is_ok());
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].event_type(), EventType::CoinBalanceChange);
    assert_eq!(events[1].event_type(), EventType::NewObject);
    let new_parent = effects.created()[0].0;

    // Transfer the child to the new_parent.
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
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
    let events = if let Some(digest) = &effects.events_digest() {
        authority.database.get_events(digest).unwrap().data
    } else {
        vec![]
    };
    assert!(effects.status().is_ok());
    assert_eq!(events.len(), 7);
    // TODO: figure out why an extra event is emitted.
    // assert_eq!(events.len(), 6);
    let num_transfers = events
        .iter()
        .filter(|e| matches!(e.event_type(), EventType::TransferObject { .. }))
        .count();
    assert_eq!(num_transfers, 1);
    let num_created = events
        .iter()
        .filter(|e| matches!(e.event_type(), EventType::NewObject { .. }))
        .count();
    assert_eq!(num_created, 1);
    let child_event = events
        .iter()
        .find(|e| e.object_id() == Some(child.0))
        .unwrap();
    let num_deleted = events
        .iter()
        .filter(|e| matches!(e.event_type(), EventType::DeleteObject { .. }))
        .count();
    assert_eq!(num_deleted, 1);
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
        .mutated()
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
        &package.0,
        "object_owner",
        "delete_child",
        vec![],
        vec![TestCallArg::Object(child.0)],
    )
    .await;
    assert!(effects.is_err());
}

#[tokio::test]
#[cfg_attr(msim, ignore)]
async fn test_create_then_delete_parent_child() {
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
        &package.0,
        "object_owner",
        "create_parent_and_child",
        vec![],
        vec![],
    )
    .await
    .unwrap();
    let events = if let Some(digest) = &effects.events_digest() {
        authority.database.get_events(digest).unwrap().data
    } else {
        vec![]
    };
    assert!(effects.status().is_ok());
    // Creates 3 objects, the parent, a field, and the child
    assert_eq!(effects.created().len(), 3);
    // Creates 4 events, gas charge, child, parent and wrapped object
    assert_eq!(events.len(), 4);
    let parent = effects
        .created()
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
        &package.0,
        "object_owner",
        "delete_parent_and_child",
        vec![],
        vec![TestCallArg::Object(parent.0)],
    )
    .await
    .unwrap();
    assert!(effects.status().is_ok());
    // Check that both objects were deleted.
    assert_eq!(effects.deleted().len(), 3);
    assert_eq!(events.len(), 4);
}

#[tokio::test]
#[cfg_attr(msim, ignore)]
async fn test_create_then_delete_parent_child_wrap() {
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
        &package.0,
        "object_owner",
        "create_parent_and_child_wrapped",
        vec![],
        vec![],
    )
    .await
    .unwrap();
    let events = if let Some(digest) = &effects.events_digest() {
        authority.database.get_events(digest).unwrap().data
    } else {
        vec![]
    };
    assert!(effects.status().is_ok());
    // Modifies the gas object
    assert_eq!(effects.mutated().len(), 1);
    // Creates 3 objects, the parent, a field, and the child
    assert_eq!(effects.created().len(), 2);
    // not wrapped as it wasn't first created
    assert_eq!(effects.wrapped().len(), 0);
    assert_eq!(events.len(), 3);

    let gas_ref = effects.mutated()[0].0;

    let parent = effects
        .created()
        .iter()
        .find(|(_, owner)| matches!(owner, Owner::AddressOwner(_)))
        .unwrap()
        .0;

    let field = effects
        .created()
        .iter()
        .find(|((id, _, _), _)| id != &parent.0)
        .unwrap()
        .0;

    // Delete the parent and child altogether.
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
        "object_owner",
        "delete_parent_and_child_wrapped",
        vec![],
        vec![TestCallArg::Object(parent.0)],
    )
    .await
    .unwrap();
    let events = if let Some(digest) = &effects.events_digest() {
        authority.database.get_events(digest).unwrap().data
    } else {
        vec![]
    };
    assert!(effects.status().is_ok());

    // The parent and field are considered deleted, the child doesn't count because it wasn't
    // considered created in the first place.
    assert_eq!(effects.deleted().len(), 2);
    // The child was never created so it is not unwrapped.
    assert_eq!(effects.unwrapped_then_deleted().len(), 0);
    assert_eq!(events.len(), 3);

    assert_eq!(
        effects
            .modified_at_versions()
            .iter()
            .cloned()
            .collect::<HashSet<_>>(),
        HashSet::from([
            (gas_ref.0, gas_ref.1),
            (parent.0, parent.1),
            (field.0, field.1)
        ]),
    );
}

#[tokio::test]
#[cfg_attr(msim, ignore)]
async fn test_create_then_delete_parent_child_wrap_separate() {
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
        &package.0,
        "object_owner",
        "create_parent",
        vec![],
        vec![],
    )
    .await
    .unwrap();
    let events = if let Some(digest) = &effects.events_digest() {
        authority.database.get_events(digest).unwrap().data
    } else {
        vec![]
    };
    assert!(effects.status().is_ok());
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].event_type(), EventType::CoinBalanceChange);
    assert_eq!(events[1].event_type(), EventType::NewObject);
    let parent = effects.created()[0].0;
    assert_eq!(events[1].object_id(), Some(parent.0));

    // Create a child.
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
        "object_owner",
        "create_child",
        vec![],
        vec![],
    )
    .await
    .unwrap();
    let events = if let Some(digest) = &effects.events_digest() {
        authority.database.get_events(digest).unwrap().data
    } else {
        vec![]
    };
    assert!(effects.status().is_ok());
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].event_type(), EventType::CoinBalanceChange);
    assert_eq!(events[1].event_type(), EventType::NewObject);
    let child = effects.created()[0].0;

    // Add the child to the parent.
    println!("add_child_wrapped");
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
        "object_owner",
        "add_child_wrapped",
        vec![],
        vec![TestCallArg::Object(parent.0), TestCallArg::Object(child.0)],
    )
    .await
    .unwrap();
    let events = if let Some(digest) = &effects.events_digest() {
        authority.database.get_events(digest).unwrap().data
    } else {
        vec![]
    };
    assert!(effects.status().is_ok());
    assert_eq!(effects.created().len(), 1);
    assert_eq!(effects.wrapped().len(), 1);
    // assert_eq!(events.len(), 4);
    // TODO: figure out why an extra event is being emitted here.
    assert_eq!(events.len(), 5);

    // Delete the parent and child altogether.
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
        "object_owner",
        "delete_parent_and_child_wrapped",
        vec![],
        vec![TestCallArg::Object(parent.0)],
    )
    .await
    .unwrap();
    let events = if let Some(digest) = &effects.events_digest() {
        authority.database.get_events(digest).unwrap().data
    } else {
        vec![]
    };
    assert!(effects.status().is_ok());
    // Check that parent object was deleted.
    assert_eq!(effects.deleted().len(), 2);
    // Check that child object was unwrapped and deleted.
    assert_eq!(effects.unwrapped_then_deleted().len(), 1);
    assert_eq!(events.len(), 4);
}

#[tokio::test]
#[cfg_attr(msim, ignore)]
async fn test_entry_point_vector_empty() {
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
        &package.0,
        "entry_point_vector",
        "obj_vec_empty",
        vec![],
        vec![TestCallArg::ObjVec(vec![])],
    )
    .await
    .unwrap();
    assert!(
        matches!(effects.status(), ExecutionStatus::Success { .. }),
        "{:?}",
        effects.status()
    );

    // call a function with an empty vector whose type is generic
    let type_tag =
        TypeTag::from_str(format!("{}::entry_point_vector::Obj", package.0).as_str()).unwrap();
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
        "entry_point_vector",
        "type_param_vec_empty",
        vec![type_tag],
        vec![TestCallArg::ObjVec(vec![])],
    )
    .await
    .unwrap();
    assert!(
        matches!(effects.status(), ExecutionStatus::Success { .. }),
        "{:?}",
        effects.status()
    );
}

#[tokio::test]
#[cfg_attr(msim, ignore)]
async fn test_entry_point_vector_primitive() {
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
        &package.0,
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
        matches!(effects.status(), ExecutionStatus::Success { .. }),
        "{:?}",
        effects.status()
    );
}

#[tokio::test]
#[cfg_attr(msim, ignore)]
async fn test_entry_point_vector() {
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
        &package.0,
        "entry_point_vector",
        "mint",
        vec![],
        vec![TestCallArg::Pure(bcs::to_bytes(&(42_u64)).unwrap())],
    )
    .await
    .unwrap();
    assert!(
        matches!(effects.status(), ExecutionStatus::Success { .. }),
        "{:?}",
        effects.status()
    );
    let (obj_id, _, _) = effects.created()[0].0;
    // call a function with a vector containing one owned object
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
        "entry_point_vector",
        "obj_vec_destroy",
        vec![],
        vec![TestCallArg::ObjVec(vec![obj_id])],
    )
    .await
    .unwrap();
    assert!(
        matches!(effects.status(), ExecutionStatus::Success { .. }),
        "{:?}",
        effects.status()
    );

    // mint a parent object and a child object and make sure that parent stored in the vector
    // authenticates the child passed by-value
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
        "entry_point_vector",
        "mint",
        vec![],
        vec![TestCallArg::Pure(bcs::to_bytes(&(42_u64)).unwrap())],
    )
    .await
    .unwrap();
    assert!(
        matches!(effects.status(), ExecutionStatus::Success { .. }),
        "{:?}",
        effects.status()
    );
    let (parent_id, _, _) = effects.created()[0].0;
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
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
        matches!(effects.status(), ExecutionStatus::Success { .. }),
        "{:?}",
        effects.status()
    );
    let (child_id, _, _) = effects.created()[0].0;
    // call a function with a vector containing the same owned object as another one passed as
    // a reference argument
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
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
    assert!(format!("{effects:?}").contains("InvalidChildObjectArgument"));
}

#[tokio::test]
#[cfg_attr(msim, ignore)]
async fn test_entry_point_vector_error() {
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
        &package.0,
        "entry_point_vector",
        "mint_another",
        vec![],
        vec![TestCallArg::Pure(bcs::to_bytes(&(42_u64)).unwrap())],
    )
    .await
    .unwrap();
    assert!(
        matches!(effects.status(), ExecutionStatus::Success { .. }),
        "{:?}",
        effects.status()
    );
    let (obj_id, _, _) = effects.created()[0].0;
    // call a function with a vector containing one owned object
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
        "entry_point_vector",
        "obj_vec_destroy",
        vec![],
        vec![TestCallArg::ObjVec(vec![obj_id])],
    )
    .await
    .unwrap();
    // should fail as we passed object of the wrong type
    assert!(
        matches!(effects.status(), ExecutionStatus::Failure { .. }),
        "{:?}",
        effects.status()
    );

    // mint two objects - one of a wrong type and one of the correct type
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
        "entry_point_vector",
        "mint_another",
        vec![],
        vec![TestCallArg::Pure(bcs::to_bytes(&(42_u64)).unwrap())],
    )
    .await
    .unwrap();
    assert!(
        matches!(effects.status(), ExecutionStatus::Success { .. }),
        "{:?}",
        effects.status()
    );
    let (wrong_obj_id, _, _) = effects.created()[0].0;
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
        "entry_point_vector",
        "mint",
        vec![],
        vec![TestCallArg::Pure(bcs::to_bytes(&(42_u64)).unwrap())],
    )
    .await
    .unwrap();
    assert!(
        matches!(effects.status(), ExecutionStatus::Success { .. }),
        "{:?}",
        effects.status()
    );
    let (correct_obj_id, _, _) = effects.created()[0].0;
    // call a function with a vector containing one owned object
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
        "entry_point_vector",
        "two_obj_vec_destroy",
        vec![],
        vec![TestCallArg::ObjVec(vec![wrong_obj_id, correct_obj_id])],
    )
    .await
    .unwrap();
    // should fail as we passed object of the wrong type as the first element of the vector
    assert!(
        matches!(effects.status(), ExecutionStatus::Failure { .. }),
        "{:?}",
        effects.status()
    );

    // mint a shared object
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
        "entry_point_vector",
        "mint_shared",
        vec![],
        vec![TestCallArg::Pure(bcs::to_bytes(&(42_u64)).unwrap())],
    )
    .await
    .unwrap();
    assert!(
        matches!(effects.status(), ExecutionStatus::Success { .. }),
        "{:?}",
        effects.status()
    );
    let (shared_obj_id, _, _) = effects.created()[0].0;
    // call a function with a vector containing one shared object
    let effects = call_move_(
        &authority,
        None,
        &gas,
        &sender,
        &sender_key,
        &package.0,
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
        matches!(effects.status(), ExecutionStatus::Failure { .. }),
        "{:?}",
        effects.status()
    );

    // mint an owned object
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
        "entry_point_vector",
        "mint",
        vec![],
        vec![TestCallArg::Pure(bcs::to_bytes(&(42_u64)).unwrap())],
    )
    .await
    .unwrap();
    assert!(
        matches!(effects.status(), ExecutionStatus::Success { .. }),
        "{:?}",
        effects.status()
    );
    let (obj_id, _, _) = effects.created()[0].0;
    // call a function with a vector containing the same owned object as another one passed as
    // argument
    let result = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
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
    assert!(matches!(
        UserInputError::try_from(result.unwrap_err()).unwrap(),
        UserInputError::DuplicateObjectRefInput { .. }
    ));

    // mint an owned object
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
        "entry_point_vector",
        "mint",
        vec![],
        vec![TestCallArg::Pure(bcs::to_bytes(&(42_u64)).unwrap())],
    )
    .await
    .unwrap();
    assert!(
        matches!(effects.status(), ExecutionStatus::Success { .. }),
        "{:?}",
        effects.status()
    );
    let (obj_id, _, _) = effects.created()[0].0;
    // call a function with a vector containing the same owned object as another one passed as
    // a reference argument
    let result = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
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
    assert!(matches!(
        UserInputError::try_from(result.unwrap_err()).unwrap(),
        UserInputError::DuplicateObjectRefInput { .. }
    ));
}

#[tokio::test]
#[cfg_attr(msim, ignore)]
async fn test_entry_point_vector_any() {
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
        &package.0,
        "entry_point_vector",
        "mint_any",
        vec![any_type_tag.clone()],
        vec![TestCallArg::Pure(bcs::to_bytes(&(42_u64)).unwrap())],
    )
    .await
    .unwrap();
    assert!(
        matches!(effects.status(), ExecutionStatus::Success { .. }),
        "{:?}",
        effects.status()
    );
    let (obj_id, _, _) = effects.created()[0].0;
    // call a function with a vector containing one owned object
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
        "entry_point_vector",
        "obj_vec_destroy_any",
        vec![any_type_tag.clone()],
        vec![TestCallArg::ObjVec(vec![obj_id])],
    )
    .await
    .unwrap();
    assert!(
        matches!(effects.status(), ExecutionStatus::Success { .. }),
        "{:?}",
        effects.status()
    );

    // mint a parent object and a child object and make sure that parent stored in the vector
    // authenticates the child passed by-value
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
        "entry_point_vector",
        "mint_any",
        vec![any_type_tag.clone()],
        vec![TestCallArg::Pure(bcs::to_bytes(&(42_u64)).unwrap())],
    )
    .await
    .unwrap();
    assert!(
        matches!(effects.status(), ExecutionStatus::Success { .. }),
        "{:?}",
        effects.status()
    );
    let (parent_id, _, _) = effects.created()[0].0;
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
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
        matches!(effects.status(), ExecutionStatus::Success { .. }),
        "{:?}",
        effects.status()
    );
    let (child_id, _, _) = effects.created()[0].0;
    // call a function with a vector containing the same owned object as another one passed as
    // a reference argument
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
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
    assert!(format!("{effects:?}").contains("InvalidChildObjectArgument"));
}

#[tokio::test]
#[cfg_attr(msim, ignore)]
async fn test_entry_point_vector_any_error() {
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
        &package.0,
        "entry_point_vector",
        "mint_another_any",
        vec![any_type_tag.clone()],
        vec![TestCallArg::Pure(bcs::to_bytes(&(42_u64)).unwrap())],
    )
    .await
    .unwrap();
    assert!(
        matches!(effects.status(), ExecutionStatus::Success { .. }),
        "{:?}",
        effects.status()
    );
    let (obj_id, _, _) = effects.created()[0].0;
    // call a function with a vector containing one owned object
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
        "entry_point_vector",
        "obj_vec_destroy_any",
        vec![any_type_tag.clone()],
        vec![TestCallArg::ObjVec(vec![obj_id])],
    )
    .await
    .unwrap();
    // should fail as we passed object of the wrong type
    assert!(
        matches!(effects.status(), ExecutionStatus::Failure { .. }),
        "{:?}",
        effects.status()
    );

    // mint two objects - one of a wrong type and one of the correct type
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
        "entry_point_vector",
        "mint_another_any",
        vec![any_type_tag.clone()],
        vec![TestCallArg::Pure(bcs::to_bytes(&(42_u64)).unwrap())],
    )
    .await
    .unwrap();
    assert!(
        matches!(effects.status(), ExecutionStatus::Success { .. }),
        "{:?}",
        effects.status()
    );
    let (wrong_obj_id, _, _) = effects.created()[0].0;
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
        "entry_point_vector",
        "mint_any",
        vec![any_type_tag.clone()],
        vec![TestCallArg::Pure(bcs::to_bytes(&(42_u64)).unwrap())],
    )
    .await
    .unwrap();
    assert!(
        matches!(effects.status(), ExecutionStatus::Success { .. }),
        "{:?}",
        effects.status()
    );
    let (correct_obj_id, _, _) = effects.created()[0].0;
    // call a function with a vector containing one owned object
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
        "entry_point_vector",
        "two_obj_vec_destroy_any",
        vec![any_type_tag.clone()],
        vec![TestCallArg::ObjVec(vec![wrong_obj_id, correct_obj_id])],
    )
    .await
    .unwrap();
    // should fail as we passed object of the wrong type as the first element of the vector
    assert!(
        matches!(effects.status(), ExecutionStatus::Failure { .. }),
        "{:?}",
        effects.status()
    );

    // mint a shared object
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
        "entry_point_vector",
        "mint_shared_any",
        vec![any_type_tag.clone()],
        vec![TestCallArg::Pure(bcs::to_bytes(&(42_u64)).unwrap())],
    )
    .await
    .unwrap();
    assert!(
        matches!(effects.status(), ExecutionStatus::Success { .. }),
        "{:?}",
        effects.status()
    );
    let (shared_obj_id, _, _) = effects.created()[0].0;
    // call a function with a vector containing one shared object
    let effects = call_move_(
        &authority,
        None,
        &gas,
        &sender,
        &sender_key,
        &package.0,
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
        matches!(effects.status(), ExecutionStatus::Failure { .. }),
        "{:?}",
        effects.status()
    );

    // mint an owned object
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
        "entry_point_vector",
        "mint_any",
        vec![any_type_tag.clone()],
        vec![TestCallArg::Pure(bcs::to_bytes(&(42_u64)).unwrap())],
    )
    .await
    .unwrap();
    assert!(
        matches!(effects.status(), ExecutionStatus::Success { .. }),
        "{:?}",
        effects.status()
    );
    let (obj_id, _, _) = effects.created()[0].0;
    // call a function with a vector containing the same owned object as another one passed as
    // argument
    let result = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
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
    assert!(matches!(
        UserInputError::try_from(result.unwrap_err()).unwrap(),
        UserInputError::DuplicateObjectRefInput { .. }
    ));

    // mint an owned object
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
        "entry_point_vector",
        "mint_any",
        vec![any_type_tag.clone()],
        vec![TestCallArg::Pure(bcs::to_bytes(&(42_u64)).unwrap())],
    )
    .await
    .unwrap();
    assert!(
        matches!(effects.status(), ExecutionStatus::Success { .. }),
        "{:?}",
        effects.status()
    );
    let (obj_id, _, _) = effects.created()[0].0;
    // call a function with a vector containing the same owned object as another one passed as
    // a reference argument
    let result = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
        "entry_point_vector",
        "same_objects_ref_any",
        vec![any_type_tag.clone()],
        vec![
            TestCallArg::Object(obj_id),
            TestCallArg::ObjVec(vec![obj_id]),
        ],
    )
    .await;
    assert!(matches!(
        UserInputError::try_from(result.unwrap_err()).unwrap(),
        UserInputError::DuplicateObjectRefInput { .. }
    ));
}

#[tokio::test]
#[cfg_attr(msim, ignore)]
async fn test_entry_point_string() {
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
        &package.0,
        "entry_point_string",
        "ascii_arg",
        vec![],
        vec![TestCallArg::Pure(ascii_str_bcs)],
    )
    .await
    .unwrap();
    assert!(
        matches!(effects.status(), ExecutionStatus::Success { .. }),
        "{:?}",
        effects.status()
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
        &package.0,
        "entry_point_string",
        "utf8_arg",
        vec![],
        vec![TestCallArg::Pure(utf_str_bcs)],
    )
    .await
    .unwrap();
    assert!(
        matches!(effects.status(), ExecutionStatus::Success { .. }),
        "{:?}",
        effects.status()
    );
}

#[tokio::test]
#[cfg_attr(msim, ignore)]
async fn test_entry_point_string_vec() {
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
        &package.0,
        "entry_point_string",
        "utf8_vec_arg",
        vec![],
        vec![TestCallArg::Pure(utf_str_vec_bcs)],
    )
    .await
    .unwrap();
    assert!(
        matches!(effects.status(), ExecutionStatus::Success { .. }),
        "{:?}",
        effects.status()
    );
}

#[tokio::test]
#[cfg_attr(msim, ignore)]
async fn test_entry_point_string_error() {
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
        &package.0,
        "entry_point_string",
        "ascii_arg",
        vec![],
        vec![TestCallArg::Pure(ascii_str_bcs)],
    )
    .await
    .unwrap();
    assert!(
        matches!(effects.status(), ExecutionStatus::Failure { .. }),
        "{:?}",
        effects.status()
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

    let utf8_str_bcs = MoveValue::Struct(MoveStruct::Runtime(vec![MoveValue::Vector(utf8_u8_vec)]))
        .simple_serialize()
        .unwrap();

    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package.0,
        "entry_point_string",
        "utf8_arg",
        vec![],
        vec![TestCallArg::Pure(utf8_str_bcs)],
    )
    .await
    .unwrap();
    assert!(
        matches!(effects.status(), ExecutionStatus::Failure { .. }),
        "{:?}",
        effects.status()
    );
}

#[tokio::test]
#[cfg_attr(msim, ignore)]
async fn test_entry_point_string_vec_error() {
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
        &package.0,
        "entry_point_string",
        "utf8_vec_arg",
        vec![],
        vec![TestCallArg::Pure(utf_str_vec_bcs)],
    )
    .await
    .unwrap();
    assert!(
        matches!(effects.status(), ExecutionStatus::Failure { .. }),
        "{:?}",
        effects.status()
    );
}

#[tokio::test]
#[cfg_attr(msim, ignore)]
async fn test_object_no_id_error() {
    let mut build_config = BuildConfig::new_for_testing();
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
}

#[tokio::test]
#[cfg_attr(msim, ignore)]
async fn test_generate_lock_file() {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["src", "unit_tests", "data", "generate_move_lock_file"]);

    let tmp = tempfile::tempdir().expect("Could not create temp dir for Move.lock");
    let lock_file_path = tmp.path().join("Move.lock");

    let mut build_config = BuildConfig::new_for_testing();
    build_config.config.lock_file = Some(lock_file_path.clone());
    sui_framework::build_move_package(&path, build_config).expect("Move package did not build");

    let mut lock_file_contents = String::new();
    File::open(lock_file_path)
        .expect("Cannot open lock file")
        .read_to_string(&mut lock_file_contents)
        .expect("Error reading Move.lock file");

    let expected = expect![[r##"
        # @generated by Move, please check-in and do not edit manually.

        [move]
        version = 0

        dependencies = [
          { name = "Examples" },
          { name = "Sui" },
        ]

        [[move.package]]
        name = "Examples"
        source = { local = "../object_basics" }

        dependencies = [
          { name = "Sui" },
        ]

        [[move.package]]
        name = "MoveStdlib"
        source = { local = "../../../../../sui-framework/deps/move-stdlib" }

        [[move.package]]
        name = "Sui"
        source = { local = "../../../../../sui-framework" }

        dependencies = [
          { name = "MoveStdlib" },
        ]
    "##]];
    expected.assert_eq(lock_file_contents.as_str());
}

pub async fn build_and_try_publish_test_package(
    authority: &AuthorityState,
    sender: &SuiAddress,
    sender_key: &AccountKeyPair,
    gas_object_id: &ObjectID,
    test_dir: &str,
    gas_budget: u64,
    with_unpublished_deps: bool,
) -> (Transaction, SignedTransactionEffects) {
    let build_config = BuildConfig::new_for_testing();
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("src/unit_tests/data/");
    path.push(test_dir);
    let all_module_bytes = sui_framework::build_move_package(&path, build_config)
        .unwrap()
        .get_package_bytes(with_unpublished_deps);

    let gas_object = authority.get_object(gas_object_id).await.unwrap();
    let gas_object_ref = gas_object.unwrap().compute_object_reference();

    let data = TransactionData::new_module_with_dummy_gas_price(
        *sender,
        gas_object_ref,
        all_module_bytes,
        gas_budget,
    );
    let transaction = to_sender_signed_transaction(data, sender_key);

    (
        transaction.clone().into_inner(),
        send_and_confirm_transaction(authority, transaction)
            .await
            .unwrap()
            .1,
    )
}

pub async fn build_and_publish_test_package(
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
        /* with_unpublished_deps */ false,
    )
    .await
    .1
    .into_data();
    assert!(
        matches!(effects.status(), ExecutionStatus::Success { .. }),
        "{:?}",
        effects.status()
    );
    effects.created()[0].0
}

async fn check_latest_object_ref(
    authority: &AuthorityState,
    object_ref: &ObjectRef,
    expect_not_found: bool,
) {
    let response = authority
        .handle_object_info_request(ObjectInfoRequest {
            object_id: object_ref.0,
            object_format_options: None,
            request_kind: ObjectInfoRequestKind::LatestObjectInfo,
        })
        .await;
    if expect_not_found {
        assert!(matches!(
            UserInputError::try_from(response.unwrap_err()).unwrap(),
            UserInputError::ObjectNotFound { .. },
        ));
    } else {
        assert_eq!(
            &response.unwrap().object.compute_object_reference(),
            object_ref
        );
    }
}
