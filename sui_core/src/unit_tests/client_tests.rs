// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![allow(clippy::same_item_push)] // get_key_pair returns random elements

use super::*;
use crate::authority::{AuthorityState, AuthorityStore};
use crate::client::client_store::ClientSingleAddressStore;
use crate::client::{Client, ClientState};
use async_trait::async_trait;
use futures::lock::Mutex;
use move_core_types::{account_address::AccountAddress, ident_str, identifier::Identifier};
use std::{
    collections::{BTreeMap, HashMap},
    convert::TryInto,
    sync::Arc,
};
use sui_adapter::genesis;
use sui_types::crypto::get_key_pair;
use sui_types::crypto::Signature;
use sui_types::gas_coin::GasCoin;
use sui_types::object::{Data, Object, Owner, GAS_VALUE_FOR_TESTING, OBJECT_START_VERSION};
use tokio::runtime::Runtime;
use typed_store::Map;

use std::env;
use std::fs;
use sui_types::error::SuiError::ObjectNotFound;

// Only relevant in a ser/de context : the `CertifiedTransaction` for a transaction is not unique
fn compare_certified_transactions(o1: &CertifiedTransaction, o2: &CertifiedTransaction) {
    assert_eq!(o1.transaction.digest(), o2.transaction.digest());
    // in this ser/de context it's relevant to compare signatures
    assert_eq!(o1.signatures, o2.signatures);
}

pub fn system_maxfiles() -> usize {
    fdlimit::raise_fd_limit().unwrap_or(256u64) as usize
}

fn max_files_client_tests() -> i32 {
    (system_maxfiles() / 8).try_into().unwrap()
}

#[derive(Clone)]
struct LocalAuthorityClient(Arc<Mutex<AuthorityState>>);

#[async_trait]
impl AuthorityAPI for LocalAuthorityClient {
    async fn handle_transaction(
        &self,
        transaction: Transaction,
    ) -> Result<TransactionInfoResponse, SuiError> {
        let state = self.0.clone();
        let result = state.lock().await.handle_transaction(transaction).await;
        result
    }

    async fn handle_confirmation_transaction(
        &self,
        transaction: ConfirmationTransaction,
    ) -> Result<TransactionInfoResponse, SuiError> {
        let state = self.0.clone();
        let result = state
            .lock()
            .await
            .handle_confirmation_transaction(transaction)
            .await;
        result
    }

    async fn handle_account_info_request(
        &self,
        request: AccountInfoRequest,
    ) -> Result<AccountInfoResponse, SuiError> {
        let state = self.0.clone();

        let result = state
            .lock()
            .await
            .handle_account_info_request(request)
            .await;
        result
    }

    async fn handle_object_info_request(
        &self,
        request: ObjectInfoRequest,
    ) -> Result<ObjectInfoResponse, SuiError> {
        let state = self.0.clone();
        let x = state.lock().await.handle_object_info_request(request).await;
        x
    }

    /// Handle Object information requests for this account.
    async fn handle_transaction_info_request(
        &self,
        request: TransactionInfoRequest,
    ) -> Result<TransactionInfoResponse, SuiError> {
        let state = self.0.clone();

        let result = state
            .lock()
            .await
            .handle_transaction_info_request(request)
            .await;
        result
    }
}

impl LocalAuthorityClient {
    fn new(state: AuthorityState) -> Self {
        Self(Arc::new(Mutex::new(state)))
    }
}

#[cfg(test)]
async fn extract_cert(
    authorities: &BTreeMap<AuthorityName, LocalAuthorityClient>,
    commitee: &Committee,
    transaction_digest: TransactionDigest,
) -> CertifiedTransaction {
    let mut votes = vec![];
    let mut transaction = None;
    for authority in authorities.values() {
        if let Ok(TransactionInfoResponse {
            signed_transaction: Some(signed),
            ..
        }) = authority
            .handle_transaction_info_request(TransactionInfoRequest::from(transaction_digest))
            .await
        {
            votes.push((signed.authority, signed.signature));
            if let Some(inner_transaction) = transaction {
                assert!(inner_transaction == signed.transaction);
            }
            transaction = Some(signed.transaction);
        }
    }

    let stake: usize = votes.iter().map(|(name, _)| commitee.weight(name)).sum();
    assert!(stake >= commitee.quorum_threshold());

    CertifiedTransaction {
        transaction: transaction.unwrap(),
        signatures: votes,
    }
}

#[cfg(test)]
fn transaction_create(
    src: SuiAddress,
    secret: &dyn signature::Signer<Signature>,
    dest: SuiAddress,
    value: u64,
    framework_obj_ref: ObjectRef,
    gas_object_ref: ObjectRef,
) -> Transaction {
    // When creating an ObjectBasics object, we provide the value (u64) and address which will own the object

    let pure_arguments = vec![
        value.to_le_bytes().to_vec(),
        bcs::to_bytes(&AccountAddress::from(dest)).unwrap(),
    ];

    Transaction::new_move_call(
        src,
        framework_obj_ref,
        ident_str!("ObjectBasics").to_owned(),
        ident_str!("create").to_owned(),
        Vec::new(),
        gas_object_ref,
        Vec::new(),
        pure_arguments,
        GAS_VALUE_FOR_TESTING / 2,
        &*secret,
    )
}

#[cfg(test)]
fn transaction_transfer(
    src: SuiAddress,
    secret: &dyn signature::Signer<Signature>,
    dest: SuiAddress,
    object_ref: ObjectRef,
    framework_obj_ref: ObjectRef,
    gas_object_ref: ObjectRef,
) -> Transaction {
    let pure_args = vec![bcs::to_bytes(&AccountAddress::from(dest)).unwrap()];

    Transaction::new_move_call(
        src,
        framework_obj_ref,
        ident_str!("ObjectBasics").to_owned(),
        ident_str!("transfer").to_owned(),
        Vec::new(),
        gas_object_ref,
        vec![object_ref],
        pure_args,
        GAS_VALUE_FOR_TESTING / 2,
        secret,
    )
}

#[cfg(test)]
fn transaction_set(
    src: SuiAddress,
    secret: &dyn signature::Signer<Signature>,
    object_ref: ObjectRef,
    value: u64,
    framework_obj_ref: ObjectRef,
    gas_object_ref: ObjectRef,
) -> Transaction {
    let pure_args = vec![bcs::to_bytes(&value).unwrap()];

    Transaction::new_move_call(
        src,
        framework_obj_ref,
        ident_str!("ObjectBasics").to_owned(),
        ident_str!("set_value").to_owned(),
        Vec::new(),
        gas_object_ref,
        vec![object_ref],
        pure_args,
        GAS_VALUE_FOR_TESTING / 2,
        secret,
    )
}

#[cfg(test)]
fn transaction_delete(
    src: SuiAddress,
    secret: &dyn signature::Signer<Signature>,
    object_ref: ObjectRef,
    framework_obj_ref: ObjectRef,
    gas_object_ref: ObjectRef,
) -> Transaction {
    Transaction::new_move_call(
        src,
        framework_obj_ref,
        ident_str!("ObjectBasics").to_owned(),
        ident_str!("delete").to_owned(),
        Vec::new(),
        gas_object_ref,
        vec![object_ref],
        Vec::new(),
        GAS_VALUE_FOR_TESTING / 2,
        secret,
    )
}

#[cfg(test)]
async fn do_transaction(authority: &LocalAuthorityClient, transaction: &Transaction) {
    authority
        .handle_transaction(transaction.clone())
        .await
        .unwrap();
}

#[cfg(test)]
async fn do_cert(
    authority: &LocalAuthorityClient,
    cert: &CertifiedTransaction,
) -> TransactionEffects {
    authority
        .handle_confirmation_transaction(ConfirmationTransaction::new(cert.clone()))
        .await
        .unwrap()
        .signed_effects
        .unwrap()
        .effects
}

#[cfg(test)]
async fn init_local_authorities(
    count: usize,
) -> (BTreeMap<AuthorityName, LocalAuthorityClient>, Committee) {
    let mut key_pairs = Vec::new();
    let mut voting_rights = BTreeMap::new();
    for _ in 0..count {
        let (_, key_pair) = get_key_pair();
        voting_rights.insert(*key_pair.public_key_bytes(), 1);
        key_pairs.push(key_pair);
    }
    let committee = Committee::new(voting_rights);

    let mut clients = BTreeMap::new();
    for secret in key_pairs {
        // Random directory for the DB
        let dir = env::temp_dir();
        let path = dir.join(format!("DB_{:?}", ObjectID::random()));
        fs::create_dir(&path).unwrap();

        let mut opts = rocksdb::Options::default();
        opts.set_max_open_files(max_files_client_tests());
        let store = Arc::new(AuthorityStore::open(path, Some(opts)));
        let authority_name = *secret.public_key_bytes();

        let state = AuthorityState::new(
            committee.clone(),
            authority_name,
            Box::pin(secret),
            store,
            genesis::clone_genesis_modules(),
        )
        .await;
        clients.insert(authority_name, LocalAuthorityClient::new(state));
    }
    (clients, committee)
}

#[cfg(test)]
async fn init_local_authorities_bad_1(
    count: usize,
) -> (BTreeMap<AuthorityName, LocalAuthorityClient>, Committee) {
    let mut key_pairs = Vec::new();
    let mut voting_rights = BTreeMap::new();
    for i in 0..count {
        let (_, secret) = get_key_pair();
        let authority_name = *secret.public_key_bytes();
        voting_rights.insert(authority_name, 1);
        if i + 1 < (count + 2) / 3 {
            // init 1 authority with a bad keypair
            let kp = {
                let (_, secret) = get_key_pair();
                let authority_name = *secret.public_key_bytes();
                (authority_name, secret)
            };
            key_pairs.push(kp);
        } else {
            key_pairs.push((authority_name, secret));
        }
    }
    let committee = Committee::new(voting_rights);

    let mut clients = BTreeMap::new();
    for (address, secret) in key_pairs {
        // Random directory
        let dir = env::temp_dir();
        let path = dir.join(format!("DB_{:?}", ObjectID::random()));
        fs::create_dir(&path).unwrap();

        let mut opts = rocksdb::Options::default();
        opts.set_max_open_files(max_files_client_tests());
        let store = Arc::new(AuthorityStore::open(path, Some(opts)));
        let state = AuthorityState::new(
            committee.clone(),
            address,
            Box::pin(secret),
            store,
            genesis::clone_genesis_modules(),
        )
        .await;
        clients.insert(address, LocalAuthorityClient::new(state));
    }
    (clients, committee)
}

#[cfg(test)]
fn make_client(
    authority_clients: BTreeMap<AuthorityName, LocalAuthorityClient>,
    committee: Committee,
) -> ClientState<LocalAuthorityClient> {
    let (address, secret) = get_key_pair();
    let pb_secret = Box::pin(secret);
    ClientState::new(
        env::temp_dir().join(format!("CLIENT_DB_{:?}", ObjectID::random())),
        address,
        pb_secret,
        committee,
        authority_clients,
    )
}

#[cfg(test)]
fn make_admin_client(
    authority_clients: BTreeMap<AuthorityName, LocalAuthorityClient>,
    committee: Committee,
) -> ClientState<LocalAuthorityClient> {
    use sui_types::crypto::get_key_pair_from_bytes;

    let (admin, admin_key) = get_key_pair_from_bytes(&[
        10, 112, 5, 142, 174, 127, 187, 146, 251, 68, 22, 191, 128, 68, 84, 13, 102, 71, 77, 57,
        92, 154, 128, 240, 158, 45, 13, 123, 57, 21, 194, 214, 189, 215, 127, 86, 129, 189, 1, 4,
        90, 106, 17, 10, 123, 200, 40, 18, 34, 173, 240, 91, 213, 72, 183, 249, 213, 210, 39, 181,
        105, 254, 59, 163,
    ]);
    let pb_secret = Box::pin(admin_key);
    ClientState::new(
        env::temp_dir().join(format!("CLIENT_DB_{:?}", ObjectID::random())),
        admin,
        pb_secret,
        committee,
        authority_clients,
    )
}

#[cfg(test)]
async fn fund_account_with_same_objects(
    authorities: Vec<&LocalAuthorityClient>,
    client: &mut ClientState<LocalAuthorityClient>,
    object_ids: Vec<ObjectID>,
) -> HashMap<ObjectID, Object> {
    let objs: Vec<_> = (0..authorities.len()).map(|_| object_ids.clone()).collect();
    fund_account(authorities, client, objs).await
}

#[cfg(test)]
async fn fund_account(
    authorities: Vec<&LocalAuthorityClient>,
    client: &mut ClientState<LocalAuthorityClient>,
    object_ids: Vec<Vec<ObjectID>>,
) -> HashMap<ObjectID, Object> {
    let mut created_objects = HashMap::new();
    for (authority, object_ids) in authorities.into_iter().zip(object_ids.into_iter()) {
        for object_id in object_ids {
            let object = Object::with_id_owner_for_testing(object_id, client.address());
            let client_ref = authority.0.as_ref().try_lock().unwrap();
            created_objects.insert(object_id, object.clone());

            let object_ref: ObjectRef = (object_id, 0.into(), object.digest());

            client_ref.init_transaction_lock(object_ref).await;
            client_ref.insert_object(object).await;
            client
                .store()
                .object_sequence_numbers
                .insert(&object_id, &SequenceNumber::new())
                .unwrap();
            client
                .store()
                .object_refs
                .insert(&object_id, &object_ref)
                .unwrap();
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
    let (authority_clients, committee) = init_local_authorities_bad_1(object_ids.len()).await;
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
fn test_initiating_valid_transfer() {
    let rt = Runtime::new().unwrap();
    let recipient = get_new_address();
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
        rt.block_on(sender.authorities().get_latest_owner(object_id_1)),
        (sender.address(), SequenceNumber::from(0))
    );
    assert_eq!(
        rt.block_on(sender.authorities().get_latest_owner(object_id_2)),
        (sender.address(), SequenceNumber::from(0))
    );
    let (certificate, _) = rt
        .block_on(sender.transfer_object(object_id_1, gas_object, recipient))
        .unwrap();
    assert_eq!(
        sender.highest_known_version(&object_id_1),
        Err(SuiError::ObjectNotFound {
            object_id: object_id_1
        })
    );
    assert!(sender.store().pending_transactions.is_empty());
    assert_eq!(
        rt.block_on(sender.authorities().get_latest_owner(object_id_1)),
        (recipient, SequenceNumber::from(1))
    );
    assert_eq!(
        rt.block_on(sender.authorities().get_latest_owner(object_id_2)),
        (sender.address(), SequenceNumber::from(0))
    );
    // valid since our test authority should not update its certificate set
    compare_certified_transactions(
        &rt.block_on(sender.authorities().request_certificate(
            sender.address(),
            object_id_1,
            SequenceNumber::from(0),
        ))
        .unwrap(),
        &certificate,
    );
}

#[test]
fn test_initiating_valid_transfer_despite_bad_authority() {
    let rt = Runtime::new().unwrap();
    let recipient = get_new_address();
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
    let (certificate, _) = rt
        .block_on(sender.transfer_object(object_id, gas_object, recipient))
        .unwrap();
    assert_eq!(
        sender.highest_known_version(&object_id),
        Err(ObjectNotFound { object_id })
    );
    assert!(sender.store().pending_transactions.is_empty());
    assert_eq!(
        rt.block_on(sender.authorities().get_latest_owner(object_id)),
        (recipient, SequenceNumber::from(1))
    );
    // valid since our test authority shouldn't update its certificate set
    compare_certified_transactions(
        &rt.block_on(sender.authorities().request_certificate(
            sender.address(),
            object_id,
            SequenceNumber::from(0),
        ))
        .unwrap(),
        &certificate,
    );
}

#[test]
fn test_initiating_transfer_low_funds() {
    let rt = Runtime::new().unwrap();
    let recipient = get_new_address();
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
        sender.highest_known_version(&object_id_2),
        Ok(SequenceNumber::from(0))
    );
    // assert_eq!(sender.pending_transfer, None);
    assert_eq!(
        rt.block_on(sender.authorities().get_latest_owner(object_id_1)),
        (sender.address(), SequenceNumber::from(0)),
    );
    assert_eq!(
        rt.block_on(sender.authorities().get_latest_owner(object_id_2))
            .1,
        SequenceNumber::from(0),
    );
}

#[tokio::test]
async fn test_bidirectional_transfer() {
    let (authority_clients, committee) = init_local_authorities(4).await;
    let mut client1 = make_client(authority_clients.clone(), committee.clone());
    let mut client2 = make_client(authority_clients.clone(), committee);

    let object_id = ObjectID::random();
    let gas_object1 = ObjectID::random();
    let gas_object2 = ObjectID::random();

    fund_account_with_same_objects(
        authority_clients.values().collect(),
        &mut client1,
        vec![object_id, gas_object1],
    )
    .await;
    fund_account_with_same_objects(
        authority_clients.values().collect(),
        &mut client2,
        vec![gas_object2],
    )
    .await;

    // Confirm client1 have ownership of the object.
    assert_eq!(
        client1.authorities().get_latest_owner(object_id).await,
        (client1.address(), SequenceNumber::from(0))
    );
    // Confirm client2 doesn't have ownership of the object.
    assert_eq!(
        client2.authorities().get_latest_owner(object_id).await,
        (client1.address(), SequenceNumber::from(0))
    );
    // Transfer object to client2.
    let (certificate, _) = client1
        .transfer_object(object_id, gas_object1, client2.address())
        .await
        .unwrap();

    assert!(client1.store().pending_transactions.is_empty());
    // Confirm client1 lose ownership of the object.
    assert_eq!(
        client1.authorities().get_latest_owner(object_id).await,
        (client2.address(), SequenceNumber::from(1))
    );
    // Confirm client2 acquired ownership of the object.
    assert_eq!(
        client2.authorities().get_latest_owner(object_id).await,
        (client2.address(), SequenceNumber::from(1))
    );

    // Confirm certificate is consistent between authorities and client.
    // valid since our test authority should not update its certificate set
    compare_certified_transactions(
        &client1
            .authorities()
            .request_certificate(client1.address(), object_id, SequenceNumber::from(0))
            .await
            .unwrap(),
        &certificate,
    );

    // Update client2's local object data.
    client2.sync_client_state().await.unwrap();

    // Confirm sequence number are consistent between clients.
    assert_eq!(
        client2.authorities().get_latest_owner(object_id).await,
        (client2.address(), SequenceNumber::from(1))
    );

    // Transfer the object back to Client1
    client2
        .transfer_object(object_id, gas_object2, client1.address())
        .await
        .unwrap();

    assert!((client2.store().pending_transactions.is_empty()));

    // Confirm client2 lose ownership of the object.
    assert_eq!(
        client2.authorities().get_latest_owner(object_id).await,
        (client1.address(), SequenceNumber::from(2))
    );
    assert_eq!(
        client2
            .authorities()
            .get_latest_sequence_number(object_id)
            .await,
        SequenceNumber::from(2)
    );
    // Confirm client1 acquired ownership of the object.
    assert_eq!(
        client1.authorities().get_latest_owner(object_id).await,
        (client1.address(), SequenceNumber::from(2))
    );

    // Should fail if Client 2 double spend the object
    assert!(client2
        .transfer_object(object_id, gas_object2, client1.address(),)
        .await
        .is_err());
}

#[test]
fn test_client_state_sync() {
    let rt = Runtime::new().unwrap();

    let object_ids = (0..20)
        .map(|_| ObjectID::random())
        .collect::<Vec<ObjectID>>();
    let authority_objects = (0..10).map(|_| object_ids.clone()).collect();

    let mut sender = rt.block_on(init_local_client_state(authority_objects));

    let old_object_ids: BTreeMap<_, _> = sender.store().object_sequence_numbers.iter().collect();
    let old_certificates: BTreeMap<_, _> = sender.store().certificates.iter().collect();

    // Remove all client-side data
    sender.store().object_sequence_numbers.clear().unwrap();
    sender.store().certificates.clear().unwrap();
    sender.store().object_refs.clear().unwrap();
    assert!(sender.get_owned_objects().is_empty());

    // Sync client state
    rt.block_on(sender.sync_client_state()).unwrap();

    // Confirm data are the same after sync
    assert!(!sender.get_owned_objects().is_empty());
    assert_eq!(
        &old_object_ids,
        &sender.store().object_sequence_numbers.iter().collect()
    );
    for tx_digest in old_certificates.keys() {
        // valid since our test authority should not lead us to download new certs
        compare_certified_transactions(
            old_certificates.get(tx_digest).unwrap(),
            &sender.store().certificates.get(tx_digest).unwrap().unwrap(),
        );
    }
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
        .transfer_object(object_id, gas_object_id, client2.address())
        .await
        .unwrap();

    // Confirm client2 acquired ownership of the object.
    assert_eq!(
        client2.authorities().get_latest_owner(object_id).await,
        (client2.address(), SequenceNumber::from(1))
    );

    // Client 2's local object_id and cert should be empty before sync
    assert!(client2.get_owned_objects().is_empty());
    assert!(client2.store().object_sequence_numbers.is_empty());
    assert!(&client2.store().certificates.is_empty());

    // Sync client state
    client2.sync_client_state().await.unwrap();

    // Confirm client 2 received the new object id and cert
    assert_eq!(1, client2.get_owned_objects().len());
    assert_eq!(1, client2.store().object_sequence_numbers.iter().count());
    assert_eq!(1, client2.store().certificates.iter().count());
}

#[tokio::test]
async fn test_move_calls_object_create() {
    let (authority_clients, committee) = init_local_authorities(4).await;
    let mut client1 = make_client(authority_clients.clone(), committee);

    let object_value: u64 = 100;
    let gas_object_id = ObjectID::random();
    let framework_obj_ref = client1.get_framework_object_ref().await.unwrap();

    // Populate authorities with obj data
    let gas_object_ref = fund_account_with_same_objects(
        authority_clients.values().collect(),
        &mut client1,
        vec![gas_object_id],
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
        bcs::to_bytes(&AccountAddress::from(client1.address())).unwrap(),
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

    // Check effects are good
    let (_, transaction_effects) = call_response.unwrap();
    // Status flag should be success
    assert!(matches!(
        transaction_effects.status,
        ExecutionStatus::Success { .. }
    ));
    // Nothing should be deleted during a creation
    assert!(transaction_effects.deleted.is_empty());
    // A new object is created. Gas is mutated.
    assert_eq!(
        (
            transaction_effects.created.len(),
            transaction_effects.mutated.len()
        ),
        (1, 1)
    );
    assert_eq!(transaction_effects.gas_object.0 .0, gas_object_id);
}

#[tokio::test]
async fn test_move_calls_object_transfer() {
    let (authority_clients, committee) = init_local_authorities(4).await;
    let mut client1 = make_client(authority_clients.clone(), committee.clone());
    let client2 = make_client(authority_clients.clone(), committee);

    let object_value: u64 = 100;
    let gas_object_id = ObjectID::random();
    let framework_obj_ref = client1.get_framework_object_ref().await.unwrap();

    // Populate authorities with obj data
    let mut gas_object_ref = fund_account_with_same_objects(
        authority_clients.values().collect(),
        &mut client1,
        vec![gas_object_id],
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
        bcs::to_bytes(&AccountAddress::from(client1.address())).unwrap(),
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

    let (_, transaction_effects) = call_response.unwrap();

    assert_eq!(transaction_effects.gas_object.0 .0, gas_object_id);

    // Get the object created from the call
    let (new_obj_ref, _) = transaction_effects.created[0];
    gas_object_ref = client_object(&mut client1, gas_object_ref.0).await.0;

    let pure_args = vec![bcs::to_bytes(&AccountAddress::from(client2.address())).unwrap()];
    let call_response = client1
        .move_call(
            framework_obj_ref,
            ident_str!("ObjectBasics").to_owned(),
            ident_str!("transfer").to_owned(),
            Vec::new(),
            gas_object_ref,
            vec![new_obj_ref],
            pure_args,
            GAS_VALUE_FOR_TESTING / 2,
        )
        .await;

    // Check effects are good
    let (_, transaction_effects) = call_response.unwrap();
    // Status flag should be success
    assert!(matches!(
        transaction_effects.status,
        ExecutionStatus::Success { .. }
    ));
    // Nothing should be deleted during a transfer
    assert!(transaction_effects.deleted.is_empty());
    // The object being transfered will be in mutated.
    assert_eq!(transaction_effects.mutated.len(), 2);
    // Confirm the items
    assert_eq!(transaction_effects.gas_object.0 .0, gas_object_id);

    let (transferred_obj_ref, _) = *transaction_effects.mutated_excluding_gas().next().unwrap();
    assert_ne!(gas_object_ref, transferred_obj_ref);

    assert_eq!(transferred_obj_ref.0, new_obj_ref.0);

    let transferred_obj = client_object(&mut client1, new_obj_ref.0).await.1;

    // Confirm new owner
    assert!(transferred_obj.owner == client2.address());
}

#[tokio::test]
async fn test_move_calls_object_transfer_and_freeze() {
    let (authority_clients, committee) = init_local_authorities(4).await;
    let mut client1 = make_client(authority_clients.clone(), committee.clone());
    let client2 = make_client(authority_clients.clone(), committee);

    let object_value: u64 = 100;
    let gas_object_id = ObjectID::random();
    let framework_obj_ref = client1.get_framework_object_ref().await.unwrap();

    // Populate authorities with obj data
    let mut gas_object_ref = fund_account_with_same_objects(
        authority_clients.values().collect(),
        &mut client1,
        vec![gas_object_id],
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
        bcs::to_bytes(&AccountAddress::from(client1.address())).unwrap(),
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

    let (_, transaction_effects) = call_response.unwrap();
    // Get the object created from the call
    let (new_obj_ref, _) = transaction_effects.created[0];
    // Fetch the full object
    let new_obj_ref = client_object(&mut client1, new_obj_ref.0).await.0;
    gas_object_ref = client_object(&mut client1, gas_object_ref.0).await.0;

    let pure_args = vec![bcs::to_bytes(&AccountAddress::from(client2.address())).unwrap()];
    let call_response = client1
        .move_call(
            framework_obj_ref,
            ident_str!("ObjectBasics").to_owned(),
            ident_str!("transfer_and_freeze").to_owned(),
            Vec::new(),
            gas_object_ref,
            vec![new_obj_ref],
            pure_args,
            GAS_VALUE_FOR_TESTING / 2,
        )
        .await;

    // Check effects are good
    let (_, transaction_effects) = call_response.unwrap();
    // Status flag should be success
    assert!(matches!(
        transaction_effects.status,
        ExecutionStatus::Success { .. }
    ));
    // Nothing should be deleted during a transfer
    assert!(transaction_effects.deleted.is_empty());
    // Item being transfered is mutated. Plus gas object.
    assert_eq!(transaction_effects.mutated.len(), 2);

    let (transferred_obj_ref, _) = *transaction_effects.mutated_excluding_gas().next().unwrap();
    assert_ne!(gas_object_ref, transferred_obj_ref);

    assert_eq!(transferred_obj_ref.0, new_obj_ref.0);

    let transferred_obj = client_object(&mut client1, new_obj_ref.0).await.1;

    // Confirm new owner
    assert!(transferred_obj.owner == Owner::SharedImmutable);

    // Confirm read only
    assert!(transferred_obj.is_read_only());
}

#[tokio::test]
async fn test_move_calls_object_delete() {
    let (authority_clients, committee) = init_local_authorities(4).await;
    let mut client1 = make_client(authority_clients.clone(), committee);

    let object_value: u64 = 100;
    let gas_object_id = ObjectID::random();
    let framework_obj_ref = client1.get_framework_object_ref().await.unwrap();

    // Populate authorities with obj data
    let mut gas_object_ref = fund_account_with_same_objects(
        authority_clients.values().collect(),
        &mut client1,
        vec![gas_object_id],
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
        bcs::to_bytes(&AccountAddress::from(client1.address())).unwrap(),
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

    let (_, transaction_effects) = call_response.unwrap();
    // Get the object created from the call
    let (new_obj_ref, _) = transaction_effects.created[0];

    gas_object_ref = client_object(&mut client1, gas_object_ref.0).await.0;

    let call_response = client1
        .move_call(
            framework_obj_ref,
            ident_str!("ObjectBasics").to_owned(),
            ident_str!("delete").to_owned(),
            Vec::new(),
            gas_object_ref,
            vec![new_obj_ref],
            Vec::new(),
            GAS_VALUE_FOR_TESTING / 2,
        )
        .await;

    // Check effects are good
    let (_, transaction_effects) = call_response.unwrap();
    // Status flag should be success
    assert!(matches!(
        transaction_effects.status,
        ExecutionStatus::Success { .. }
    ));
    // Object be deleted during a delete
    assert_eq!(transaction_effects.deleted.len(), 1);
    // Only gas is mutated.
    assert_eq!(transaction_effects.mutated.len(), 1);
    // Confirm the items
    assert_eq!(transaction_effects.gas_object.0 .0, gas_object_id);

    // Try to fetch the deleted object
    let deleted_object_resp = client1.get_object_info(new_obj_ref.0).await.unwrap();

    if let ObjectRead::Deleted(_) = deleted_object_resp {
    } else {
        panic!("Object should be deleted.")
    }
}

async fn get_package_obj(
    client: &mut ClientState<LocalAuthorityClient>,
    objects: &[(ObjectRef, Owner)],
    gas_object_ref: &ObjectRef,
) -> Option<ObjectRead> {
    let mut pkg_obj_opt = None;
    for (new_obj_ref, _) in objects {
        assert_ne!(gas_object_ref, new_obj_ref);
        let new_obj = client.get_object_info(new_obj_ref.0).await.unwrap();
        if let Data::Package(_) = new_obj.object().unwrap().data {
            pkg_obj_opt = Some(new_obj);
        }
    }
    pkg_obj_opt
}

#[tokio::test]
async fn test_module_publish_and_call_good() {
    // Init the states
    let (authority_clients, committee) = init_local_authorities(4).await;
    let mut client1 = make_admin_client(authority_clients.clone(), committee);

    let gas_object_id = ObjectID::random();

    // Populate authorities with gas obj data
    let gas_object_ref = fund_account_with_same_objects(
        authority_clients.values().collect(),
        &mut client1,
        vec![gas_object_id],
    )
    .await
    .iter()
    .next()
    .unwrap()
    .1
    .to_object_reference();

    // Provide path to well formed package sources
    let mut hero_path = env!("CARGO_MANIFEST_DIR").to_owned();
    hero_path.push_str("/src/unit_tests/data/hero/");

    let pub_res = client1
        .publish(hero_path, gas_object_ref, GAS_VALUE_FOR_TESTING / 2)
        .await;

    let (_, published_effects) = pub_res.unwrap();

    assert!(matches!(
        published_effects.status,
        ExecutionStatus::Success { .. }
    ));

    // A package obj and two objects resulting from two
    // initializer runs in different modules should be created.
    assert_eq!(published_effects.created.len(), 3);

    // Verify gas obj
    assert_eq!(published_effects.gas_object.0 .0, gas_object_ref.0);

    for (new_obj_ref, _) in &published_effects.created {
        assert_ne!(gas_object_ref, *new_obj_ref);
    }

    // find the package object and inspect it

    let new_obj = get_package_obj(&mut client1, &published_effects.created, &gas_object_ref)
        .await
        .unwrap();

    // Version should be 1 for all modules
    assert_eq!(new_obj.object().unwrap().version(), OBJECT_START_VERSION);
    // Must be immutable
    assert!(new_obj.object().unwrap().is_read_only());

    // StructTag type is not defined for package
    assert!(new_obj.object().unwrap().type_().is_none());

    // Data should be castable as a package
    assert!(new_obj.object().unwrap().data.try_as_package().is_some());

    // This gets the treasury cap for the coin and gives it to the sender
    let mut tres_cap_opt = None;
    for (new_obj_ref, _) in &published_effects.created {
        let new_obj = client1.get_object_info(new_obj_ref.0).await.unwrap();
        if let Data::Move(move_obj) = &new_obj.object().unwrap().data {
            if move_obj.type_.module == Identifier::new("Coin").unwrap()
                && move_obj.type_.name == Identifier::new("TreasuryCap").unwrap()
            {
                tres_cap_opt = Some(new_obj);
            }
        }
    }

    let tres_cap_obj_info = tres_cap_opt.unwrap();

    // Retrieve latest gas obj spec
    let (gas_object_ref, gas_object) = client_object(&mut client1, gas_object_id).await;

    // Confirm we own this object
    assert_eq!(tres_cap_obj_info.object().unwrap().owner, gas_object.owner);

    //Try to call a function in TrustedCoin module
    let call_resp = client1
        .move_call(
            new_obj.object().unwrap().to_object_reference(),
            ident_str!("TrustedCoin").to_owned(),
            ident_str!("mint").to_owned(),
            vec![],
            gas_object_ref,
            vec![tres_cap_obj_info.object().unwrap().to_object_reference()],
            vec![42u64.to_le_bytes().to_vec()],
            1000,
        )
        .await
        .unwrap();

    let effects = call_resp.1;
    assert!(matches!(effects.status, ExecutionStatus::Success { .. }));

    // This gets the treasury cap for the coin and gives it to the sender
    let tres_cap_ref = effects
        .created
        .iter()
        .find(|r| r.0 .0 != gas_object_ref.0)
        .unwrap()
        .0;

    // Fetch the full obj info
    let (_, tres_cap_obj) = client_object(&mut client1, tres_cap_ref.0).await;

    // Confirm we own this object
    assert_eq!(tres_cap_obj.owner, gas_object.owner);
}

// Pass a file in a package dir instead of the root. The builder should be able to infer the root
#[tokio::test]
async fn test_module_publish_file_path() {
    // Init the states
    let (authority_clients, committee) = init_local_authorities(4).await;
    let mut client1 = make_admin_client(authority_clients.clone(), committee);

    let gas_object_id = ObjectID::random();

    // Populate authorities with gas obj data
    let gas_object_ref = fund_account_with_same_objects(
        authority_clients.values().collect(),
        &mut client1,
        vec![gas_object_id],
    )
    .await
    .iter()
    .next()
    .unwrap()
    .1
    .to_object_reference();

    // Compile
    let mut hero_path = env!("CARGO_MANIFEST_DIR").to_owned();

    // Use a path pointing to a different file
    hero_path.push_str("/src/unit_tests/data/hero/Hero.move");

    let pub_resp = client1
        .publish(hero_path, gas_object_ref, GAS_VALUE_FOR_TESTING / 2)
        .await;

    let (_, published_effects) = pub_resp.unwrap();

    assert!(matches!(
        published_effects.status,
        ExecutionStatus::Success { .. }
    ));

    // Even though we provided a path to Hero.move, the builder is
    // able to find the package root build all in the package,
    // including TrustedCoin module
    //
    // Consequently,a package obj and two objects resulting from two
    // initializer runs in different modules should be created.
    assert_eq!(published_effects.created.len(), 3);

    // Verify gas
    assert_eq!(published_effects.gas_object.0 .0, gas_object_ref.0);

    for (new_obj_ref, _) in &published_effects.created {
        assert_ne!(gas_object_ref, *new_obj_ref);
    }
    // find the package object and inspect it

    let new_obj = get_package_obj(&mut client1, &published_effects.created, &gas_object_ref)
        .await
        .unwrap();

    // Version should be 1 for all modules
    assert_eq!(new_obj.object().unwrap().version(), OBJECT_START_VERSION);
    // Must be immutable
    assert!(new_obj.object().unwrap().is_read_only());

    // StructTag type is not defined for package
    assert!(new_obj.object().unwrap().type_().is_none());

    // Data should be castable as a package
    assert!(new_obj.object().unwrap().data.try_as_package().is_some());
}

#[tokio::test]
async fn test_module_publish_bad_path() {
    // Init the states
    let (authority_clients, committee) = init_local_authorities(4).await;
    let mut client1 = make_client(authority_clients.clone(), committee);

    let gas_object_id = ObjectID::random();

    // Populate authorities with gas obj data
    let gas_object_ref = fund_account_with_same_objects(
        authority_clients.values().collect(),
        &mut client1,
        vec![gas_object_id],
    )
    .await
    .iter()
    .next()
    .unwrap()
    .1
    .to_object_reference();

    // Compile
    let mut hero_path = env!("CARGO_MANIFEST_DIR").to_owned();

    // Use a bad path
    hero_path.push_str("/src/unit____________tests/data/hero/");

    let pub_resp = client1.publish(hero_path, gas_object_ref, 1000).await;
    // Has to fail
    assert!(pub_resp.is_err());
}

#[tokio::test]
async fn test_module_publish_naughty_path() {
    // Init the states
    let (authority_clients, committee) = init_local_authorities(4).await;
    let mut client1 = make_client(authority_clients.clone(), committee);

    let gas_object_id = ObjectID::random();

    // Populate authorities with gas obj data
    let gas_object_ref = fund_account_with_same_objects(
        authority_clients.values().collect(),
        &mut client1,
        vec![gas_object_id],
    )
    .await
    .iter()
    .next()
    .unwrap()
    .1
    .to_object_reference();

    for ns in naughty_strings::BLNS {
        // Compile
        let mut hero_path = env!("CARGO_MANIFEST_DIR").to_owned();

        // Use a bad path
        hero_path.push_str(&format!("/../{}", ns));

        let pub_resp = client1.publish(hero_path, gas_object_ref, 1000).await;
        // Has to fail
        assert!(pub_resp.is_err());
    }
}

#[test]
fn test_transfer_object_error() {
    let rt = Runtime::new().unwrap();
    let recipient = get_new_address();

    let objects: Vec<ObjectID> = (0..10).map(|_| ObjectID::random()).collect();
    let gas_object = ObjectID::random();
    let number_of_authorities = 4;

    let mut all_objects = objects.clone();
    all_objects.push(gas_object);
    let authority_objects = (0..number_of_authorities)
        .map(|_| all_objects.clone())
        .collect();

    let mut sender = rt.block_on(init_local_client_state(authority_objects));

    let mut objects = objects.iter();

    // Test 1: Double spend
    let object_id = *objects.next().unwrap();
    rt.block_on(sender.transfer_object(object_id, gas_object, recipient))
        .unwrap();
    let result = rt.block_on(sender.transfer_object(object_id, gas_object, recipient));

    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err().downcast_ref(),
        Some(SuiError::ObjectNotFound { .. })
    ));

    // Test 2: Object not known to authorities
    let obj = Object::with_id_owner_for_testing(ObjectID::random(), sender.address());
    sender
        .store()
        .object_refs
        .insert(&obj.id(), &obj.to_object_reference())
        .unwrap();
    sender
        .store()
        .object_sequence_numbers
        .insert(&obj.id(), &SequenceNumber::new())
        .unwrap();
    let result = rt.block_on(sender.transfer_object(obj.id(), gas_object, recipient));
    assert!(result.is_err());

    // Test 3: invalid object digest
    let object_id = *objects.next().unwrap();

    // give object an incorrect object digest
    sender
        .store()
        .object_refs
        .insert(
            &object_id,
            &(object_id, SequenceNumber::new(), ObjectDigest([0; 32])),
        )
        .unwrap();

    let result = rt.block_on(sender.transfer_object(object_id, gas_object, recipient));
    assert!(result.is_err());

    // Test 4: Invalid sequence number;
    let object_id = *objects.next().unwrap();

    // give object an incorrect sequence number
    sender
        .store()
        .object_sequence_numbers
        .insert(&object_id, &SequenceNumber::from(2))
        .unwrap();

    let result = rt.block_on(sender.transfer_object(object_id, gas_object, recipient));
    assert!(result.is_err());

    // Test 5: The client does not allow concurrent transfer;
    let object_id = *objects.next().unwrap();
    // Fabricate a fake pending transfer
    sender
        .lock_pending_transaction_objects(&Transaction::new_transfer(
            SuiAddress::random_for_testing_only(),
            (object_id, Default::default(), ObjectDigest::new([0; 32])),
            sender.address(),
            (gas_object, Default::default(), ObjectDigest::new([0; 32])),
            &get_key_pair().1,
        ))
        .unwrap();

    let result = rt.block_on(sender.transfer_object(object_id, gas_object, recipient));
    assert!(result.is_err());
}

#[test]
fn test_client_store() {
    let store = ClientSingleAddressStore::new(
        env::temp_dir().join(format!("CLIENT_DB_{:?}", ObjectID::random())),
    );

    // Make random sequence numbers
    let keys_vals = (0..100)
        .map(|i| (ObjectID::random(), SequenceNumber::from(i)))
        .collect::<Vec<_>>();
    // Try insert batch
    store
        .object_sequence_numbers
        .multi_insert(keys_vals.clone().into_iter())
        .unwrap();

    // Check the size
    assert_eq!(store.object_sequence_numbers.iter().count(), 100);

    // Check that the items are all correct
    keys_vals.iter().for_each(|(k, v)| {
        assert_eq!(*v, store.object_sequence_numbers.get(k).unwrap().unwrap());
    });

    // Check that are removed
    store
        .object_sequence_numbers
        .multi_remove(keys_vals.into_iter().map(|(k, _)| k))
        .unwrap();

    assert!(store.object_sequence_numbers.is_empty());
}

#[tokio::test]
async fn test_object_store() {
    // Init the states
    let (authority_clients, committee) = init_local_authorities(4).await;
    // We need admin account as we will be calling initializers on
    // modules which check if the caller/publisher is the admin
    // account.
    let mut client1 = make_admin_client(authority_clients.clone(), committee.clone());

    let gas_object_id = ObjectID::random();

    // Populate authorities with gas obj data
    let gas_object = fund_account_with_same_objects(
        authority_clients.values().collect(),
        &mut client1,
        vec![gas_object_id],
    )
    .await
    .iter()
    .next()
    .unwrap()
    .1
    .clone();
    let gas_object_ref = gas_object.clone().to_object_reference();
    // Ensure that object store is empty
    assert!(client1.store().objects.is_empty());

    // Run a few syncs to retrieve objects ids
    for _ in 0..4 {
        let _ = client1.sync_client_state().await.unwrap();
    }
    // Try to download objects which are not already in storage
    client1.download_owned_objects_not_in_db().await.unwrap();

    // Gas object should be in storage now
    assert_eq!(client1.store().objects.iter().count(), 1);

    // Verify that we indeed have the object
    let gas_obj_from_store = client1
        .store()
        .objects
        .get(&gas_object_ref)
        .unwrap()
        .unwrap();
    assert_eq!(gas_obj_from_store, gas_object);

    // Provide path to well formed package sources
    let mut hero_path = env!("CARGO_MANIFEST_DIR").to_owned();
    hero_path.push_str("/src/unit_tests/data/hero/");

    let pub_res = client1
        .publish(hero_path, gas_object_ref, GAS_VALUE_FOR_TESTING / 2)
        .await;

    let (_, published_effects) = pub_res.as_ref().unwrap();

    assert!(matches!(
        published_effects.status,
        ExecutionStatus::Success { .. }
    ));

    // A package obj and two objects resulting from two
    // initializer runs in different modules should be created.
    assert_eq!(published_effects.created.len(), 3);

    // Verify gas obj
    assert_eq!(published_effects.gas_object.0 .0, gas_object_ref.0);

    for (new_obj_ref, _) in &published_effects.created {
        assert_ne!(gas_object_ref, *new_obj_ref);
    }

    // find the package object and inspect it

    let _new_obj = get_package_obj(&mut client1, &published_effects.created, &gas_object_ref)
        .await
        .unwrap();

    // New gas object should be in storage, so 1 new items, plus 3 from before
    // The published package is not in the store because it's not owned by anyone.
    assert_eq!(client1.store().objects.iter().count(), 4);

    // TODO: Verify that we have new_obj in the local store once we can store shared immutable objects.
}

#[tokio::test]
async fn test_object_store_transfer() {
    let (authority_clients, committee) = init_local_authorities(4).await;
    let mut client1 = make_client(authority_clients.clone(), committee.clone());
    let mut client2 = make_client(authority_clients.clone(), committee);

    let object_id = ObjectID::random();
    let gas_object1 = ObjectID::random();
    let gas_object2 = ObjectID::random();

    fund_account_with_same_objects(
        authority_clients.values().collect(),
        &mut client1,
        vec![object_id, gas_object1],
    )
    .await;
    fund_account_with_same_objects(
        authority_clients.values().collect(),
        &mut client2,
        vec![gas_object2],
    )
    .await;

    // Clients should not have retrieved objects
    assert_eq!(client1.store().objects.iter().count(), 0);
    assert_eq!(client2.store().objects.iter().count(), 0);

    // Run a few syncs to populate object ids
    for _ in 0..4 {
        let _ = client1.sync_client_state().await.unwrap();
        let _ = client2.sync_client_state().await.unwrap();
    }

    // Try to download objects which are not already in storage
    client1.download_owned_objects_not_in_db().await.unwrap();
    client2.download_owned_objects_not_in_db().await.unwrap();

    // Gas object and another object should be in storage now for client 1
    assert_eq!(client1.store().objects.iter().count(), 2);

    // Only gas object should be in storage now for client 2
    assert_eq!(client2.store().objects.iter().count(), 1);

    // Transfer object to client2.
    let _certificate = client1
        .transfer_object(object_id, gas_object1, client2.address())
        .await
        .unwrap();

    // Update client2's local object data.
    client2.sync_client_state().await.unwrap();

    // Client 1 should not have lost its objects
    // Plus it should have a new gas object
    assert_eq!(client1.store().objects.iter().count(), 3);
    // Client 2 should now have the new object
    assert_eq!(client2.store().objects.iter().count(), 1);

    // Transfer the object back to Client1
    let _certificate = client2
        .transfer_object(object_id, gas_object2, client1.address())
        .await
        .unwrap();

    // Update client1's local object data.
    client1.sync_client_state().await.unwrap();

    // Client 1 should have a new version of the object back
    assert_eq!(client1.store().objects.iter().count(), 3);
    // Client 2 should have new gas object version
    assert_eq!(client2.store().objects.iter().count(), 2);
}

// A helper function to make tests less verbose
async fn client_object(client: &mut dyn Client, object_id: ObjectID) -> (ObjectRef, Object) {
    let info = client.get_object_info(object_id).await.unwrap();

    (info.reference().unwrap(), info.object().unwrap().clone())
}

// A helper function to make tests less verbose
#[allow(dead_code)]
async fn auth_object(authority: &LocalAuthorityClient, object_id: ObjectID) -> (ObjectRef, Object) {
    let response = authority
        .handle_object_info_request(ObjectInfoRequest::latest_object_info_request(
            object_id, None,
        ))
        .await
        .unwrap();

    let object = response.object_and_lock.unwrap().object;
    (object.to_object_reference(), object)
}

#[tokio::test]
async fn test_map_reducer() {
    let (authority_clients, committee) = init_local_authorities(4).await;
    let client1 = make_client(authority_clients.clone(), committee.clone());

    // Test: reducer errors get propagated up
    let res = client1
        .authorities()
        .quorum_map_then_reduce_with_timeout(
            0usize,
            |_name, _client| Box::pin(async move { Ok(()) }),
            |_accumulated_state, _authority_name, _authority_weight, _result| {
                Box::pin(async move { Err(SuiError::TooManyIncorrectAuthorities) })
            },
            Duration::from_millis(1000),
        )
        .await;
    assert!(Err(SuiError::TooManyIncorrectAuthorities) == res);

    // Test: mapper errors do not get propagated up, reducer works
    let res = client1
        .authorities()
        .quorum_map_then_reduce_with_timeout(
            0usize,
            |_name, _client| {
                Box::pin(async move {
                    let res: Result<usize, SuiError> = Err(SuiError::TooManyIncorrectAuthorities);
                    res
                })
            },
            |mut accumulated_state, _authority_name, _authority_weight, result| {
                Box::pin(async move {
                    assert!(Err(SuiError::TooManyIncorrectAuthorities) == result);
                    accumulated_state += 1;
                    Ok(ReduceOutput::Continue(accumulated_state))
                })
            },
            Duration::from_millis(1000),
        )
        .await;
    assert_eq!(Ok(4), res);

    // Test: early end
    let res = client1
        .authorities()
        .quorum_map_then_reduce_with_timeout(
            0usize,
            |_name, _client| Box::pin(async move { Ok(()) }),
            |mut accumulated_state, _authority_name, _authority_weight, _result| {
                Box::pin(async move {
                    if accumulated_state > 2 {
                        Ok(ReduceOutput::End(accumulated_state))
                    } else {
                        accumulated_state += 1;
                        Ok(ReduceOutput::Continue(accumulated_state))
                    }
                })
            },
            Duration::from_millis(1000),
        )
        .await;
    assert_eq!(Ok(3), res);

    // Test: Global timeout works
    let res = client1
        .authorities()
        .quorum_map_then_reduce_with_timeout(
            0usize,
            |_name, _client| {
                Box::pin(async move {
                    // 10 mins
                    tokio::time::sleep(Duration::from_secs(10 * 60)).await;
                    Ok(())
                })
            },
            |_accumulated_state, _authority_name, _authority_weight, _result| {
                Box::pin(async move { Err(SuiError::TooManyIncorrectAuthorities) })
            },
            Duration::from_millis(10),
        )
        .await;
    assert_eq!(Ok(0), res);

    // Test: Local timeout works
    let bad_auth = *committee.sample();
    let res = client1
        .authorities()
        .quorum_map_then_reduce_with_timeout(
            HashSet::new(),
            |_name, _client| {
                Box::pin(async move {
                    // 10 mins
                    if _name == bad_auth {
                        tokio::time::sleep(Duration::from_secs(10 * 60)).await;
                    }
                    Ok(())
                })
            },
            |mut accumulated_state, authority_name, _authority_weight, _result| {
                Box::pin(async move {
                    accumulated_state.insert(authority_name);
                    if accumulated_state.len() <= 3 {
                        Ok(ReduceOutput::Continue(accumulated_state))
                    } else {
                        Ok(ReduceOutput::ContinueWithTimeout(
                            accumulated_state,
                            Duration::from_millis(10),
                        ))
                    }
                })
            },
            // large delay
            Duration::from_millis(10 * 60),
        )
        .await;
    assert_eq!(res.as_ref().unwrap().len(), 3);
    assert!(!res.as_ref().unwrap().contains(&bad_auth));
}

async fn get_latest_ref(authority: &LocalAuthorityClient, object_id: ObjectID) -> ObjectRef {
    if let Ok(ObjectInfoResponse {
        requested_object_reference: Some(object_ref),
        ..
    }) = authority
        .handle_object_info_request(ObjectInfoRequest::latest_object_info_request(
            object_id, None,
        ))
        .await
    {
        return object_ref;
    }
    panic!("Object not found!");
}

#[tokio::test]
async fn test_get_all_owned_objects() {
    let (authority_clients, committee) = init_local_authorities(4).await;
    let auth_vec: Vec<_> = authority_clients.values().cloned().collect();

    let mut client1 = make_client(authority_clients.clone(), committee.clone());
    let framework_obj_ref = client1.get_framework_object_ref().await.unwrap();
    let mut client2 = make_client(authority_clients.clone(), committee.clone());

    let gas_object1 = ObjectID::random();
    let gas_object2 = ObjectID::random();

    fund_account_with_same_objects(auth_vec.iter().collect(), &mut client1, vec![gas_object1])
        .await;
    fund_account_with_same_objects(auth_vec.iter().collect(), &mut client2, vec![gas_object2])
        .await;

    // Make a schedule of transactions
    let gas_ref_1 = get_latest_ref(&auth_vec[0], gas_object1).await;
    let create1 = transaction_create(
        client1.address(),
        client1.secret(),
        client1.address(),
        100,
        framework_obj_ref,
        gas_ref_1,
    );

    // Submit to 3 authorities, but not 4th
    do_transaction(&auth_vec[0], &create1).await;
    do_transaction(&auth_vec[1], &create1).await;
    do_transaction(&auth_vec[2], &create1).await;

    // Get a cert
    let cert1 = extract_cert(&authority_clients, &committee, create1.digest()).await;

    // Test 1: Before the cert is submitted no one knows of the new object.
    let (owned_object, _) = client1
        .authorities()
        .get_all_owned_objects(client1.address(), Duration::from_secs(10))
        .await
        .unwrap();
    assert_eq!(1, owned_object.len());
    assert!(owned_object.contains_key(&gas_ref_1));

    // Submit the cert to first authority.
    let effects = do_cert(&auth_vec[0], &cert1).await;

    // Test 2: Once the cert is submitted one auth returns the new object,
    //         but now two versions of gas exist.
    let (owned_object, _) = client1
        .authorities()
        .get_all_owned_objects(client1.address(), Duration::from_secs(10))
        .await
        .unwrap();
    assert_eq!(3, owned_object.len());

    assert!(owned_object.contains_key(&effects.gas_object.0));
    assert!(owned_object.contains_key(&effects.created[0].0));
    let created_ref = effects.created[0].0;

    // Submit to next 3 authorities.
    do_cert(&auth_vec[1], &cert1).await;
    do_cert(&auth_vec[2], &cert1).await;
    do_cert(&auth_vec[3], &cert1).await;

    // Make a delete transaction
    let gas_ref_del = get_latest_ref(&auth_vec[0], gas_object1).await;
    let delete1 = transaction_delete(
        client1.address(),
        client1.secret(),
        created_ref,
        framework_obj_ref,
        gas_ref_del,
    );

    // Get cert for delete transaction, and submit to first authority
    do_transaction(&auth_vec[0], &delete1).await;
    do_transaction(&auth_vec[1], &delete1).await;
    do_transaction(&auth_vec[2], &delete1).await;
    let cert2 = extract_cert(&authority_clients, &committee, delete1.digest()).await;
    let _effects = do_cert(&auth_vec[0], &cert2).await;

    // Test 3: dealing with deleted objects on some authorities
    let (owned_object, _) = client1
        .authorities()
        .get_all_owned_objects(client1.address(), Duration::from_secs(10))
        .await
        .unwrap();
    // Since not all authorities know the object is deleted, we get back
    // the new gas object, the delete object and the old gas object.
    assert_eq!(3, owned_object.len());

    // Update rest of authorities
    do_cert(&auth_vec[1], &cert2).await;
    do_cert(&auth_vec[2], &cert2).await;
    do_cert(&auth_vec[3], &cert2).await;

    // Test 4: dealing with deleted objects on all authorities
    let (owned_object, _) = client1
        .authorities()
        .get_all_owned_objects(client1.address(), Duration::from_secs(10))
        .await
        .unwrap();

    // Just the gas object is returned
    assert_eq!(1, owned_object.len());
}

#[tokio::test]
async fn test_sync_all_owned_objects() {
    let (authority_clients, committee) = init_local_authorities(4).await;
    let auth_vec: Vec<_> = authority_clients.values().cloned().collect();

    let mut client1 = make_client(authority_clients.clone(), committee.clone());
    let framework_obj_ref = client1.get_framework_object_ref().await.unwrap();
    let client2 = make_client(authority_clients.clone(), committee.clone());

    let gas_object1 = ObjectID::random();
    let gas_object2 = ObjectID::random();

    fund_account_with_same_objects(
        auth_vec.iter().collect(),
        &mut client1,
        vec![gas_object1, gas_object2],
    )
    .await;

    // Make a schedule of transactions
    let gas_ref_1 = get_latest_ref(&auth_vec[0], gas_object1).await;
    let create1 = transaction_create(
        client1.address(),
        client1.secret(),
        client1.address(),
        100,
        framework_obj_ref,
        gas_ref_1,
    );

    let gas_ref_2 = get_latest_ref(&auth_vec[0], gas_object2).await;
    let create2 = transaction_create(
        client1.address(),
        client1.secret(),
        client1.address(),
        101,
        framework_obj_ref,
        gas_ref_2,
    );

    // Submit to 3 authorities, but not 4th
    do_transaction(&auth_vec[0], &create1).await;
    do_transaction(&auth_vec[1], &create1).await;
    do_transaction(&auth_vec[2], &create1).await;

    do_transaction(&auth_vec[1], &create2).await;
    do_transaction(&auth_vec[2], &create2).await;
    do_transaction(&auth_vec[3], &create2).await;

    // Get a cert
    let cert1 = extract_cert(&authority_clients, &committee, create1.digest()).await;
    let cert2 = extract_cert(&authority_clients, &committee, create2.digest()).await;

    // Submit the cert to 1 authority.
    let new_ref_1 = do_cert(&auth_vec[0], &cert1).await.created[0].0;
    let new_ref_2 = do_cert(&auth_vec[3], &cert2).await.created[0].0;

    // Test 1: Once the cert is submitted one auth returns the new object,
    //         but now two versions of gas exist. Ie total 2x3 = 6.
    let (owned_object, _) = client1
        .authorities()
        .get_all_owned_objects(client1.address(), Duration::from_secs(10))
        .await
        .unwrap();
    assert_eq!(6, owned_object.len());

    // After sync we are back to having 4.
    let (owned_object, _) = client1
        .authorities()
        .sync_all_owned_objects(client1.address(), Duration::from_secs(10))
        .await
        .unwrap();
    assert_eq!(4, owned_object.len());

    // Now lets delete and move objects

    // Make a delete transaction
    let gas_ref_del = get_latest_ref(&auth_vec[0], gas_object1).await;
    let delete1 = transaction_delete(
        client1.address(),
        client1.secret(),
        new_ref_1,
        framework_obj_ref,
        gas_ref_del,
    );

    // Make a transfer transaction
    let gas_ref_trans = get_latest_ref(&auth_vec[0], gas_object2).await;
    let transfer1 = transaction_transfer(
        client1.address(),
        client1.secret(),
        client2.address(),
        new_ref_2,
        framework_obj_ref,
        gas_ref_trans,
    );

    do_transaction(&auth_vec[0], &delete1).await;
    do_transaction(&auth_vec[1], &delete1).await;
    do_transaction(&auth_vec[2], &delete1).await;

    do_transaction(&auth_vec[1], &transfer1).await;
    do_transaction(&auth_vec[2], &transfer1).await;
    do_transaction(&auth_vec[3], &transfer1).await;

    let cert1 = extract_cert(&authority_clients, &committee, delete1.digest()).await;
    let cert2 = extract_cert(&authority_clients, &committee, transfer1.digest()).await;

    do_cert(&auth_vec[0], &cert1).await;
    do_cert(&auth_vec[3], &cert2).await;

    // Test 2: Before we sync we see 6 object, incl: (old + new gas) x 2, and 2 x old objects
    // after we see just 2 (one deleted one transfered.)
    let (owned_object, _) = client1
        .authorities()
        .get_all_owned_objects(client1.address(), Duration::from_secs(10))
        .await
        .unwrap();
    assert_eq!(6, owned_object.len());

    // After sync we are back to having 2.
    let (owned_object, _) = client1
        .authorities()
        .sync_all_owned_objects(client1.address(), Duration::from_secs(10))
        .await
        .unwrap();
    assert_eq!(
        2,
        owned_object
            .iter()
            .filter(|(o, _, _)| o.owner == client1.address())
            .count()
    );
}

#[tokio::test]
async fn test_process_transaction() {
    let (authority_clients, committee) = init_local_authorities(4).await;
    let auth_vec: Vec<_> = authority_clients.values().cloned().collect();

    let mut client1 = make_client(authority_clients.clone(), committee.clone());
    let framework_obj_ref = client1.get_framework_object_ref().await.unwrap();

    let gas_object1 = ObjectID::random();
    let gas_object2 = ObjectID::random();

    fund_account_with_same_objects(
        auth_vec.iter().collect(),
        &mut client1,
        vec![gas_object1, gas_object2],
    )
    .await;

    // Make a schedule of transactions
    let gas_ref_1 = get_latest_ref(&auth_vec[0], gas_object1).await;
    let create1 = transaction_create(
        client1.address(),
        client1.secret(),
        client1.address(),
        100,
        framework_obj_ref,
        gas_ref_1,
    );

    do_transaction(&auth_vec[0], &create1).await;
    do_transaction(&auth_vec[1], &create1).await;
    do_transaction(&auth_vec[2], &create1).await;

    // Get a cert
    let cert1 = extract_cert(&authority_clients, &committee, create1.digest()).await;

    // Submit the cert to 1 authority.
    let new_ref_1 = do_cert(&auth_vec[0], &cert1).await.created[0].0;

    // Make a schedule of transactions
    let gas_ref_set = get_latest_ref(&auth_vec[0], gas_object1).await;
    let create2 = transaction_set(
        client1.address(),
        client1.secret(),
        new_ref_1,
        100,
        framework_obj_ref,
        gas_ref_set,
    );

    // Test 1: When we call process transaction on the second transaction, the process_transaction
    // updates all authorities with latest objects, and then the transaction goes through
    // on all of them. Note that one authority has processed cert 1, and none cert2,
    // and auth 3 has no seen either.
    client1
        .authorities()
        .process_transaction(create2.clone(), Duration::from_secs(10))
        .await
        .unwrap();

    // The transaction still only has 3 votes, as only these are needed.
    let cert2 = extract_cert(&authority_clients, &committee, create2.digest()).await;
    assert_eq!(3, cert2.signatures.len());
}

#[tokio::test]
async fn test_process_certificate() {
    let (authority_clients, committee) = init_local_authorities(4).await;
    let auth_vec: Vec<_> = authority_clients.values().cloned().collect();

    let mut client1 = make_client(authority_clients.clone(), committee.clone());
    let framework_obj_ref = client1.get_framework_object_ref().await.unwrap();

    let gas_object1 = ObjectID::random();
    let gas_object2 = ObjectID::random();

    fund_account_with_same_objects(
        auth_vec.iter().collect(),
        &mut client1,
        vec![gas_object1, gas_object2],
    )
    .await;

    // Make a schedule of transactions
    let gas_ref_1 = get_latest_ref(&auth_vec[0], gas_object1).await;
    let create1 = transaction_create(
        client1.address(),
        client1.secret(),
        client1.address(),
        100,
        framework_obj_ref,
        gas_ref_1,
    );

    do_transaction(&auth_vec[0], &create1).await;
    do_transaction(&auth_vec[1], &create1).await;
    do_transaction(&auth_vec[2], &create1).await;

    // Get a cert
    let cert1 = extract_cert(&authority_clients, &committee, create1.digest()).await;

    // Submit the cert to 1 authority.
    let new_ref_1 = do_cert(&auth_vec[0], &cert1).await.created[0].0;
    do_cert(&auth_vec[1], &cert1).await;
    do_cert(&auth_vec[2], &cert1).await;

    // Check the new object is at version 1
    let new_object_ref = client_object(&mut client1, new_ref_1.0).await.0;
    assert_eq!(SequenceNumber::from(1), new_object_ref.1);

    // Make a schedule of transactions
    let gas_ref_set = get_latest_ref(&auth_vec[0], gas_object1).await;
    let create2 = transaction_set(
        client1.address(),
        client1.secret(),
        new_ref_1,
        100,
        framework_obj_ref,
        gas_ref_set,
    );

    do_transaction(&auth_vec[0], &create2).await;
    do_transaction(&auth_vec[1], &create2).await;
    do_transaction(&auth_vec[2], &create2).await;

    let cert2 = extract_cert(&authority_clients, &committee, create2.digest()).await;

    // Test: process the certificate, including bring up to date authority 3.
    //       which is 2 certs behind.
    client1
        .authorities()
        .process_certificate(cert2, Duration::from_secs(10))
        .await
        .unwrap();

    let (owned_object, _) = client1
        .authorities()
        .get_all_owned_objects(client1.address(), Duration::from_secs(10))
        .await
        .unwrap();

    // As a result, we have 2 gas objects and 1 created object.
    assert_eq!(3, owned_object.len());
    // Check this is the latest version.
    let new_object_ref = client_object(&mut client1, new_ref_1.0).await.0;
    assert_eq!(SequenceNumber::from(2), new_object_ref.1);
}

#[tokio::test]
async fn test_transfer_pending_transactions() {
    let objects: Vec<ObjectID> = (0..15).map(|_| ObjectID::random()).collect();
    let gas_object = ObjectID::random();
    let number_of_authorities = 4;

    let mut all_objects = objects.clone();
    all_objects.push(gas_object);
    let authority_objects = (0..number_of_authorities)
        .map(|_| all_objects.clone())
        .collect();

    let mut sender_state = init_local_client_state(authority_objects).await;
    let recipient = init_local_client_state(vec![vec![]]).await.address();

    let mut objects = objects.iter();

    // Test 1: Normal transfer
    let object_id = *objects.next().unwrap();
    sender_state
        .transfer_object(object_id, gas_object, recipient)
        .await
        .unwrap();
    // Pending transaction should be cleared
    assert!(sender_state.store().pending_transactions.is_empty());

    // Test 2: Object not known to authorities. This has no side effect
    let obj = Object::with_id_owner_for_testing(ObjectID::random(), sender_state.address());
    sender_state
        .store()
        .object_refs
        .insert(&obj.id(), &obj.to_object_reference())
        .unwrap();
    sender_state
        .store()
        .object_sequence_numbers
        .insert(&obj.id(), &SequenceNumber::new())
        .unwrap();
    let result = sender_state
        .transfer_object(obj.id(), gas_object, recipient)
        .await;
    assert!(result.is_err());
    // assert!(matches!(result.unwrap_err().downcast_ref(),
    //        Some(SuiError::QuorumNotReached {errors, ..}) if matches!(errors.as_slice(), [SuiError::ObjectNotFound{..}, ..])));
    // Pending transaction should be cleared
    assert!(sender_state.store().pending_transactions.is_empty());

    // Test 3: invalid object digest. This also has no side effect
    let object_id = *objects.next().unwrap();

    // give object an incorrect object digest
    sender_state
        .store()
        .object_refs
        .insert(
            &object_id,
            &(object_id, SequenceNumber::new(), ObjectDigest([0; 32])),
        )
        .unwrap();

    let result = sender_state
        .transfer_object(object_id, gas_object, recipient)
        .await;
    assert!(result.is_err());
    //assert!(matches!(result.unwrap_err().downcast_ref(),
    //        Some(SuiError::QuorumNotReached {errors, ..}) if matches!(errors.as_slice(), [SuiError::LockErrors{..}, ..])));

    // Pending transaction should be cleared
    assert!(sender_state.store().pending_transactions.is_empty());

    // Test 4: Conflicting transactions touching same objects
    let object_id = *objects.next().unwrap();
    // Fabricate a fake pending transfer and simulate locking some objects
    sender_state
        .lock_pending_transaction_objects(&Transaction::new_transfer(
            SuiAddress::random_for_testing_only(),
            (object_id, Default::default(), ObjectDigest::new([0; 32])),
            sender_state.address(),
            (gas_object, Default::default(), ObjectDigest::new([0; 32])),
            &get_key_pair().1,
        ))
        .unwrap();
    // Try to use those objects in another transaction
    let result = sender_state
        .transfer_object(object_id, gas_object, recipient)
        .await;
    assert!(result.is_err());
    assert!(matches!(
        result.unwrap_err().downcast_ref(),
        Some(SuiError::ConcurrentTransactionError)
    ));
    // clear the pending transactions
    sender_state.store().pending_transactions.clear().unwrap();
    assert_eq!(sender_state.store().pending_transactions.iter().count(), 0);
}

#[tokio::test]
async fn test_address_manager() {
    let path = tempfile::tempdir().unwrap().into_path();
    let (authority_clients, committee) = init_local_authorities(4).await;

    let mut address_manager: crate::client::ClientAddressManager<LocalAuthorityClient> =
        crate::client::ClientAddressManager::new(path, committee, authority_clients.clone());

    // Ensure nothing being managed
    assert!(address_manager.get_managed_address_states().is_empty());

    // Try adding new addresses to manage
    let (address, secret) = get_key_pair();
    let _secret2 = secret.copy();
    let secret = Box::pin(secret);
    let gas_object1 = ObjectID::random();
    let gas_object2 = ObjectID::random();
    let client1 = address_manager
        .get_or_create_state_mut(address, secret)
        .unwrap();
    fund_account_with_same_objects(
        authority_clients.values().collect(),
        client1,
        vec![gas_object1, gas_object2],
    )
    .await;

    client1.sync_client_state().await.unwrap();
    client1.download_owned_objects_not_in_db().await.unwrap();

    // Confirm expected behavior
    assert_eq!(client1.store().objects.iter().count(), 2);
    let framework_obj_ref = client1.get_framework_object_ref().await.unwrap();
    let sample_auth = &authority_clients.iter().next().unwrap().1;

    // Make a transaction
    let gas_ref_1 = get_latest_ref(sample_auth, gas_object1).await;
    let pure_args = vec![
        bcs::to_bytes(&100u64).unwrap(),
        bcs::to_bytes(&AccountAddress::from(client1.address())).unwrap(),
    ];
    let call_response = client1
        .move_call(
            framework_obj_ref,
            ident_str!("ObjectBasics").to_owned(),
            ident_str!("create").to_owned(),
            Vec::new(),
            gas_ref_1,
            Vec::new(),
            pure_args,
            GAS_VALUE_FOR_TESTING - 1, // Make sure budget is less than gas value
        )
        .await;

    // Check effects are good
    let (_, transaction_effects) = call_response.unwrap();
    // Status flag should be success
    assert!(matches!(
        transaction_effects.status,
        ExecutionStatus::Success { .. }
    ));

    assert_eq!(transaction_effects.created.len(), 1);
    assert_eq!(client1.store().objects.iter().count(), 4);
}

#[tokio::test]
async fn test_coin_split() {
    let (authority_clients, committee) = init_local_authorities(4).await;
    let mut client1 = make_client(authority_clients.clone(), committee);

    let coin_object_id = ObjectID::random();
    let gas_object_id = ObjectID::random();

    // Populate authorities with obj data
    let objects = fund_account_with_same_objects(
        authority_clients.values().collect(),
        &mut client1,
        vec![coin_object_id, gas_object_id],
    )
    .await;
    let coin_object = objects.get(&coin_object_id).unwrap();
    let gas_object = objects.get(&gas_object_id).unwrap();

    let split_amounts = vec![100, 200, 300, 400, 500];
    let total_amount: u64 = split_amounts.iter().sum();

    let response = client1
        .split_coin(
            coin_object.to_object_reference(),
            split_amounts.clone(),
            gas_object.to_object_reference(),
            GAS_VALUE_FOR_TESTING,
        )
        .await
        .unwrap();
    assert_eq!(
        (coin_object_id, coin_object.version().increment()),
        (response.updated_coin.id(), response.updated_coin.version())
    );
    assert_eq!(
        (gas_object_id, gas_object.version().increment()),
        (response.updated_gas.id(), response.updated_gas.version())
    );
    let update_coin = GasCoin::try_from(response.updated_coin.data.try_as_move().unwrap()).unwrap();
    assert_eq!(update_coin.value(), GAS_VALUE_FOR_TESTING - total_amount);
    let split_coin_values = response
        .new_coins
        .iter()
        .map(|o| {
            GasCoin::try_from(o.data.try_as_move().unwrap())
                .unwrap()
                .value()
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        split_amounts,
        split_coin_values.into_iter().collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn test_coin_merge() {
    let (authority_clients, committee) = init_local_authorities(4).await;
    let mut client1 = make_client(authority_clients.clone(), committee);

    let coin_object_id1 = ObjectID::random();
    let coin_object_id2 = ObjectID::random();
    let gas_object_id = ObjectID::random();

    // Populate authorities with obj data
    let objects = fund_account_with_same_objects(
        authority_clients.values().collect(),
        &mut client1,
        vec![coin_object_id1, coin_object_id2, gas_object_id],
    )
    .await;
    let coin_object1 = objects.get(&coin_object_id1).unwrap();
    let coin_object2 = objects.get(&coin_object_id2).unwrap();
    let gas_object = objects.get(&gas_object_id).unwrap();

    let response = client1
        .merge_coins(
            coin_object1.to_object_reference(),
            coin_object2.to_object_reference(),
            gas_object.to_object_reference(),
            GAS_VALUE_FOR_TESTING,
        )
        .await
        .unwrap();
    assert_eq!(
        (coin_object_id1, coin_object1.version().increment()),
        (response.updated_coin.id(), response.updated_coin.version())
    );
    assert_eq!(
        (gas_object_id, gas_object.version().increment()),
        (response.updated_gas.id(), response.updated_gas.version())
    );
    let update_coin = GasCoin::try_from(response.updated_coin.data.try_as_move().unwrap()).unwrap();
    assert_eq!(update_coin.value(), GAS_VALUE_FOR_TESTING * 2);
}
