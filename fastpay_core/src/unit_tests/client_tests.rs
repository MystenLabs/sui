// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0
#![allow(clippy::same_item_push)] // get_key_pair returns random elements

use super::*;
use crate::authority::{AuthorityState, AuthorityStore};
use fastx_types::{
    object::{Object, GAS_VALUE_FOR_TESTING},
    FASTX_FRAMEWORK_ADDRESS,
};
use futures::lock::Mutex;
use move_core_types::ident_str;
use std::{
    collections::{BTreeMap, HashMap},
    convert::TryInto,
    sync::Arc,
};
use tokio::runtime::Runtime;

use fastx_types::error::FastPayError::ObjectNotFound;
use move_core_types::account_address::AccountAddress;
use std::env;
use std::fs;

pub fn system_maxfiles() -> usize {
    fdlimit::raise_fd_limit().unwrap_or(256u64) as usize
}

fn max_files_client_tests() -> i32 {
    (system_maxfiles() / 8).try_into().unwrap()
}

#[derive(Clone)]
struct LocalAuthorityClient(Arc<Mutex<AuthorityState>>);

impl AuthorityClient for LocalAuthorityClient {
    fn handle_order(&mut self, order: Order) -> AsyncResult<'_, OrderInfoResponse, FastPayError> {
        let state = self.0.clone();
        Box::pin(async move { state.lock().await.handle_order(order).await })
    }

    fn handle_confirmation_order(
        &mut self,
        order: ConfirmationOrder,
    ) -> AsyncResult<'_, OrderInfoResponse, FastPayError> {
        let state = self.0.clone();
        Box::pin(async move { state.lock().await.handle_confirmation_order(order).await })
    }

    fn handle_account_info_request(
        &self,
        request: AccountInfoRequest,
    ) -> AsyncResult<'_, AccountInfoResponse, FastPayError> {
        let state = self.0.clone();
        Box::pin(async move {
            state
                .lock()
                .await
                .handle_account_info_request(request)
                .await
        })
    }

    fn handle_object_info_request(
        &self,
        request: ObjectInfoRequest,
    ) -> AsyncResult<'_, ObjectInfoResponse, FastPayError> {
        let state = self.0.clone();
        Box::pin(async move { state.lock().await.handle_object_info_request(request).await })
    }
}

impl LocalAuthorityClient {
    fn new(state: AuthorityState) -> Self {
        Self(Arc::new(Mutex::new(state)))
    }
}

#[cfg(test)]
async fn init_local_authorities(
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
        // Random directory for the DB
        let dir = env::temp_dir();
        let path = dir.join(format!("DB_{:?}", ObjectID::random()));
        fs::create_dir(&path).unwrap();

        let mut opts = rocksdb::Options::default();
        opts.set_max_open_files(max_files_client_tests());
        let store = Arc::new(AuthorityStore::open(path, Some(opts)));

        let state =
            AuthorityState::new_with_genesis_modules(committee.clone(), address, secret, store)
                .await;
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
        // Random directory
        let dir = env::temp_dir();
        let path = dir.join(format!("DB_{:?}", ObjectID::random()));
        fs::create_dir(&path).unwrap();

        let mut opts = rocksdb::Options::default();
        opts.set_max_open_files(max_files_client_tests());
        let store = Arc::new(AuthorityStore::open(path, Some(opts)));
        let state = AuthorityState::new_without_genesis_for_testing(
            committee.clone(),
            address,
            secret,
            store,
        );
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
        BTreeMap::new(),
        BTreeMap::new(),
    )
}

#[cfg(test)]
async fn fund_account<I: IntoIterator<Item = Vec<ObjectID>>>(
    authorities: Vec<&LocalAuthorityClient>,
    client: &mut ClientState<LocalAuthorityClient>,
    object_ids: I,
) -> HashMap<AccountAddress, Object> {
    let mut created_objects = HashMap::new();
    for (authority, object_ids) in authorities.into_iter().zip(object_ids.into_iter()) {
        for object_id in object_ids {
            let object = Object::with_id_owner_for_testing(object_id, client.address);
            let client_ref = authority.0.as_ref().try_lock().unwrap();
            created_objects.insert(object_id, object.clone());

            client_ref
                .init_order_lock((object_id, 0.into(), object.digest()))
                .await;
            client_ref.insert_object(object).await;
            client.object_ids.insert(object_id, SequenceNumber::new());
        }
    }
    created_objects
}

#[cfg(test)]
async fn init_local_client_state(
    object_ids: Vec<Vec<ObjectID>>,
) -> ClientState<LocalAuthorityClient> {
    let (authority_clients, committee) = init_local_authorities(object_ids.len()).await;
    let mut client = make_client(authority_clients.clone(), committee);
    fund_account(
        authority_clients.values().collect(),
        &mut client,
        object_ids,
    )
    .await;
    client
}

#[cfg(test)]
async fn init_local_client_state_with_bad_authority(
    object_ids: Vec<Vec<ObjectID>>,
) -> ClientState<LocalAuthorityClient> {
    let (authority_clients, committee) = init_local_authorities_bad_1(object_ids.len());
    let mut client = make_client(authority_clients.clone(), committee);
    fund_account(
        authority_clients.values().collect(),
        &mut client,
        object_ids,
    )
    .await;
    client
}

#[test]
fn test_get_strong_majority_owner() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let object_id_1 = ObjectID::random();
        let object_id_2 = ObjectID::random();
        let authority_objects = vec![
            vec![object_id_1],
            vec![object_id_1, object_id_2],
            vec![object_id_1, object_id_2],
            vec![object_id_1, object_id_2],
        ];
        let client = init_local_client_state(authority_objects).await;
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
        let client = init_local_client_state(authority_objects).await;
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
    let rt = Runtime::new().unwrap();
    let (recipient, _) = get_key_pair();
    let object_id_1 = ObjectID::random();
    let object_id_2 = ObjectID::random();
    let gas_object = ObjectID::random();
    let authority_objects = vec![
        vec![object_id_1, gas_object],
        vec![object_id_1, object_id_2, gas_object],
        vec![object_id_1, object_id_2, gas_object],
        vec![object_id_1, object_id_2, gas_object],
    ];

    let mut sender = rt.block_on(init_local_client_state(authority_objects));
    assert_eq!(
        rt.block_on(sender.get_strong_majority_owner(object_id_1)),
        Some((sender.address, SequenceNumber::from(0)))
    );
    assert_eq!(
        rt.block_on(sender.get_strong_majority_owner(object_id_2)),
        Some((sender.address, SequenceNumber::from(0)))
    );
    let certificate = rt
        .block_on(sender.transfer_object(object_id_1, gas_object, recipient))
        .unwrap();
    assert_eq!(
        sender.next_sequence_number(&object_id_1),
        Err(ObjectNotFound)
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
    let rt = Runtime::new().unwrap();
    let (recipient, _) = get_key_pair();
    let object_id = ObjectID::random();
    let gas_object = ObjectID::random();
    let authority_objects = vec![
        vec![object_id, gas_object],
        vec![object_id, gas_object],
        vec![object_id, gas_object],
        vec![object_id, gas_object],
    ];
    let mut sender = rt.block_on(init_local_client_state_with_bad_authority(
        authority_objects,
    ));
    let certificate = rt
        .block_on(sender.transfer_object(object_id, gas_object, recipient))
        .unwrap();
    assert_eq!(sender.next_sequence_number(&object_id), Err(ObjectNotFound));
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
    let rt = Runtime::new().unwrap();
    let (recipient, _) = get_key_pair();
    let object_id_1 = ObjectID::random();
    let object_id_2 = ObjectID::random();
    let gas_object = ObjectID::random();
    let authority_objects = vec![
        vec![object_id_1, gas_object],
        vec![object_id_1, gas_object],
        vec![object_id_1, object_id_2, gas_object],
        vec![object_id_1, object_id_2, gas_object],
    ];
    let mut sender = rt.block_on(init_local_client_state(authority_objects));
    assert!(rt
        .block_on(sender.transfer_object(object_id_2, gas_object, recipient))
        .is_err());
    // Trying to overspend does not block an account.
    assert_eq!(
        sender.next_sequence_number(&object_id_2),
        Ok(SequenceNumber::from(0))
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
    let rt = Runtime::new().unwrap();
    let (authority_clients, committee) = rt.block_on(init_local_authorities(4));
    let mut client1 = make_client(authority_clients.clone(), committee.clone());
    let mut client2 = make_client(authority_clients.clone(), committee);

    let object_id = ObjectID::random();
    let gas_object1 = ObjectID::random();
    let gas_object2 = ObjectID::random();
    let authority1_objects = vec![
        vec![object_id, gas_object1],
        vec![object_id, gas_object1],
        vec![object_id, gas_object1],
        vec![object_id, gas_object1],
    ];
    let authority2_objects = vec![
        vec![gas_object2],
        vec![gas_object2],
        vec![gas_object2],
        vec![gas_object2],
    ];
    rt.block_on(fund_account(
        authority_clients.values().collect(),
        &mut client1,
        authority1_objects,
    ));
    rt.block_on(fund_account(
        authority_clients.values().collect(),
        &mut client2,
        authority2_objects,
    ));

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
        .block_on(client1.transfer_object(object_id, gas_object1, client2.address))
        .unwrap();

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
    rt.block_on(client2.receive_object(certificate)).unwrap();

    // Confirm sequence number are consistent between clients.
    assert_eq!(
        rt.block_on(client2.get_strong_majority_owner(object_id)),
        Some((client2.address, SequenceNumber::from(1)))
    );

    // Transfer the object back to Client1
    rt.block_on(client2.transfer_object(object_id, gas_object2, client1.address))
        .unwrap();

    assert_eq!(client2.pending_transfer, None);

    // Confirm client2 lose ownership of the object.
    assert_eq!(
        rt.block_on(client2.get_strong_majority_owner(object_id)),
        Some((client1.address, SequenceNumber::from(2)))
    );
    assert_eq!(
        rt.block_on(client2.get_strong_majority_sequence_number(object_id)),
        SequenceNumber::from(2)
    );
    // Confirm client1 acquired ownership of the object.
    assert_eq!(
        rt.block_on(client1.get_strong_majority_owner(object_id)),
        Some((client1.address, SequenceNumber::from(2)))
    );

    // Should fail if Client 2 double spend the object
    assert!(rt
        .block_on(client2.transfer_object(object_id, gas_object2, client1.address,))
        .is_err());
}

#[test]
fn test_receiving_unconfirmed_transfer() {
    let rt = Runtime::new().unwrap();
    let (authority_clients, committee) = rt.block_on(init_local_authorities(4));
    let mut client1 = make_client(authority_clients.clone(), committee.clone());
    let mut client2 = make_client(authority_clients.clone(), committee);

    let object_id = ObjectID::random();
    let gas_object_id = ObjectID::random();
    let authority_objects = vec![
        vec![object_id, gas_object_id],
        vec![object_id, gas_object_id],
        vec![object_id, gas_object_id],
        vec![object_id, gas_object_id],
    ];

    rt.block_on(fund_account(
        authority_clients.values().collect(),
        &mut client1,
        authority_objects,
    ));
    // not updating client1.balance

    let certificate = rt
        .block_on(client1.transfer_to_fastx_unsafe_unconfirmed(
            client2.address,
            object_id,
            gas_object_id,
        ))
        .unwrap();
    assert_eq!(
        client1.next_sequence_number(&object_id),
        Ok(SequenceNumber::from(1))
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
    rt.block_on(client2.receive_object(certificate)).unwrap();
    assert_eq!(
        rt.block_on(client2.get_strong_majority_owner(object_id)),
        Some((client2.address, SequenceNumber::from(1)))
    );
}

#[test]
fn test_client_state_sync() {
    let rt = Runtime::new().unwrap();

    let object_ids = (0..20)
        .map(|_| ObjectID::random())
        .collect::<Vec<ObjectID>>();
    let authority_objects = (0..10).map(|_| object_ids.clone()).collect();

    let mut sender = rt.block_on(init_local_client_state(authority_objects));

    let old_object_ids = sender.object_ids.clone();
    let old_certificate = sender.certificates.clone();

    // Remove all client-side data
    sender.object_ids.clear();
    sender.certificates.clear();
    assert!(rt.block_on(sender.get_owned_objects()).unwrap().is_empty());
    assert!(sender.object_ids.is_empty());
    assert!(sender.certificates.is_empty());

    // Sync client state
    rt.block_on(sender.sync_client_state_with_random_authority())
        .unwrap();

    // Confirm data are the same after sync
    assert!(!rt.block_on(sender.get_owned_objects()).unwrap().is_empty());
    assert_eq!(old_object_ids, sender.object_ids);
    assert_eq!(old_certificate, sender.certificates);
}

#[tokio::test]
async fn test_client_state_sync_with_transferred_object() {
    let (authority_clients, committee) = init_local_authorities(1).await;
    let mut client1 = make_client(authority_clients.clone(), committee.clone());
    let mut client2 = make_client(authority_clients.clone(), committee);

    let object_id = ObjectID::random();
    let gas_object_id = ObjectID::random();

    let authority_objects = vec![vec![object_id, gas_object_id]];

    fund_account(
        authority_clients.values().collect(),
        &mut client1,
        authority_objects,
    )
    .await;

    // Transfer object to client2.
    client1
        .transfer_object(object_id, gas_object_id, client2.address)
        .await
        .unwrap();

    // Confirm client2 acquired ownership of the object.
    assert_eq!(
        client2.get_strong_majority_owner(object_id).await,
        Some((client2.address, SequenceNumber::from(1)))
    );

    // Client 2's local object_id and cert should be empty before sync
    assert!(client2.get_owned_objects().await.unwrap().is_empty());
    assert!(client2.object_ids.is_empty());
    assert!(client2.certificates.is_empty());

    // Sync client state
    client2
        .sync_client_state_with_random_authority()
        .await
        .unwrap();

    // Confirm client 2 received the new object id and cert
    assert_eq!(1, client2.get_owned_objects().await.unwrap().len());
    assert_eq!(1, client2.object_ids.len());
    assert_eq!(1, client2.certificates.len());
}

#[tokio::test]
async fn test_client_certificate_state() {
    let number_of_authorities = 1;
    let (authority_clients, committee) = init_local_authorities(number_of_authorities).await;
    let mut client1 = make_client(authority_clients.clone(), committee.clone());
    let mut client2 = make_client(authority_clients.clone(), committee);

    let object_id_1 = ObjectID::random();
    let object_id_2 = ObjectID::random();
    let gas_object_id_1 = ObjectID::random();
    let gas_object_id_2 = ObjectID::random();

    let client1_objects = vec![object_id_1, object_id_2, gas_object_id_1];
    let client2_objects = vec![gas_object_id_2];

    let client1_objects: Vec<Vec<ObjectID>> = (0..number_of_authorities)
        .map(|_| client1_objects.clone())
        .collect();

    let client2_objects: Vec<Vec<ObjectID>> = (0..number_of_authorities)
        .map(|_| client2_objects.clone())
        .collect();

    fund_account(
        authority_clients.values().collect(),
        &mut client1,
        client1_objects,
    )
    .await;

    fund_account(
        authority_clients.values().collect(),
        &mut client2,
        client2_objects,
    )
    .await;

    // Transfer object to client2.
    client1
        .transfer_object(object_id_1, gas_object_id_1, client2.address)
        .await
        .unwrap();
    client1
        .transfer_object(object_id_2, gas_object_id_1, client2.address)
        .await
        .unwrap();
    // Should have 2 certs after 2 transfer
    assert_eq!(2, client1.certificates.len());
    // Only gas_object left in account, so object_certs link should only have 1 entry
    assert_eq!(1, client1.object_certs.len());
    // it should have 2 certificates associated with the gas object
    assert!(client1.object_certs.contains_key(&gas_object_id_1));
    assert_eq!(2, client1.object_certs.get(&gas_object_id_1).unwrap().len());
    // Sequence number should be 2 for gas object after 2 mutation.
    assert_eq!(
        Ok(SequenceNumber::from(2)),
        client1.next_sequence_number(&gas_object_id_1)
    );

    client2
        .sync_client_state_with_random_authority()
        .await
        .unwrap();

    // Client 2 should retrieve 2 certificates for the 2 transactions after sync
    assert_eq!(2, client2.certificates.len());
    assert!(client2.object_certs.contains_key(&object_id_1));
    assert!(client2.object_certs.contains_key(&object_id_2));
    assert_eq!(1, client2.object_certs.get(&object_id_1).unwrap().len());
    assert_eq!(1, client2.object_certs.get(&object_id_2).unwrap().len());
    // Sequence number for object 1 and 2 should be 1 after 1 mutation.
    assert_eq!(
        Ok(SequenceNumber::from(1)),
        client2.next_sequence_number(&object_id_1)
    );
    assert_eq!(
        Ok(SequenceNumber::from(1)),
        client2.next_sequence_number(&object_id_2)
    );
    // Transfer object 2 back to client 1.
    client2
        .transfer_object(object_id_2, gas_object_id_2, client1.address)
        .await
        .unwrap();

    assert_eq!(3, client2.certificates.len());
    assert!(client2.object_certs.contains_key(&object_id_1));
    assert!(!client2.object_certs.contains_key(&object_id_2));
    assert!(client2.object_certs.contains_key(&gas_object_id_2));
    assert_eq!(1, client2.object_certs.get(&object_id_1).unwrap().len());
    assert_eq!(1, client2.object_certs.get(&gas_object_id_2).unwrap().len());

    client1
        .sync_client_state_with_random_authority()
        .await
        .unwrap();

    assert_eq!(3, client1.certificates.len());
    assert!(client1.object_certs.contains_key(&object_id_2));
    assert_eq!(2, client1.object_certs.get(&object_id_2).unwrap().len());
}

#[tokio::test]
async fn test_move_calls_object_create() {
    let (authority_clients, committee) = init_local_authorities(4).await;
    let mut client1 = make_client(authority_clients.clone(), committee);

    let object_value: u64 = 100;
    let gas_object_id = ObjectID::random();

    // TODO: authority should not require seq# or digets for package in Move calls. Use dummy values
    let framework_obj_ref = (
        FASTX_FRAMEWORK_ADDRESS,
        SequenceNumber::new(),
        ObjectDigest::new([0; 32]),
    );

    // Populate authorities with obj data
    let authority_objects = vec![
        vec![gas_object_id],
        vec![gas_object_id],
        vec![gas_object_id],
        vec![gas_object_id],
    ];
    let gas_object_ref = fund_account(
        authority_clients.values().collect(),
        &mut client1,
        authority_objects,
    )
    .await
    .iter()
    .next()
    .unwrap()
    .1
    .to_object_reference();

    // When creating an ObjectBasics object, we provide the value (u64) and address which will own the object
    let pure_args = vec![
        object_value.to_le_bytes().to_vec(),
        bcs::to_bytes(&client1.address.to_vec()).unwrap(),
    ];
    let call_response = client1
        .move_call(
            framework_obj_ref,
            ident_str!("ObjectBasics").to_owned(),
            ident_str!("create").to_owned(),
            Vec::new(),
            gas_object_ref,
            Vec::new(),
            pure_args,
            GAS_VALUE_FOR_TESTING - 1, // Make sure budget is less than gas value
        )
        .await;

    // Check all went well
    assert!(call_response.is_ok());
    // Check effects are good
    let (_, order_effects) = call_response.unwrap();
    // Status flag should be success
    assert_eq!(order_effects.status, ExecutionStatus::Success);
    // Nothing should be deleted during a creation
    assert!(order_effects.deleted.is_empty());
    // Two items should be mutated during a creation (gas and new object)
    assert_eq!(order_effects.mutated.len(), 2);
    // Confirm the items
    let gas_obj_idx = order_effects
        .mutated
        .iter()
        .position(|e| e.0 == gas_object_ref.0);

    assert!(gas_obj_idx.is_some());
    let new_obj_ref = order_effects.mutated.get(gas_obj_idx.unwrap() ^ 1).unwrap();
    assert_ne!(gas_object_ref, *new_obj_ref);
}

#[tokio::test]
async fn test_move_calls_object_transfer() {
    let (authority_clients, committee) = init_local_authorities(4).await;
    let mut client1 = make_client(authority_clients.clone(), committee.clone());
    let client2 = make_client(authority_clients.clone(), committee);

    let object_value: u64 = 100;
    let gas_object_id = ObjectID::random();

    // TODO: authority should not require seq# or digets for package in Move calls. Use dummy values
    let framework_obj_ref = (
        FASTX_FRAMEWORK_ADDRESS,
        SequenceNumber::new(),
        ObjectDigest::new([0; 32]),
    );

    // Populate authorities with obj data
    let authority_objects = vec![
        vec![gas_object_id],
        vec![gas_object_id],
        vec![gas_object_id],
        vec![gas_object_id],
    ];
    let mut gas_object_ref = fund_account(
        authority_clients.values().collect(),
        &mut client1,
        authority_objects,
    )
    .await
    .iter()
    .next()
    .unwrap()
    .1
    .to_object_reference();

    // When creating an ObjectBasics object, we provide the value (u64) and address which will own the object
    let pure_args = vec![
        object_value.to_le_bytes().to_vec(),
        bcs::to_bytes(&client1.address.to_vec()).unwrap(),
    ];
    let call_response = client1
        .move_call(
            framework_obj_ref,
            ident_str!("ObjectBasics").to_owned(),
            ident_str!("create").to_owned(),
            Vec::new(),
            gas_object_ref,
            Vec::new(),
            pure_args,
            GAS_VALUE_FOR_TESTING - 1, // Make sure budget is less than gas value
        )
        .await;

    let (_, order_effects) = call_response.unwrap();
    let gas_obj_idx = order_effects
        .mutated
        .iter()
        .position(|e| e.0 == gas_object_ref.0);
    // Get the object created from the call
    let new_obj_ref = order_effects.mutated.get(gas_obj_idx.unwrap() ^ 1).unwrap();
    // Fetch the full object
    let new_obj = client1
        .get_object_info(ObjectInfoRequest {
            object_id: new_obj_ref.0,
            request_sequence_number: None,
            request_received_transfers_excluding_first_nth: None,
        })
        .await
        .unwrap();

    gas_object_ref = client1
        .get_object_info(ObjectInfoRequest {
            object_id: gas_object_ref.0,
            request_sequence_number: None,
            request_received_transfers_excluding_first_nth: None,
        })
        .await
        .unwrap()
        .object
        .to_object_reference();

    let pure_args = vec![bcs::to_bytes(&client2.address.to_vec()).unwrap()];
    let call_response = client1
        .move_call(
            framework_obj_ref,
            ident_str!("ObjectBasics").to_owned(),
            ident_str!("transfer").to_owned(),
            Vec::new(),
            gas_object_ref,
            vec![new_obj.object.to_object_reference()],
            pure_args,
            GAS_VALUE_FOR_TESTING / 2,
        )
        .await;

    // Check all went well
    assert!(call_response.is_ok());
    // Check effects are good
    let (_, order_effects) = call_response.unwrap();
    // Status flag should be success
    assert_eq!(order_effects.status, ExecutionStatus::Success);
    // Nothing should be deleted during a transfer
    assert!(order_effects.deleted.is_empty());
    // Two items should be mutated during a transfer (gas and object being transferred)
    assert_eq!(order_effects.mutated.len(), 2);
    // Confirm the items
    let gas_obj_idx = order_effects
        .mutated
        .iter()
        .position(|e| e.0 == gas_object_ref.0);

    assert!(gas_obj_idx.is_some());
    let transferred_obj_ref = order_effects.mutated.get(gas_obj_idx.unwrap() ^ 1).unwrap();
    assert_ne!(gas_object_ref, *transferred_obj_ref);

    assert_eq!(transferred_obj_ref.0, new_obj_ref.0);

    let transferred_obj_info = client1
        .get_object_info(ObjectInfoRequest {
            object_id: new_obj_ref.0,
            request_sequence_number: None,
            request_received_transfers_excluding_first_nth: None,
        })
        .await
        .unwrap();

    // Confirm new owner
    assert_eq!(transferred_obj_info.object.owner, client2.address);
}

#[tokio::test]
async fn test_move_calls_object_transfer_and_freeze() {
    let (authority_clients, committee) = init_local_authorities(4).await;
    let mut client1 = make_client(authority_clients.clone(), committee.clone());
    let client2 = make_client(authority_clients.clone(), committee);

    let object_value: u64 = 100;
    let gas_object_id = ObjectID::random();

    // TODO: authority should not require seq# or digets for package in Move calls. Use dummy values
    let framework_obj_ref = (
        FASTX_FRAMEWORK_ADDRESS,
        SequenceNumber::new(),
        ObjectDigest::new([0; 32]),
    );

    // Populate authorities with obj data
    let authority_objects = vec![
        vec![gas_object_id],
        vec![gas_object_id],
        vec![gas_object_id],
        vec![gas_object_id],
    ];
    let mut gas_object_ref = fund_account(
        authority_clients.values().collect(),
        &mut client1,
        authority_objects,
    )
    .await
    .iter()
    .next()
    .unwrap()
    .1
    .to_object_reference();

    // When creating an ObjectBasics object, we provide the value (u64) and address which will own the object
    let pure_args = vec![
        object_value.to_le_bytes().to_vec(),
        bcs::to_bytes(&client1.address.to_vec()).unwrap(),
    ];
    let call_response = client1
        .move_call(
            framework_obj_ref,
            ident_str!("ObjectBasics").to_owned(),
            ident_str!("create").to_owned(),
            Vec::new(),
            gas_object_ref,
            Vec::new(),
            pure_args,
            GAS_VALUE_FOR_TESTING - 1, // Make sure budget is less than gas value
        )
        .await;

    let (_, order_effects) = call_response.unwrap();
    let gas_obj_idx = order_effects
        .mutated
        .iter()
        .position(|e| e.0 == gas_object_ref.0);
    // Get the object created from the call
    let new_obj_ref = order_effects.mutated.get(gas_obj_idx.unwrap() ^ 1).unwrap();
    // Fetch the full object
    let new_obj = client1
        .get_object_info(ObjectInfoRequest {
            object_id: new_obj_ref.0,
            request_sequence_number: None,
            request_received_transfers_excluding_first_nth: None,
        })
        .await
        .unwrap();

    gas_object_ref = client1
        .get_object_info(ObjectInfoRequest {
            object_id: gas_object_ref.0,
            request_sequence_number: None,
            request_received_transfers_excluding_first_nth: None,
        })
        .await
        .unwrap()
        .object
        .to_object_reference();

    let pure_args = vec![bcs::to_bytes(&client2.address.to_vec()).unwrap()];
    let call_response = client1
        .move_call(
            framework_obj_ref,
            ident_str!("ObjectBasics").to_owned(),
            ident_str!("transfer_and_freeze").to_owned(),
            Vec::new(),
            gas_object_ref,
            vec![new_obj.object.to_object_reference()],
            pure_args,
            GAS_VALUE_FOR_TESTING / 2,
        )
        .await;

    // Check all went well
    assert!(call_response.is_ok());
    // Check effects are good
    let (_, order_effects) = call_response.unwrap();
    // Status flag should be success
    assert_eq!(order_effects.status, ExecutionStatus::Success);
    // Nothing should be deleted during a transfer
    assert!(order_effects.deleted.is_empty());
    // Two items should be mutated during a transfer (gas and object being transferred)
    assert_eq!(order_effects.mutated.len(), 2);
    // Confirm the items
    let gas_obj_idx = order_effects
        .mutated
        .iter()
        .position(|e| e.0 == gas_object_ref.0);

    assert!(gas_obj_idx.is_some());
    let transferred_obj_ref = order_effects
        .mutated
        .get((gas_obj_idx.unwrap() + 1) % 2)
        .unwrap();
    assert_ne!(gas_object_ref, *transferred_obj_ref);

    assert_eq!(transferred_obj_ref.0, new_obj_ref.0);

    let transferred_obj_info = client1
        .get_object_info(ObjectInfoRequest {
            object_id: new_obj_ref.0,
            request_sequence_number: None,
            request_received_transfers_excluding_first_nth: None,
        })
        .await
        .unwrap();

    // Confirm new owner
    assert_eq!(transferred_obj_info.object.owner, client2.address);

    // Confirm read only
    assert!(transferred_obj_info.object.is_read_only());
}

#[tokio::test]
async fn test_move_calls_object_delete() {
    let (authority_clients, committee) = init_local_authorities(4).await;
    let mut client1 = make_client(authority_clients.clone(), committee);

    let object_value: u64 = 100;
    let gas_object_id = ObjectID::random();

    // TODO: authority should not require seq# or digets for package in Move calls. Use dummy values
    let framework_obj_ref = (
        FASTX_FRAMEWORK_ADDRESS,
        SequenceNumber::new(),
        ObjectDigest::new([0; 32]),
    );

    // Populate authorities with obj data
    let authority_objects = vec![
        vec![gas_object_id],
        vec![gas_object_id],
        vec![gas_object_id],
        vec![gas_object_id],
    ];
    let mut gas_object_ref = fund_account(
        authority_clients.values().collect(),
        &mut client1,
        authority_objects,
    )
    .await
    .iter()
    .next()
    .unwrap()
    .1
    .to_object_reference();

    // When creating an ObjectBasics object, we provide the value (u64) and address which will own the object
    let pure_args = vec![
        object_value.to_le_bytes().to_vec(),
        bcs::to_bytes(&client1.address.to_vec()).unwrap(),
    ];
    let call_response = client1
        .move_call(
            framework_obj_ref,
            ident_str!("ObjectBasics").to_owned(),
            ident_str!("create").to_owned(),
            Vec::new(),
            gas_object_ref,
            Vec::new(),
            pure_args,
            GAS_VALUE_FOR_TESTING - 1, // Make sure budget is less than gas value
        )
        .await;

    let (_, order_effects) = call_response.unwrap();
    let gas_obj_idx = order_effects
        .mutated
        .iter()
        .position(|e| e.0 == gas_object_ref.0);
    // Get the object created from the call
    let new_obj_ref = order_effects
        .mutated
        .get((gas_obj_idx.unwrap() + 1) % 2)
        .unwrap();
    // Fetch the full object
    let new_obj = client1
        .get_object_info(ObjectInfoRequest {
            object_id: new_obj_ref.0,
            request_sequence_number: None,
            request_received_transfers_excluding_first_nth: None,
        })
        .await
        .unwrap();

    gas_object_ref = client1
        .get_object_info(ObjectInfoRequest {
            object_id: gas_object_ref.0,
            request_sequence_number: None,
            request_received_transfers_excluding_first_nth: None,
        })
        .await
        .unwrap()
        .object
        .to_object_reference();

    let call_response = client1
        .move_call(
            framework_obj_ref,
            ident_str!("ObjectBasics").to_owned(),
            ident_str!("delete").to_owned(),
            Vec::new(),
            gas_object_ref,
            vec![new_obj.object.to_object_reference()],
            Vec::new(),
            GAS_VALUE_FOR_TESTING / 2,
        )
        .await;

    // Check all went well
    assert!(call_response.is_ok());
    // Check effects are good
    let (_, order_effects) = call_response.unwrap();
    // Status flag should be success
    assert_eq!(order_effects.status, ExecutionStatus::Success);
    // Object be deleted during a delete
    assert_eq!(order_effects.deleted.len(), 1);
    // One item should be mutated during a delete (gas)
    assert_eq!(order_effects.mutated.len(), 1);
    // Confirm the items
    let gas_obj_idx = order_effects
        .mutated
        .iter()
        .position(|e| e.0 == gas_object_ref.0);

    assert_eq!(gas_obj_idx.unwrap(), 0);
    // Try to fetch the deleted object
    let deleted_object_resp = client1
        .get_object_info(ObjectInfoRequest {
            object_id: new_obj_ref.0,
            request_sequence_number: None,
            request_received_transfers_excluding_first_nth: None,
        })
        .await;

    assert!(deleted_object_resp.is_err());
}
