// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0
#![allow(clippy::same_item_push)] // get_key_pair returns random elements

use super::*;
use crate::{
    authority::{AccountOffchainState, Authority, AuthorityState},
    base_types::Amount,
};
use futures::lock::Mutex;
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};
use tokio::runtime::Runtime;

#[derive(Clone)]
struct LocalAuthorityClient(Arc<Mutex<AuthorityState>>);

impl AuthorityClient for LocalAuthorityClient {
    fn handle_transfer_order(
        &mut self,
        order: TransferOrder,
    ) -> AsyncResult<AccountInfoResponse, FastPayError> {
        let state = self.0.clone();
        Box::pin(async move { state.lock().await.handle_transfer_order(order) })
    }

    fn handle_confirmation_order(
        &mut self,
        order: ConfirmationOrder,
    ) -> AsyncResult<AccountInfoResponse, FastPayError> {
        let state = self.0.clone();
        Box::pin(async move {
            state
                .lock()
                .await
                .handle_confirmation_order(order)
                .map(|(info, _)| info)
        })
    }

    fn handle_account_info_request(
        &mut self,
        request: AccountInfoRequest,
    ) -> AsyncResult<AccountInfoResponse, FastPayError> {
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
        SequenceNumber::new(),
        Vec::new(),
        Vec::new(),
        Balance::from(0),
    )
}

#[cfg(test)]
fn fund_account<I: IntoIterator<Item = i128>>(
    clients: &mut HashMap<AuthorityName, LocalAuthorityClient>,
    address: FastPayAddress,
    balances: I,
) {
    let mut balances = balances.into_iter().map(Balance::from);
    for (_, client) in clients.iter_mut() {
        client.0.as_ref().try_lock().unwrap().accounts_mut().insert(
            address,
            AccountOffchainState::new_with_balance(
                balances.next().unwrap_or_else(Balance::zero),
                /* no receive log to justify the balances */ Vec::new(),
            ),
        );
    }
}

#[cfg(test)]
fn init_local_client_state(balances: Vec<i128>) -> ClientState<LocalAuthorityClient> {
    let (mut authority_clients, committee) = init_local_authorities(balances.len());
    let client = make_client(authority_clients.clone(), committee);
    fund_account(&mut authority_clients, client.address, balances);
    client
}

#[cfg(test)]
fn init_local_client_state_with_bad_authority(
    balances: Vec<i128>,
) -> ClientState<LocalAuthorityClient> {
    let (mut authority_clients, committee) = init_local_authorities_bad_1(balances.len());
    let client = make_client(authority_clients.clone(), committee);
    fund_account(&mut authority_clients, client.address, balances);
    client
}

#[test]
fn test_get_strong_majority_balance() {
    let mut rt = Runtime::new().unwrap();
    rt.block_on(async {
        let mut client = init_local_client_state(vec![3, 4, 4, 4]);
        assert_eq!(client.get_strong_majority_balance().await, Balance::from(4));

        let mut client = init_local_client_state(vec![0, 3, 4, 4]);
        assert_eq!(client.get_strong_majority_balance().await, Balance::from(3));

        let mut client = init_local_client_state(vec![0, 3, 4]);
        assert_eq!(client.get_strong_majority_balance().await, Balance::from(0));
    });
}

#[test]
fn test_initiating_valid_transfer() {
    let mut rt = Runtime::new().unwrap();
    let (recipient, _) = get_key_pair();

    let mut sender = init_local_client_state(vec![2, 4, 4, 4]);
    sender.balance = Balance::from(4);
    let certificate = rt
        .block_on(sender.transfer_to_fastpay(
            Amount::from(3),
            recipient,
            UserData(Some(*b"hello...........hello...........")),
        ))
        .unwrap();
    assert_eq!(sender.next_sequence_number, SequenceNumber::from(1));
    assert_eq!(sender.pending_transfer, None);
    assert_eq!(
        rt.block_on(sender.get_strong_majority_balance()),
        Balance::from(1)
    );
    assert_eq!(
        rt.block_on(sender.request_certificate(sender.address, SequenceNumber::from(0)))
            .unwrap(),
        certificate
    );
}

#[test]
fn test_initiating_valid_transfer_despite_bad_authority() {
    let mut rt = Runtime::new().unwrap();
    let (recipient, _) = get_key_pair();

    let mut sender = init_local_client_state_with_bad_authority(vec![4, 4, 4, 4]);
    sender.balance = Balance::from(4);
    let certificate = rt
        .block_on(sender.transfer_to_fastpay(
            Amount::from(3),
            recipient,
            UserData(Some(*b"hello...........hello...........")),
        ))
        .unwrap();
    assert_eq!(sender.next_sequence_number, SequenceNumber::from(1));
    assert_eq!(sender.pending_transfer, None);
    assert_eq!(
        rt.block_on(sender.get_strong_majority_balance()),
        Balance::from(1)
    );
    assert_eq!(
        rt.block_on(sender.request_certificate(sender.address, SequenceNumber::from(0)))
            .unwrap(),
        certificate
    );
}

#[test]
fn test_initiating_transfer_low_funds() {
    let mut rt = Runtime::new().unwrap();
    let (recipient, _) = get_key_pair();

    let mut sender = init_local_client_state(vec![2, 2, 4, 4]);
    sender.balance = Balance::from(2);
    assert!(rt
        .block_on(sender.transfer_to_fastpay(Amount::from(3), recipient, UserData::default()))
        .is_err());
    // Trying to overspend does not block an account.
    assert_eq!(sender.next_sequence_number, SequenceNumber::from(0));
    assert_eq!(sender.pending_transfer, None);
    assert_eq!(
        rt.block_on(sender.get_strong_majority_balance()),
        Balance::from(2)
    );
}

#[test]
fn test_bidirectional_transfer() {
    let mut rt = Runtime::new().unwrap();
    let (mut authority_clients, committee) = init_local_authorities(4);
    let mut client1 = make_client(authority_clients.clone(), committee.clone());
    let mut client2 = make_client(authority_clients.clone(), committee);
    fund_account(&mut authority_clients, client1.address, vec![2, 3, 4, 4]);
    // Update client1's local balance accordingly.
    client1.balance = rt.block_on(client1.get_strong_majority_balance());
    assert_eq!(client1.balance, Balance::from(3));

    let certificate = rt
        .block_on(client1.transfer_to_fastpay(
            Amount::from(3),
            client2.address,
            UserData::default(),
        ))
        .unwrap();

    assert_eq!(client1.next_sequence_number, SequenceNumber::from(1));
    assert_eq!(client1.pending_transfer, None);
    assert_eq!(
        rt.block_on(client1.get_strong_majority_balance()),
        Balance::from(0)
    );
    assert_eq!(client1.balance, Balance::from(0));
    assert_eq!(
        rt.block_on(client1.get_strong_majority_sequence_number(client1.address)),
        SequenceNumber::from(1)
    );

    assert_eq!(
        rt.block_on(client1.request_certificate(client1.address, SequenceNumber::from(0)))
            .unwrap(),
        certificate
    );
    // Our sender already confirmed.
    assert_eq!(
        rt.block_on(client2.get_strong_majority_balance()),
        Balance::from(3)
    );
    assert_eq!(client2.balance, Balance::from(0));
    // Try to confirm again.
    rt.block_on(client2.receive_from_fastpay(certificate))
        .unwrap();
    assert_eq!(
        rt.block_on(client2.get_strong_majority_balance()),
        Balance::from(3)
    );
    assert_eq!(client2.balance, Balance::from(3));

    // Send back some money.
    assert_eq!(client2.next_sequence_number, SequenceNumber::from(0));
    rt.block_on(client2.transfer_to_fastpay(Amount::from(1), client1.address, UserData::default()))
        .unwrap();
    assert_eq!(client2.next_sequence_number, SequenceNumber::from(1));
    assert_eq!(client2.pending_transfer, None);
    assert_eq!(
        rt.block_on(client2.get_strong_majority_balance()),
        Balance::from(2)
    );
    assert_eq!(
        rt.block_on(client2.get_strong_majority_sequence_number(client2.address)),
        SequenceNumber::from(1)
    );
    assert_eq!(
        rt.block_on(client1.get_strong_majority_balance()),
        Balance::from(1)
    );
}

#[test]
fn test_receiving_unconfirmed_transfer() {
    let mut rt = Runtime::new().unwrap();
    let (mut authority_clients, committee) = init_local_authorities(4);
    let mut client1 = make_client(authority_clients.clone(), committee.clone());
    let mut client2 = make_client(authority_clients.clone(), committee);
    fund_account(&mut authority_clients, client1.address, vec![2, 3, 4, 4]);
    // not updating client1.balance

    let certificate = rt
        .block_on(client1.transfer_to_fastpay_unsafe_unconfirmed(
            Amount::from(2),
            client2.address,
            UserData::default(),
        ))
        .unwrap();
    // Transfer was executed locally, creating negative balance.
    assert_eq!(client1.balance, Balance::from(-2));
    assert_eq!(client1.next_sequence_number, SequenceNumber::from(1));
    assert_eq!(client1.pending_transfer, None);
    // ..but not confirmed remotely, hence an unchanged balance and sequence number.
    assert_eq!(
        rt.block_on(client1.get_strong_majority_balance()),
        Balance::from(3)
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
