// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use crate::authority::authority_tests::{
    call_move, init_state_with_ids, send_and_confirm_transaction,
};

use move_package::BuildConfig;
use sui_types::{
    crypto::KeyPair,
    crypto::{get_key_pair, Signature},
    messages::ExecutionStatus,
    object::OBJECT_START_VERSION,
};

use std::env;
use std::path::PathBuf;

const MAX_GAS: u64 = 10000;

#[tokio::test]
async fn test_object_wrapping_unwrapping() {
    let (sender, sender_key) = get_key_pair();
    let gas = ObjectID::random();
    let authority = init_state_with_ids(vec![(sender, gas)]).await;

    let package =
        build_and_publish_test_package(&authority, &sender, &sender_key, &gas, "object_wrapping")
            .await;

    // Create a Child object.
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package,
        "ObjectWrapping",
        "create_child",
        vec![],
        vec![],
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
        "ObjectWrapping",
        "create_parent",
        vec![],
        vec![child_object_ref.0],
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
    println!("before this call");
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package,
        "ObjectWrapping",
        "extract_child",
        vec![],
        vec![parent_object_ref.0],
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
        "ObjectWrapping",
        "set_child",
        vec![],
        vec![parent_object_ref.0, child_object_ref.0],
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
        "ObjectWrapping",
        "delete_parent",
        vec![],
        vec![parent_object_ref.0],
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
}

#[tokio::test]
async fn test_object_owning_another_object() {
    let (sender, sender_key) = get_key_pair();
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
        "ObjectOwner",
        "create_parent",
        vec![],
        vec![],
        vec![],
        vec![],
    )
    .await
    .unwrap();
    assert!(effects.status.is_ok());
    let parent = effects.created[0].0;

    // Create a child.
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package,
        "ObjectOwner",
        "create_child",
        vec![],
        vec![],
        vec![],
        vec![],
    )
    .await
    .unwrap();
    assert!(effects.status.is_ok());
    let child = effects.created[0].0;

    // Mutate the child directly should work fine.
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package,
        "ObjectOwner",
        "mutate_child",
        vec![],
        vec![child.0],
        vec![],
        vec![],
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
        "ObjectOwner",
        "add_child",
        vec![],
        vec![parent.0, child.0],
        vec![],
        vec![],
    )
    .await
    .unwrap();
    assert!(effects.status.is_ok());
    let child_effect = effects
        .mutated
        .iter()
        .find(|((id, _, _), _)| id == &child.0)
        .unwrap();
    // Check that the child is now owned by the parent.
    assert_eq!(child_effect.1, parent.0);

    // Mutate the child directly will now fail because we need the parent to authenticate.
    let result = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package,
        "ObjectOwner",
        "mutate_child",
        vec![],
        vec![child.0],
        vec![],
        vec![],
    )
    .await;
    assert!(result.is_err());

    // Mutate the child with the parent will succeed.
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package,
        "ObjectOwner",
        "mutate_child_with_parent",
        vec![],
        vec![child.0, parent.0],
        vec![],
        vec![],
    )
    .await
    .unwrap();
    assert!(effects.status.is_ok());

    // Create another parent.
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package,
        "ObjectOwner",
        "create_parent",
        vec![],
        vec![],
        vec![],
        vec![],
    )
    .await
    .unwrap();
    assert!(effects.status.is_ok());
    let new_parent = effects.created[0].0;

    // Transfer the child to the new_parent.
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package,
        "ObjectOwner",
        "transfer_child",
        vec![],
        vec![parent.0, child.0, new_parent.0],
        vec![],
        vec![],
    )
    .await
    .unwrap();
    assert!(effects.status.is_ok());
    let child_effect = effects
        .mutated
        .iter()
        .find(|((id, _, _), _)| id == &child.0)
        .unwrap();
    // Check that the child is now owned by the new parent.
    assert_eq!(child_effect.1, new_parent.0);

    // Delete the child. This should fail because the child is still owned by a parent,
    // it cannot yet be deleted.
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package,
        "ObjectOwner",
        "delete_child",
        vec![],
        vec![child.0, new_parent.0],
        vec![],
        vec![],
    )
    .await
    .unwrap();
    assert!(matches!(
        effects.status.unwrap_err().1,
        SuiError::DeleteObjectOwnedObject
    ));

    // Remove the child from the parent.
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package,
        "ObjectOwner",
        "remove_child",
        vec![],
        vec![new_parent.0, child.0],
        vec![],
        vec![],
    )
    .await
    .unwrap();
    assert!(effects.status.is_ok());

    // Delete the child again. This time it will succeed because it's no longer owned by a parent.
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package,
        "ObjectOwner",
        "delete_child",
        vec![],
        vec![child.0, new_parent.0],
        vec![],
        vec![],
    )
    .await
    .unwrap();
    assert!(effects.status.is_ok());

    // Create a parent and a child together. This tests the
    // Transfer::transfer_to_object_id() API.
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package,
        "ObjectOwner",
        "create_parent_and_child",
        vec![],
        vec![],
        vec![],
        vec![],
    )
    .await
    .unwrap();
    assert!(effects.status.is_ok());
    assert_eq!(effects.created.len(), 2);
    // Check that one of them is the parent and the other is the child.
    let (parent, child) = if effects.created[0].1 == sender {
        assert_eq!(effects.created[1].1, effects.created[0].0 .0);
        (effects.created[0].0, effects.created[1].0)
    } else {
        assert!(effects.created[1].1 == sender && effects.created[0].1 == effects.created[1].0 .0);
        (effects.created[1].0, effects.created[0].0)
    };

    // Delete the parent and child altogether.
    let effects = call_move(
        &authority,
        &gas,
        &sender,
        &sender_key,
        &package,
        "ObjectOwner",
        "delete_parent_and_child",
        vec![],
        vec![parent.0, child.0],
        vec![],
        vec![],
    )
    .await
    .unwrap();
    assert!(effects.status.is_ok());
    // Check that both objects were deleted.
    assert_eq!(effects.deleted.len(), 2);
}

async fn build_and_publish_test_package(
    authority: &AuthorityState,
    sender: &SuiAddress,
    sender_key: &KeyPair,
    gas_object_id: &ObjectID,
    test_dir: &str,
) -> ObjectRef {
    let build_config = BuildConfig::default();
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("src/unit_tests/data/");
    path.push(test_dir);
    let modules = sui_framework::build_move_package(&path, build_config, false).unwrap();

    let all_module_bytes = modules
        .iter()
        .map(|m| {
            let mut module_bytes = Vec::new();
            m.serialize(&mut module_bytes).unwrap();
            module_bytes
        })
        .collect();

    let gas_object = authority.get_object(gas_object_id).await.unwrap();
    let gas_object_ref = gas_object.unwrap().to_object_reference();

    let data = TransactionData::new_module(*sender, gas_object_ref, all_module_bytes, MAX_GAS);
    let signature = Signature::new(&data, &*sender_key);
    let transaction = Transaction::new(data, signature);
    let effects = send_and_confirm_transaction(authority, transaction)
        .await
        .unwrap()
        .signed_effects
        .unwrap()
        .effects;

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
