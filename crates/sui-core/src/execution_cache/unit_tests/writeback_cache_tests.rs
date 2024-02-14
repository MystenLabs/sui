// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use rand::{rngs::StdRng, SeedableRng};
use std::{
    collections::BTreeMap,
    future::Future,
    sync::atomic::Ordering,
    sync::{atomic::AtomicU32, Arc},
};
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

    let perpetual_tables = Arc::new(AuthorityPerpetualTables::open(db_path, None));
    AuthorityStore::open_with_committee_for_testing(perpetual_tables, &committee, &genesis, 0)
        .await
        .unwrap()
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

type ActionCb = Box<dyn Fn(&mut Scenario)>;

struct Scenario {
    store: Arc<AuthorityStore>,
    cache: Box<WritebackCache>,

    id_map: BTreeMap<u32, ObjectID>,
    objects: BTreeMap<ObjectID, Object>,
    outputs: TransactionOutputs,
    transactions: BTreeSet<TransactionDigest>,

    action_count: Arc<AtomicU32>,
    do_after: Option<(u32, ActionCb)>,
}

impl Scenario {
    async fn new(do_after: Option<(u32, ActionCb)>, action_count: Arc<AtomicU32>) -> Self {
        let store = init_authority_store().await;
        let cache = Box::new(WritebackCache::new_with_no_metrics(store.clone()));
        Self {
            store,
            cache,
            id_map: BTreeMap::new(),
            objects: BTreeMap::new(),
            outputs: Self::new_outputs(),
            transactions: BTreeSet::new(),

            action_count,
            do_after,
        }
    }

    fn cache(&self) -> &dyn ExecutionCacheAPI {
        &*self.cache
    }

    fn count_action(&mut self) {
        let prev = self.action_count.fetch_add(1, Ordering::Relaxed);
        if let Some((count, f)) = &self.do_after.take() {
            if prev == *count {
                f(self);
            }
        }
    }

    // This method runs a test scenario multiple times, and each time it clears the
    // evictable caches after a different step.
    async fn iterate<F, Fut>(f: F)
    where
        F: Fn(Scenario) -> Fut,
        Fut: Future<Output = ()>,
    {
        // run once to get a baseline
        println!("running baseline");
        let num_steps = {
            let count = Arc::new(AtomicU32::new(0));
            let fut = f(Scenario::new(None, count.clone()).await);
            fut.await;
            count.load(Ordering::Relaxed)
        };

        for i in 0..num_steps {
            println!("running with cache eviction after step {}", i);
            let count = Arc::new(AtomicU32::new(0));
            let action = Box::new(|s: &mut Scenario| {
                s.evict_caches();
            });
            let fut = f(Scenario::new(Some((i, action)), count.clone()).await);
            fut.await;
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
        // for every id in short_ids, assert than an object with that id exists, and
        // mutate it
        for short_id in short_ids {
            let id = self.id_map.get(short_id).expect("object not found");
            let object = self.objects.get(id).cloned().expect("object not found");
            self.outputs
                .locks_to_delete
                .push(object.compute_object_reference());
            let object = Self::bump_version(object);
            self.objects.insert(*id, object.clone());
            self.outputs
                .new_locks_to_init
                .push(object.compute_object_reference());
            self.outputs.written.insert(object.id(), object);
        }
    }

    fn with_deleted(&mut self, short_ids: &[u32]) {
        // for every id in short_ids, assert than an object with that id exists, and
        // delete it
        for short_id in short_ids {
            let id = self.id_map.get(short_id).expect("object not found");
            let object = self.objects.remove(id).expect("object not found");
            let mut object_ref = object.compute_object_reference();
            self.outputs.locks_to_delete.push(object_ref);
            // in the authority this would be set to the lamport version of the tx
            object_ref.1.increment();
            self.outputs.deleted.push(object_ref.into());
        }
    }

    fn with_wrapped(&mut self, short_ids: &[u32]) {
        // for every id in short_ids, assert than an object with that id exists, and
        // wrap it
        for short_id in short_ids {
            let id = self.id_map.get(short_id).expect("object not found");
            let object = self.objects.get(id).cloned().expect("object not found");
            let mut object_ref = object.compute_object_reference();
            self.outputs.locks_to_delete.push(object_ref);
            // in the authority this would be set to the lamport version of the tx
            object_ref.1.increment();
            self.outputs.wrapped.push(object_ref.into());
        }
    }

    fn with_received(&mut self, short_ids: &[u32]) {
        // for every id in short_ids, assert than an object with that id exists, that
        // it has a new lock (which proves it was mutated) and then write a received
        // marker for it
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

    // Commit the current tx to the cache, return its digest, and reset the transaction
    // outputs to a new empty one.
    async fn do_tx(&mut self) -> TransactionDigest {
        // Resets outputs, but not objects, so that subsequent runs must respect
        // the state so far.
        let mut outputs = Self::new_outputs();
        std::mem::swap(&mut self.outputs, &mut outputs);
        let outputs = Arc::new(outputs);

        let tx = *outputs.transaction.digest();
        assert!(self.transactions.insert(tx), "transaction is not unique");

        self.cache()
            .write_transaction_outputs(1 /* epoch */, outputs.clone())
            .await
            .expect("write_transaction_outputs failed");

        self.count_action();
        tx
    }

    // commit a transaction to the database
    async fn commit(&mut self, tx: TransactionDigest) -> SuiResult {
        let res = self.cache().commit_transaction_outputs(1, &tx).await;
        self.count_action();
        res
    }

    fn evict_caches(&self) {
        self.cache.clear_caches();
    }

    fn reset_cache(&mut self) {
        self.cache = Box::new(WritebackCache::new_with_no_metrics(self.store.clone()));

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
            let id = self.id_map.get(short_id).expect("no such object");
            let expected = self.objects.get(id).expect("no such object");
            let version = expected.version();
            assert_eq!(
                self.cache()
                    .get_object_by_key(id, version)
                    .unwrap()
                    .unwrap(),
                *expected
            );
            assert_eq!(
                self.cache().get_object(&expected.id()).unwrap().unwrap(),
                *expected
            );
            // TODO: enable after lock caching is implemented
            // assert!(!self
            //  .cache()
            //  .get_lock(expected.compute_object_reference(), 1)
            //  .unwrap()
            //  .is_locked());
        }
    }

    fn assert_received(&self, short_ids: &[u32]) {
        for short_id in short_ids {
            let id = self.id_map.get(short_id).expect("no such object");
            let object = self.objects.get(id).expect("no such object");
            assert_eq!(
                self.cache()
                    .get_object_by_key(id, object.version())
                    .unwrap()
                    .unwrap(),
                *object
            );
            assert!(self
                .cache()
                .have_received_object_at_version(id, object.version(), 1)
                .unwrap());
        }
    }

    fn assert_not_exists(&self, short_ids: &[u32]) {
        for short_id in short_ids {
            let id = self.id_map.get(short_id).expect("no such id");

            assert!(
                self.cache().get_object(id).unwrap().is_none(),
                "object exists in cache"
            );
        }
    }

    fn obj_id(&self, short_id: u32) -> ObjectID {
        *self.id_map.get(&short_id).expect("no such id")
    }
}

#[tokio::test]
async fn test_uncommitted() {
    telemetry_subscribers::init_for_testing();
    Scenario::iterate(|mut s| async move {
        s.with_created(&[1, 2]);
        s.do_tx().await;

        s.assert_live(&[1, 2]);
        s.reset_cache();
        s.assert_not_exists(&[1, 2]);
    })
    .await;
}

#[tokio::test]
async fn test_committed() {
    telemetry_subscribers::init_for_testing();
    Scenario::iterate(|mut s| async move {
        s.with_created(&[1, 2]);
        let tx = s.do_tx().await;

        s.assert_live(&[1, 2]);
        s.cache()
            .commit_transaction_outputs(1, &tx)
            .await
            .expect("commit failed");
        s.reset_cache();
        s.assert_live(&[1, 2]);
    })
    .await;
}

#[tokio::test]
async fn test_mutated() {
    telemetry_subscribers::init_for_testing();
    Scenario::iterate(|mut s| async move {
        s.with_created(&[1, 2]);
        s.do_tx().await;

        s.with_mutated(&[1, 2]);
        s.do_tx().await;

        s.assert_live(&[1, 2]);
    })
    .await;
}

#[tokio::test]
async fn test_deleted() {
    telemetry_subscribers::init_for_testing();
    Scenario::iterate(|mut s| async move {
        s.with_created(&[1, 2]);
        s.do_tx().await;

        s.with_deleted(&[1]);
        s.do_tx().await;

        s.assert_live(&[2]);
        s.assert_not_exists(&[1]);
    })
    .await;
}

#[tokio::test]
async fn test_wrapped() {
    telemetry_subscribers::init_for_testing();
    Scenario::iterate(|mut s| async move {
        s.with_created(&[1, 2]);
        s.do_tx().await;

        s.with_wrapped(&[1]);
        s.do_tx().await;

        s.assert_live(&[2]);
        s.assert_not_exists(&[1]);
    })
    .await;
}

#[tokio::test]
async fn test_received() {
    telemetry_subscribers::init_for_testing();
    Scenario::iterate(|mut s| async move {
        s.with_created(&[1, 2]);
        s.do_tx().await;

        s.with_mutated(&[1, 2]);
        s.with_received(&[1]);
        s.do_tx().await;

        s.assert_received(&[1]);
        s.assert_live(&[1, 2]);
    })
    .await;
}

#[tokio::test]
async fn test_out_of_order_commit() {
    telemetry_subscribers::init_for_testing();
    Scenario::iterate(|mut s| async move {
        s.with_created(&[1, 2]);
        s.do_tx().await;

        s.with_mutated(&[1, 2]);
        let tx2 = s.do_tx().await;

        s.commit(tx2).await.unwrap_err();
    })
    .await;
}

#[tokio::test]
async fn test_lt_or_eq() {
    telemetry_subscribers::init_for_testing();
    Scenario::iterate(|mut s| async move {
        let check_all_versions = |s: &Scenario| {
            for i in 1u64..=3 {
                let v = SequenceNumber::from_u64(i);
                assert_eq!(
                    s.cache()
                        .find_object_lt_or_eq_version(s.obj_id(1), v)
                        .unwrap()
                        .unwrap()
                        .version(),
                    v
                );
            }
        };

        // make 3 versions of the object
        s.with_created(&[1]);
        let tx1 = s.do_tx().await;
        s.with_mutated(&[1]);
        let tx2 = s.do_tx().await;
        s.with_mutated(&[1]);
        let tx3 = s.do_tx().await;

        // make sure we find the correct version regardless of which
        // txns are committed vs uncommitted. Scenario::iterate repeats
        // the test with cache eviction at each possible point.
        check_all_versions(&s);
        s.commit(tx1).await.unwrap();
        check_all_versions(&s);
        s.commit(tx2).await.unwrap();
        check_all_versions(&s);
        s.commit(tx3).await.unwrap();
        check_all_versions(&s);
    })
    .await;
}
