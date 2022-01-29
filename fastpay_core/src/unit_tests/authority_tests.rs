// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use bcs;
use fastx_adapter::genesis;
use fastx_types::{
    base_types::dbg_addr,
    gas::{calculate_module_publish_cost, get_gas_balance},
    messages::ExecutionStatus,
    object::OBJECT_START_VERSION,
};
use move_binary_format::{
    file_format::{self, AddressIdentifierIndex, IdentifierIndex, ModuleHandle},
    CompiledModule,
};
use move_core_types::{ident_str, identifier::Identifier};
use move_package::BuildConfig;

use std::fs;
use std::path::PathBuf;
use std::{convert::TryInto, env};

pub fn system_maxfiles() -> usize {
    fdlimit::raise_fd_limit().unwrap_or(256u64) as usize
}

fn max_files_authority_tests() -> i32 {
    (system_maxfiles() / 8).try_into().unwrap()
}

const MAX_GAS: u64 = 10000;

// Only relevant in a ser/de context : the `CertifiedOrder` for a transaction is not unique
fn compare_certified_orders(o1: &CertifiedOrder, o2: &CertifiedOrder) {
    assert_eq!(o1.order.digest(), o2.order.digest());
    // in this ser/de context it's relevant to compare signatures
    assert_eq!(o1.signatures, o2.signatures);
}

// Only relevant in a ser/de context : the `CertifiedOrder` for a transaction is not unique
fn compare_order_info_responses(o1: &OrderInfoResponse, o2: &OrderInfoResponse) {
    assert_eq!(o1.signed_order, o2.signed_order);
    assert_eq!(o1.signed_effects, o2.signed_effects);
    match (o1.certified_order.as_ref(), o2.certified_order.as_ref()) {
        (Some(cert1), Some(cert2)) => {
            assert_eq!(cert1.order.digest(), cert2.order.digest());
            assert_eq!(cert1.signatures, cert2.signatures);
        }
        (None, None) => (),
        _ => panic!("certificate structure between responses differs"),
    }
}

#[tokio::test]
async fn test_handle_transfer_order_bad_signature() {
    let (sender, sender_key) = get_key_pair();
    let recipient = Address::FastPay(dbg_addr(2));
    let object_id = ObjectID::random();
    let gas_object_id = ObjectID::random();
    let authority_state =
        init_state_with_ids(vec![(sender, object_id), (sender, gas_object_id)]).await;
    let object = authority_state.object_state(&object_id).await.unwrap();
    let gas_object = authority_state.object_state(&gas_object_id).await.unwrap();
    let transfer_order = init_transfer_order(
        sender,
        &sender_key,
        recipient,
        object.to_object_reference(),
        gas_object.to_object_reference(),
    );
    let object_id = *transfer_order.object_id();
    let (_unknown_address, unknown_key) = get_key_pair();
    let mut bad_signature_transfer_order = transfer_order.clone();
    bad_signature_transfer_order.signature = Signature::new(&transfer_order.kind, &unknown_key);
    assert!(authority_state
        .handle_order(bad_signature_transfer_order)
        .await
        .is_err());

    let object = authority_state.object_state(&object_id).await.unwrap();
    assert!(authority_state
        .get_order_lock(&object.to_object_reference())
        .await
        .unwrap()
        .is_none());

    assert!(authority_state
        .get_order_lock(&object.to_object_reference())
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn test_handle_transfer_order_unknown_sender() {
    let (sender, sender_key) = get_key_pair();
    let (unknown_address, unknown_key) = get_key_pair();
    let object_id: ObjectID = ObjectID::random();
    let gas_object_id = ObjectID::random();
    let recipient = Address::FastPay(dbg_addr(2));
    let authority_state =
        init_state_with_ids(vec![(sender, object_id), (sender, gas_object_id)]).await;
    let object = authority_state.object_state(&object_id).await.unwrap();
    let gas_object = authority_state.object_state(&gas_object_id).await.unwrap();

    let transfer_order = init_transfer_order(
        unknown_address,
        &sender_key,
        recipient,
        object.to_object_reference(),
        gas_object.to_object_reference(),
    );

    let unknown_sender_transfer = transfer_order.kind;
    let unknown_sender_transfer_order = Order::new(unknown_sender_transfer, &unknown_key);
    assert!(authority_state
        .handle_order(unknown_sender_transfer_order)
        .await
        .is_err());

    let object = authority_state.object_state(&object_id).await.unwrap();
    assert!(authority_state
        .get_order_lock(&object.to_object_reference())
        .await
        .unwrap()
        .is_none());

    assert!(authority_state
        .get_order_lock(&object.to_object_reference())
        .await
        .unwrap()
        .is_none());
}

/* FIXME: This tests the submission of out of order certs, but modifies object sequence numbers manually
   and leaves the authority in an inconsistent state. We should re-code it in a proper way.

#[test]
fn test_handle_transfer_order_bad_sequence_number() {
    let (sender, sender_key) = get_key_pair();
    let object_id: ObjectID = random_object_id();
    let recipient = Address::FastPay(dbg_addr(2));
    let authority_state = init_state_with_object(sender, object_id);
    let transfer_order = init_transfer_order(sender, &sender_key, recipient, object_id);

    let mut sequence_number_state = authority_state;
    let sequence_number_state_sender_account =
        sequence_number_state.objects.get_mut(&object_id).unwrap();
    sequence_number_state_sender_account.version() =
        sequence_number_state_sender_account
            .version()
            .increment()
            .unwrap();
    assert!(sequence_number_state
        .handle_transfer_order(transfer_order)
        .is_err());

        let object = sequence_number_state.objects.get(&object_id).unwrap();
        assert!(sequence_number_state.get_order_lock(object.id, object.version()).unwrap().is_none());
}
*/

#[tokio::test]
async fn test_handle_transfer_order_ok() {
    let (sender, sender_key) = get_key_pair();
    let recipient = Address::FastPay(dbg_addr(2));
    let object_id = ObjectID::random();
    let gas_object_id = ObjectID::random();
    let authority_state =
        init_state_with_ids(vec![(sender, object_id), (sender, gas_object_id)]).await;
    let object = authority_state.object_state(&object_id).await.unwrap();
    let gas_object = authority_state.object_state(&gas_object_id).await.unwrap();
    let transfer_order = init_transfer_order(
        sender,
        &sender_key,
        recipient,
        object.to_object_reference(),
        gas_object.to_object_reference(),
    );

    let test_object = authority_state.object_state(&object_id).await.unwrap();

    // Check the initial state of the locks
    assert!(authority_state
        .get_order_lock(&(object_id, 0.into(), test_object.digest()))
        .await
        .unwrap()
        .is_none());
    assert!(authority_state
        .get_order_lock(&(object_id, 1.into(), test_object.digest()))
        .await
        .is_err());

    let account_info = authority_state
        .handle_order(transfer_order.clone())
        .await
        .unwrap();

    let object = authority_state.object_state(&object_id).await.unwrap();
    let pending_confirmation = authority_state
        .get_order_lock(&object.to_object_reference())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(account_info.signed_order.unwrap(), pending_confirmation);

    // Check the final state of the locks
    assert!(authority_state
        .get_order_lock(&(object_id, 0.into(), object.digest()))
        .await
        .unwrap()
        .is_some());
    assert_eq!(
        authority_state
            .get_order_lock(&(object_id, 0.into(), object.digest()))
            .await
            .unwrap()
            .as_ref()
            .unwrap()
            .order
            .kind,
        transfer_order.kind
    );
}

#[tokio::test]
async fn test_handle_transfer_zero_balance() {
    let (sender, sender_key) = get_key_pair();
    let recipient = Address::FastPay(dbg_addr(2));
    let object_id = ObjectID::random();
    let authority_state = init_state_with_ids(vec![(sender, object_id)]).await;
    let object = authority_state.object_state(&object_id).await.unwrap();

    // Create a gas object with 0 balance.
    let gas_object_id = ObjectID::random();
    let gas_object =
        Object::with_id_owner_gas_for_testing(gas_object_id, SequenceNumber::new(), sender, 0);
    authority_state
        .init_order_lock((gas_object_id, 0.into(), gas_object.digest()))
        .await;
    let gas_object_ref = gas_object.to_object_reference();
    authority_state.insert_object(gas_object).await;

    let transfer_order = init_transfer_order(
        sender,
        &sender_key,
        recipient,
        object.to_object_reference(),
        gas_object_ref,
    );

    let result = authority_state.handle_order(transfer_order.clone()).await;
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Gas balance is 0, smaller than minimum requirement of 8 for object transfer."));
}

async fn send_and_confirm_order(
    authority: &mut AuthorityState,
    order: Order,
) -> Result<OrderInfoResponse, FastPayError> {
    // Make the initial request
    let response = authority.handle_order(order.clone()).await.unwrap();
    let vote = response.signed_order.unwrap();

    // Collect signatures from a quorum of authorities
    let mut builder = SignatureAggregator::try_new(order, &authority.committee).unwrap();
    let certificate = builder
        .append(vote.authority, vote.signature)
        .unwrap()
        .unwrap();
    // Submit the confirmation. *Now* execution actually happens, and it should fail when we try to look up our dummy module.
    // we unfortunately don't get a very descriptive error message, but we can at least see that something went wrong inside the VM
    authority
        .handle_confirmation_order(ConfirmationOrder::new(certificate))
        .await
}

/// Create a `CompiledModule` that depends on `m`
fn make_dependent_module(m: &CompiledModule) -> CompiledModule {
    let mut dependent_module = file_format::empty_module();
    dependent_module
        .identifiers
        .push(m.self_id().name().to_owned());
    dependent_module
        .address_identifiers
        .push(*m.self_id().address());
    dependent_module.module_handles.push(ModuleHandle {
        address: AddressIdentifierIndex((dependent_module.address_identifiers.len() - 1) as u16),
        name: IdentifierIndex((dependent_module.identifiers.len() - 1) as u16),
    });
    dependent_module
}

#[cfg(test)]
fn check_gas_object(
    gas_object: &Object,
    expected_balance: u64,
    expected_sequence_number: SequenceNumber,
) {
    assert_eq!(gas_object.version(), expected_sequence_number);
    let new_balance = get_gas_balance(gas_object).unwrap();
    assert_eq!(new_balance, expected_balance);
}

// Test that publishing a module that depends on an existing one works
#[tokio::test]
async fn test_publish_dependent_module_ok() {
    let (sender, sender_key) = get_key_pair();
    let gas_payment_object_id = ObjectID::random();
    let gas_payment_object = Object::with_id_owner_for_testing(gas_payment_object_id, sender);
    let gas_payment_object_ref = gas_payment_object.to_object_reference();
    // create a genesis state that contains the gas object and genesis modules
    let (genesis_module_objects, _) = genesis::clone_genesis_data();
    let genesis_module = match &genesis_module_objects[0].data {
        Data::Package(m) => CompiledModule::deserialize(m.values().next().unwrap()).unwrap(),
        _ => unreachable!(),
    };
    // create a module that depends on a genesis module
    let dependent_module = make_dependent_module(&genesis_module);
    let dependent_module_bytes = {
        let mut bytes = Vec::new();
        dependent_module.serialize(&mut bytes).unwrap();
        bytes
    };
    let mut authority = init_state_with_objects(vec![gas_payment_object]).await;

    let order = Order::new_module(
        sender,
        gas_payment_object_ref,
        vec![dependent_module_bytes],
        &sender_key,
    );
    let dependent_module_id = TxContext::new(&sender, order.digest()).fresh_id();

    // Object does not exist
    assert!(authority.object_state(&dependent_module_id).await.is_err());
    let response = send_and_confirm_order(&mut authority, order).await.unwrap();
    response.signed_effects.unwrap().effects.status.unwrap();

    // check that the dependent module got published
    assert!(authority.object_state(&dependent_module_id).await.is_ok());
}

// Test that publishing a module with no dependencies works
#[tokio::test]
async fn test_publish_module_no_dependencies_ok() {
    let (sender, sender_key) = get_key_pair();
    let gas_payment_object_id = ObjectID::random();
    let gas_balance = MAX_GAS;
    let gas_seq = SequenceNumber::new();
    let gas_payment_object =
        Object::with_id_owner_gas_for_testing(gas_payment_object_id, gas_seq, sender, gas_balance);
    let gas_payment_object_ref = gas_payment_object.to_object_reference();
    let mut authority = init_state_with_objects(vec![gas_payment_object]).await;

    let module = file_format::empty_module();
    let mut module_bytes = Vec::new();
    module.serialize(&mut module_bytes).unwrap();
    let module_bytes = vec![module_bytes];
    let gas_cost = calculate_module_publish_cost(&module_bytes);
    let order = Order::new_module(sender, gas_payment_object_ref, module_bytes, &sender_key);
    let _module_object_id = TxContext::new(&sender, order.digest()).fresh_id();
    let response = send_and_confirm_order(&mut authority, order).await.unwrap();
    response.signed_effects.unwrap().effects.status.unwrap();

    // check that the module actually got published
    assert!(response.certified_order.is_some());

    // Check that gas is properly deducted.
    let gas_payment_object = authority
        .object_state(&gas_payment_object_id)
        .await
        .unwrap();
    check_gas_object(
        &gas_payment_object,
        gas_balance - gas_cost,
        gas_seq.increment(),
    )
}

// Test the case when the gas provided is less than minimum requirement during module publish.
// Note that the case where gas is insufficient to publish the module is tested
// separately in the adapter tests.
#[tokio::test]
async fn test_publish_module_insufficient_gas() {
    let (sender, sender_key) = get_key_pair();
    let gas_payment_object_id = ObjectID::random();
    let gas_balance = 9;
    let gas_payment_object = Object::with_id_owner_gas_for_testing(
        gas_payment_object_id,
        SequenceNumber::new(),
        sender,
        gas_balance,
    );
    let gas_payment_object_ref = gas_payment_object.to_object_reference();
    let authority = init_state_with_objects(vec![gas_payment_object]).await;

    let module = file_format::empty_module();
    let mut module_bytes = Vec::new();
    module.serialize(&mut module_bytes).unwrap();
    let module_bytes = vec![module_bytes];
    let order = Order::new_module(sender, gas_payment_object_ref, module_bytes, &sender_key);
    let response = authority.handle_order(order.clone()).await.unwrap_err();
    assert!(response
        .to_string()
        .contains("Gas balance is 9, smaller than minimum requirement of 10 for module publish"));
}

#[tokio::test]
async fn test_handle_move_order() {
    let (sender, sender_key) = get_key_pair();
    let gas_payment_object_id = ObjectID::random();
    let gas_balance = MAX_GAS;
    let gas_seq = SequenceNumber::new();
    let gas_payment_object =
        Object::with_id_owner_gas_for_testing(gas_payment_object_id, gas_seq, sender, gas_balance);
    let gas_payment_object_ref = gas_payment_object.to_object_reference();
    // find the function Object::create and call it to create a new object
    let (mut genesis_package_objects, native_functions) = genesis::clone_genesis_data();
    let package_object_ref =
        get_genesis_package_by_module(&genesis_package_objects, "ObjectBasics");

    genesis_package_objects.push(gas_payment_object);
    let mut authority_state = init_state_with_objects(genesis_package_objects).await;
    authority_state._native_functions = native_functions.clone();
    authority_state.move_vm = adapter::new_move_vm(native_functions).unwrap();

    let function = ident_str!("create").to_owned();
    let order = Order::new_move_call(
        sender,
        package_object_ref,
        ident_str!("ObjectBasics").to_owned(),
        function,
        Vec::new(),
        gas_payment_object_ref,
        Vec::new(),
        vec![
            16u64.to_le_bytes().to_vec(),
            bcs::to_bytes(&sender.to_vec()).unwrap(),
        ],
        MAX_GAS,
        &sender_key,
    );
    // If the number changes, we want to verify that the change is intended.
    let gas_cost = 62;
    let effects = send_and_confirm_order(&mut authority_state, order)
        .await
        .unwrap()
        .signed_effects
        .unwrap()
        .effects;

    assert_eq!(effects.status, ExecutionStatus::Success);
    assert_eq!(effects.created.len(), 1);
    assert!(effects.mutated.is_empty());

    let created_object_id = effects.created[0].0 .0;
    // check that order actually created an object with the expected ID, owner, sequence number
    let created_obj = authority_state
        .object_state(&created_object_id)
        .await
        .unwrap();
    assert_eq!(created_obj.owner, sender,);
    assert_eq!(created_obj.id(), created_object_id);
    assert_eq!(created_obj.version(), OBJECT_START_VERSION);

    // Check that gas is properly deducted.
    let gas_payment_object = authority_state
        .object_state(&gas_payment_object_id)
        .await
        .unwrap();
    check_gas_object(
        &gas_payment_object,
        gas_balance - gas_cost,
        gas_seq.increment(),
    )
}

// Test the case when the gas budget provided is less than minimum requirement during move call.
// Note that the case where gas is insufficient to execute move bytecode is tested
// separately in the adapter tests.
#[tokio::test]
async fn test_handle_move_order_insufficient_budget() {
    let (sender, sender_key) = get_key_pair();
    let gas_payment_object_id = ObjectID::random();
    let gas_payment_object = Object::with_id_owner_for_testing(gas_payment_object_id, sender);
    let gas_payment_object_ref = gas_payment_object.to_object_reference();
    // find the function Object::create and call it to create a new object
    let (mut genesis_package_objects, native_functions) = genesis::clone_genesis_data();
    let package_object_ref =
        get_genesis_package_by_module(&genesis_package_objects, "ObjectBasics");

    genesis_package_objects.push(gas_payment_object);
    let mut authority_state = init_state_with_objects(genesis_package_objects).await;
    authority_state._native_functions = native_functions.clone();
    authority_state.move_vm = adapter::new_move_vm(native_functions).unwrap();

    let function = ident_str!("create").to_owned();
    let order = Order::new_move_call(
        sender,
        package_object_ref,
        ident_str!("ObjectBasics").to_owned(),
        function,
        Vec::new(),
        gas_payment_object_ref,
        Vec::new(),
        vec![
            16u64.to_le_bytes().to_vec(),
            bcs::to_bytes(&sender.to_vec()).unwrap(),
        ],
        9,
        &sender_key,
    );
    let response = authority_state
        .handle_order(order.clone())
        .await
        .unwrap_err();
    assert!(response
        .to_string()
        .contains("Gas budget is 9, smaller than minimum requirement of 10 for move call"));
}

#[tokio::test]
async fn test_handle_transfer_order_double_spend() {
    let (sender, sender_key) = get_key_pair();
    let recipient = Address::FastPay(dbg_addr(2));
    let object_id = ObjectID::random();
    let gas_object_id = ObjectID::random();
    let authority_state =
        init_state_with_ids(vec![(sender, object_id), (sender, gas_object_id)]).await;
    let object = authority_state.object_state(&object_id).await.unwrap();
    let gas_object = authority_state.object_state(&gas_object_id).await.unwrap();
    let transfer_order = init_transfer_order(
        sender,
        &sender_key,
        recipient,
        object.to_object_reference(),
        gas_object.to_object_reference(),
    );

    let signed_order = authority_state
        .handle_order(transfer_order.clone())
        .await
        .unwrap();
    // calls to handlers are idempotent -- returns the same.
    let double_spend_signed_order = authority_state.handle_order(transfer_order).await.unwrap();
    // this is valid because our test authority should not change its certified order
    compare_order_info_responses(&signed_order, &double_spend_signed_order);
}

#[tokio::test]
async fn test_handle_confirmation_order_unknown_sender() {
    let recipient = dbg_addr(2);
    let (sender, sender_key) = get_key_pair();
    let authority_state = init_state().await;

    let object = Object::with_id_owner_for_testing(
        ObjectID::random(),
        FastPayAddress::random_for_testing_only(),
    );
    let gas_object = Object::with_id_owner_for_testing(
        ObjectID::random(),
        FastPayAddress::random_for_testing_only(),
    );

    let certified_transfer_order = init_certified_transfer_order(
        sender,
        &sender_key,
        Address::FastPay(recipient),
        object.to_object_reference(),
        gas_object.to_object_reference(),
        &authority_state,
    );

    assert!(authority_state
        .handle_confirmation_order(ConfirmationOrder::new(certified_transfer_order))
        .await
        .is_err());
}

#[tokio::test]
async fn test_handle_confirmation_order_bad_sequence_number() {
    // TODO: refactor this test to be less magic:
    // * Create an explicit state within an authority, by passing objects.
    // * Create an explicit transfer, and execute it.
    // * Then try to execute it again.

    let (sender, sender_key) = get_key_pair();
    let object_id: ObjectID = ObjectID::random();
    let recipient = dbg_addr(2);
    let gas_object_id = ObjectID::random();
    let authority_state =
        init_state_with_ids(vec![(sender, object_id), (sender, gas_object_id)]).await;
    let object = authority_state.object_state(&object_id).await.unwrap();
    let gas_object = authority_state.object_state(&gas_object_id).await.unwrap();

    // Record the old sequence number
    let old_seq_num;
    {
        let old_account = authority_state.object_state(&object_id).await.unwrap();
        old_seq_num = old_account.version();
    }

    let certified_transfer_order = init_certified_transfer_order(
        sender,
        &sender_key,
        Address::FastPay(recipient),
        object.to_object_reference(),
        gas_object.to_object_reference(),
        &authority_state,
    );

    // Increment the sequence number
    {
        let mut sender_object = authority_state.object_state(&object_id).await.unwrap();
        let o = sender_object.data.try_as_move_mut().unwrap();
        let old_contents = o.contents().to_vec();
        // update object contents, which will increment the sequence number
        o.update_contents(old_contents);
        authority_state.insert_object(sender_object).await;
    }

    // Explanation: providing an old cert that has already need applied
    //              returns a Ok(_) with info about the new object states.
    let response = authority_state
        .handle_confirmation_order(ConfirmationOrder::new(certified_transfer_order))
        .await
        .unwrap();
    assert!(response.signed_effects.is_none());

    // Check that the new object is the one recorded.
    let new_object = authority_state.object_state(&object_id).await.unwrap();
    assert_eq!(old_seq_num.increment(), new_object.version());

    // No recipient object was created.
    assert!(authority_state
        .object_state(&dbg_object_id(2))
        .await
        .is_err());
}

#[tokio::test]
async fn test_handle_confirmation_order_receiver_equal_sender() {
    let (address, key) = get_key_pair();
    let object_id: ObjectID = ObjectID::random();
    let gas_object_id = ObjectID::random();
    let authority_state =
        init_state_with_ids(vec![(address, object_id), (address, gas_object_id)]).await;
    let object = authority_state.object_state(&object_id).await.unwrap();
    let gas_object = authority_state.object_state(&gas_object_id).await.unwrap();

    let certified_transfer_order = init_certified_transfer_order(
        address,
        &key,
        Address::FastPay(address),
        object.to_object_reference(),
        gas_object.to_object_reference(),
        &authority_state,
    );
    let response = authority_state
        .handle_confirmation_order(ConfirmationOrder::new(certified_transfer_order))
        .await
        .unwrap();
    response.signed_effects.unwrap().effects.status.unwrap();
    let account = authority_state.object_state(&object_id).await.unwrap();
    assert_eq!(OBJECT_START_VERSION, account.version());

    assert!(authority_state
        .parent(&(object_id, account.version(), account.digest()))
        .await
        .is_some());
}

#[tokio::test]
async fn test_handle_confirmation_order_gas() {
    let run_test_with_gas = |gas: u64| async move {
        let (sender, sender_key) = get_key_pair();
        let recipient = dbg_addr(2);
        let object_id = ObjectID::random();
        let authority_state = init_state_with_ids(vec![(sender, object_id)]).await;
        let object = authority_state.object_state(&object_id).await.unwrap();

        // Create a gas object with insufficient balance.
        let gas_object_id = ObjectID::random();
        let gas_object = Object::with_id_owner_gas_for_testing(
            gas_object_id,
            SequenceNumber::new(),
            sender,
            gas,
        );
        authority_state
            .init_order_lock((gas_object_id, 0.into(), gas_object.digest()))
            .await;
        let gas_object_ref = gas_object.to_object_reference();
        authority_state.insert_object(gas_object).await;

        let certified_transfer_order = init_certified_transfer_order(
            sender,
            &sender_key,
            Address::FastPay(recipient),
            object.to_object_reference(),
            gas_object_ref,
            &authority_state,
        );

        authority_state
            .handle_confirmation_order(ConfirmationOrder::new(certified_transfer_order.clone()))
            .await
            .unwrap()
            .signed_effects
            .unwrap()
            .effects
            .status
    };
    let result = run_test_with_gas(10).await;
    let err_string = result.unwrap_err().1.to_string();
    assert!(err_string.contains("Gas balance is 10, not enough to pay 18"));
    // This will execute sufccessfully.
    let result = run_test_with_gas(20).await;
    result.unwrap();
}

#[tokio::test]
async fn test_handle_confirmation_order_ok() {
    let (sender, sender_key) = get_key_pair();
    let recipient = dbg_addr(2);
    let object_id = ObjectID::random();
    let gas_object_id = ObjectID::random();
    let authority_state =
        init_state_with_ids(vec![(sender, object_id), (sender, gas_object_id)]).await;
    let object = authority_state.object_state(&object_id).await.unwrap();
    let gas_object = authority_state.object_state(&gas_object_id).await.unwrap();

    let certified_transfer_order = init_certified_transfer_order(
        sender,
        &sender_key,
        Address::FastPay(recipient),
        object.to_object_reference(),
        gas_object.to_object_reference(),
        &authority_state,
    );

    let old_account = authority_state.object_state(&object_id).await.unwrap();
    let mut next_sequence_number = old_account.version();
    next_sequence_number = next_sequence_number.increment();

    let info = authority_state
        .handle_confirmation_order(ConfirmationOrder::new(certified_transfer_order.clone()))
        .await
        .unwrap();
    info.signed_effects.unwrap().effects.status.unwrap();
    // Key check: the ownership has changed

    let new_account = authority_state.object_state(&object_id).await.unwrap();
    assert_eq!(recipient, new_account.owner);
    assert_eq!(next_sequence_number, new_account.version());
    assert_eq!(None, info.signed_order);
    let opt_cert = {
        let refx = authority_state
            .parent(&(object_id, new_account.version(), new_account.digest()))
            .await
            .unwrap();
        authority_state.read_certificate(&refx).await.unwrap()
    };
    if let Some(certified_order) = opt_cert {
        // valid since our test authority should not update its certificate set
        compare_certified_orders(&certified_order, &certified_transfer_order);
    } else {
        panic!("parent certificate not avaailable from the authority!");
    }

    // Check locks are set and archived correctly
    assert!(authority_state
        .get_order_lock(&(object_id, 0.into(), old_account.digest()))
        .await
        .is_err());
    assert!(authority_state
        .get_order_lock(&(object_id, 1.into(), new_account.digest()))
        .await
        .expect("Exists")
        .is_none());

    // Check that all the parents are returned.
    assert!(
        authority_state.get_parent_iterator(object_id, None).await
            == Ok(vec![(
                (object_id, 1.into(), new_account.digest()),
                certified_transfer_order.order.digest()
            )])
    );
}

#[tokio::test]
async fn test_handle_confirmation_order_idempotent() {
    let (sender, sender_key) = get_key_pair();
    let recipient = dbg_addr(2);
    let object_id = ObjectID::random();
    let gas_object_id = ObjectID::random();
    let authority_state =
        init_state_with_ids(vec![(sender, object_id), (sender, gas_object_id)]).await;
    let object = authority_state.object_state(&object_id).await.unwrap();
    let gas_object = authority_state.object_state(&gas_object_id).await.unwrap();

    let certified_transfer_order = init_certified_transfer_order(
        sender,
        &sender_key,
        Address::FastPay(recipient),
        object.to_object_reference(),
        gas_object.to_object_reference(),
        &authority_state,
    );

    let info = authority_state
        .handle_confirmation_order(ConfirmationOrder::new(certified_transfer_order.clone()))
        .await
        .unwrap();
    assert_eq!(
        info.signed_effects.as_ref().unwrap().effects.status,
        ExecutionStatus::Success
    );

    let info2 = authority_state
        .handle_confirmation_order(ConfirmationOrder::new(certified_transfer_order.clone()))
        .await
        .unwrap();
    assert_eq!(
        info2.signed_effects.as_ref().unwrap().effects.status,
        ExecutionStatus::Success
    );

    // this is valid because we're checking the authority state does not change the certificate
    compare_order_info_responses(&info, &info2);
}

#[tokio::test]
async fn test_move_call_mutable_object_not_mutated() {
    let (sender, sender_key) = get_key_pair();
    let gas_object_id = ObjectID::random();
    let mut authority_state = init_state_with_ids(vec![(sender, gas_object_id)]).await;

    let (genesis_package_objects, _) = genesis::clone_genesis_data();
    let package_object_ref =
        get_genesis_package_by_module(&genesis_package_objects, "ObjectBasics");

    let gas_object_ref = authority_state
        .object_state(&gas_object_id)
        .await
        .unwrap()
        .to_object_reference();
    let order = Order::new_move_call(
        sender,
        package_object_ref,
        ident_str!("ObjectBasics").to_owned(),
        ident_str!("create").to_owned(),
        Vec::new(),
        gas_object_ref,
        Vec::new(),
        vec![
            16u64.to_le_bytes().to_vec(),
            bcs::to_bytes(&sender.to_vec()).unwrap(),
        ],
        1000,
        &sender_key,
    );
    let effects = send_and_confirm_order(&mut authority_state, order)
        .await
        .unwrap()
        .signed_effects
        .unwrap()
        .effects;
    assert_eq!(effects.status, ExecutionStatus::Success);
    assert_eq!((effects.created.len(), effects.mutated.len()), (1, 0));
    let new_object_ref1 = effects.created[0].0;

    let gas_object_ref = effects.gas_object.0;
    let order = Order::new_move_call(
        sender,
        package_object_ref,
        ident_str!("ObjectBasics").to_owned(),
        ident_str!("create").to_owned(),
        Vec::new(),
        gas_object_ref,
        Vec::new(),
        vec![
            16u64.to_le_bytes().to_vec(),
            bcs::to_bytes(&sender.to_vec()).unwrap(),
        ],
        1000,
        &sender_key,
    );
    let effects = send_and_confirm_order(&mut authority_state, order)
        .await
        .unwrap()
        .signed_effects
        .unwrap()
        .effects;
    assert_eq!(effects.status, ExecutionStatus::Success);
    assert_eq!((effects.created.len(), effects.mutated.len()), (1, 0));
    let new_object_ref2 = effects.created[0].0;

    let gas_object_ref = effects.gas_object.0;
    let order = Order::new_move_call(
        sender,
        package_object_ref,
        ident_str!("ObjectBasics").to_owned(),
        ident_str!("update").to_owned(),
        Vec::new(),
        gas_object_ref,
        vec![new_object_ref1, new_object_ref2],
        vec![],
        1000,
        &sender_key,
    );
    let effects = send_and_confirm_order(&mut authority_state, order)
        .await
        .unwrap()
        .signed_effects
        .unwrap()
        .effects;
    assert_eq!(effects.status, ExecutionStatus::Success);
    assert_eq!((effects.created.len(), effects.mutated.len()), (0, 2));
    // Verify that both objects' version increased, even though only one object was updated.
    assert_eq!(
        authority_state
            .object_state(&new_object_ref1.0)
            .await
            .unwrap()
            .version(),
        new_object_ref1.1.increment()
    );
    assert_eq!(
        authority_state
            .object_state(&new_object_ref2.0)
            .await
            .unwrap()
            .version(),
        new_object_ref2.1.increment()
    );
}

#[tokio::test]
async fn test_account_state_ok() {
    let sender = dbg_addr(1);
    let object_id = dbg_object_id(1);

    let authority_state = init_state_with_object_id(sender, object_id).await;
    authority_state.object_state(&object_id).await.unwrap();
}

#[tokio::test]
async fn test_account_state_unknown_account() {
    let sender = dbg_addr(1);
    let unknown_address = dbg_object_id(99);
    let authority_state = init_state_with_object_id(sender, ObjectID::random()).await;
    assert!(authority_state
        .object_state(&unknown_address)
        .await
        .is_err());
}

#[tokio::test]
async fn test_authority_persist() {
    let (authority_address, authority_key) = get_key_pair();
    let mut authorities = BTreeMap::new();
    authorities.insert(
        /* address */ authority_address,
        /* voting right */ 1,
    );
    let committee = Committee::new(authorities);

    // Create a random directory to store the DB
    let dir = env::temp_dir();
    let path = dir.join(format!("DB_{:?}", ObjectID::random()));
    fs::create_dir(&path).unwrap();

    // Create an authority
    let mut opts = rocksdb::Options::default();
    opts.set_max_open_files(max_files_authority_tests());
    let store = Arc::new(AuthorityStore::open(&path, Some(opts)));
    let authority = AuthorityState::new_without_genesis_for_testing(
        committee.clone(),
        authority_address,
        authority_key.copy(),
        store,
    );

    // Create an object
    let recipient = dbg_addr(2);
    let object_id = ObjectID::random();
    let obj = Object::with_id_owner_for_testing(object_id, recipient);

    // Store an object
    authority
        .init_order_lock((object_id, 0.into(), obj.digest()))
        .await;
    authority.insert_object(obj).await;

    // Close the authority
    drop(authority);

    // Reopen the authority with the same path
    let mut opts = rocksdb::Options::default();
    opts.set_max_open_files(max_files_authority_tests());
    let store = Arc::new(AuthorityStore::open(&path, Some(opts)));
    let authority2 = AuthorityState::new_without_genesis_for_testing(
        committee,
        authority_address,
        authority_key,
        store,
    );
    let obj2 = authority2.object_state(&object_id).await.unwrap();

    // Check the object is present
    assert_eq!(obj2.id(), object_id);
    assert_eq!(obj2.owner, recipient);
}

async fn call_move(
    authority: &mut AuthorityState,
    gas_object_id: &ObjectID,
    sender: &PublicKeyBytes,
    sender_key: &KeyPair,
    package: &ObjectRef,
    module: Identifier,
    function: Identifier,
    object_arg_ids: Vec<ObjectID>,
    pure_args: Vec<Vec<u8>>,
) -> OrderEffects {
    let gas_object = authority.object_state(gas_object_id).await.unwrap();
    let gas_object_ref = gas_object.to_object_reference();
    let mut object_args = vec![];
    for id in object_arg_ids {
        object_args.push(
            authority
                .object_state(&id)
                .await
                .unwrap()
                .to_object_reference(),
        );
    }
    let order = Order::new_move_call(
        *sender,
        *package,
        module,
        function,
        vec![],
        gas_object_ref,
        object_args,
        pure_args,
        MAX_GAS,
        sender_key,
    );
    let response = send_and_confirm_order(authority, order).await.unwrap();
    response.signed_effects.unwrap().effects
}

#[tokio::test]
async fn test_hero() {
    // 1. Compile the Hero Move code.
    let build_config = BuildConfig::default();
    let mut hero_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    hero_path.push("../fastx_programmability/examples/");
    let modules = fastx_framework::build_move_package(&hero_path, build_config, false).unwrap();

    // 2. Create an admin account, and a player account.
    // Using a hard-coded key to match the address in the Move code.
    // This needs to be hard-coded because the module needs to know the admin's address
    // in advance.
    let (admin, admin_key) = get_key_pair_from_bytes(&[
        10, 112, 5, 142, 174, 127, 187, 146, 251, 68, 22, 191, 128, 68, 84, 13, 102, 71, 77, 57,
        92, 154, 128, 240, 158, 45, 13, 123, 57, 21, 194, 214, 189, 215, 127, 86, 129, 189, 1, 4,
        90, 106, 17, 10, 123, 200, 40, 18, 34, 173, 240, 91, 213, 72, 183, 249, 213, 210, 39, 181,
        105, 254, 59, 163,
    ]);
    let admin_gas_object = Object::with_id_owner_for_testing(ObjectID::random(), admin);
    let admin_gas_object_ref = admin_gas_object.to_object_reference();
    let (player, player_key) = get_key_pair();
    let player_gas_object = Object::with_id_owner_for_testing(ObjectID::random(), player);
    let player_gas_object_ref = player_gas_object.to_object_reference();
    let mut authority = init_state_with_objects(vec![admin_gas_object, player_gas_object]).await;

    // 3. Publish the Hero modules to FastX.
    let all_module_bytes = modules
        .iter()
        .map(|m| {
            let mut module_bytes = Vec::new();
            m.serialize(&mut module_bytes).unwrap();
            module_bytes
        })
        .collect();
    let order = Order::new_module(admin, admin_gas_object_ref, all_module_bytes, &admin_key);
    let effects = send_and_confirm_order(&mut authority, order)
        .await
        .unwrap()
        .signed_effects
        .unwrap()
        .effects;
    assert_eq!(effects.status, ExecutionStatus::Success);
    let package_object = effects.created[0].0;

    // 4. Init the game by minting the GameAdmin.
    let effects = call_move(
        &mut authority,
        &admin_gas_object_ref.0,
        &admin,
        &admin_key,
        &package_object,
        ident_str!("Hero").to_owned(),
        ident_str!("init").to_owned(),
        vec![],
        vec![],
    )
    .await;
    assert_eq!(effects.status, ExecutionStatus::Success);
    let (admin_object, admin_object_owner) = effects.created[0];
    assert_eq!(admin_object_owner, admin);

    // 5. Create Trusted Coin Treasury.
    let effects = call_move(
        &mut authority,
        &player_gas_object_ref.0,
        &player,
        &player_key,
        &package_object,
        ident_str!("TrustedCoin").to_owned(),
        ident_str!("init").to_owned(),
        vec![],
        vec![],
    )
    .await;
    assert_eq!(effects.status, ExecutionStatus::Success);
    let (cap, cap_owner) = effects.created[0];
    assert_eq!(cap_owner, player);

    // 6. Mint 500 EXAMPLE TrustedCoin.
    let effects = call_move(
        &mut authority,
        &player_gas_object_ref.0,
        &player,
        &player_key,
        &package_object,
        ident_str!("TrustedCoin").to_owned(),
        ident_str!("mint").to_owned(),
        vec![cap.0],
        vec![bcs::to_bytes(&500_u64).unwrap()],
    )
    .await;
    assert_eq!(effects.status, ExecutionStatus::Success);
    assert_eq!(effects.mutated.len(), 1); // cap
    let (coin, coin_owner) = effects.created[0];
    assert_eq!(coin_owner, player);

    // 7. Purchase a sword using 500 coin. This sword will have magic = 4, sword_strength = 5.
    let effects = call_move(
        &mut authority,
        &player_gas_object_ref.0,
        &player,
        &player_key,
        &package_object,
        ident_str!("Hero").to_owned(),
        ident_str!("acquire_hero").to_owned(),
        vec![coin.0],
        vec![],
    )
    .await;
    assert_eq!(effects.status, ExecutionStatus::Success);
    assert_eq!(effects.mutated.len(), 1); // coin
    let (hero, hero_owner) = effects.created[0];
    assert_eq!(hero_owner, player);
    // The payment goes to the admin.
    assert_eq!(effects.mutated[0].1, admin);

    // 8. Verify the hero is what we exepct with strength 5.
    let effects = call_move(
        &mut authority,
        &player_gas_object_ref.0,
        &player,
        &player_key,
        &package_object,
        ident_str!("Hero").to_owned(),
        ident_str!("assert_hero_strength").to_owned(),
        vec![hero.0],
        vec![bcs::to_bytes(&5_u64).unwrap()],
    )
    .await;
    assert_eq!(effects.status, ExecutionStatus::Success);

    // 9. Give them a boar!
    let pure_args = vec![
        bcs::to_bytes(&10_u64).unwrap(),          // hp
        bcs::to_bytes(&10_u64).unwrap(),          // strength
        bcs::to_bytes(&player.to_vec()).unwrap(), // recipient
    ];
    let effects = call_move(
        &mut authority,
        &admin_gas_object_ref.0,
        &admin,
        &admin_key,
        &package_object,
        ident_str!("Hero").to_owned(),
        ident_str!("send_boar").to_owned(),
        vec![admin_object.0],
        pure_args,
    )
    .await;
    assert_eq!(effects.status, ExecutionStatus::Success);
    let (boar, boar_owner) = effects.created[0];
    assert_eq!(boar_owner, player);

    // 10. Slay the boar!
    let effects = call_move(
        &mut authority,
        &player_gas_object_ref.0,
        &player,
        &player_key,
        &package_object,
        ident_str!("Hero").to_owned(),
        ident_str!("slay").to_owned(),
        vec![hero.0, boar.0],
        vec![],
    )
    .await;
    assert_eq!(effects.status, ExecutionStatus::Success);
    let events = effects.events;
    // should emit one BoarSlainEvent
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].type_.name.to_string(), "BoarSlainEvent")
}

// helpers

#[cfg(test)]
fn init_state_parameters() -> (Committee, PublicKeyBytes, KeyPair, Arc<AuthorityStore>) {
    let (authority_address, authority_key) = get_key_pair();
    let mut authorities = BTreeMap::new();
    authorities.insert(
        /* address */ authority_address,
        /* voting right */ 1,
    );
    let committee = Committee::new(authorities);

    // Create a random directory to store the DB
    let dir = env::temp_dir();
    let path = dir.join(format!("DB_{:?}", ObjectID::random()));
    fs::create_dir(&path).unwrap();

    let mut opts = rocksdb::Options::default();
    opts.set_max_open_files(max_files_authority_tests());
    let store = Arc::new(AuthorityStore::open(path, Some(opts)));
    (committee, authority_address, authority_key, store)
}

#[cfg(test)]
async fn init_state() -> AuthorityState {
    let (committee, authority_address, authority_key, store) = init_state_parameters();
    AuthorityState::new_with_genesis_modules(committee, authority_address, authority_key, store)
        .await
}

#[cfg(test)]
async fn init_state_with_ids<I: IntoIterator<Item = (FastPayAddress, ObjectID)>>(
    objects: I,
) -> AuthorityState {
    let state = init_state().await;
    for (address, object_id) in objects {
        let obj = Object::with_id_owner_for_testing(object_id, address);
        state
            .init_order_lock((object_id, 0.into(), obj.digest()))
            .await;
        state.insert_object(obj).await;
    }
    state
}

async fn init_state_with_objects<I: IntoIterator<Item = Object>>(objects: I) -> AuthorityState {
    let state = init_state().await;

    for o in objects {
        let obj_ref = o.to_object_reference();
        state.insert_object(o).await;
        state.init_order_lock(obj_ref).await;
    }
    state
}

#[cfg(test)]
async fn init_state_with_object_id(address: FastPayAddress, object: ObjectID) -> AuthorityState {
    init_state_with_ids(std::iter::once((address, object))).await
}

#[cfg(test)]
fn init_transfer_order(
    sender: FastPayAddress,
    secret: &KeyPair,
    recipient: Address,
    object_ref: ObjectRef,
    gas_object_ref: ObjectRef,
) -> Order {
    let transfer = Transfer {
        object_ref,
        sender,
        recipient,
        gas_payment: gas_object_ref,
    };
    Order::new_transfer(transfer, secret)
}

#[cfg(test)]
fn init_certified_transfer_order(
    sender: FastPayAddress,
    secret: &KeyPair,
    recipient: Address,
    object_ref: ObjectRef,
    gas_object_ref: ObjectRef,
    authority_state: &AuthorityState,
) -> CertifiedOrder {
    let transfer_order = init_transfer_order(sender, secret, recipient, object_ref, gas_object_ref);
    let vote = SignedOrder::new(
        transfer_order.clone(),
        authority_state.name,
        &authority_state.secret,
    );
    let mut builder =
        SignatureAggregator::try_new(transfer_order, &authority_state.committee).unwrap();
    builder
        .append(vote.authority, vote.signature)
        .unwrap()
        .unwrap()
}

fn get_genesis_package_by_module(genesis_objects: &[Object], module: &str) -> ObjectRef {
    genesis_objects
        .iter()
        .find_map(|o| match o.data.try_as_package() {
            Some(p) => {
                if p.keys().any(|name| name == module) {
                    Some(o.to_object_reference())
                } else {
                    None
                }
            }
            None => None,
        })
        .unwrap()
}
