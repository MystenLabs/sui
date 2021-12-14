// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

use super::*;
#[cfg(test)]
use fastx_types::base_types::dbg_addr;
use move_binary_format::file_format;
use move_core_types::ident_str;
#[cfg(test)]
use move_core_types::language_storage::ModuleId;

#[test]
fn test_handle_transfer_order_bad_signature() {
    let (sender, sender_key) = get_key_pair();
    let recipient = Address::FastPay(dbg_addr(2));
    let object_id = ObjectID::random();
    let mut authority_state = init_state_with_object(sender, object_id);
    let transfer_order = init_transfer_order(sender, &sender_key, recipient, object_id);
    let object_id = *transfer_order.object_id();
    let (_unknown_address, unknown_key) = get_key_pair();
    let mut bad_signature_transfer_order = transfer_order.clone();
    bad_signature_transfer_order.signature = Signature::new(&transfer_order.kind, &unknown_key);
    assert!(authority_state
        .handle_order(bad_signature_transfer_order)
        .is_err());

    let object = authority_state.objects.get(&object_id).unwrap();
    assert!(authority_state
        .get_order_lock(&object.to_object_reference())
        .unwrap()
        .is_none());

    assert!(authority_state
        .get_order_lock(&object.to_object_reference())
        .unwrap()
        .is_none());
}

#[test]
fn test_handle_transfer_order_unknown_sender() {
    let (sender, sender_key) = get_key_pair();
    let (unknown_address, unknown_key) = get_key_pair();
    let object_id: ObjectID = ObjectID::random();
    let recipient = Address::FastPay(dbg_addr(2));
    let mut authority_state = init_state_with_object(sender, object_id);
    let transfer_order = init_transfer_order(unknown_address, &sender_key, recipient, object_id);

    let unknown_sender_transfer = transfer_order.kind;
    let unknown_sender_transfer_order = Order::new(unknown_sender_transfer, &unknown_key);
    assert!(authority_state
        .handle_order(unknown_sender_transfer_order)
        .is_err());

    let object = authority_state.objects.get(&object_id).unwrap();
    assert!(authority_state
        .get_order_lock(&object.to_object_reference())
        .unwrap()
        .is_none());

    assert!(authority_state
        .get_order_lock(&object.to_object_reference())
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

#[test]
fn test_handle_transfer_order_ok() {
    let (sender, sender_key) = get_key_pair();
    let recipient = Address::FastPay(dbg_addr(2));
    let object_id = ObjectID::random();
    let mut authority_state = init_state_with_object(sender, object_id);
    let transfer_order = init_transfer_order(sender, &sender_key, recipient, object_id);

    // Check the initial state of the locks
    assert!(authority_state
        .get_order_lock(&(object_id, 0.into()))
        .unwrap()
        .is_none());
    assert!(authority_state
        .get_order_lock(&(object_id, 1.into()))
        .is_err());

    let account_info = authority_state
        .handle_order(transfer_order.clone())
        .unwrap();

    let object = authority_state.objects.get(&object_id).unwrap();
    let pending_confirmation = authority_state
        .get_order_lock(&object.to_object_reference())
        .unwrap()
        .clone()
        .unwrap();
    assert_eq!(
        account_info.pending_confirmation.unwrap(),
        pending_confirmation
    );

    // Check the final state of the locks
    assert!(authority_state
        .get_order_lock(&(object_id, 0.into()))
        .unwrap()
        .is_some());
    assert_eq!(
        authority_state
            .get_order_lock(&(object_id, 0.into()))
            .unwrap()
            .as_ref()
            .unwrap()
            .order
            .kind,
        transfer_order.kind
    );
}

fn send_and_confirm_order(
    authority: &mut AuthorityState,
    order: Order,
) -> Result<AccountInfoResponse, FastPayError> {
    // Make the initial request
    let response = authority.handle_order(order.clone()).unwrap();
    let vote = response.pending_confirmation.unwrap();

    // Collect signatures from a quorum of authorities
    let mut builder = SignatureAggregator::try_new(order, &authority.committee).unwrap();
    let certificate = builder
        .append(vote.authority, vote.signature)
        .unwrap()
        .unwrap();
    // Submit the confirmation. *Now* execution actually happens, and it should fail when we try to look up our dummy module.
    // we unfortunately don't get a very descriptive error message, but we can at least see that something went wrong inside the VM
    authority.handle_confirmation_order(ConfirmationOrder::new(certificate))
}

#[test]
fn test_publish_module_ok() {
    let (sender, sender_key) = get_key_pair();
    // create a dummy gas payment object. ok for now because we don't check gas
    let gas_payment_object_id = ObjectID::random();
    let gas_payment_object = Object::with_id_owner_for_testing(gas_payment_object_id, sender);
    let gas_payment_object_ref = gas_payment_object.to_object_reference();
    let mut authority = init_state_with_objects(vec![gas_payment_object]);

    let mut module = file_format::empty_module();
    module.address_identifiers[0] = sender.to_address_hack();
    let mut module_bytes = Vec::new();
    module.serialize(&mut module_bytes).unwrap();
    let order = Order::new_module(
        sender,
        gas_payment_object_ref,
        vec![module_bytes],
        &sender_key,
    );
    let module_object_id = TxContext::new(order.digest()).fresh_id();
    let response = send_and_confirm_order(&mut authority, order).unwrap();
    // check that the module actually got published
    assert!(response.object_id == module_object_id);
    // TODO: eventually, `response` should return a vector of ID's created + we should check non-emptiness of this vector
}

#[test]
fn test_handle_move_order_bad() {
    let (sender, sender_key) = get_key_pair();
    // create a dummy gas payment object. ok for now because we don't check gas
    let gas_payment_object_id = ObjectID::random();
    let gas_payment_object = Object::with_id_owner_for_testing(gas_payment_object_id, sender);
    let gas_payment_object_ref = gas_payment_object.to_object_reference();
    // create a dummy module. execution will fail when we try to read it
    let dummy_module_object_id = ObjectID::random();
    let dummy_module_object = Object::with_id_owner_for_testing(dummy_module_object_id, sender);

    let mut authority_state =
        init_state_with_objects(vec![gas_payment_object, dummy_module_object]);

    let module_id = ModuleId::new(dummy_module_object_id, ident_str!("Module").to_owned());
    let function = ident_str!("function_name").to_owned();
    let order = Order::new_move_call(
        sender,
        module_id,
        function,
        Vec::new(),
        gas_payment_object_ref,
        Vec::new(),
        Vec::new(),
        1000,
        &sender_key,
    );

    // Submit the confirmation. *Now* execution actually happens, and it should fail when we try to look up our dummy module.
    // we unfortunately don't get a very descriptive error message, but we can at least see that something went wrong inside the VM
    match send_and_confirm_order(&mut authority_state, order) {
        Err(FastPayError::MoveExecutionFailure) => (),
        r => panic!("Unexpected result {:?}", r),
    }
}

#[test]
fn test_handle_transfer_order_double_spend() {
    let (sender, sender_key) = get_key_pair();
    let recipient = Address::FastPay(dbg_addr(2));
    let object_id = ObjectID::random();
    let mut authority_state = init_state_with_object(sender, object_id);
    let transfer_order = init_transfer_order(sender, &sender_key, recipient, object_id);

    let signed_order = authority_state
        .handle_order(transfer_order.clone())
        .unwrap();
    let double_spend_signed_order = authority_state.handle_order(transfer_order).unwrap();
    assert_eq!(signed_order, double_spend_signed_order);
}

#[test]
fn test_handle_confirmation_order_unknown_sender() {
    let recipient = dbg_addr(2);
    let (sender, sender_key) = get_key_pair();
    let mut authority_state = init_state();
    let certified_transfer_order = init_certified_transfer_order(
        sender,
        &sender_key,
        Address::FastPay(recipient),
        ObjectID::random(),
        &authority_state,
    );

    assert!(authority_state
        .handle_confirmation_order(ConfirmationOrder::new(certified_transfer_order))
        .is_err());
}

#[test]
fn test_handle_confirmation_order_bad_sequence_number() {
    let (sender, sender_key) = get_key_pair();
    let object_id: ObjectID = ObjectID::random();
    let recipient = dbg_addr(2);
    let mut authority_state = init_state_with_object(sender, object_id);
    let sender_account = authority_state.objects.get_mut(&object_id).unwrap();
    sender_account.next_sequence_number = sender_account.next_sequence_number.increment().unwrap();

    let old_seq_num;
    {
        let old_account = authority_state.objects.get_mut(&object_id).unwrap();
        old_seq_num = old_account.next_sequence_number;
    }

    let certified_transfer_order = init_certified_transfer_order(
        sender,
        &sender_key,
        Address::FastPay(recipient),
        object_id,
        &authority_state,
    );
    // Replays are ignored.

    assert!(authority_state
        .handle_confirmation_order(ConfirmationOrder::new(certified_transfer_order))
        .is_err());

    let new_account = authority_state.objects.get_mut(&object_id).unwrap();
    assert_eq!(old_seq_num, new_account.next_sequence_number);

    assert!(authority_state
        .parent_sync
        .get(&(object_id, new_account.next_sequence_number))
        .is_none());

    assert!(authority_state.objects.get(&dbg_object_id(2)).is_none());
}

#[test]
fn test_handle_confirmation_order_exceed_balance() {
    let (sender, sender_key) = get_key_pair();
    let object_id: ObjectID = ObjectID::random();
    let recipient = dbg_addr(2);
    let mut authority_state = init_state_with_object(sender, object_id);

    let certified_transfer_order = init_certified_transfer_order(
        sender,
        &sender_key,
        Address::FastPay(recipient),
        object_id,
        &authority_state,
    );
    assert!(authority_state
        .handle_confirmation_order(ConfirmationOrder::new(certified_transfer_order))
        .is_ok());
    let new_account = authority_state.objects.get(&object_id).unwrap();
    assert_eq!(SequenceNumber::from(1), new_account.next_sequence_number);
    assert!(authority_state
        .parent_sync
        .get(&(object_id, new_account.next_sequence_number))
        .is_some());
}

#[test]
fn test_handle_confirmation_order_receiver_balance_overflow() {
    let (sender, sender_key) = get_key_pair();
    let object_id: ObjectID = ObjectID::random();
    let (recipient, _) = get_key_pair();
    let mut authority_state =
        init_state_with_ids(vec![(sender, object_id), (recipient, ObjectID::random())]);

    let certified_transfer_order = init_certified_transfer_order(
        sender,
        &sender_key,
        Address::FastPay(recipient),
        object_id,
        &authority_state,
    );
    assert!(authority_state
        .handle_confirmation_order(ConfirmationOrder::new(certified_transfer_order))
        .is_ok());
    let new_sender_account = authority_state.objects.get(&object_id).unwrap();
    assert_eq!(
        SequenceNumber::from(1),
        new_sender_account.next_sequence_number
    );

    assert!(authority_state
        .parent_sync
        .get(&(object_id, new_sender_account.next_sequence_number))
        .is_some());
}

#[test]
fn test_handle_confirmation_order_receiver_equal_sender() {
    let (address, key) = get_key_pair();
    let object_id: ObjectID = ObjectID::random();
    let mut authority_state = init_state_with_object(address, object_id);

    let certified_transfer_order = init_certified_transfer_order(
        address,
        &key,
        Address::FastPay(address),
        object_id,
        &authority_state,
    );
    assert!(authority_state
        .handle_confirmation_order(ConfirmationOrder::new(certified_transfer_order))
        .is_ok());
    let account = authority_state.objects.get(&object_id).unwrap();
    assert_eq!(SequenceNumber::from(1), account.next_sequence_number);

    assert!(authority_state
        .parent_sync
        .get(&(object_id, account.next_sequence_number))
        .is_some());
}

#[test]
fn test_handle_confirmation_order_ok() {
    let (sender, sender_key) = get_key_pair();
    let recipient = dbg_addr(2);
    let object_id = ObjectID::random();
    let mut authority_state = init_state_with_object(sender, object_id);
    let certified_transfer_order = init_certified_transfer_order(
        sender,
        &sender_key,
        Address::FastPay(recipient),
        object_id,
        &authority_state,
    );

    let old_account = authority_state.objects.get_mut(&object_id).unwrap();
    let mut next_sequence_number = old_account.next_sequence_number;
    next_sequence_number = next_sequence_number.increment().unwrap();

    let info = authority_state
        .handle_confirmation_order(ConfirmationOrder::new(certified_transfer_order.clone()))
        .unwrap();
    // Key check: the ownership has changed
    assert_eq!(recipient, info.owner);
    assert_eq!(next_sequence_number, info.next_sequence_number);
    assert_eq!(None, info.pending_confirmation);
    assert_eq!(
        authority_state.certificates.get(
            authority_state
                .parent_sync
                .get(&(object_id, info.next_sequence_number))
                .unwrap()
        ),
        Some(&certified_transfer_order)
    );

    // Check locks are set and archived correctly
    assert!(authority_state
        .get_order_lock(&(object_id, 0.into()))
        .is_err());
    assert!(authority_state
        .get_order_lock(&(object_id, 1.into()))
        .expect("Exists")
        .is_none());
}

#[test]
fn test_account_state_ok() {
    let sender = dbg_addr(1);
    let object_id = dbg_object_id(1);

    let authority_state = init_state_with_object(sender, object_id);
    assert_eq!(
        authority_state.objects.get(&object_id).unwrap(),
        authority_state.object_state(&object_id).unwrap()
    );
}

#[test]
fn test_account_state_unknown_account() {
    let sender = dbg_addr(1);
    let unknown_address = dbg_object_id(99);
    let authority_state = init_state_with_object(sender, ObjectID::random());
    assert!(authority_state.object_state(&unknown_address).is_err());
}

#[test]
fn test_get_shards() {
    let num_shards = 16u32;
    let mut found = vec![false; num_shards as usize];
    let mut left = num_shards;
    loop {
        let object_id = ObjectID::random();
        let shard = AuthorityState::get_shard(num_shards, &object_id) as usize;
        println!("found {}", shard);
        if !found[shard] {
            found[shard] = true;
            left -= 1;
            if left == 0 {
                break;
            }
        }
    }
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
    AuthorityState::new(committee, authority_address, authority_key)
}

#[cfg(test)]
fn init_state_with_ids<I: IntoIterator<Item = (FastPayAddress, ObjectID)>>(
    objects: I,
) -> AuthorityState {
    let mut state = init_state();
    for (address, object_id) in objects {
        let account = state
            .objects
            .entry(object_id)
            .or_insert_with(|| Object::with_id_for_testing(object_id));
        account.transfer(address);

        state.init_order_lock((object_id, 0.into()));
    }
    state
}

fn init_state_with_objects<I: IntoIterator<Item = Object>>(objects: I) -> AuthorityState {
    let mut state = init_state();
    for o in objects {
        let obj_ref = o.to_object_reference();
        state.objects.insert(o.id(), o);
        state.init_order_lock(obj_ref);
    }
    state
}

#[cfg(test)]
fn init_state_with_object(address: FastPayAddress, object: ObjectID) -> AuthorityState {
    init_state_with_ids(std::iter::once((address, object)))
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
