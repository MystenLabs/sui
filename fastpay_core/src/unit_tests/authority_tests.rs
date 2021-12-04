// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

use super::*;

#[test]
fn test_handle_transfer_order_bad_signature() {
    let (sender, sender_key) = get_key_pair();
    let recipient = Address::FastPay(dbg_addr(2));
    let object_id = get_object_id();
    let mut authority_state = init_state_with_object(sender, object_id);
    let transfer_order = init_transfer_order(sender, &sender_key, recipient, object_id);
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
    let object_id: ObjectID = get_object_id();
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
    let object_id = get_object_id();
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

#[test]
fn test_handle_transfer_order_double_spend() {
    let (sender, sender_key) = get_key_pair();
    let recipient = Address::FastPay(dbg_addr(2));
    let object_id = get_object_id();
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
        get_object_id(),
        &authority_state,
    );

    assert!(authority_state
        .handle_confirmation_order(ConfirmationOrder::new(certified_transfer_order))
        .is_err());
}

#[test]
fn test_handle_confirmation_order_bad_sequence_number() {
    let (sender, sender_key) = get_key_pair();
    let object_id: ObjectID = get_object_id();
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
    let object_id: ObjectID = get_object_id();
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
    let object_id: ObjectID = get_object_id();
    let (recipient, _) = get_key_pair();
    let mut authority_state =
        init_state_with_objects(vec![(sender, object_id), (recipient, ObjectID::random())]);

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
    let object_id: ObjectID = get_object_id();
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
    let object_id = get_object_id();
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
    let authority_state = init_state_with_object(sender, get_object_id());
    assert!(authority_state.object_state(&unknown_address).is_err());
}

#[test]
fn test_get_shards() {
    let num_shards = 16u32;
    let mut found = vec![false; num_shards as usize];
    let mut left = num_shards;
    loop {
        let object_id = get_object_id();
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
fn init_state_with_objects<I: IntoIterator<Item = (FastPayAddress, ObjectID)>>(
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

#[cfg(test)]
fn init_state_with_object(address: FastPayAddress, object: ObjectID) -> AuthorityState {
    init_state_with_objects(std::iter::once((address, object)))
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
