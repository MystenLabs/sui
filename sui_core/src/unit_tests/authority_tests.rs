// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use bcs;

use move_binary_format::{
    file_format::{self, AddressIdentifierIndex, IdentifierIndex, ModuleHandle},
    CompiledModule,
};
use move_core_types::{
    account_address::AccountAddress, ident_str, identifier::Identifier, language_storage::TypeTag,
};
use move_package::BuildConfig;
use sui_adapter::genesis;
use sui_types::{
    base_types::dbg_addr,
    crypto::KeyPair,
    crypto::{get_key_pair, Signature},
    gas::{calculate_module_publish_cost, get_gas_balance},
    messages::{ExecutionStatus, Transaction},
    object::{GAS_VALUE_FOR_TESTING, OBJECT_START_VERSION},
};

use std::fs;
use std::path::PathBuf;
use std::{convert::TryInto, env};

pub fn system_maxfiles() -> usize {
    fdlimit::raise_fd_limit().unwrap_or(256u64) as usize
}

pub fn max_files_authority_tests() -> i32 {
    (system_maxfiles() / 8).try_into().unwrap()
}

const MAX_GAS: u64 = 10000;

// Only relevant in a ser/de context : the `CertifiedTransaction` for a transaction is not unique
fn compare_certified_transactions(o1: &CertifiedTransaction, o2: &CertifiedTransaction) {
    assert_eq!(o1.transaction.digest(), o2.transaction.digest());
    // in this ser/de context it's relevant to compare signatures
    assert_eq!(o1.signatures, o2.signatures);
}

// Only relevant in a ser/de context : the `CertifiedTransaction` for a transaction is not unique
fn compare_transaction_info_responses(o1: &TransactionInfoResponse, o2: &TransactionInfoResponse) {
    assert_eq!(o1.signed_transaction, o2.signed_transaction);
    assert_eq!(o1.signed_effects, o2.signed_effects);
    match (
        o1.certified_transaction.as_ref(),
        o2.certified_transaction.as_ref(),
    ) {
        (Some(cert1), Some(cert2)) => {
            assert_eq!(cert1.transaction.digest(), cert2.transaction.digest());
            assert_eq!(cert1.signatures, cert2.signatures);
        }
        (None, None) => (),
        _ => panic!("certificate structure between responses differs"),
    }
}

#[tokio::test]
async fn test_handle_transfer_transaction_bad_signature() {
    let (sender, sender_key) = get_key_pair();
    let recipient = dbg_addr(2);
    let object_id = ObjectID::random();
    let gas_object_id = ObjectID::random();
    let authority_state =
        init_state_with_ids(vec![(sender, object_id), (sender, gas_object_id)]).await;
    let object = authority_state
        .get_object(&object_id)
        .await
        .unwrap()
        .unwrap();
    let gas_object = authority_state
        .get_object(&gas_object_id)
        .await
        .unwrap()
        .unwrap();
    let transfer_transaction = init_transfer_transaction(
        sender,
        &sender_key,
        recipient,
        object.to_object_reference(),
        gas_object.to_object_reference(),
    );
    let (_unknown_address, unknown_key) = get_key_pair();
    let mut bad_signature_transfer_transaction = transfer_transaction.clone();
    bad_signature_transfer_transaction.signature =
        Signature::new(&transfer_transaction.data, &unknown_key);
    assert!(authority_state
        .handle_transaction(bad_signature_transfer_transaction)
        .await
        .is_err());

    let object = authority_state
        .get_object(&object_id)
        .await
        .unwrap()
        .unwrap();
    assert!(authority_state
        .get_transaction_lock(&object.to_object_reference())
        .await
        .unwrap()
        .is_none());

    assert!(authority_state
        .get_transaction_lock(&object.to_object_reference())
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn test_handle_transfer_transaction_unknown_sender() {
    let sender = get_new_address();
    let (unknown_address, unknown_key) = get_key_pair();
    let object_id: ObjectID = ObjectID::random();
    let gas_object_id = ObjectID::random();
    let recipient = dbg_addr(2);
    let authority_state =
        init_state_with_ids(vec![(sender, object_id), (sender, gas_object_id)]).await;
    let object = authority_state
        .get_object(&object_id)
        .await
        .unwrap()
        .unwrap();
    let gas_object = authority_state
        .get_object(&gas_object_id)
        .await
        .unwrap()
        .unwrap();

    let unknown_sender_transfer_transaction = init_transfer_transaction(
        unknown_address,
        &unknown_key,
        recipient,
        object.to_object_reference(),
        gas_object.to_object_reference(),
    );

    assert!(authority_state
        .handle_transaction(unknown_sender_transfer_transaction)
        .await
        .is_err());

    let object = authority_state
        .get_object(&object_id)
        .await
        .unwrap()
        .unwrap();
    assert!(authority_state
        .get_transaction_lock(&object.to_object_reference())
        .await
        .unwrap()
        .is_none());

    assert!(authority_state
        .get_transaction_lock(&object.to_object_reference())
        .await
        .unwrap()
        .is_none());
}

/* FIXME: This tests the submission of out of transaction certs, but modifies object sequence numbers manually
   and leaves the authority in an inconsistent state. We should re-code it in a proper way.

#[test]
fn test_handle_transfer_transaction_bad_sequence_number() {
    let (sender, sender_key) = get_key_pair();
    let object_id: ObjectID = random_object_id();
    let recipient = Address::Sui(dbg_addr(2));
    let authority_state = init_state_with_object(sender, object_id);
    let transfer_transaction = init_transfer_transaction(sender, &sender_key, recipient, object_id);

    let mut sequence_number_state = authority_state;
    let sequence_number_state_sender_account =
        sequence_number_state.objects.get_mut(&object_id).unwrap();
    sequence_number_state_sender_account.version() =
        sequence_number_state_sender_account
            .version()
            .increment()
            .unwrap();
    assert!(sequence_number_state
        .handle_transfer_transaction(transfer_transaction)
        .is_err());

        let object = sequence_number_state.objects.get(&object_id).unwrap();
        assert!(sequence_number_state.get_transaction_lock(object.id, object.version()).unwrap().is_none());
}
*/

#[tokio::test]
async fn test_handle_transfer_transaction_ok() {
    let (sender, sender_key) = get_key_pair();
    let recipient = dbg_addr(2);
    let object_id = ObjectID::random();
    let gas_object_id = ObjectID::random();
    let authority_state =
        init_state_with_ids(vec![(sender, object_id), (sender, gas_object_id)]).await;
    let object = authority_state
        .get_object(&object_id)
        .await
        .unwrap()
        .unwrap();
    let gas_object = authority_state
        .get_object(&gas_object_id)
        .await
        .unwrap()
        .unwrap();
    let transfer_transaction = init_transfer_transaction(
        sender,
        &sender_key,
        recipient,
        object.to_object_reference(),
        gas_object.to_object_reference(),
    );

    let test_object = authority_state
        .get_object(&object_id)
        .await
        .unwrap()
        .unwrap();

    // Check the initial state of the locks
    assert!(authority_state
        .get_transaction_lock(&(object_id, 0.into(), test_object.digest()))
        .await
        .unwrap()
        .is_none());
    assert!(authority_state
        .get_transaction_lock(&(object_id, 1.into(), test_object.digest()))
        .await
        .is_err());

    let account_info = authority_state
        .handle_transaction(transfer_transaction.clone())
        .await
        .unwrap();

    let object = authority_state
        .get_object(&object_id)
        .await
        .unwrap()
        .unwrap();
    let pending_confirmation = authority_state
        .get_transaction_lock(&object.to_object_reference())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        account_info.signed_transaction.unwrap(),
        pending_confirmation
    );

    // Check the final state of the locks
    assert!(authority_state
        .get_transaction_lock(&(object_id, 0.into(), object.digest()))
        .await
        .unwrap()
        .is_some());
    assert_eq!(
        authority_state
            .get_transaction_lock(&(object_id, 0.into(), object.digest()))
            .await
            .unwrap()
            .as_ref()
            .unwrap()
            .transaction
            .data,
        transfer_transaction.data
    );
}

#[tokio::test]
async fn test_transfer_immutable() {
    let (sender, sender_key) = get_key_pair();
    let recipient = dbg_addr(2);
    let object_id = ObjectID::random();
    let authority_state = init_state_with_ids(vec![(sender, object_id)]).await;
    let gas_object = authority_state
        .get_object(&object_id)
        .await
        .unwrap()
        .unwrap();
    let genesis_package_objects = genesis::clone_genesis_packages();
    let package_object_ref = get_genesis_package_by_module(&genesis_package_objects, "ID");
    // We are trying to transfer the genesis package object, which is immutable.
    let transfer_transaction = init_transfer_transaction(
        sender,
        &sender_key,
        recipient,
        package_object_ref,
        gas_object.to_object_reference(),
    );
    let result = authority_state
        .handle_transaction(transfer_transaction.clone())
        .await;
    assert_eq!(
        result.unwrap_err(),
        SuiError::LockErrors {
            errors: vec![SuiError::TransferSharedError]
        }
    );
}

#[tokio::test]
async fn test_handle_transfer_zero_balance() {
    let (sender, sender_key) = get_key_pair();
    let recipient = dbg_addr(2);
    let object_id = ObjectID::random();
    let authority_state = init_state_with_ids(vec![(sender, object_id)]).await;
    let object = authority_state
        .get_object(&object_id)
        .await
        .unwrap()
        .unwrap();

    // Create a gas object with 0 balance.
    let gas_object_id = ObjectID::random();
    let gas_object =
        Object::with_id_owner_gas_for_testing(gas_object_id, SequenceNumber::new(), sender, 0);
    authority_state
        .init_transaction_lock((gas_object_id, 0.into(), gas_object.digest()))
        .await;
    let gas_object_ref = gas_object.to_object_reference();
    authority_state.insert_object(gas_object).await;

    let transfer_transaction = init_transfer_transaction(
        sender,
        &sender_key,
        recipient,
        object.to_object_reference(),
        gas_object_ref,
    );

    let result = authority_state
        .handle_transaction(transfer_transaction.clone())
        .await;
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Gas balance is 0, smaller than minimum requirement of 8 for object transfer."));
}

async fn send_and_confirm_transaction(
    authority: &AuthorityState,
    transaction: Transaction,
) -> Result<TransactionInfoResponse, SuiError> {
    // Make the initial request
    let response = authority.handle_transaction(transaction.clone()).await?;
    let vote = response.signed_transaction.unwrap();

    // Collect signatures from a quorum of authorities
    let mut builder = SignatureAggregator::try_new(transaction, &authority.committee).unwrap();
    let certificate = builder
        .append(vote.authority, vote.signature)
        .unwrap()
        .unwrap();
    // Submit the confirmation. *Now* execution actually happens, and it should fail when we try to look up our dummy module.
    // we unfortunately don't get a very descriptive error message, but we can at least see that something went wrong inside the VM
    authority
        .handle_confirmation_transaction(ConfirmationTransaction::new(certificate))
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
pub fn check_gas_object(
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
    let genesis_module_objects = genesis::clone_genesis_packages();
    let genesis_module = match &genesis_module_objects[0].data {
        Data::Package(m) => {
            CompiledModule::deserialize(m.serialized_module_map().values().next().unwrap()).unwrap()
        }
        _ => unreachable!(),
    };
    // create a module that depends on a genesis module
    let dependent_module = make_dependent_module(&genesis_module);
    let dependent_module_bytes = {
        let mut bytes = Vec::new();
        dependent_module.serialize(&mut bytes).unwrap();
        bytes
    };
    let authority = init_state_with_objects(vec![gas_payment_object]).await;

    let data = TransactionData::new_module(
        sender,
        gas_payment_object_ref,
        vec![dependent_module_bytes],
        MAX_GAS,
    );
    let signature = Signature::new(&data, &sender_key);
    let transaction = Transaction::new(data, signature);

    let dependent_module_id = TxContext::new(&sender, transaction.digest()).fresh_id();

    // Object does not exist
    assert!(authority
        .get_object(&dependent_module_id)
        .await
        .unwrap()
        .is_none());
    let response = send_and_confirm_transaction(&authority, transaction)
        .await
        .unwrap();
    response.signed_effects.unwrap().effects.status.unwrap();

    // check that the dependent module got published
    assert!(authority.get_object(&dependent_module_id).await.is_ok());
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
    let authority = init_state_with_objects(vec![gas_payment_object]).await;

    let module = file_format::empty_module();
    let mut module_bytes = Vec::new();
    module.serialize(&mut module_bytes).unwrap();
    let module_bytes = vec![module_bytes];
    let gas_cost = calculate_module_publish_cost(&module_bytes);
    let data = TransactionData::new_module(sender, gas_payment_object_ref, module_bytes, MAX_GAS);
    let signature = Signature::new(&data, &sender_key);
    let transaction = Transaction::new(data, signature);
    let _module_object_id = TxContext::new(&sender, transaction.digest()).fresh_id();
    let response = send_and_confirm_transaction(&authority, transaction)
        .await
        .unwrap();
    response.signed_effects.unwrap().effects.status.unwrap();

    // check that the module actually got published
    assert!(response.certified_transaction.is_some());

    // Check that gas is properly deducted.
    let gas_payment_object = authority
        .get_object(&gas_payment_object_id)
        .await
        .unwrap()
        .unwrap();
    check_gas_object(
        &gas_payment_object,
        gas_balance - gas_cost,
        gas_seq.increment(),
    )
}

#[tokio::test]
async fn test_publish_non_existing_dependent_module() {
    let (sender, sender_key) = get_key_pair();
    let gas_payment_object_id = ObjectID::random();
    let gas_payment_object = Object::with_id_owner_for_testing(gas_payment_object_id, sender);
    let gas_payment_object_ref = gas_payment_object.to_object_reference();
    // create a genesis state that contains the gas object and genesis modules
    let genesis_module_objects = genesis::clone_genesis_packages();
    let genesis_module = match &genesis_module_objects[0].data {
        Data::Package(m) => {
            CompiledModule::deserialize(m.serialized_module_map().values().next().unwrap()).unwrap()
        }
        _ => unreachable!(),
    };
    // create a module that depends on a genesis module
    let mut dependent_module = make_dependent_module(&genesis_module);
    // Add another dependent module that points to a random address, hence does not exist on-chain.
    dependent_module
        .address_identifiers
        .push(AccountAddress::from(ObjectID::random()));
    dependent_module.module_handles.push(ModuleHandle {
        address: AddressIdentifierIndex((dependent_module.address_identifiers.len() - 1) as u16),
        name: IdentifierIndex(0),
    });
    let dependent_module_bytes = {
        let mut bytes = Vec::new();
        dependent_module.serialize(&mut bytes).unwrap();
        bytes
    };
    let authority = init_state_with_objects(vec![gas_payment_object]).await;

    let data = TransactionData::new_module(
        sender,
        gas_payment_object_ref,
        vec![dependent_module_bytes],
        MAX_GAS,
    );
    let signature = Signature::new(&data, &sender_key);
    let transaction = Transaction::new(data, signature);

    let response = authority.handle_transaction(transaction).await;
    assert!(response
        .unwrap_err()
        .to_string()
        .contains("DependentPackageNotFound"));
    // Check that gas was not charged.
    assert_eq!(
        authority
            .get_object(&gas_payment_object_id)
            .await
            .unwrap()
            .unwrap()
            .version(),
        gas_payment_object_ref.1
    );
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
    let data = TransactionData::new_module(sender, gas_payment_object_ref, module_bytes, 10);
    let signature = Signature::new(&data, &sender_key);
    let transaction = Transaction::new(data, signature);
    let response = authority
        .handle_transaction(transaction.clone())
        .await
        .unwrap_err();
    assert!(response
        .to_string()
        .contains("Gas balance is 9, smaller than the budget 10 for move operation"));
}

#[tokio::test]
async fn test_handle_move_transaction() {
    let (sender, sender_key) = get_key_pair();
    let gas_payment_object_id = ObjectID::random();
    let gas_payment_object = Object::with_id_owner_for_testing(gas_payment_object_id, sender);
    let gas_seq = gas_payment_object.version();
    let authority_state = init_state_with_objects(vec![gas_payment_object]).await;

    let effects = create_move_object(
        &authority_state,
        &gas_payment_object_id,
        &sender,
        &sender_key,
    )
    .await
    .unwrap();

    assert!(matches!(effects.status, ExecutionStatus::Success { .. }));
    assert_eq!(effects.created.len(), 1);
    assert_eq!(effects.mutated.len(), 1);

    let created_object_id = effects.created[0].0 .0;
    // check that transaction actually created an object with the expected ID, owner, sequence number
    let created_obj = authority_state
        .get_object(&created_object_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(created_obj.owner, sender);
    assert_eq!(created_obj.id(), created_object_id);
    assert_eq!(created_obj.version(), OBJECT_START_VERSION);

    // Check that gas is properly deducted.
    // If the number changes, we want to verify that the change is intended.
    let gas_cost = 54;
    let gas_payment_object = authority_state
        .get_object(&gas_payment_object_id)
        .await
        .unwrap()
        .unwrap();
    check_gas_object(
        &gas_payment_object,
        GAS_VALUE_FOR_TESTING - gas_cost,
        gas_seq.increment(),
    )
}

// Test the case when the gas budget provided is less than minimum requirement during move call.
// Note that the case where gas is insufficient to execute move bytecode is tested
// separately in the adapter tests.
#[tokio::test]
async fn test_handle_move_transaction_insufficient_budget() {
    let (sender, sender_key) = get_key_pair();
    let gas_payment_object_id = ObjectID::random();
    let gas_payment_object = Object::with_id_owner_for_testing(gas_payment_object_id, sender);
    let gas_payment_object_ref = gas_payment_object.to_object_reference();
    // find the function Object::create and call it to create a new object
    let genesis_package_objects = genesis::clone_genesis_packages();
    let package_object_ref =
        get_genesis_package_by_module(&genesis_package_objects, "ObjectBasics");

    let authority_state = init_state_with_objects(vec![gas_payment_object]).await;

    let function = ident_str!("create").to_owned();
    let data = TransactionData::new_move_call(
        sender,
        package_object_ref,
        ident_str!("ObjectBasics").to_owned(),
        function,
        Vec::new(),
        gas_payment_object_ref,
        Vec::new(),
        vec![],
        vec![
            16u64.to_le_bytes().to_vec(),
            bcs::to_bytes(&AccountAddress::from(sender)).unwrap(),
        ],
        9,
    );
    let signature = Signature::new(&data, &sender_key);
    let transaction = Transaction::new(data, signature);
    let response = authority_state
        .handle_transaction(transaction.clone())
        .await
        .unwrap_err();
    assert!(response
        .to_string()
        .contains("Gas budget is 9, smaller than minimum requirement of 10 for move operation"));
}

#[tokio::test]
async fn test_handle_transfer_transaction_double_spend() {
    let (sender, sender_key) = get_key_pair();
    let recipient = dbg_addr(2);
    let object_id = ObjectID::random();
    let gas_object_id = ObjectID::random();
    let authority_state =
        init_state_with_ids(vec![(sender, object_id), (sender, gas_object_id)]).await;
    let object = authority_state
        .get_object(&object_id)
        .await
        .unwrap()
        .unwrap();
    let gas_object = authority_state
        .get_object(&gas_object_id)
        .await
        .unwrap()
        .unwrap();
    let transfer_transaction = init_transfer_transaction(
        sender,
        &sender_key,
        recipient,
        object.to_object_reference(),
        gas_object.to_object_reference(),
    );

    let signed_transaction = authority_state
        .handle_transaction(transfer_transaction.clone())
        .await
        .unwrap();
    // calls to handlers are idempotent -- returns the same.
    let double_spend_signed_transaction = authority_state
        .handle_transaction(transfer_transaction)
        .await
        .unwrap();
    // this is valid because our test authority should not change its certified transaction
    compare_transaction_info_responses(&signed_transaction, &double_spend_signed_transaction);
}

#[tokio::test]
async fn test_handle_confirmation_transaction_unknown_sender() {
    let recipient = dbg_addr(2);
    let (sender, sender_key) = get_key_pair();
    let authority_state = init_state().await;

    let object = Object::with_id_owner_for_testing(
        ObjectID::random(),
        SuiAddress::random_for_testing_only(),
    );
    let gas_object = Object::with_id_owner_for_testing(
        ObjectID::random(),
        SuiAddress::random_for_testing_only(),
    );

    let certified_transfer_transaction = init_certified_transfer_transaction(
        sender,
        &sender_key,
        recipient,
        object.to_object_reference(),
        gas_object.to_object_reference(),
        &authority_state,
    );

    assert!(authority_state
        .handle_confirmation_transaction(ConfirmationTransaction::new(
            certified_transfer_transaction
        ))
        .await
        .is_err());
}

#[ignore]
#[tokio::test]
async fn test_handle_confirmation_transaction_bad_sequence_number() {
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
    let object = authority_state
        .get_object(&object_id)
        .await
        .unwrap()
        .unwrap();
    let gas_object = authority_state
        .get_object(&gas_object_id)
        .await
        .unwrap()
        .unwrap();

    // Record the old sequence number
    let old_seq_num;
    {
        let old_account = authority_state
            .get_object(&object_id)
            .await
            .unwrap()
            .unwrap();
        old_seq_num = old_account.version();
    }

    let certified_transfer_transaction = init_certified_transfer_transaction(
        sender,
        &sender_key,
        recipient,
        object.to_object_reference(),
        gas_object.to_object_reference(),
        &authority_state,
    );

    // Increment the sequence number
    {
        let mut sender_object = authority_state
            .get_object(&object_id)
            .await
            .unwrap()
            .unwrap();
        let o = sender_object.data.try_as_move_mut().unwrap();
        let old_contents = o.contents().to_vec();
        // update object contents, which will increment the sequence number
        o.update_contents(old_contents);
        authority_state.insert_object(sender_object).await;
    }

    // Explanation: providing an old cert that has already need applied
    //              returns a Ok(_) with info about the new object states.
    let response = authority_state
        .handle_confirmation_transaction(ConfirmationTransaction::new(
            certified_transfer_transaction,
        ))
        .await
        .unwrap();
    assert!(response.signed_effects.is_none());

    // Check that the new object is the one recorded.
    let new_object = authority_state
        .get_object(&object_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(old_seq_num.increment(), new_object.version());

    // No recipient object was created.
    assert!(authority_state.get_object(&dbg_object_id(2)).await.is_err());
}

#[tokio::test]
async fn test_handle_confirmation_transaction_receiver_equal_sender() {
    let (address, key) = get_key_pair();
    let object_id: ObjectID = ObjectID::random();
    let gas_object_id = ObjectID::random();
    let authority_state =
        init_state_with_ids(vec![(address, object_id), (address, gas_object_id)]).await;
    let object = authority_state
        .get_object(&object_id)
        .await
        .unwrap()
        .unwrap();
    let gas_object = authority_state
        .get_object(&gas_object_id)
        .await
        .unwrap()
        .unwrap();

    let certified_transfer_transaction = init_certified_transfer_transaction(
        address,
        &key,
        address,
        object.to_object_reference(),
        gas_object.to_object_reference(),
        &authority_state,
    );
    let response = authority_state
        .handle_confirmation_transaction(ConfirmationTransaction::new(
            certified_transfer_transaction,
        ))
        .await
        .unwrap();
    response.signed_effects.unwrap().effects.status.unwrap();
    let account = authority_state
        .get_object(&object_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(OBJECT_START_VERSION, account.version());

    assert!(authority_state
        .parent(&(object_id, account.version(), account.digest()))
        .await
        .is_some());
}

#[tokio::test]
async fn test_handle_confirmation_transaction_gas() {
    let run_test_with_gas = |gas: u64| async move {
        let (sender, sender_key) = get_key_pair();
        let recipient = dbg_addr(2);
        let object_id = ObjectID::random();
        let authority_state = init_state_with_ids(vec![(sender, object_id)]).await;
        let object = authority_state
            .get_object(&object_id)
            .await
            .unwrap()
            .unwrap();

        // Create a gas object with insufficient balance.
        let gas_object_id = ObjectID::random();
        let gas_object = Object::with_id_owner_gas_for_testing(
            gas_object_id,
            SequenceNumber::new(),
            sender,
            gas,
        );
        authority_state
            .init_transaction_lock((gas_object_id, 0.into(), gas_object.digest()))
            .await;
        let gas_object_ref = gas_object.to_object_reference();
        authority_state.insert_object(gas_object).await;

        let certified_transfer_transaction = init_certified_transfer_transaction(
            sender,
            &sender_key,
            recipient,
            object.to_object_reference(),
            gas_object_ref,
            &authority_state,
        );

        authority_state
            .handle_confirmation_transaction(ConfirmationTransaction::new(
                certified_transfer_transaction.clone(),
            ))
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
async fn test_handle_confirmation_transaction_ok() {
    let (sender, sender_key) = get_key_pair();
    let recipient = dbg_addr(2);
    let object_id = ObjectID::random();
    let gas_object_id = ObjectID::random();
    let authority_state =
        init_state_with_ids(vec![(sender, object_id), (sender, gas_object_id)]).await;
    let object = authority_state
        .get_object(&object_id)
        .await
        .unwrap()
        .unwrap();
    let gas_object = authority_state
        .get_object(&gas_object_id)
        .await
        .unwrap()
        .unwrap();

    let certified_transfer_transaction = init_certified_transfer_transaction(
        sender,
        &sender_key,
        recipient,
        object.to_object_reference(),
        gas_object.to_object_reference(),
        &authority_state,
    );

    let old_account = authority_state
        .get_object(&object_id)
        .await
        .unwrap()
        .unwrap();
    let mut next_sequence_number = old_account.version();
    next_sequence_number = next_sequence_number.increment();

    let info = authority_state
        .handle_confirmation_transaction(ConfirmationTransaction::new(
            certified_transfer_transaction.clone(),
        ))
        .await
        .unwrap();
    info.signed_effects.unwrap().effects.status.unwrap();
    // Key check: the ownership has changed

    let new_account = authority_state
        .get_object(&object_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(new_account.owner, recipient);
    assert_eq!(next_sequence_number, new_account.version());
    assert_eq!(None, info.signed_transaction);
    let opt_cert = {
        let refx = authority_state
            .parent(&(object_id, new_account.version(), new_account.digest()))
            .await
            .unwrap();
        authority_state.read_certificate(&refx).await.unwrap()
    };
    if let Some(certified_transaction) = opt_cert {
        // valid since our test authority should not update its certificate set
        compare_certified_transactions(&certified_transaction, &certified_transfer_transaction);
    } else {
        panic!("parent certificate not avaailable from the authority!");
    }

    // Check locks are set and archived correctly
    assert!(authority_state
        .get_transaction_lock(&(object_id, 0.into(), old_account.digest()))
        .await
        .is_err());
    assert!(authority_state
        .get_transaction_lock(&(object_id, 1.into(), new_account.digest()))
        .await
        .expect("Exists")
        .is_none());

    // Check that all the parents are returned.
    assert_eq!(
        authority_state
            .get_parent_iterator(object_id, None)
            .await
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn test_handle_confirmation_transaction_idempotent() {
    let (sender, sender_key) = get_key_pair();
    let recipient = dbg_addr(2);
    let object_id = ObjectID::random();
    let gas_object_id = ObjectID::random();
    let authority_state =
        init_state_with_ids(vec![(sender, object_id), (sender, gas_object_id)]).await;
    let object = authority_state
        .get_object(&object_id)
        .await
        .unwrap()
        .unwrap();
    let gas_object = authority_state
        .get_object(&gas_object_id)
        .await
        .unwrap()
        .unwrap();

    let certified_transfer_transaction = init_certified_transfer_transaction(
        sender,
        &sender_key,
        recipient,
        object.to_object_reference(),
        gas_object.to_object_reference(),
        &authority_state,
    );

    let info = authority_state
        .handle_confirmation_transaction(ConfirmationTransaction::new(
            certified_transfer_transaction.clone(),
        ))
        .await
        .unwrap();
    assert!(matches!(
        info.signed_effects.as_ref().unwrap().effects.status,
        ExecutionStatus::Success { .. }
    ));

    let info2 = authority_state
        .handle_confirmation_transaction(ConfirmationTransaction::new(
            certified_transfer_transaction.clone(),
        ))
        .await
        .unwrap();
    assert!(matches!(
        info2.signed_effects.as_ref().unwrap().effects.status,
        ExecutionStatus::Success { .. }
    ));

    // this is valid because we're checking the authority state does not change the certificate
    compare_transaction_info_responses(&info, &info2);

    // Now check the transaction info request is also the same
    let info3 = authority_state
        .handle_transaction_info_request(TransactionInfoRequest {
            transaction_digest: certified_transfer_transaction.transaction.digest(),
        })
        .await
        .unwrap();

    compare_transaction_info_responses(&info, &info3);
}

#[tokio::test]
async fn test_move_call_mutable_object_not_mutated() {
    let (sender, sender_key) = get_key_pair();
    let gas_object_id = ObjectID::random();
    let authority_state = init_state_with_ids(vec![(sender, gas_object_id)]).await;

    let effects = create_move_object(&authority_state, &gas_object_id, &sender, &sender_key)
        .await
        .unwrap();
    assert!(matches!(effects.status, ExecutionStatus::Success { .. }));
    assert_eq!((effects.created.len(), effects.mutated.len()), (1, 1));
    let (new_object_id1, seq1, _) = effects.created[0].0;

    let effects = create_move_object(&authority_state, &gas_object_id, &sender, &sender_key)
        .await
        .unwrap();
    assert!(matches!(effects.status, ExecutionStatus::Success { .. }));
    assert_eq!((effects.created.len(), effects.mutated.len()), (1, 1));
    let (new_object_id2, seq2, _) = effects.created[0].0;

    let effects = call_framework_code(
        &authority_state,
        &gas_object_id,
        &sender,
        &sender_key,
        "ObjectBasics",
        "update",
        vec![],
        vec![new_object_id1, new_object_id2],
        vec![],
        vec![],
    )
    .await
    .unwrap();
    assert!(matches!(effects.status, ExecutionStatus::Success { .. }));
    assert_eq!((effects.created.len(), effects.mutated.len()), (0, 3));
    // Verify that both objects' version increased, even though only one object was updated.
    assert_eq!(
        authority_state
            .get_object(&new_object_id1)
            .await
            .unwrap()
            .unwrap()
            .version(),
        seq1.increment()
    );
    assert_eq!(
        authority_state
            .get_object(&new_object_id2)
            .await
            .unwrap()
            .unwrap()
            .version(),
        seq2.increment()
    );
}

#[tokio::test]
async fn test_move_call_delete() {
    let (sender, sender_key) = get_key_pair();
    let gas_object_id = ObjectID::random();
    let authority_state = init_state_with_ids(vec![(sender, gas_object_id)]).await;

    let effects = create_move_object(&authority_state, &gas_object_id, &sender, &sender_key)
        .await
        .unwrap();
    assert!(matches!(effects.status, ExecutionStatus::Success { .. }));
    assert_eq!((effects.created.len(), effects.mutated.len()), (1, 1));
    let (new_object_id1, _seq1, _) = effects.created[0].0;

    let effects = create_move_object(&authority_state, &gas_object_id, &sender, &sender_key)
        .await
        .unwrap();
    assert!(matches!(effects.status, ExecutionStatus::Success { .. }));
    assert_eq!((effects.created.len(), effects.mutated.len()), (1, 1));
    let (new_object_id2, _seq2, _) = effects.created[0].0;

    let effects = call_framework_code(
        &authority_state,
        &gas_object_id,
        &sender,
        &sender_key,
        "ObjectBasics",
        "update",
        vec![],
        vec![new_object_id1, new_object_id2],
        vec![],
        vec![],
    )
    .await
    .unwrap();
    assert!(matches!(effects.status, ExecutionStatus::Success { .. }));
    // All mutable objects will appear to be mutated, even if they are not.
    // obj1, obj2 and gas are all mutated here.
    assert_eq!((effects.created.len(), effects.mutated.len()), (0, 3));

    let effects = call_framework_code(
        &authority_state,
        &gas_object_id,
        &sender,
        &sender_key,
        "ObjectBasics",
        "delete",
        vec![],
        vec![new_object_id1],
        vec![],
        vec![],
    )
    .await
    .unwrap();
    assert!(matches!(effects.status, ExecutionStatus::Success { .. }));
    assert_eq!((effects.deleted.len(), effects.mutated.len()), (1, 1));
}

#[tokio::test]
async fn test_get_latest_parent_entry() {
    let (sender, sender_key) = get_key_pair();
    let gas_object_id = ObjectID::random();
    let authority_state = init_state_with_ids(vec![(sender, gas_object_id)]).await;

    let effects = create_move_object(&authority_state, &gas_object_id, &sender, &sender_key)
        .await
        .unwrap();
    let (new_object_id1, _seq1, _) = effects.created[0].0;

    let effects = create_move_object(&authority_state, &gas_object_id, &sender, &sender_key)
        .await
        .unwrap();
    let (new_object_id2, _seq2, _) = effects.created[0].0;

    let effects = call_framework_code(
        &authority_state,
        &gas_object_id,
        &sender,
        &sender_key,
        "ObjectBasics",
        "update",
        vec![],
        vec![new_object_id1, new_object_id2],
        vec![],
        vec![],
    )
    .await
    .unwrap();

    // Check entry for object to be deleted is returned
    let (obj_ref, tx) = authority_state
        .get_latest_parent_entry(new_object_id1)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(obj_ref.0, new_object_id1);
    assert_eq!(obj_ref.1, SequenceNumber::from(2));
    assert_eq!(effects.transaction_digest, tx);

    let effects = call_framework_code(
        &authority_state,
        &gas_object_id,
        &sender,
        &sender_key,
        "ObjectBasics",
        "delete",
        vec![],
        vec![new_object_id1],
        vec![],
        vec![],
    )
    .await
    .unwrap();

    // Test get_latest_parent_entry function

    // The very first object returns None
    assert!(authority_state
        .get_latest_parent_entry(ObjectID::ZERO)
        .await
        .unwrap()
        .is_none());

    // The objects just after the gas object also returns None
    let mut x = gas_object_id.to_vec();
    let last_index = x.len() - 1;
    // Prevent overflow
    x[last_index] = u8::MAX - x[last_index];
    let unknown_object_id: ObjectID = x.try_into().unwrap();
    assert!(authority_state
        .get_latest_parent_entry(unknown_object_id)
        .await
        .unwrap()
        .is_none());

    // Check gas object is returned.
    let (obj_ref, tx) = authority_state
        .get_latest_parent_entry(gas_object_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(obj_ref.0, gas_object_id);
    assert_eq!(obj_ref.1, SequenceNumber::from(4));
    assert_eq!(effects.transaction_digest, tx);

    // Check entry for deleted object is returned
    let (obj_ref, tx) = authority_state
        .get_latest_parent_entry(new_object_id1)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(obj_ref.0, new_object_id1);
    assert_eq!(obj_ref.1, SequenceNumber::from(3));
    assert_eq!(obj_ref.2, ObjectDigest::OBJECT_DIGEST_DELETED);
    assert_eq!(effects.transaction_digest, tx);
}

#[tokio::test]
async fn test_account_state_ok() {
    let sender = dbg_addr(1);
    let object_id = dbg_object_id(1);

    let authority_state = init_state_with_object_id(sender, object_id).await;
    authority_state
        .get_object(&object_id)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn test_account_state_unknown_account() {
    let sender = dbg_addr(1);
    let unknown_address = dbg_object_id(99);
    let authority_state = init_state_with_object_id(sender, ObjectID::random()).await;
    assert!(authority_state
        .get_object(&unknown_address)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn test_authority_persist() {
    let (_, authority_key) = get_key_pair();
    let mut authorities = BTreeMap::new();
    authorities.insert(
        /* address */ *authority_key.public_key_bytes(),
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
    let authority = AuthorityState::new(
        committee.clone(),
        *authority_key.public_key_bytes(),
        // we assume that the node runner is in charge for its key -> it's ok to reopen a copy below.
        Arc::pin(authority_key.copy()),
        store,
        vec![],
        &mut genesis::get_genesis_context(),
    )
    .await;

    // Create an object
    let recipient = dbg_addr(2);
    let object_id = ObjectID::random();
    let obj = Object::with_id_owner_for_testing(object_id, recipient);

    // Store an object
    authority
        .init_transaction_lock((object_id, 0.into(), obj.digest()))
        .await;
    authority.insert_object(obj).await;

    // Close the authority
    drop(authority);

    // Reopen the authority with the same path
    let mut opts = rocksdb::Options::default();
    opts.set_max_open_files(max_files_authority_tests());
    let store = Arc::new(AuthorityStore::open(&path, Some(opts)));
    let authority2 = AuthorityState::new(
        committee,
        *authority_key.public_key_bytes(),
        Arc::pin(authority_key),
        store,
        vec![],
        &mut genesis::get_genesis_context(),
    )
    .await;
    let obj2 = authority2.get_object(&object_id).await.unwrap().unwrap();

    // Check the object is present
    assert_eq!(obj2.id(), object_id);
    assert_eq!(obj2.owner, recipient);
}

#[tokio::test]
async fn test_object_owning_another_object() {
    let (sender1, sender1_key) = get_key_pair();
    let (sender2, sender2_key) = get_key_pair();
    let gas1 = ObjectID::random();
    let gas2 = ObjectID::random();
    let authority = init_state_with_ids(vec![(sender1, gas1), (sender2, gas2)]).await;

    // Created 3 objects, all owned by sender1.
    let effects = create_move_object(&authority, &gas1, &sender1, &sender1_key)
        .await
        .unwrap();
    let (obj1, _, _) = effects.created[0].0;
    let effects = create_move_object(&authority, &gas1, &sender1, &sender1_key)
        .await
        .unwrap();
    let (obj2, _, _) = effects.created[0].0;
    let effects = create_move_object(&authority, &gas1, &sender1, &sender1_key)
        .await
        .unwrap();
    let (obj3, _, _) = effects.created[0].0;

    // Transfer obj1 to obj2.
    let effects = call_framework_code(
        &authority,
        &gas1,
        &sender1,
        &sender1_key,
        "ObjectBasics",
        "transfer_to_object",
        vec![],
        vec![obj1, obj2],
        vec![],
        vec![],
    )
    .await
    .unwrap();
    assert!(matches!(effects.status, ExecutionStatus::Success { .. }));
    assert_eq!(effects.mutated.len(), 3);
    assert_eq!(
        authority.get_object(&obj1).await.unwrap().unwrap().owner,
        obj2,
    );

    // Try to transfer obj1 to obj3, this time it will fail since obj1 is now owned by obj2,
    // and obj2 must be in the input to mutate obj1.
    let effects = call_framework_code(
        &authority,
        &gas1,
        &sender1,
        &sender1_key,
        "ObjectBasics",
        "transfer_to_object",
        vec![],
        vec![obj1, obj3],
        vec![],
        vec![],
    )
    .await;
    assert!(effects.unwrap_err().to_string().contains("IncorrectSigner"));

    // Try to transfer obj2 to obj1, this will create circular ownership and fail.
    let effects = call_framework_code(
        &authority,
        &gas1,
        &sender1,
        &sender1_key,
        "ObjectBasics",
        "transfer_to_object",
        vec![],
        vec![obj2, obj1],
        vec![],
        vec![],
    )
    .await
    .unwrap();
    assert!(effects
        .status
        .unwrap_err()
        .1
        .to_string()
        .contains("Circular object ownership detected"));

    // Transfer obj2 to sender2, now sender 2 owns obj2, which owns obj1.
    let effects = call_framework_code(
        &authority,
        &gas1,
        &sender1,
        &sender1_key,
        "ObjectBasics",
        "transfer",
        vec![],
        vec![obj2],
        vec![],
        vec![bcs::to_bytes(&AccountAddress::from(sender2)).unwrap()],
    )
    .await
    .unwrap();
    assert!(matches!(effects.status, ExecutionStatus::Success { .. }));
    assert_eq!(effects.mutated.len(), 2);
    assert_eq!(
        authority.get_object(&obj2).await.unwrap().unwrap().owner,
        sender2
    );

    // Sender 1 try to transfer obj1 to obj2 again.
    // This will fail since sender1 no longer owns obj2.
    let effects = call_framework_code(
        &authority,
        &gas1,
        &sender1,
        &sender1_key,
        "ObjectBasics",
        "transfer_to_object",
        vec![],
        vec![obj1, obj2],
        vec![],
        vec![],
    )
    .await;
    assert!(effects.unwrap_err().to_string().contains("IncorrectSigner"));

    // Sender2 transfers obj1 to obj2. This should be a successful noop
    // since obj1 is already owned by obj2.
    let effects = call_framework_code(
        &authority,
        &gas2,
        &sender2,
        &sender2_key,
        "ObjectBasics",
        "transfer_to_object",
        vec![],
        vec![obj1, obj2],
        vec![],
        vec![],
    )
    .await
    .unwrap();
    assert!(matches!(effects.status, ExecutionStatus::Success { .. }));
    assert_eq!(effects.mutated.len(), 3);
    assert_eq!(
        authority.get_object(&obj1).await.unwrap().unwrap().owner,
        obj2,
    );
}

#[tokio::test]
async fn test_object_wrapping_unwrapping() {
    let (sender, sender_key) = get_key_pair();
    let gas_object = Object::with_id_owner_for_testing(ObjectID::random(), sender);
    let gas_object_ref = gas_object.to_object_reference();
    let authority = init_state_with_objects(vec![gas_object]).await;

    // Compile the ObjectWrapping Move code.
    let build_config = BuildConfig::default();
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("src/unit_tests/data/object_wrapping/");
    let modules = sui_framework::build_move_package(&path, build_config, false).unwrap();

    // Publish the modules
    let all_module_bytes = modules
        .iter()
        .map(|m| {
            let mut module_bytes = Vec::new();
            m.serialize(&mut module_bytes).unwrap();
            module_bytes
        })
        .collect();
    let data = TransactionData::new_module(sender, gas_object_ref, all_module_bytes, MAX_GAS);
    let signature = Signature::new(&data, &sender_key);
    let transaction = Transaction::new(data, signature);
    let effects = send_and_confirm_transaction(&authority, transaction)
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
    let package_ref = effects.created[0].0;

    // Create a Child object.
    let effects = call_move(
        &authority,
        &gas_object_ref.0,
        &sender,
        &sender_key,
        &package_ref,
        ident_str!("ObjectWrapping").to_owned(),
        ident_str!("create_child").to_owned(),
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
        &gas_object_ref.0,
        &sender,
        &sender_key,
        &package_ref,
        ident_str!("ObjectWrapping").to_owned(),
        ident_str!("create_parent").to_owned(),
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
        &gas_object_ref.0,
        &sender,
        &sender_key,
        &package_ref,
        ident_str!("ObjectWrapping").to_owned(),
        ident_str!("extract_child").to_owned(),
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
        &gas_object_ref.0,
        &sender,
        &sender_key,
        &package_ref,
        ident_str!("ObjectWrapping").to_owned(),
        ident_str!("set_child").to_owned(),
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
        &gas_object_ref.0,
        &sender,
        &sender_key,
        &package_ref,
        ident_str!("ObjectWrapping").to_owned(),
        ident_str!("delete_parent").to_owned(),
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

// helpers

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

#[cfg(test)]
fn init_state_parameters() -> (Committee, SuiAddress, KeyPair, Arc<AuthorityStore>) {
    let (authority_address, authority_key) = get_key_pair();
    let mut authorities = BTreeMap::new();
    authorities.insert(
        /* address */ *authority_key.public_key_bytes(),
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
    let (committee, _, authority_key, store) = init_state_parameters();
    AuthorityState::new(
        committee,
        *authority_key.public_key_bytes(),
        Arc::pin(authority_key),
        store,
        genesis::clone_genesis_compiled_modules(),
        &mut genesis::get_genesis_context(),
    )
    .await
}

#[cfg(test)]
async fn init_state_with_ids<I: IntoIterator<Item = (SuiAddress, ObjectID)>>(
    objects: I,
) -> AuthorityState {
    let state = init_state().await;
    for (address, object_id) in objects {
        let obj = Object::with_id_owner_for_testing(object_id, address);
        state
            .init_transaction_lock((object_id, 0.into(), obj.digest()))
            .await;
        state.insert_object(obj).await;
    }
    state
}

pub async fn init_state_with_objects<I: IntoIterator<Item = Object>>(objects: I) -> AuthorityState {
    let state = init_state().await;

    for o in objects {
        let obj_ref = o.to_object_reference();
        state.insert_object(o).await;
        state.init_transaction_lock(obj_ref).await;
    }
    state
}

#[cfg(test)]
pub async fn init_state_with_object_id(address: SuiAddress, object: ObjectID) -> AuthorityState {
    init_state_with_ids(std::iter::once((address, object))).await
}

#[cfg(test)]
fn init_transfer_transaction(
    sender: SuiAddress,
    secret: &KeyPair,
    recipient: SuiAddress,
    object_ref: ObjectRef,
    gas_object_ref: ObjectRef,
) -> Transaction {
    let data = TransactionData::new_transfer(recipient, object_ref, sender, gas_object_ref);
    let signature = Signature::new(&data, secret);
    Transaction::new(data, signature)
}

#[cfg(test)]
fn init_certified_transfer_transaction(
    sender: SuiAddress,
    secret: &KeyPair,
    recipient: SuiAddress,
    object_ref: ObjectRef,
    gas_object_ref: ObjectRef,
    authority_state: &AuthorityState,
) -> CertifiedTransaction {
    let transfer_transaction =
        init_transfer_transaction(sender, secret, recipient, object_ref, gas_object_ref);
    let vote = SignedTransaction::new(
        transfer_transaction.clone(),
        authority_state.name,
        &*authority_state.secret,
    );
    let mut builder =
        SignatureAggregator::try_new(transfer_transaction, &authority_state.committee).unwrap();
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
                if p.serialized_module_map().keys().any(|name| name == module) {
                    Some(o.to_object_reference())
                } else {
                    None
                }
            }
            None => None,
        })
        .unwrap()
}

async fn call_move(
    authority: &AuthorityState,
    gas_object_id: &ObjectID,
    sender: &SuiAddress,
    sender_key: &KeyPair,
    package: &ObjectRef,
    module: Identifier,
    function: Identifier,
    type_args: Vec<TypeTag>,
    object_arg_ids: Vec<ObjectID>,
    shared_object_args_ids: Vec<ObjectID>,
    pure_args: Vec<Vec<u8>>,
) -> SuiResult<TransactionEffects> {
    let gas_object = authority.get_object(gas_object_id).await.unwrap();
    let gas_object_ref = gas_object.unwrap().to_object_reference();
    let mut object_args = vec![];
    for id in object_arg_ids {
        object_args.push(
            authority
                .get_object(&id)
                .await
                .unwrap()
                .unwrap()
                .to_object_reference(),
        );
    }
    let data = TransactionData::new_move_call(
        *sender,
        *package,
        module,
        function,
        type_args,
        gas_object_ref,
        object_args,
        shared_object_args_ids,
        pure_args,
        MAX_GAS,
    );

    let signature = Signature::new(&data, sender_key);
    let transaction = Transaction::new(data, signature);

    let response = send_and_confirm_transaction(authority, transaction).await?;
    Ok(response.signed_effects.unwrap().effects)
}

async fn call_framework_code(
    authority: &AuthorityState,
    gas_object_id: &ObjectID,
    sender: &SuiAddress,
    sender_key: &KeyPair,
    module: &'static str,
    function: &'static str,
    type_args: Vec<TypeTag>,
    object_arg_ids: Vec<ObjectID>,
    shared_object_arg_ids: Vec<ObjectID>,
    pure_args: Vec<Vec<u8>>,
) -> SuiResult<TransactionEffects> {
    let genesis_package_objects = genesis::clone_genesis_packages();
    let package_object_ref = get_genesis_package_by_module(&genesis_package_objects, module);

    call_move(
        authority,
        gas_object_id,
        sender,
        sender_key,
        &package_object_ref,
        ident_str!(module).to_owned(),
        ident_str!(function).to_owned(),
        type_args,
        object_arg_ids,
        shared_object_arg_ids,
        pure_args,
    )
    .await
}

pub async fn create_move_object(
    authority: &AuthorityState,
    gas_object_id: &ObjectID,
    sender: &SuiAddress,
    sender_key: &KeyPair,
) -> SuiResult<TransactionEffects> {
    call_framework_code(
        authority,
        gas_object_id,
        sender,
        sender_key,
        "ObjectBasics",
        "create",
        vec![],
        vec![],
        vec![],
        vec![
            16u64.to_le_bytes().to_vec(),
            bcs::to_bytes(&AccountAddress::from(*sender)).unwrap(),
        ],
    )
    .await
}

#[tokio::test]
async fn shared_object() {
    let (sender, keypair) = get_key_pair();

    // Initialize an authority with a (owned) gas object and a shared object.
    let gas_object_id = ObjectID::random();
    let gas_object = Object::with_id_owner_for_testing(gas_object_id, sender);
    let gas_object_ref = gas_object.to_object_reference();

    let shared_object_id = ObjectID::random();
    let shared_object = {
        use sui_types::gas_coin::GasCoin;
        use sui_types::object::MoveObject;

        let content = GasCoin::new(shared_object_id, SequenceNumber::new(), 10);
        let data = Data::Move(MoveObject::new(
            /* type */ GasCoin::type_(),
            content.to_bcs_bytes(),
        ));
        Object {
            data,
            owner: Owner::SharedMutable,
            previous_transaction: TransactionDigest::genesis(),
        }
    };

    let authority = init_state_with_objects(vec![gas_object, shared_object]).await;

    // Make a sample transaction.
    let module = "ObjectBasics";
    let function = "create";
    let genesis_package_objects = genesis::clone_genesis_packages();
    let package_object_ref = get_genesis_package_by_module(&genesis_package_objects, module);

    let data = TransactionData::new_move_call(
        sender,
        package_object_ref,
        ident_str!(module).to_owned(),
        ident_str!(function).to_owned(),
        /* type_args */ vec![],
        gas_object_ref,
        /* object_args */ vec![],
        vec![shared_object_id],
        /* pure_args */
        vec![
            16u64.to_le_bytes().to_vec(),
            bcs::to_bytes(&AccountAddress::from(sender)).unwrap(),
        ],
        MAX_GAS,
    );
    let signature = Signature::new(&data, &keypair);
    let transaction = Transaction::new(data, signature);
    let transaction_digest = transaction.digest();

    // Submit the transaction and assemble a certificate.
    let response = authority
        .handle_transaction(transaction.clone())
        .await
        .unwrap();
    let vote = response.signed_transaction.unwrap();
    let certificate = SignatureAggregator::try_new(transaction, &authority.committee)
        .unwrap()
        .append(vote.authority, vote.signature)
        .unwrap()
        .unwrap();
    let confirmation_transaction = ConfirmationTransaction::new(certificate.clone());

    // Sending the certificate now fails since it was not sequenced.
    let result = authority
        .handle_confirmation_transaction(confirmation_transaction.clone())
        .await;
    assert!(matches!(result, Err(SuiError::LockErrors { .. })));

    // Sequence the certificate to assign a sequence number to the shared object.
    authority
        .handle_consensus_certificate(&certificate)
        .await
        .unwrap();

    let shared_object_version = authority
        .db()
        .sequenced(transaction_digest, shared_object_id)
        .unwrap()
        .unwrap();
    assert_eq!(shared_object_version, SequenceNumber::new());

    // Finally process the certificate and execute the contract. Ensure that the
    // shared object lock is cleaned up and that its sequence number increased.
    authority
        .handle_confirmation_transaction(confirmation_transaction)
        .await
        .unwrap();

    let shared_object_lock = authority
        .db()
        .sequenced(transaction_digest, shared_object_id)
        .unwrap();
    assert!(shared_object_lock.is_none());

    let shared_object_version = authority
        .get_object(&shared_object_id)
        .await
        .unwrap()
        .unwrap()
        .version();
    assert_eq!(shared_object_version, SequenceNumber::from(1));
}
