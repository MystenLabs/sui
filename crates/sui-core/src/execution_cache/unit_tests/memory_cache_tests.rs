// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use rand::{rngs::StdRng, SeedableRng};
use std::{collections::BTreeMap, sync::Arc};
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::{
    base_types::{random_object_ref, SuiAddress},
    crypto::{deterministic_random_account_key, get_key_pair_from_rng, AccountKeyPair},
    object::{MoveObject, Owner, OBJECT_START_VERSION},
};
use tempfile::tempdir;

use super::*;
use crate::{
    authority::{authority_store_tables::AuthorityPerpetualTables, AuthorityStore},
    execution_cache::ExecutionCacheAPI,
    test_utils::init_state_parameters_from_rng,
};

async fn init_authority_store() -> Arc<AuthorityStore> {
    let seed = [1u8; 32];
    let (genesis, _) = init_state_parameters_from_rng(&mut StdRng::from_seed(seed));
    let committee = genesis.committee().unwrap();

    // Create a random directory to store the DB
    let dir = tempdir().unwrap();
    let db_path = dir.path();

    let perpetual_tables = Arc::new(AuthorityPerpetualTables::open(&db_path, None));
    AuthorityStore::open_with_committee_for_testing(perpetual_tables, &committee, &genesis, 0)
        .await
        .unwrap()
        .into()
}

trait AssertInserted {
    fn assert_inserted(&self);
}

impl<T> AssertInserted for Option<T> {
    fn assert_inserted(&self) {
        assert!(self.is_none());
    }
}

impl AssertInserted for bool {
    fn assert_inserted(&self) {
        assert!(*self);
    }
}

struct Scenario {
    store: Arc<AuthorityStore>,
    cache: Box<dyn ExecutionCacheAPI>,

    id_map: BTreeMap<u32, ObjectID>,
    objects: BTreeMap<ObjectID, Object>,
    outputs: TransactionOutputs,
    transactions: BTreeSet<TransactionDigest>,
}

impl Scenario {
    async fn new() -> Self {
        let store = init_authority_store().await;
        let cache = Box::new(MemoryCache::new_with_no_metrics(store.clone()));
        Self {
            store,
            cache,
            id_map: BTreeMap::new(),
            objects: BTreeMap::new(),
            outputs: Self::new_outputs(),
            transactions: BTreeSet::new(),
        }
    }

    fn new_outputs() -> TransactionOutputs {
        let mut rng = StdRng::from_seed([0; 32]);
        let (sender, keypair): (SuiAddress, AccountKeyPair) = get_key_pair_from_rng(&mut rng);
        let (receiver, _): (SuiAddress, AccountKeyPair) = get_key_pair_from_rng(&mut rng);

        // Tx is opaque to the cache, so we just build a dummy tx. The only requirement is
        // that it has a unique digest every time.
        let tx = TestTransactionBuilder::new(sender, random_object_ref(), 100)
            .transfer(random_object_ref(), receiver)
            .build_and_sign(&keypair);

        let tx = VerifiedTransaction::new_unchecked(tx);
        let effects = TransactionEffects::new_with_tx(tx.inner());

        TransactionOutputs {
            transaction: Arc::new(tx),
            effects,
            events: Default::default(),
            markers: Default::default(),
            wrapped: Default::default(),
            deleted: Default::default(),
            locks_to_delete: Default::default(),
            new_locks_to_init: Default::default(),
            written: Default::default(),
        }
    }

    fn new_object() -> Object {
        let id = ObjectID::random();
        let (owner, _) = deterministic_random_account_key();
        Object::new_move(
            MoveObject::new_gas_coin(OBJECT_START_VERSION, id, 100),
            Owner::AddressOwner(owner),
            TransactionDigest::ZERO,
        )
    }

    fn bump_version(object: Object) -> Object {
        let version = object.version();
        let mut inner = object.into_inner();
        inner
            .data
            .try_as_move_mut()
            .unwrap()
            .increment_version_to(version.next());
        inner.into()
    }

    fn with_created(&mut self, short_ids: &[u32]) {
        // for every id in short_ids, create an object with that id if it doesn't exist
        for short_id in short_ids {
            let object = Self::new_object();
            self.outputs
                .new_locks_to_init
                .push(object.compute_object_reference());
            let id = object.id();
            assert!(self.id_map.insert(*short_id, id).is_none());
            self.outputs.written.insert(id, object.clone());
            self.objects.insert(id, object).assert_inserted();
        }
    }

    fn with_mutated(&mut self, short_ids: &[u32]) {
        // for every id in short_ids, create an object with that id if it doesn't exist
        for short_id in short_ids {
            let id = self.id_map.get(short_id).expect("object not found");
            let object = self.objects.get(id).cloned().expect("object not found");
            self.outputs
                .locks_to_delete
                .push(object.compute_object_reference());
            let object = Self::bump_version(object);
            self.outputs
                .new_locks_to_init
                .push(object.compute_object_reference());
            self.outputs.written.insert(object.id(), object);
        }
    }

    fn with_deleted(&mut self, short_ids: &[u32]) {
        for short_id in short_ids {
            let id = self.id_map.get(short_id).expect("object not found");
            let object = self.objects.remove(id).expect("object not found");
            let object_ref = object.compute_object_reference();
            self.outputs.locks_to_delete.push(object_ref);
            self.outputs.deleted.push(object_ref.into());
        }
    }

    fn with_wrapped(&mut self, short_ids: &[u32]) {
        for short_id in short_ids {
            let id = self.id_map.get(short_id).expect("object not found");
            let object = self.objects.get(id).cloned().expect("object not found");
            let object_ref = object.compute_object_reference();
            self.outputs.locks_to_delete.push(object_ref);
            self.outputs
                .wrapped
                .push(object.compute_object_reference().into());
        }
    }

    fn with_received(&mut self, short_ids: &[u32]) {
        for short_id in short_ids {
            let id = self.id_map.get(short_id).expect("object not found");
            let object = self.objects.get(id).cloned().expect("object not found");
            self.outputs
                .new_locks_to_init
                .iter()
                .find(|o| **o == object.compute_object_reference())
                .expect("received object must have new lock");
            self.outputs.markers.push((
                object.compute_object_reference().into(),
                MarkerValue::Received,
            ));
        }
    }

    fn get_object(&self, id: u32) -> Object {
        let id = self.id_map.get(&id).expect("no such object");
        self.objects.get(&id).unwrap().clone()
    }

    fn assert_object(&self, id: u32, object: Object) {
        assert_eq!(self.get_object(id), object);
    }

    async fn do_tx(&mut self) -> TransactionDigest {
        // Resets outputs, but not objects, so that subsequent runs must respect
        // the state so far.
        let mut outputs = Self::new_outputs();
        std::mem::swap(&mut self.outputs, &mut outputs);
        let outputs = Arc::new(outputs);

        let tx = *outputs.transaction.digest();
        assert!(self.transactions.insert(tx));

        self.cache
            .write_transaction_outputs(1 /* epoch */, outputs.clone())
            .await
            .expect("write_transaction_outputs failed");

        tx
    }

    fn reset_cache(&mut self) {
        self.cache = Box::new(MemoryCache::new_with_no_metrics(self.store.clone()));

        // reset the scenario state to match the db
        let reverse_id_map: BTreeMap<_, _> = self.id_map.iter().map(|(k, v)| (*v, *k)).collect();
        self.objects.clear();

        self.store.iter_live_object_set(false).for_each(|o| {
            let LiveObject::Normal(o) = o else {
                panic!("expected normal object")
            };
            let id = o.id();
            // genesis objects are not managed by Scenario, ignore them
            if reverse_id_map.get(&id).is_some() {
                self.objects.insert(id, o);
            }
        });
    }

    fn assert_live(&self, short_ids: &[u32]) {
        for short_id in short_ids {
            let id = self.id_map.get(&short_id).expect("no such object");
            let expected = self.objects.get(&id).expect("no such object");
            let version = expected.version();
            assert_eq!(
                self.cache.get_object_by_key(&id, version).unwrap().unwrap(),
                *expected
            );
            assert_eq!(
                self.cache.get_object(&expected.id()).unwrap().unwrap(),
                *expected
            );
            // TODO
            // assert!(!self
            //  .cache
            //  .get_lock(expected.compute_object_reference(), 1)
            //  .unwrap()
            //  .is_locked());
        }
    }

    fn assert_not_exists(&self, short_ids: &[u32]) {
        for short_id in short_ids {
            let id = self.id_map.get(&short_id).expect("no such id");

            assert!(
                self.cache.get_object(id).unwrap().is_none(),
                "object exists in cache"
            );
        }
    }
}

#[tokio::test]
async fn test_uncommitted() {
    let mut b = Scenario::new().await;

    b.with_created(&[1, 2]);
    b.do_tx().await;

    b.assert_live(&[1, 2]);
    b.reset_cache();
    b.assert_not_exists(&[1, 2]);
}

#[tokio::test]
async fn test_committed() {
    let mut b = Scenario::new().await;

    b.with_created(&[1, 2]);
    let tx = b.do_tx().await;

    b.assert_live(&[1, 2]);
    b.cache
        .commit_transaction_outputs(1, &tx)
        .await
        .expect("commit failed");
    b.reset_cache();
    b.assert_live(&[1, 2]);
}

#[tokio::test]
async fn test_out_of_order_commit() {
    telemetry_subscribers::init_for_testing();
    let mut b = Scenario::new().await;
    b.with_created(&[1, 2]);
    let tx1 = b.do_tx().await;

    b.with_mutated(&[1, 2]);
    let tx2 = b.do_tx().await;

    // cannot commit out of order
    b.cache
        .commit_transaction_outputs(1, &tx2)
        .await
        .unwrap_err();
}
