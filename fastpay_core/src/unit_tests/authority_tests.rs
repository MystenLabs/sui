// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use bcs;
use fastx_adapter::genesis;
#[cfg(test)]
use fastx_types::{
    base_types::dbg_addr,
    gas::{calculate_module_publish_cost, get_gas_balance},
};
use move_binary_format::{
    file_format::{self, AddressIdentifierIndex, IdentifierIndex, ModuleHandle},
    CompiledModule,
};
use move_core_types::ident_str;

use std::fs;
use std::{convert::TryInto, env};

pub fn system_maxfiles() -> usize {
    fdlimit::raise_fd_limit().unwrap_or(256u64) as usize
}

fn max_files_authority_tests() -> i32 {
    (system_maxfiles() / 8).try_into().unwrap()
}

const MAX_GAS: u64 = 100000;

#[tokio::test]
async fn test_handle_transfer_order_bad_signature() {
    let (sender, sender_key) = get_key_pair();
    let recipient = Address::FastPay(dbg_addr(2));
    let object_id = ObjectID::random();
    let gas_object_id = ObjectID::random();
    let authority_state =
        init_state_with_ids(vec![(sender, object_id), (sender, gas_object_id)]).await;
    let transfer_order =
        init_transfer_order(sender, &sender_key, recipient, object_id, gas_object_id);
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
    let transfer_order = init_transfer_order(
        unknown_address,
        &sender_key,
        recipient,
        object_id,
        gas_object_id,
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
    let transfer_order =
        init_transfer_order(sender, &sender_key, recipient, object_id, gas_object_id);

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

    // Create a gas object with 0 balance.
    let gas_object_id = ObjectID::random();
    let gas_object =
        Object::with_id_owner_gas_for_testing(gas_object_id, SequenceNumber::new(), sender, 0);
    authority_state
        .init_order_lock((gas_object_id, 0.into(), gas_object.digest()))
        .await;
    authority_state.insert_object(gas_object).await;

    let transfer_order =
        init_transfer_order(sender, &sender_key, recipient, object_id, gas_object_id);

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
    let mut authority = init_state_with_genesis(vec![gas_payment_object]).await;

    let order = Order::new_module(
        sender,
        gas_payment_object_ref,
        vec![dependent_module_bytes],
        &sender_key,
    );
    let dependent_module_id = TxContext::new(order.digest()).fresh_id();

    // Object does not exist
    assert!(authority.object_state(&dependent_module_id).await.is_err());
    let _response = send_and_confirm_order(&mut authority, order).await.unwrap();

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
    let _module_object_id = TxContext::new(order.digest()).fresh_id();
    let _response = send_and_confirm_order(&mut authority, order).await.unwrap();

    // check that the module actually got published
    assert!(_response.certified_order.is_some());

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
    let package_object_ref = genesis_package_objects
        .iter()
        .find_map(|o| match o.data.try_as_package() {
            Some(p) => {
                if p.keys().any(|name| name == "ObjectBasics") {
                    Some(o.to_object_reference())
                } else {
                    None
                }
            }
            None => None,
        })
        .unwrap();

    genesis_package_objects.push(gas_payment_object);
    let mut authority_state = init_state_with_objects(genesis_package_objects).await;
    authority_state.native_functions = native_functions;

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
    // 34 is for bytecode execution, 24 is for object creation.
    // If the number changes, we want to verify that the change is intended.
    let gas_cost = 34 + 24;
    let res = send_and_confirm_order(&mut authority_state, order)
        .await
        .unwrap();

    // Check that effects are reported
    assert!(res.signed_effects.is_some());
    let mutated = res.signed_effects.unwrap().effects.mutated;
    assert!(mutated.len() == 2);

    let created_object_id = mutated[0].0; // res.object_id;
                                          // check that order actually created an object with the expected ID, owner, sequence number
    let created_obj = authority_state
        .object_state(&created_object_id)
        .await
        .unwrap();
    assert_eq!(created_obj.owner, sender,);
    assert_eq!(created_obj.id(), created_object_id);
    assert_eq!(created_obj.version(), SequenceNumber::from(1));

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
    let package_object_ref = genesis_package_objects
        .iter()
        .find_map(|o| match o.data.try_as_package() {
            Some(p) => {
                if p.keys().any(|name| name == "ObjectBasics") {
                    Some(o.to_object_reference())
                } else {
                    None
                }
            }
            None => None,
        })
        .unwrap();

    genesis_package_objects.push(gas_payment_object);
    let mut authority_state = init_state_with_objects(genesis_package_objects).await;
    authority_state.native_functions = native_functions;

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
    let transfer_order =
        init_transfer_order(sender, &sender_key, recipient, object_id, gas_object_id);

    let signed_order = authority_state
        .handle_order(transfer_order.clone())
        .await
        .unwrap();
    // calls to handlers are idempotent -- returns the same.
    let double_spend_signed_order = authority_state.handle_order(transfer_order).await.unwrap();
    assert_eq!(signed_order, double_spend_signed_order);
}

#[tokio::test]
async fn test_handle_confirmation_order_unknown_sender() {
    let recipient = dbg_addr(2);
    let (sender, sender_key) = get_key_pair();
    let authority_state = init_state();
    let certified_transfer_order = init_certified_transfer_order(
        sender,
        &sender_key,
        Address::FastPay(recipient),
        ObjectID::random(),
        ObjectID::random(),
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
        object_id,
        gas_object_id,
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
    assert!(authority_state
        .handle_confirmation_order(ConfirmationOrder::new(certified_transfer_order))
        .await
        .is_ok());

    // Check that the new object is the one recorded.
    let new_account = authority_state.object_state(&object_id).await.unwrap();
    assert_eq!(old_seq_num.increment(), new_account.version());

    // No recipient object was created.
    assert!(authority_state
        .object_state(&dbg_object_id(2))
        .await
        .is_err());
}

#[tokio::test]
async fn test_handle_confirmation_order_exceed_balance() {
    let (sender, sender_key) = get_key_pair();
    let object_id = ObjectID::random();
    let gas_object_id = ObjectID::random();
    let recipient = dbg_addr(2);
    let authority_state =
        init_state_with_ids(vec![(sender, object_id), (sender, gas_object_id)]).await;

    let certified_transfer_order = init_certified_transfer_order(
        sender,
        &sender_key,
        Address::FastPay(recipient),
        object_id,
        gas_object_id,
        &authority_state,
    );
    assert!(authority_state
        .handle_confirmation_order(ConfirmationOrder::new(certified_transfer_order))
        .await
        .is_ok());
    let new_account = authority_state.object_state(&object_id).await.unwrap();
    assert_eq!(SequenceNumber::from(1), new_account.version());
    assert!(authority_state
        .parent(&(object_id, new_account.version(), new_account.digest()))
        .await
        .is_some());
}

#[tokio::test]
async fn test_handle_confirmation_order_receiver_balance_overflow() {
    let (sender, sender_key) = get_key_pair();
    let object_id = ObjectID::random();
    let gas_object_id = ObjectID::random();
    let (recipient, _) = get_key_pair();
    let authority_state = init_state_with_ids(vec![
        (sender, object_id),
        (sender, gas_object_id),
        (recipient, ObjectID::random()),
    ])
    .await;

    let certified_transfer_order = init_certified_transfer_order(
        sender,
        &sender_key,
        Address::FastPay(recipient),
        object_id,
        gas_object_id,
        &authority_state,
    );
    assert!(authority_state
        .handle_confirmation_order(ConfirmationOrder::new(certified_transfer_order))
        .await
        .is_ok());
    let new_sender_account = authority_state.object_state(&object_id).await.unwrap();
    assert_eq!(SequenceNumber::from(1), new_sender_account.version());

    assert!(authority_state
        .parent(&(
            object_id,
            new_sender_account.version(),
            new_sender_account.digest()
        ))
        .await
        .is_some());
}

#[tokio::test]
async fn test_handle_confirmation_order_receiver_equal_sender() {
    let (address, key) = get_key_pair();
    let object_id: ObjectID = ObjectID::random();
    let gas_object_id = ObjectID::random();
    let authority_state =
        init_state_with_ids(vec![(address, object_id), (address, gas_object_id)]).await;

    let certified_transfer_order = init_certified_transfer_order(
        address,
        &key,
        Address::FastPay(address),
        object_id,
        gas_object_id,
        &authority_state,
    );
    assert!(authority_state
        .handle_confirmation_order(ConfirmationOrder::new(certified_transfer_order))
        .await
        .is_ok());
    let account = authority_state.object_state(&object_id).await.unwrap();
    assert_eq!(SequenceNumber::from(1), account.version());

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
        authority_state.insert_object(gas_object).await;

        let certified_transfer_order = init_certified_transfer_order(
            sender,
            &sender_key,
            Address::FastPay(recipient),
            object_id,
            gas_object_id,
            &authority_state,
        );

        authority_state
            .handle_confirmation_order(ConfirmationOrder::new(certified_transfer_order.clone()))
            .await
    };
    let result = run_test_with_gas(10).await;
    let err_string = result.unwrap_err().to_string();
    assert!(err_string.contains("Gas balance is 10, not enough to pay 16"));
    let result = run_test_with_gas(20).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_handle_confirmation_order_ok() {
    let (sender, sender_key) = get_key_pair();
    let recipient = dbg_addr(2);
    let object_id = ObjectID::random();
    let gas_object_id = ObjectID::random();
    let authority_state =
        init_state_with_ids(vec![(sender, object_id), (sender, gas_object_id)]).await;
    let certified_transfer_order = init_certified_transfer_order(
        sender,
        &sender_key,
        Address::FastPay(recipient),
        object_id,
        gas_object_id,
        &authority_state,
    );

    let old_account = authority_state.object_state(&object_id).await.unwrap();
    let mut next_sequence_number = old_account.version();
    next_sequence_number = next_sequence_number.increment();

    let info = authority_state
        .handle_confirmation_order(ConfirmationOrder::new(certified_transfer_order.clone()))
        .await
        .unwrap();
    // Key check: the ownership has changed

    let new_account = authority_state.object_state(&object_id).await.unwrap();
    assert_eq!(recipient, new_account.owner);
    assert_eq!(next_sequence_number, new_account.version());
    assert_eq!(None, info.signed_order);
    assert_eq!(
        {
            let refx = authority_state
                .parent(&(object_id, new_account.version(), new_account.digest()))
                .await
                .unwrap();
            authority_state.read_certificate(&refx).await.unwrap()
        },
        Some(certified_transfer_order.clone())
    );

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
    let certified_transfer_order = init_certified_transfer_order(
        sender,
        &sender_key,
        Address::FastPay(recipient),
        object_id,
        gas_object_id,
        &authority_state,
    );

    let info = authority_state
        .handle_confirmation_order(ConfirmationOrder::new(certified_transfer_order.clone()))
        .await
        .unwrap();

    let info2 = authority_state
        .handle_confirmation_order(ConfirmationOrder::new(certified_transfer_order.clone()))
        .await
        .unwrap();

    assert_eq!(info, info2);
    assert!(info2.certified_order.is_some());
    assert!(info2.signed_effects.is_some());
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
async fn init_state_with_genesis<I: IntoIterator<Item = Object>>(
    genesis_objects: I,
) -> AuthorityState {
    let (committee, authority_address, authority_key, store) = init_state_parameters();
    let state = AuthorityState::new_with_genesis_modules(
        committee,
        authority_address,
        authority_key,
        store,
    )
    .await;
    for obj in genesis_objects {
        state
            .init_order_lock((obj.id(), 0.into(), obj.digest()))
            .await;
        state.insert_object(obj).await;
    }
    state
}

#[cfg(test)]
fn init_state() -> AuthorityState {
    let (committee, authority_address, authority_key, store) = init_state_parameters();
    AuthorityState::new_without_genesis_for_testing(
        committee,
        authority_address,
        authority_key,
        store,
    )
}

#[cfg(test)]
async fn init_state_with_ids<I: IntoIterator<Item = (FastPayAddress, ObjectID)>>(
    objects: I,
) -> AuthorityState {
    let state = init_state();
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
    let state = init_state();

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
    object_id: ObjectID,
    gas_object_id: ObjectID,
) -> Order {
    let transfer = Transfer {
        // TODO(https://github.com/MystenLabs/fastnft/issues/123): Include actual object digest here
        object_ref: (object_id, SequenceNumber::new(), ObjectDigest::new([0; 32])),
        sender,
        recipient,
        gas_payment: (
            gas_object_id,
            SequenceNumber::new(),
            ObjectDigest::new([0; 32]),
        ),
    };
    Order::new_transfer(transfer, secret)
}

#[cfg(test)]
fn init_certified_transfer_order(
    sender: FastPayAddress,
    secret: &KeyPair,
    recipient: Address,
    object_id: ObjectID,
    gas_object_id: ObjectID,
    authority_state: &AuthorityState,
) -> CertifiedOrder {
    let transfer_order = init_transfer_order(sender, secret, recipient, object_id, gas_object_id);
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
