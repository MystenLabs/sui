// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0
#![allow(clippy::same_item_push)] // get_key_pair returns random elements

use super::*;
use crate::authority::{Authority, AuthorityState};
use fastx_types::object::Object;
use futures::lock::Mutex;
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};
use tokio::runtime::Runtime;

#[derive(Clone)]
struct LocalAuthorityClient(Arc<Mutex<AuthorityState>>);

impl AuthorityClient for LocalAuthorityClient {
    fn handle_order(&mut self, order: Order) -> AsyncResult<'_, AccountInfoResponse, FastPayError> {
        let state = self.0.clone();
        Box::pin(async move { state.lock().await.handle_order(order) })
    }

    fn handle_confirmation_order(
        &mut self,
        order: ConfirmationOrder,
    ) -> AsyncResult<'_, AccountInfoResponse, FastPayError> {
        let state = self.0.clone();
        Box::pin(async move { state.lock().await.handle_confirmation_order(order) })
    }

    fn handle_account_info_request(
        &self,
        request: AccountInfoRequest,
    ) -> AsyncResult<'_, AccountInfoResponse, FastPayError> {
        let state = self.0.clone();
        Box::pin(async move { state.lock().await.handle_account_info_request(request) })
    }
}

impl LocalAuthorityClient {
    fn new(state: AuthorityState) -> Self {
        Self(Arc::new(Mutex::new(state)))
    }
}

#[cfg(test)]
fn init_local_authorities(
    count: usize,
) -> (HashMap<AuthorityName, LocalAuthorityClient>, Committee) {
    let mut key_pairs = Vec::new();
    let mut voting_rights = BTreeMap::new();
    for _ in 0..count {
        let key_pair = get_key_pair();
        voting_rights.insert(key_pair.0, 1);
        key_pairs.push(key_pair);
    }
    let committee = Committee::new(voting_rights);

    let mut clients = HashMap::new();
    for (address, secret) in key_pairs {
        let state = AuthorityState::new(committee.clone(), address, secret);
        clients.insert(address, LocalAuthorityClient::new(state));
    }
    (clients, committee)
}

#[cfg(test)]
fn init_local_authorities_bad_1(
    count: usize,
) -> (HashMap<AuthorityName, LocalAuthorityClient>, Committee) {
    let mut key_pairs = Vec::new();
    let mut voting_rights = BTreeMap::new();
    for i in 0..count {
        let key_pair = get_key_pair();
        voting_rights.insert(key_pair.0, 1);
        if i + 1 < (count + 2) / 3 {
            // init 1 authority with a bad keypair
            key_pairs.push(get_key_pair());
        } else {
            key_pairs.push(key_pair);
        }
    }
    let committee = Committee::new(voting_rights);

    let mut clients = HashMap::new();
    for (address, secret) in key_pairs {
        let state = AuthorityState::new(committee.clone(), address, secret);
        clients.insert(address, LocalAuthorityClient::new(state));
    }
    (clients, committee)
}

#[cfg(test)]
fn make_client(
    authority_clients: HashMap<AuthorityName, LocalAuthorityClient>,
    committee: Committee,
) -> ClientState<LocalAuthorityClient> {
    let (address, secret) = get_key_pair();
    ClientState::new(
        address,
        secret,
        committee,
        authority_clients,
        Vec::new(),
        Vec::new(),
        BTreeMap::new(),
    )
}

#[cfg(test)]
fn fund_account<I: IntoIterator<Item = Vec<ObjectID>>>(
    authorities: Vec<&LocalAuthorityClient>,
    client: &mut ClientState<LocalAuthorityClient>,
    object_ids: I,
) {
    for (client, object_ids) in clients.into_iter().zip(object_ids.into_iter()) {
        for object_id in object_ids {
            let mut object = Object::with_id_for_testing(object_id);
            object.transfer(address);
            let mut client_ref = client.0.as_ref().try_lock().unwrap();
            client_ref
                .accounts_mut()
                .lock()
                .unwrap()
                .insert(object_id, object);
            client_ref.init_order_lock((object_id, 0.into()));
        }
    }
}

#[cfg(test)]
fn init_local_client_state(object_ids: Vec<Vec<ObjectID>>) -> ClientState<LocalAuthorityClient> {
    let (authority_clients, committee) = init_local_authorities(object_ids.len());
    let mut client = make_client(authority_clients.clone(), committee);
    fund_account(
        authority_clients.values().collect(),
        &mut client,
        object_ids,
    );
    client
}

#[cfg(test)]
fn init_local_client_state_with_bad_authority(
    object_ids: Vec<Vec<ObjectID>>,
) -> ClientState<LocalAuthorityClient> {
    let (authority_clients, committee) = init_local_authorities_bad_1(object_ids.len());
    let mut client = make_client(authority_clients.clone(), committee);
    fund_account(
        authority_clients.values().collect(),
        &mut client,
        object_ids,
    );
    client
}

#[test]
fn test_get_strong_majority_owner() {
    let mut rt = Runtime::new().unwrap();
    rt.block_on(async {
        let object_id_1 = ObjectID::random();
        let object_id_2 = ObjectID::random();
        let authority_objects = vec![
            vec![object_id_1],
            vec![object_id_1, object_id_2],
            vec![object_id_1, object_id_2],
            vec![object_id_1, object_id_2],
        ];
        let client = init_local_client_state(authority_objects);
        assert_eq!(
            client.get_strong_majority_owner(object_id_1).await,
            Some((client.address, SequenceNumber::from(0)))
        );
        assert_eq!(
            client.get_strong_majority_owner(object_id_2).await,
            Some((client.address, SequenceNumber::from(0)))
        );

        let object_id_1 = ObjectID::random();
        let object_id_2 = ObjectID::random();
        let object_id_3 = ObjectID::random();
        let authority_objects = vec![
            vec![object_id_1],
            vec![object_id_2, object_id_3],
            vec![object_id_3, object_id_2],
            vec![object_id_3],
        ];
        let client = init_local_client_state(authority_objects);
        assert_eq!(client.get_strong_majority_owner(object_id_1).await, None);
        assert_eq!(client.get_strong_majority_owner(object_id_2).await, None);
        assert_eq!(
            client.get_strong_majority_owner(object_id_3).await,
            Some((client.address, SequenceNumber::from(0)))
        );
    });
}

#[test]
fn test_initiating_valid_transfer() {
    let mut rt = Runtime::new().unwrap();
    let (recipient, _) = get_key_pair();
    let object_id_1 = ObjectID::random();
    let object_id_2 = ObjectID::random();
    let authority_objects = vec![
        vec![object_id_1],
        vec![object_id_1, object_id_2],
        vec![object_id_1, object_id_2],
        vec![object_id_1, object_id_2],
    ];

    let mut sender = init_local_client_state(authority_objects);
    assert_eq!(
        rt.block_on(sender.get_strong_majority_owner(object_id_1)),
        Some((sender.address, SequenceNumber::from(0)))
    );
    assert_eq!(
        rt.block_on(sender.get_strong_majority_owner(object_id_2)),
        Some((sender.address, SequenceNumber::from(0)))
    );
    let certificate = rt
        .block_on(sender.transfer_to_fastpay(
            object_id_1,
            recipient,
            UserData(Some(*b"hello...........hello...........")),
        ))
        .unwrap();
    assert_eq!(
        sender.next_sequence_number(object_id_1),
        SequenceNumber::from(1)
    );
    assert_eq!(sender.pending_transfer, None);
    assert_eq!(
        rt.block_on(sender.get_strong_majority_owner(object_id_1)),
        Some((recipient, SequenceNumber::from(1)))
    );
    assert_eq!(
        rt.block_on(sender.get_strong_majority_owner(object_id_2)),
        Some((sender.address, SequenceNumber::from(0)))
    );
    assert_eq!(
        rt.block_on(sender.request_certificate(
            sender.address,
            object_id_1,
            SequenceNumber::from(0),
        ))
        .unwrap(),
        certificate
    );
}

#[test]
fn test_initiating_valid_transfer_despite_bad_authority() {
    let mut rt = Runtime::new().unwrap();
    let (recipient, _) = get_key_pair();
    let object_id = ObjectID::random();
    let authority_objects = vec![
        vec![object_id],
        vec![object_id],
        vec![object_id],
        vec![object_id],
    ];
    let mut sender = init_local_client_state_with_bad_authority(authority_objects);
    let certificate = rt
        .block_on(sender.transfer_to_fastpay(
            object_id,
            recipient,
            UserData(Some(*b"hello...........hello...........")),
        ))
        .unwrap();
    assert_eq!(
        sender.next_sequence_number(object_id),
        SequenceNumber::from(1)
    );
    assert_eq!(sender.pending_transfer, None);
    assert_eq!(
        rt.block_on(sender.get_strong_majority_owner(object_id)),
        Some((recipient, SequenceNumber::from(1)))
    );
    assert_eq!(
        rt.block_on(sender.request_certificate(sender.address, object_id, SequenceNumber::from(0)))
            .unwrap(),
        certificate
    );
}

#[test]
fn test_initiating_transfer_low_funds() {
    let mut rt = Runtime::new().unwrap();
    let (recipient, _) = get_key_pair();
    let object_id_1 = ObjectID::random();
    let object_id_2 = ObjectID::random();
    let authority_objects = vec![
        vec![object_id_1],
        vec![object_id_1],
        vec![object_id_1, object_id_2],
        vec![object_id_1, object_id_2],
    ];
    let mut sender = init_local_client_state(authority_objects);
    assert!(rt
        .block_on(sender.transfer_to_fastpay(object_id_2, recipient, UserData::default()))
        .is_err());
    // Trying to overspend does not block an account.
    assert_eq!(
        sender.next_sequence_number(object_id_2),
        SequenceNumber::from(0)
    );
    // assert_eq!(sender.pending_transfer, None);
    assert_eq!(
        rt.block_on(sender.get_strong_majority_owner(object_id_1)),
        Some((sender.address, SequenceNumber::from(0))),
    );
    assert_eq!(
        rt.block_on(sender.get_strong_majority_owner(object_id_2)),
        None,
    );
}

#[test]
fn test_bidirectional_transfer() {
    let mut rt = Runtime::new().unwrap();
    let (authority_clients, committee) = init_local_authorities(4);
    let mut client1 = make_client(authority_clients.clone(), committee.clone());
    let mut client2 = make_client(authority_clients.clone(), committee);

    let object_id = ObjectID::random();
    let authority_objects = vec![
        vec![object_id],
        vec![object_id],
        vec![object_id],
        vec![object_id],
    ];
    fund_account(
        authority_clients.values().collect(),
        &mut client1,
        authority_objects,
    );

    // Confirm client1 have ownership of the object.
    assert_eq!(
        rt.block_on(client1.get_strong_majority_owner(object_id)),
        Some((client1.address, SequenceNumber::from(0)))
    );
    // Confirm client2 doesn't have ownership of the object.
    assert_eq!(
        rt.block_on(client2.get_strong_majority_owner(object_id)),
        Some((client1.address, SequenceNumber::from(0)))
    );
    // Transfer object to client2.
    let certificate = rt
        .block_on(client1.transfer_to_fastpay(object_id, client2.address, UserData::default()))
        .unwrap();

    assert_eq!(
        client1.next_sequence_number(object_id),
        SequenceNumber::from(1)
    );
    assert_eq!(client1.pending_transfer, None);

    // Confirm client1 lose ownership of the object.
    assert_eq!(
        rt.block_on(client1.get_strong_majority_owner(object_id)),
        Some((client2.address, SequenceNumber::from(1)))
    );
    // Confirm client2 acquired ownership of the object.
    assert_eq!(
        rt.block_on(client2.get_strong_majority_owner(object_id)),
        Some((client2.address, SequenceNumber::from(1)))
    );
    // Confirm sequence number is consistent between authorities and client.
    assert_eq!(
        rt.block_on(client1.get_strong_majority_sequence_number(object_id)),
        client1.next_sequence_number(object_id)
    );
    // Confirm certificate is consistent between authorities and client.
    assert_eq!(
        rt.block_on(client1.request_certificate(
            client1.address,
            object_id,
            SequenceNumber::from(0),
        ))
        .unwrap(),
        certificate
    );

    // Update client2's local object data.
    rt.block_on(client2.receive_from_fastpay(certificate))
        .unwrap();

    // Confirm sequence number are consistent between clients.
    assert_eq!(
        rt.block_on(client2.get_strong_majority_owner(object_id)),
        Some((client2.address, SequenceNumber::from(1)))
    );

    // Transfer the object back to Client1
    rt.block_on(client2.transfer_to_fastpay(object_id, client1.address, UserData::default()))
        .unwrap();

    assert_eq!(
        client2.next_sequence_number(object_id),
        SequenceNumber::from(2)
    );
    assert_eq!(client2.pending_transfer, None);

    // Confirm client2 lose ownership of the object.
    assert_eq!(
        rt.block_on(client2.object_ownership_have_quorum(object_id)),
        None
    );
    assert_eq!(
        rt.block_on(client2.get_strong_majority_sequence_number(object_id)),
        SequenceNumber::from(2)
    );
    // Confirm client1 acquired ownership of the object.
    assert_eq!(
        rt.block_on(client1.object_ownership_have_quorum(object_id)),
        Some(SequenceNumber::from(2))
    );

    // Should fail if Client 2 double spend the object
    assert!(rt
        .block_on(client2.transfer_to_fastpay(object_id, client1.address, UserData::default()))
        .is_err());
}

#[test]
fn test_receiving_unconfirmed_transfer() {
    let mut rt = Runtime::new().unwrap();
    let (authority_clients, committee) = init_local_authorities(4);
    let mut client1 = make_client(authority_clients.clone(), committee.clone());
    let mut client2 = make_client(authority_clients.clone(), committee);

    let object_id = ObjectID::random();
    let authority_objects = vec![
        vec![object_id],
        vec![object_id],
        vec![object_id],
        vec![object_id],
    ];

    fund_account(
        authority_clients.values().collect(),
        &mut client1,
        authority_objects,
    );
    // not updating client1.balance

    let certificate = rt
        .block_on(client1.transfer_to_fastpay_unsafe_unconfirmed(
            client2.address,
            object_id,
            UserData::default(),
        ))
        .unwrap();
    // Transfer was executed locally, creating negative balance.
    // assert_eq!(client1.balance, Balance::from(-2));
    assert_eq!(
        client1.next_sequence_number(object_id),
        SequenceNumber::from(1)
    );
    assert_eq!(client1.pending_transfer, None);
    // ..but not confirmed remotely, hence an unchanged balance and sequence number.
    assert_eq!(
        rt.block_on(client1.get_strong_majority_owner(object_id)),
        Some((client1.address, SequenceNumber::from(0)))
    );
    assert_eq!(
        rt.block_on(client1.get_strong_majority_sequence_number(object_id)),
        SequenceNumber::from(0)
    );
    // Let the receiver confirm in last resort.
    rt.block_on(client2.receive_from_fastpay(certificate))
        .unwrap();
    assert_eq!(
        rt.block_on(client2.get_strong_majority_owner(object_id)),
        Some((client2.address, SequenceNumber::from(1)))
    );
}

/*
#[test]
fn test_receiving_unconfirmed_transfer_with_lagging_sender_balances() {
    let mut rt = Runtime::new().unwrap();
    let (mut authority_clients, committee) = init_local_authorities(4);
    let mut client0 = make_client(authority_clients.clone(), committee.clone());
    let mut client1 = make_client(authority_clients.clone(), committee.clone());
    let mut client2 = make_client(authority_clients.clone(), committee);
    fund_account(&mut authority_clients, client0.address, vec![2, 3, 4, 4]);
    // not updating client balances

    // transferring funds from client0 to client1.
    // confirming to a quorum of node only at the end.
    rt.block_on(async {
        client0
            .transfer_to_fastpay_unsafe_unconfirmed(
                Amount::from(1),
                client1.address,
                UserData::default(),
            )
            .await
            .unwrap();
        client0
            .transfer_to_fastpay_unsafe_unconfirmed(
                Amount::from(1),
                client1.address,
                UserData::default(),
            )
            .await
            .unwrap();
        client0
            .communicate_transfers(
                client0.address,
                client0.sent_certificates.clone(),
                CommunicateAction::SynchronizeNextSequenceNumber(client0.next_sequence_number),
            )
            .await
            .unwrap();
    });
    // transferring funds from client1 to client2 without confirmation
    let certificate = rt
        .block_on(client1.transfer_to_fastpay_unsafe_unconfirmed(
            Amount::from(2),
            client2.address,
            UserData::default(),
        ))
        .unwrap();
    // Transfers were executed locally, possibly creating negative balances.
    assert_eq!(client0.balance, Balance::from(-2));
    assert_eq!(client0.next_sequence_number, SequenceNumber::from(2));
    assert_eq!(client0.pending_transfer, None);
    assert_eq!(client1.balance, Balance::from(-2));
    assert_eq!(client1.next_sequence_number, SequenceNumber::from(1));
    assert_eq!(client1.pending_transfer, None);
    // Last one was not confirmed remotely, hence an unchanged (remote) balance and sequence number.
    assert_eq!(
        rt.block_on(client1.get_strong_majority_balance()),
        Balance::from(2)
    );
    assert_eq!(
        rt.block_on(client1.get_strong_majority_sequence_number(client1.address)),
        SequenceNumber::from(0)
    );
    // Let the receiver confirm in last resort.
    rt.block_on(client2.receive_from_fastpay(certificate))
        .unwrap();
    assert_eq!(
        rt.block_on(client2.get_strong_majority_balance()),
        Balance::from(2)
    );
}
*/
