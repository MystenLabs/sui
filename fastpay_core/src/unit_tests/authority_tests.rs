// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

use super::*;
use bcs;
use fastx_adapter::genesis;
#[cfg(test)]
use fastx_types::base_types::dbg_addr;
use move_binary_format::{
    file_format::{self, AddressIdentifierIndex, IdentifierIndex, ModuleHandle},
    CompiledModule,
};
use move_core_types::ident_str;

use std::env;
use std::fs;

#[tokio::test]
async fn test_handle_transfer_order_bad_signature() {
    let (sender, sender_key) = get_key_pair();
    let recipient = Address::FastPay(dbg_addr(2));
    let object_id = ObjectID::random();
    let authority_state = init_state_with_object(sender, object_id).await;
    let transfer_order = init_transfer_order(sender, &sender_key, recipient, object_id);
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
    let recipient = Address::FastPay(dbg_addr(2));
    let authority_state = init_state_with_object(sender, object_id).await;
    let transfer_order = init_transfer_order(unknown_address, &sender_key, recipient, object_id);

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
    sequence_number_state_sender_account.next_sequence_number =
        sequence_number_state_sender_account
            .next_sequence_number
            .increment()
            .unwrap();
    assert!(sequence_number_state
        .handle_transfer_order(transfer_order)
        .is_err());

        let object = sequence_number_state.objects.get(&object_id).unwrap();
        assert!(sequence_number_state.get_order_lock(object.id, object.next_sequence_number).unwrap().is_none());
}
*/

#[tokio::test]
async fn test_handle_transfer_order_ok() {
    let (sender, sender_key) = get_key_pair();
    let recipient = Address::FastPay(dbg_addr(2));
    let object_id = ObjectID::random();
    let authority_state = init_state_with_object(sender, object_id).await;
    let transfer_order = init_transfer_order(sender, &sender_key, recipient, object_id);

    // Check the initial state of the locks
    assert!(authority_state
        .get_order_lock(&(object_id, 0.into()))
        .await
        .unwrap()
        .is_none());
    assert!(authority_state
        .get_order_lock(&(object_id, 1.into()))
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
    assert_eq!(
        account_info.pending_confirmation.unwrap(),
        pending_confirmation
    );

    // Check the final state of the locks
    assert!(authority_state
        .get_order_lock(&(object_id, 0.into()))
        .await
        .unwrap()
        .is_some());
    assert_eq!(
        authority_state
            .get_order_lock(&(object_id, 0.into()))
            .await
            .unwrap()
            .as_ref()
            .unwrap()
            .order
            .kind,
        transfer_order.kind
    );
}

async fn send_and_confirm_order(
    authority: &mut AuthorityState,
    order: Order,
) -> Result<AccountInfoResponse, FastPayError> {
    // Make the initial request
    let response = authority.handle_order(order.clone()).await.unwrap();
    let vote = response.pending_confirmation.unwrap();

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

// Test that publishing a module that depends on an existing one works
#[tokio::test]
async fn test_publish_dependent_module_ok() {
    let (sender, sender_key) = get_key_pair();
    // create a dummy gas payment object. ok for now because we don't check gas
    let gas_payment_object_id = ObjectID::random();
    let gas_payment_object = Object::with_id_owner_for_testing(gas_payment_object_id, sender);
    let gas_payment_object_ref = gas_payment_object.to_object_reference();
    // create a genesis state that contains the gas object and genesis modules
    let genesis = genesis::GENESIS.lock().unwrap();
    let mut genesis_module_objects = genesis.objects.clone();
    let genesis_module = match &genesis_module_objects[0].data {
        Data::Module(m) => CompiledModule::deserialize(m).unwrap(),
        _ => unreachable!(),
    };
    // create a module that depends on a genesis module
    let dependent_module = make_dependent_module(&genesis_module);
    let dependent_module_bytes = {
        let mut bytes = Vec::new();
        dependent_module.serialize(&mut bytes).unwrap();
        bytes
    };
    genesis_module_objects.push(gas_payment_object);
    let mut authority = init_state_with_objects(genesis_module_objects).await;

    let order = Order::new_module(
        sender,
        gas_payment_object_ref,
        vec![dependent_module_bytes],
        &sender_key,
    );
    let dependent_module_id = TxContext::new(order.digest()).fresh_id();
    let response = send_and_confirm_order(&mut authority, order).await.unwrap();
    // check that the dependent module got published
    assert!(response.object_id == dependent_module_id);
}

// Test that publishing a module with no dependencies works
#[tokio::test]
async fn test_publish_module_no_dependencies_ok() {
    let (sender, sender_key) = get_key_pair();
    // create a dummy gas payment object. ok for now because we don't check gas
    let gas_payment_object_id = ObjectID::random();
    let gas_payment_object = Object::with_id_owner_for_testing(gas_payment_object_id, sender);
    let gas_payment_object_ref = gas_payment_object.to_object_reference();
    let mut authority = init_state_with_objects(vec![gas_payment_object]).await;

    let module = file_format::empty_module();
    let mut module_bytes = Vec::new();
    module.serialize(&mut module_bytes).unwrap();
    let order = Order::new_module(
        sender,
        gas_payment_object_ref,
        vec![module_bytes],
        &sender_key,
    );
    let module_object_id = TxContext::new(order.digest()).fresh_id();
    let response = send_and_confirm_order(&mut authority, order).await.unwrap();
    // check that the module actually got published
    assert!(response.object_id == module_object_id);
}

#[tokio::test]
async fn test_handle_move_order() {
    let (sender, sender_key) = get_key_pair();
    // create a dummy gas payment object. ok for now because we don't check gas
    let gas_payment_object_id = ObjectID::random();
    let gas_payment_object = Object::with_id_owner_for_testing(gas_payment_object_id, sender);
    let gas_payment_object_ref = gas_payment_object.to_object_reference();
    // find the function Object::create and call it to create a new object
    let genesis = genesis::GENESIS.lock().unwrap();
    let mut genesis_module_objects = genesis.objects.clone();
    let module_object_ref = genesis_module_objects
        .iter()
        .find_map(|o| match o.data.as_module() {
            Some(m) => {
                if m.self_id().name() == ident_str!("ObjectBasics") {
                    Some((*m.self_id().address(), SequenceNumber::new()))
                } else {
                    None
                }
            }
            None => None,
        })
        .unwrap();

    genesis_module_objects.push(gas_payment_object);
    let mut authority_state = init_state_with_objects(genesis_module_objects).await;
    authority_state.native_functions = genesis.native_functions.clone();

    let function = ident_str!("create").to_owned();
    let order = Order::new_move_call(
        sender,
        module_object_ref,
        function,
        Vec::new(),
        gas_payment_object_ref,
        Vec::new(),
        vec![
            16u64.to_le_bytes().to_vec(),
            bcs::to_bytes(&sender.to_vec()).unwrap(),
        ],
        1000,
        &sender_key,
    );
    let res = send_and_confirm_order(&mut authority_state, order)
        .await
        .unwrap();
    let created_object_id = res.object_id;
    // check that order actually created an object with the expected ID, owner, sequence number
    let created_obj = authority_state
        .object_state(&created_object_id)
        .await
        .unwrap();
    assert_eq!(created_obj.owner, sender,);
    assert_eq!(created_obj.id(), created_object_id);
    assert_eq!(created_obj.next_sequence_number, SequenceNumber::new());
}

#[tokio::test]
async fn test_handle_transfer_order_double_spend() {
    let (sender, sender_key) = get_key_pair();
    let recipient = Address::FastPay(dbg_addr(2));
    let object_id = ObjectID::random();
    let authority_state = init_state_with_object(sender, object_id).await;
    let transfer_order = init_transfer_order(sender, &sender_key, recipient, object_id);

    let signed_order = authority_state
        .handle_order(transfer_order.clone())
        .await
        .unwrap();
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
    let authority_state = init_state_with_object(sender, object_id).await;

    // Record the old sequence number
    let old_seq_num;
    {
        let old_account = authority_state.object_state(&object_id).await.unwrap();
        old_seq_num = old_account.next_sequence_number;
    }

    let certified_transfer_order = init_certified_transfer_order(
        sender,
        &sender_key,
        Address::FastPay(recipient),
        object_id,
        &authority_state,
    );

    // Increment the sequence number
    {
        let mut sender_object = authority_state.object_state(&object_id).await.unwrap();
        sender_object.next_sequence_number =
            sender_object.next_sequence_number.increment().unwrap();
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
    assert_eq!(
        old_seq_num.increment().unwrap(),
        new_account.next_sequence_number
    );

    // No recipient object was created.
    assert!(authority_state
        .object_state(&dbg_object_id(2))
        .await
        .is_err());
}

#[tokio::test]
async fn test_handle_confirmation_order_exceed_balance() {
    let (sender, sender_key) = get_key_pair();
    let object_id: ObjectID = ObjectID::random();
    let recipient = dbg_addr(2);
    let authority_state = init_state_with_object(sender, object_id).await;

    let certified_transfer_order = init_certified_transfer_order(
        sender,
        &sender_key,
        Address::FastPay(recipient),
        object_id,
        &authority_state,
    );
    assert!(authority_state
        .handle_confirmation_order(ConfirmationOrder::new(certified_transfer_order))
        .await
        .is_ok());
    let new_account = authority_state.object_state(&object_id).await.unwrap();
    assert_eq!(SequenceNumber::from(1), new_account.next_sequence_number);
    assert!(authority_state
        .parent(&(object_id, new_account.next_sequence_number))
        .await
        .is_some());
}

#[tokio::test]
async fn test_handle_confirmation_order_receiver_balance_overflow() {
    let (sender, sender_key) = get_key_pair();
    let object_id: ObjectID = ObjectID::random();
    let (recipient, _) = get_key_pair();
    let authority_state =
        init_state_with_ids(vec![(sender, object_id), (recipient, ObjectID::random())]).await;

    let certified_transfer_order = init_certified_transfer_order(
        sender,
        &sender_key,
        Address::FastPay(recipient),
        object_id,
        &authority_state,
    );
    assert!(authority_state
        .handle_confirmation_order(ConfirmationOrder::new(certified_transfer_order))
        .await
        .is_ok());
    let new_sender_account = authority_state.object_state(&object_id).await.unwrap();
    assert_eq!(
        SequenceNumber::from(1),
        new_sender_account.next_sequence_number
    );

    assert!(authority_state
        .parent(&(object_id, new_sender_account.next_sequence_number))
        .await
        .is_some());
}

#[tokio::test]
async fn test_handle_confirmation_order_receiver_equal_sender() {
    let (address, key) = get_key_pair();
    let object_id: ObjectID = ObjectID::random();
    let authority_state = init_state_with_object(address, object_id).await;

    let certified_transfer_order = init_certified_transfer_order(
        address,
        &key,
        Address::FastPay(address),
        object_id,
        &authority_state,
    );
    assert!(authority_state
        .handle_confirmation_order(ConfirmationOrder::new(certified_transfer_order))
        .await
        .is_ok());
    let account = authority_state.object_state(&object_id).await.unwrap();
    assert_eq!(SequenceNumber::from(1), account.next_sequence_number);

    assert!(authority_state
        .parent(&(object_id, account.next_sequence_number))
        .await
        .is_some());
}

#[tokio::test]
async fn test_handle_confirmation_order_ok() {
    let (sender, sender_key) = get_key_pair();
    let recipient = dbg_addr(2);
    let object_id = ObjectID::random();
    let authority_state = init_state_with_object(sender, object_id).await;
    let certified_transfer_order = init_certified_transfer_order(
        sender,
        &sender_key,
        Address::FastPay(recipient),
        object_id,
        &authority_state,
    );

    let old_account = authority_state.object_state(&object_id).await.unwrap();
    let mut next_sequence_number = old_account.next_sequence_number;
    next_sequence_number = next_sequence_number.increment().unwrap();

    let info = authority_state
        .handle_confirmation_order(ConfirmationOrder::new(certified_transfer_order.clone()))
        .await
        .unwrap();
    // Key check: the ownership has changed
    assert_eq!(recipient, info.owner);
    assert_eq!(next_sequence_number, info.next_sequence_number);
    assert_eq!(None, info.pending_confirmation);
    assert_eq!(
        {
            let refx = authority_state
                .parent(&(object_id, info.next_sequence_number))
                .await
                .unwrap();
            authority_state.read_certificate(&refx).await.unwrap()
        },
        Some(certified_transfer_order)
    );

    // Check locks are set and archived correctly
    assert!(authority_state
        .get_order_lock(&(object_id, 0.into()))
        .await
        .is_err());
    assert!(authority_state
        .get_order_lock(&(object_id, 1.into()))
        .await
        .expect("Exists")
        .is_none());
}

#[tokio::test]
async fn test_account_state_ok() {
    let sender = dbg_addr(1);
    let object_id = dbg_object_id(1);

    let authority_state = init_state_with_object(sender, object_id).await;
    authority_state.object_state(&object_id).await.unwrap();
}

#[tokio::test]
async fn test_account_state_unknown_account() {
    let sender = dbg_addr(1);
    let unknown_address = dbg_object_id(99);
    let authority_state = init_state_with_object(sender, ObjectID::random()).await;
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
    opts.set_max_open_files(10);
    let store = Arc::new(AuthorityStore::open(&path, Some(opts)));
    let authority = AuthorityState::new(
        committee.clone(),
        authority_address,
        authority_key.copy(),
        store,
    );

    // Create an object
    let recipient = dbg_addr(2);
    let object_id = ObjectID::random();
    let mut obj = Object::with_id_for_testing(object_id);
    obj.transfer(recipient);

    // Store an object
    authority.insert_object(obj).await;
    authority.init_order_lock((object_id, 0.into())).await;

    // Close the authority
    drop(authority);

    // Reopen the authority with the same path
    let mut opts = rocksdb::Options::default();
    opts.set_max_open_files(10);
    let store = Arc::new(AuthorityStore::open(&path, Some(opts)));
    let authority2 = AuthorityState::new(committee, authority_address, authority_key, store);
    let obj2 = authority2.object_state(&object_id).await.unwrap();

    // Check the object is present
    assert_eq!(obj2.id(), object_id);
    assert_eq!(obj2.owner, recipient);
}

// helpers

#[cfg(test)]
fn init_state() -> AuthorityState {
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
    opts.set_max_open_files(10);
    let store = Arc::new(AuthorityStore::open(path, Some(opts)));
    AuthorityState::new(committee, authority_address, authority_key, store)
}

#[cfg(test)]
async fn init_state_with_ids<I: IntoIterator<Item = (FastPayAddress, ObjectID)>>(
    objects: I,
) -> AuthorityState {
    let state = init_state();
    for (address, object_id) in objects {
        let mut obj = Object::with_id_for_testing(object_id);
        obj.transfer(address);
        state.insert_object(obj).await;
        state.init_order_lock((object_id, 0.into())).await;
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
async fn init_state_with_object(address: FastPayAddress, object: ObjectID) -> AuthorityState {
    init_state_with_ids(std::iter::once((address, object))).await
}

#[cfg(test)]
fn init_transfer_order(
    sender: FastPayAddress,
    secret: &KeyPair,
    recipient: Address,
    object_id: ObjectID,
) -> Order {
    let transfer = Transfer {
        object_id,
        sender,
        recipient,
        sequence_number: SequenceNumber::new(),
        user_data: UserData::default(),
    };
    Order::new_transfer(transfer, secret)
}

#[cfg(test)]
fn init_certified_transfer_order(
    sender: FastPayAddress,
    secret: &KeyPair,
    recipient: Address,
    object_id: ObjectID,
    authority_state: &AuthorityState,
) -> CertifiedOrder {
    let transfer_order = init_transfer_order(sender, secret, recipient, object_id);
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
