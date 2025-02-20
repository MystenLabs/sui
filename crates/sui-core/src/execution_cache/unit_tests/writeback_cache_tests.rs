// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::default_registry;
use rand::{rngs::StdRng, Rng, SeedableRng};
use std::{
    collections::BTreeMap,
    future::Future,
    path::PathBuf,
    sync::atomic::Ordering,
    sync::{atomic::AtomicU32, Arc},
    time::{Duration, Instant},
};
use sui_framework::BuiltInFramework;
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::{
    base_types::{random_object_ref, SuiAddress},
    crypto::{deterministic_random_account_key, get_key_pair_from_rng, AccountKeyPair},
    object::{MoveObject, Owner, OBJECT_START_VERSION},
    storage::ChildObjectResolver,
};
use sui_types::{
    effects::{TestEffectsBuilder, TransactionEffectsAPI},
    event::Event,
};
use tokio::sync::RwLock;

use super::*;
use crate::{
    authority::{test_authority_builder::TestAuthorityBuilder, AuthorityState, AuthorityStore},
    execution_cache::ExecutionCacheAPI,
};

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

type ActionCb = Box<dyn Fn(&mut Scenario) + Send>;

pub(crate) struct Scenario {
    pub authority: Arc<AuthorityState>,
    pub store: Arc<AuthorityStore>,
    pub epoch_store: Arc<AuthorityPerEpochStore>,
    pub cache: Arc<WritebackCache>,

    id_map: BTreeMap<u32, ObjectID>,
    objects: BTreeMap<ObjectID, Object>,
    outputs: TransactionOutputs,
    transactions: BTreeSet<TransactionDigest>,

    action_count: Arc<AtomicU32>,
    do_after: Option<(u32, ActionCb)>,
}

impl Scenario {
    async fn new(do_after: Option<(u32, ActionCb)>, action_count: Arc<AtomicU32>) -> Self {
        let authority = TestAuthorityBuilder::new().build().await;

        let store = authority.database_for_testing().clone();
        let epoch_store = authority.epoch_store_for_testing().clone();

        static METRICS: once_cell::sync::Lazy<Arc<ExecutionCacheMetrics>> =
            once_cell::sync::Lazy::new(|| Arc::new(ExecutionCacheMetrics::new(default_registry())));

        let cache = Arc::new(WritebackCache::new(
            &Default::default(),
            store.clone(),
            (*METRICS).clone(),
            BackpressureManager::new_for_tests(),
        ));
        Self {
            authority,
            store,
            epoch_store,
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
        if let Some((count, _)) = &self.do_after {
            if prev == *count {
                let (_, f) = self.do_after.take().unwrap();
                f(self);
            }
        }
    }

    // This method runs a test scenario multiple times, and each time it clears the
    // evictable caches after a different step.
    pub async fn iterate<F, Fut>(f: F)
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
                println!("evict_caches()");
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
        let events: TransactionEvents = Default::default();

        let effects = TestEffectsBuilder::new(tx.inner())
            .with_events_digest(events.digest())
            .build();

        TransactionOutputs {
            transaction: Arc::new(tx),
            effects,
            events,
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

    fn new_package() -> Object {
        use sui_move_build::BuildConfig;

        // add object_basics package object to genesis, since lots of test use it
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("src/unit_tests/data/object_basics");
        let modules: Vec<_> = BuildConfig::new_for_testing()
            .build(&path)
            .unwrap()
            .get_modules()
            .cloned()
            .collect();
        let digest = TransactionDigest::genesis_marker();
        Object::new_package_for_testing(&modules, digest, BuiltInFramework::genesis_move_packages())
            .unwrap()
    }

    fn new_child(owner: ObjectID) -> Object {
        let id = ObjectID::random();
        Object::new_move(
            MoveObject::new_gas_coin(OBJECT_START_VERSION, id, 100),
            Owner::ObjectOwner(owner.into()),
            TransactionDigest::ZERO,
        )
    }

    fn inc_version_by(object: Object, delta: u64) -> Object {
        let version = object.version();
        let mut inner = object.into_inner();
        inner
            .data
            .try_as_move_mut()
            .unwrap()
            .increment_version_to(SequenceNumber::from_u64(version.value() + delta));
        inner.into()
    }

    pub fn with_child(&mut self, short_id: u32, owner: u32) {
        let owner_id = self.id_map.get(&owner).expect("no such object");
        let object = Self::new_child(*owner_id);
        self.outputs
            .new_locks_to_init
            .push(object.compute_object_reference());
        let id = object.id();
        assert!(self.id_map.insert(short_id, id).is_none());
        self.outputs.written.insert(id, object.clone());
        self.objects.insert(id, object).assert_inserted();
    }

    pub fn with_created(&mut self, short_ids: &[u32]) {
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

    pub fn with_events(&mut self) {
        let mut events: TransactionEvents = Default::default();
        events.data.push(Event::random_for_testing());

        let effects = TestEffectsBuilder::new(self.outputs.transaction.inner())
            .with_events_digest(events.digest())
            .build();
        self.outputs.events = events;
        self.outputs.effects = effects;
    }

    pub fn with_packages(&mut self, short_ids: &[u32]) {
        for short_id in short_ids {
            let object = Self::new_package();
            let id = object.id();
            assert!(self.id_map.insert(*short_id, id).is_none());
            self.outputs.written.insert(id, object.clone());
            self.objects.insert(id, object).assert_inserted();
        }
    }

    pub fn with_mutated(&mut self, short_ids: &[u32]) {
        self.with_mutated_version_delta(short_ids, 1);
    }

    pub fn with_mutated_version_delta(&mut self, short_ids: &[u32], delta: u64) {
        // for every id in short_ids, assert than an object with that id exists, and
        // mutate it
        for short_id in short_ids {
            let id = self.id_map.get(short_id).expect("object not found");
            let object = self.objects.get(id).cloned().expect("object not found");
            self.outputs
                .locks_to_delete
                .push(object.compute_object_reference());
            let object = Self::inc_version_by(object, delta);
            self.objects.insert(*id, object.clone());
            self.outputs
                .new_locks_to_init
                .push(object.compute_object_reference());
            self.outputs.written.insert(object.id(), object);
        }
    }

    pub fn with_deleted(&mut self, short_ids: &[u32]) {
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

    pub fn with_wrapped(&mut self, short_ids: &[u32]) {
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

    pub fn with_received(&mut self, short_ids: &[u32]) {
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
                object.compute_full_object_reference().into(),
                MarkerValue::Received,
            ));
        }
    }

    pub fn take_outputs(&mut self) -> Arc<TransactionOutputs> {
        let mut outputs = Self::new_outputs();
        std::mem::swap(&mut self.outputs, &mut outputs);
        Arc::new(outputs)
    }

    // Commit the current tx to the cache, return its digest, and reset the transaction
    // outputs to a new empty one.
    pub async fn do_tx(&mut self) -> TransactionDigest {
        // Resets outputs, but not objects, so that subsequent runs must respect
        // the state so far.
        let outputs = self.take_outputs();

        let tx = *outputs.transaction.digest();
        assert!(self.transactions.insert(tx), "transaction is not unique");

        self.cache()
            .write_transaction_outputs(1 /* epoch */, outputs.clone(), true);

        self.count_action();
        tx
    }

    // commit a transaction to the database
    pub async fn commit(&mut self, tx: TransactionDigest) -> SuiResult {
        self.cache().commit_transaction_outputs(1, &[tx], true);
        self.count_action();
        Ok(())
    }

    pub fn clear_state_end_of_epoch(&self) {
        let execution_guard = RwLock::new(1u64);
        let lock = execution_guard.try_write().unwrap();
        self.cache().clear_state_end_of_epoch(&lock);
    }

    pub fn evict_caches(&self) {
        self.cache.clear_caches_and_assert_empty();
    }

    pub fn reset_cache(&mut self) {
        self.cache = Arc::new(WritebackCache::new(
            &Default::default(),
            self.store.clone(),
            self.cache.metrics.clone(),
            BackpressureManager::new_for_tests(),
        ));

        // reset the scenario state to match the db
        let reverse_id_map: BTreeMap<_, _> = self.id_map.iter().map(|(k, v)| (*v, *k)).collect();
        self.objects.clear();

        self.store.iter_live_object_set(false).for_each(|o| {
            let LiveObject::Normal(o) = o else {
                panic!("expected normal object")
            };
            let id = o.id();
            // genesis objects are not managed by Scenario, ignore them
            if reverse_id_map.contains_key(&id) {
                self.objects.insert(id, o);
            }
        });
    }

    pub fn assert_live(&self, short_ids: &[u32]) {
        for short_id in short_ids {
            let id = self.id_map.get(short_id).expect("no such object");
            let expected = self.objects.get(id).expect("no such object");
            let version = expected.version();
            assert_eq!(
                self.cache().get_object_by_key(id, version).unwrap(),
                *expected
            );
            assert_eq!(self.cache().get_object(&expected.id()).unwrap(), *expected);
            // TODO: enable after lock caching is implemented
            // assert!(!self
            //  .cache()
            //  .get_lock(expected.compute_object_reference(), 1)
            //  .unwrap()
            //  .is_locked());
        }
    }

    pub fn assert_packages(&self, short_ids: &[u32]) {
        self.assert_live(short_ids);
        for short_id in short_ids {
            let id = self.id_map.get(short_id).expect("no such object");
            self.cache()
                .get_package_object(id)
                .expect("no such package");
        }
    }

    pub fn get_from_dirty_cache(&self, short_id: u32) -> Option<Object> {
        let id = self.id_map.get(&short_id).expect("no such object");
        let object = self.objects.get(id).expect("no such object");
        self.cache
            .dirty
            .objects
            .get(id)?
            .get(&object.version())
            .map(|e| e.unwrap_object().clone())
    }

    pub fn assert_dirty(&self, short_ids: &[u32]) {
        // assert that all ids are in the dirty cache
        for short_id in short_ids {
            let id = self.id_map.get(short_id).expect("no such object");
            let object = self.objects.get(id).expect("no such object");
            assert_eq!(
                *object,
                self.get_from_dirty_cache(*short_id)
                    .expect("no such object in dirty cache")
            );
        }
    }

    pub fn assert_not_dirty(&self, short_ids: &[u32]) {
        for short_id in short_ids {
            assert!(
                self.get_from_dirty_cache(*short_id).is_none(),
                "object exists in dirty cache"
            );
        }
    }

    pub fn assert_cached(&self, short_ids: &[u32]) {
        for short_id in short_ids {
            let id = self.id_map.get(short_id).expect("no such object");
            let object = self.objects.get(id).expect("no such object");
            assert_eq!(
                self.cache
                    .cached
                    .object_cache
                    .get(id)
                    .unwrap()
                    .lock()
                    .get(&object.version())
                    .unwrap()
                    .unwrap_object()
                    .clone(),
                *object
            );
        }
    }

    pub fn assert_received(&self, short_ids: &[u32]) {
        for short_id in short_ids {
            let id = self.id_map.get(short_id).expect("no such object");
            let object = self.objects.get(id).expect("no such object");
            assert_eq!(
                self.cache()
                    .get_object_by_key(id, object.version())
                    .unwrap(),
                *object
            );
            assert!(self.cache().have_received_object_at_version(
                FullObjectKey::new(object.full_id(), object.version()),
                1,
                true
            ));
        }
    }

    pub fn assert_not_exists(&self, short_ids: &[u32]) {
        for short_id in short_ids {
            let id = self.id_map.get(short_id).expect("no such id");

            assert!(
                self.cache().get_object(id).is_none(),
                "object exists in cache"
            );
        }
    }

    pub fn obj_id(&self, short_id: u32) -> ObjectID {
        *self.id_map.get(&short_id).expect("no such id")
    }

    pub fn object(&self, short_id: u32) -> Object {
        self.objects
            .get(&self.obj_id(short_id))
            .expect("no such object")
            .clone()
    }

    pub fn obj_ref(&self, short_id: u32) -> ObjectRef {
        self.object(short_id).compute_object_reference()
    }

    pub fn make_signed_transaction(&self, tx: &VerifiedTransaction) -> VerifiedSignedTransaction {
        VerifiedSignedTransaction::new(
            self.epoch_store.epoch(),
            tx.clone(),
            self.authority.name,
            &*self.authority.secret,
        )
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
        s.assert_dirty(&[1, 2]);
        s.cache().commit_transaction_outputs(1, &[tx], true);
        s.assert_not_dirty(&[1, 2]);
        s.assert_cached(&[1, 2]);

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
async fn test_extra_outputs() {
    telemetry_subscribers::init_for_testing();
    Scenario::iterate(|mut s| async move {
        // make sure that events, effects, transactions are all
        // returned correctly no matter the cache state.
        s.with_created(&[1, 2]);
        s.with_events();

        let tx = s.do_tx().await;

        s.cache.get_transaction_block(&tx).unwrap();
        let fx = s.cache.get_executed_effects(&tx).unwrap();
        let events_digest = fx.events_digest().unwrap();
        s.cache.get_events(events_digest).unwrap();

        s.commit(tx).await.unwrap();

        s.cache.get_transaction_block(&tx).unwrap();
        s.cache.get_executed_effects(&tx).unwrap();
        s.cache.get_events(events_digest).unwrap();

        // clear cache
        s.reset_cache();

        s.cache.get_transaction_block(&tx).unwrap();
        s.cache.get_executed_effects(&tx).unwrap();
        s.cache.get_events(events_digest).unwrap();

        s.with_created(&[3]);
        let tx = s.do_tx().await;

        // when Events is empty, it should be treated as None
        let fx = s.cache.get_executed_effects(&tx).unwrap();
        let events_digest = fx.events_digest().unwrap();
        assert!(
            s.cache.get_events(events_digest).is_none(),
            "empty events should be none"
        );

        s.commit(tx).await.unwrap();
        assert!(
            s.cache.get_events(events_digest).is_none(),
            "empty events should be none"
        );

        s.reset_cache();
        assert!(
            s.cache.get_events(events_digest).is_none(),
            "empty events should be none"
        );
    })
    .await;
}

#[tokio::test]
#[should_panic(expected = "version must be the oldest in the map")]
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

#[tokio::test]
async fn test_lt_or_eq_caching() {
    telemetry_subscribers::init_for_testing();
    Scenario::iterate(|mut s| async move {
        // make 3 versions of the object
        s.with_created(&[1]);
        let tx1 = s.do_tx().await;
        s.with_mutated_version_delta(&[1], 2);
        let tx2 = s.do_tx().await;
        s.with_mutated_version_delta(&[1], 2);
        let tx3 = s.do_tx().await;
        s.commit(tx1).await.unwrap();
        s.commit(tx2).await.unwrap();
        s.commit(tx3).await.unwrap();

        s.reset_cache();

        let check_version = |lookup_version: u64, expected_version: u64| {
            let lookup_version = SequenceNumber::from_u64(lookup_version);
            let expected_version = SequenceNumber::from_u64(expected_version);
            assert_eq!(
                s.cache()
                    .find_object_lt_or_eq_version(s.obj_id(1), lookup_version)
                    .unwrap()
                    .version(),
                expected_version
            );
        };

        // latest object not yet cached
        assert!(!s.cache.cached.object_by_id_cache.contains_key(&s.obj_id(1)));

        // version <= 0 does not exist
        assert!(s
            .cache()
            .find_object_lt_or_eq_version(s.obj_id(1), 0.into())
            .is_none());

        // query above populates cache
        assert_eq!(
            s.cache
                .cached
                .object_by_id_cache
                .get(&s.obj_id(1))
                .unwrap()
                .lock()
                .version()
                .unwrap()
                .value(),
            5
        );

        // all queries get correct answer with a populated cache
        check_version(1, 1);
        check_version(2, 1);
        check_version(3, 3);
        check_version(4, 3);
        check_version(5, 5);
        check_version(6, 5);
        check_version(7, 5);
    })
    .await;
}

#[tokio::test]
async fn test_lt_or_eq_with_cached_tombstone() {
    telemetry_subscribers::init_for_testing();
    Scenario::iterate(|mut s| async move {
        // make an object, and a tombstone
        s.with_created(&[1]);
        let tx1 = s.do_tx().await;
        s.with_deleted(&[1]);
        let tx2 = s.do_tx().await;
        s.commit(tx1).await.unwrap();
        s.commit(tx2).await.unwrap();

        s.reset_cache();

        let check_version = |lookup_version: u64, expected_version: Option<u64>| {
            let lookup_version = SequenceNumber::from_u64(lookup_version);
            assert_eq!(
                s.cache()
                    .find_object_lt_or_eq_version(s.obj_id(1), lookup_version)
                    .map(|v| v.version()),
                expected_version.map(SequenceNumber::from_u64)
            );
        };

        // latest object not yet cached
        assert!(!s.cache.cached.object_by_id_cache.contains_key(&s.obj_id(1)));

        // version 2 is deleted
        check_version(2, None);

        // checking the version pulled the tombstone into the cache
        assert!(s.cache.cached.object_by_id_cache.contains_key(&s.obj_id(1)));

        // version 1 is still found, tombstone in cache is ignored
        check_version(1, Some(1));
    })
    .await;
}

#[tokio::test]
async fn test_write_transaction_outputs_is_sync() {
    telemetry_subscribers::init_for_testing();
    Scenario::iterate(|mut s| async move {
        s.with_created(&[1, 2]);
        let outputs = s.take_outputs();
        // assert that write_transaction_outputs is sync in non-simtest, which causes the
        // fail_point_async! macros above to be elided
        s.cache.write_transaction_outputs(1, outputs);
    })
    .await;
}

#[tokio::test]
#[should_panic(expected = "should be empty due to revert_state_update")]
async fn test_missing_reverts_panic() {
    telemetry_subscribers::init_for_testing();
    Scenario::iterate(|mut s| async move {
        s.with_created(&[1]);
        s.do_tx().await;
        s.clear_state_end_of_epoch();
    })
    .await;
}

#[tokio::test]
#[should_panic(expected = "attempt to revert committed transaction")]
async fn test_revert_committed_tx_panics() {
    telemetry_subscribers::init_for_testing();
    Scenario::iterate(|mut s| async move {
        s.with_created(&[1]);
        let tx1 = s.do_tx().await;
        s.commit(tx1).await.unwrap();
        s.cache().revert_state_update(&tx1);
    })
    .await;
}

#[tokio::test]
async fn test_revert_unexecuted_tx() {
    telemetry_subscribers::init_for_testing();
    Scenario::iterate(|mut s| async move {
        s.with_created(&[1]);
        let tx1 = s.do_tx().await;
        s.commit(tx1).await.unwrap();
        let random_digest = TransactionDigest::random();
        // must not panic - pending_consensus_transactions is a super set of
        // executed but un-checkpointed transactions
        s.cache().revert_state_update(&random_digest);
    })
    .await;
}

#[tokio::test]
async fn test_revert_state_update_created() {
    telemetry_subscribers::init_for_testing();
    Scenario::iterate(|mut s| async move {
        // newly created object
        s.with_created(&[1]);
        let tx1 = s.do_tx().await;
        s.assert_live(&[1]);

        s.cache().revert_state_update(&tx1);
        s.clear_state_end_of_epoch();

        s.assert_not_exists(&[1]);
    })
    .await;
}

#[tokio::test]
async fn test_revert_state_update_mutated() {
    telemetry_subscribers::init_for_testing();
    Scenario::iterate(|mut s| async move {
        let v1 = {
            s.with_created(&[1]);
            let tx = s.do_tx().await;
            s.commit(tx).await.unwrap();
            s.cache().get_object(&s.obj_id(1)).unwrap().version()
        };

        s.with_mutated(&[1]);
        let tx = s.do_tx().await;

        s.cache().revert_state_update(&tx);
        s.clear_state_end_of_epoch();

        let version_after_revert = s.cache().get_object(&s.obj_id(1)).unwrap().version();
        assert_eq!(v1, version_after_revert);
    })
    .await;
}

#[tokio::test]
async fn test_invalidate_package_cache_on_revert() {
    telemetry_subscribers::init_for_testing();
    Scenario::iterate(|mut s| async move {
        s.with_created(&[1]);
        s.with_packages(&[2]);
        let tx1 = s.do_tx().await;

        s.assert_live(&[1]);
        s.assert_packages(&[2]);

        s.cache().revert_state_update(&tx1);
        s.clear_state_end_of_epoch();

        assert!(s
            .cache()
            .get_package_object(&s.obj_id(2))
            .unwrap()
            .is_none());
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn test_concurrent_readers() {
    telemetry_subscribers::init_for_testing();

    let mut s = Scenario::new(None, Arc::new(AtomicU32::new(0))).await;
    let cache = s.cache.clone();
    let mut txns = Vec::new();

    for i in 0..100 {
        let parent_id = i * 2;
        let child_id = i * 2 + 1;
        s.with_created(&[parent_id]);
        s.with_child(child_id, parent_id);
        let child_full_id = s.obj_id(child_id);
        let tx1 = s.take_outputs();

        s.with_mutated(&[parent_id]);
        s.with_deleted(&[child_id]);
        let tx2 = s.take_outputs();

        txns.push((
            tx1,
            tx2,
            s.object(parent_id).compute_object_reference(),
            child_full_id,
        ));
    }

    let barrier = Arc::new(tokio::sync::Barrier::new(2));

    let t1 = {
        let txns = txns.clone();
        let cache = cache.clone();
        let barrier = barrier.clone();
        tokio::task::spawn(async move {
            for (tx1, tx2, _, _) in txns {
                println!("writing tx1");
                cache.write_transaction_outputs(1, tx1);

                barrier.wait().await;
                println!("writing tx2");
                cache.write_transaction_outputs(1, tx2);
            }
        })
    };

    let t2 = {
        let txns = txns.clone();
        let cache = cache.clone();
        let barrier = barrier.clone();
        tokio::task::spawn(async move {
            for (_, _, parent_ref, child_id) in txns {
                barrier.wait().await;

                println!("parent: {:?}", parent_ref);
                loop {
                    let parent = cache.get_object_by_key(&parent_ref.0, parent_ref.1);
                    if parent.is_none() {
                        tokio::task::yield_now().await;
                        continue;
                    }
                    assert_eq!(parent.unwrap().version(), parent_ref.1);
                    break;
                }
                let child = cache
                    .read_child_object(&parent_ref.0, &child_id, parent_ref.1)
                    .unwrap();
                assert!(child.is_none(), "Inconsistent child read detected");
            }
        })
    };

    t1.await.unwrap();
    t2.await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn test_concurrent_lockers() {
    telemetry_subscribers::init_for_testing();

    let mut s = Scenario::new(None, Arc::new(AtomicU32::new(0))).await;
    let cache = s.cache.clone();
    let mut txns = Vec::new();

    for i in 0..1000 {
        let a = i * 4;
        let b = i * 4 + 1;
        let c = i * 4 + 2;
        let d = i * 4 + 3;
        s.with_created(&[a, b]);
        s.do_tx().await;

        let a_ref = s.obj_ref(a);
        let b_ref = s.obj_ref(b);

        // these contents of these txns are never used, they are just unique transactions to use for
        // attempted equivocation
        s.with_created(&[c]);
        let tx1 = s.take_outputs();

        s.with_created(&[d]);
        let tx2 = s.take_outputs();

        let tx1 = s.make_signed_transaction(&tx1.transaction);
        let tx2 = s.make_signed_transaction(&tx2.transaction);

        txns.push((tx1, tx2, a_ref, b_ref));
    }

    let barrier = Arc::new(tokio::sync::Barrier::new(2));

    let t1 = {
        let txns = txns.clone();
        let cache = cache.clone();
        let barrier = barrier.clone();
        let epoch_store = s.epoch_store.clone();
        tokio::task::spawn(async move {
            let mut results = Vec::new();
            for (tx1, _, a_ref, b_ref) in txns {
                results.push(cache.acquire_transaction_locks(
                    &epoch_store,
                    &[a_ref, b_ref],
                    *tx1.digest(),
                    Some(tx1.clone()),
                ));
                barrier.wait().await;
            }
            results
        })
    };

    let t2 = {
        let txns = txns.clone();
        let cache = cache.clone();
        let barrier = barrier.clone();
        let epoch_store = s.epoch_store.clone();
        tokio::task::spawn(async move {
            let mut results = Vec::new();
            for (_, tx2, a_ref, b_ref) in txns {
                results.push(cache.acquire_transaction_locks(
                    &epoch_store,
                    &[a_ref, b_ref],
                    *tx2.digest(),
                    Some(tx2.clone()),
                ));
                barrier.wait().await;
            }
            results
        })
    };

    let results1 = t1.await.unwrap();
    let results2 = t2.await.unwrap();

    for (r1, r2) in results1.into_iter().zip(results2) {
        // exactly one should succeed in each case
        assert_eq!(r1.is_ok(), r2.is_err());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn test_concurrent_lockers_same_tx() {
    telemetry_subscribers::init_for_testing();

    let mut s = Scenario::new(None, Arc::new(AtomicU32::new(0))).await;
    let cache = s.cache.clone();
    let mut txns = Vec::new();

    for i in 0..1000 {
        let a = i * 4;
        let b = i * 4 + 1;
        s.with_created(&[a, b]);
        s.do_tx().await;

        let a_ref = s.obj_ref(a);
        let b_ref = s.obj_ref(b);

        let tx1 = s.take_outputs();

        let tx1 = s.make_signed_transaction(&tx1.transaction);

        txns.push((tx1, a_ref, b_ref));
    }

    let barrier = Arc::new(tokio::sync::Barrier::new(2));

    let t1 = {
        let txns = txns.clone();
        let cache = cache.clone();
        let barrier = barrier.clone();
        let epoch_store = s.epoch_store.clone();
        tokio::task::spawn(async move {
            let mut results = Vec::new();
            for (tx1, a_ref, b_ref) in txns {
                results.push(cache.acquire_transaction_locks(
                    &epoch_store,
                    &[a_ref, b_ref],
                    *tx1.digest(),
                    Some(tx1.clone()),
                ));
                barrier.wait().await;
            }
            results
        })
    };

    let t2 = {
        let txns = txns.clone();
        let cache = cache.clone();
        let barrier = barrier.clone();
        let epoch_store = s.epoch_store.clone();
        tokio::task::spawn(async move {
            let mut results = Vec::new();
            for (tx1, a_ref, b_ref) in txns {
                results.push(cache.acquire_transaction_locks(
                    &epoch_store,
                    &[a_ref, b_ref],
                    *tx1.digest(),
                    Some(tx1.clone()),
                ));
                barrier.wait().await;
            }
            results
        })
    };

    let results1 = t1.await.unwrap();
    let results2 = t2.await.unwrap();

    for (r1, r2) in results1.into_iter().zip(results2) {
        assert!(r1.is_ok());
        assert!(r2.is_ok());
    }
}

#[tokio::test]
async fn latest_object_cache_race_test() {
    telemetry_subscribers::init_for_testing();
    let authority = TestAuthorityBuilder::new().build().await;

    let store = authority.database_for_testing().clone();

    static METRICS: once_cell::sync::Lazy<Arc<ExecutionCacheMetrics>> =
        once_cell::sync::Lazy::new(|| Arc::new(ExecutionCacheMetrics::new(default_registry())));

    let cache = Arc::new(WritebackCache::new(
        &Default::default(),
        store.clone(),
        (*METRICS).clone(),
        BackpressureManager::new_for_tests(),
    ));

    let object_id = ObjectID::random();
    let owner = SuiAddress::random_for_testing_only();

    // a writer thread that keeps writing new versions
    let writer = {
        let cache = cache.clone();
        let start = Instant::now();
        std::thread::spawn(move || {
            let mut version = OBJECT_START_VERSION;
            while start.elapsed() < Duration::from_secs(2) {
                let object = Object::with_id_owner_version_for_testing(
                    object_id,
                    version,
                    Owner::AddressOwner(owner),
                );

                cache.write_object_entry(&object_id, version, object.into());

                version = version.next();
            }
        })
    };

    // a reader thread that pretends it saw some previous version on the db
    let reader = {
        let cache = cache.clone();
        let start = Instant::now();
        std::thread::spawn(move || {
            while start.elapsed() < Duration::from_secs(2) {
                // If you move the get_ticket_for_read to after we get the latest version,
                // the test will fail! (this is good, it means the test is doing something)
                let ticket = cache
                    .cached
                    .object_by_id_cache
                    .get_ticket_for_read(&object_id);

                // get the latest version, but then let it become stale
                let Some(latest_version) = cache
                    .dirty
                    .objects
                    .get(&object_id)
                    .and_then(|e| e.value().get_highest().map(|v| v.0))
                else {
                    continue;
                };

                // with probability 0.1, sleep for 1µs, so that we are further out of date.
                if rand::thread_rng().gen_bool(0.1) {
                    std::thread::sleep(Duration::from_micros(1));
                }

                let object = Object::with_id_owner_version_for_testing(
                    object_id,
                    latest_version,
                    Owner::AddressOwner(owner),
                );

                // because we obtained the ticket before reading the object, we will not write a stale
                // version to the cache.
                cache.cache_latest_object_by_id(
                    &object_id,
                    LatestObjectCacheEntry::Object(latest_version, object.into()),
                    ticket,
                );
            }
        })
    };

    // a thread that just invalidates the cache as fast as it can
    let invalidator = {
        let cache = cache.clone();
        let start = Instant::now();
        std::thread::spawn(move || {
            while start.elapsed() < Duration::from_secs(2) {
                cache.cached.object_by_id_cache.invalidate(&object_id);
                // sleep for 1 to 10µs
                std::thread::sleep(Duration::from_micros(rand::thread_rng().gen_range(1..10)));
            }
        })
    };

    // a thread that does nothing but watch to see if the cache goes back in time
    let checker = {
        let cache = cache.clone();
        let start = Instant::now();
        std::thread::spawn(move || {
            let mut latest = OBJECT_START_VERSION;

            while start.elapsed() < Duration::from_secs(2) {
                let Some(cur) = cache
                    .cached
                    .object_by_id_cache
                    .get(&object_id)
                    .and_then(|e| e.lock().version())
                else {
                    continue;
                };

                assert!(cur >= latest, "{} >= {}", cur, latest);
                latest = cur;
            }
        })
    };

    writer.join().unwrap();
    reader.join().unwrap();
    checker.join().unwrap();
    invalidator.join().unwrap();
}

#[tokio::test]
async fn test_transaction_cache_race() {
    telemetry_subscribers::init_for_testing();

    let mut s = Scenario::new(None, Arc::new(AtomicU32::new(0))).await;
    let cache = s.cache.clone();
    let mut txns = Vec::new();

    for i in 0..1000 {
        let a = i * 4;
        s.with_created(&[a]);
        s.do_tx().await;

        let outputs = s.take_outputs();
        let tx = (*outputs.transaction).clone();
        let effects = outputs.effects.clone();

        txns.push((tx, effects));
    }

    let barrier = Arc::new(std::sync::Barrier::new(2));

    let t1 = {
        let txns = txns.clone();
        let cache = cache.clone();
        let barrier = barrier.clone();
        std::thread::spawn(move || {
            for (i, (tx, effects)) in txns.into_iter().enumerate() {
                barrier.wait();
                // test both single and multi insert
                if i % 2 == 0 {
                    cache.insert_transaction_and_effects(&tx, &effects);
                } else {
                    cache.multi_insert_transaction_and_effects(&[VerifiedExecutionData::new(
                        tx, effects,
                    )]);
                }
            }
        })
    };

    let t2 = {
        let txns = txns.clone();
        let cache = cache.clone();
        let barrier = barrier.clone();
        std::thread::spawn(move || {
            for (tx, _) in txns {
                barrier.wait();
                cache.get_transaction_block(tx.digest());
            }
        })
    };

    t1.join().unwrap();
    t2.join().unwrap();
}
