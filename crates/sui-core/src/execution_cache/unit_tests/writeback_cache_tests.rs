// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use prometheus::default_registry;
use rand::{rngs::StdRng, SeedableRng};
use std::{
    collections::BTreeMap,
    future::Future,
    path::PathBuf,
    sync::atomic::Ordering,
    sync::{atomic::AtomicU32, Arc},
};
use sui_framework::BuiltInFramework;
use sui_macros::{register_fail_point_async, sim_test};
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::{
    base_types::{random_object_ref, SuiAddress},
    crypto::{deterministic_random_account_key, get_key_pair_from_rng, AccountKeyPair},
    object::{MoveObject, Owner, OBJECT_START_VERSION},
    storage::ChildObjectResolver,
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

type ActionCb = Box<dyn Fn(&mut Scenario) + Send>;

struct Scenario {
    store: Arc<AuthorityStore>,
    cache: Arc<WritebackCache>,

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
        static METRICS: once_cell::sync::Lazy<Arc<ExecutionCacheMetrics>> =
            once_cell::sync::Lazy::new(|| Arc::new(ExecutionCacheMetrics::new(default_registry())));

        let cache = Arc::new(WritebackCache::new(store.clone(), (*METRICS).clone()));
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

    fn new_with_store_and_cache(store: Arc<AuthorityStore>, cache: Arc<WritebackCache>) -> Self {
        Self {
            store,
            cache,
            id_map: BTreeMap::new(),
            objects: BTreeMap::new(),
            outputs: Self::new_outputs(),
            transactions: BTreeSet::new(),

            action_count: Arc::new(AtomicU32::new(0)),
            do_after: None,
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

    fn new_package() -> Object {
        use sui_move_build::BuildConfig;

        // add object_basics package object to genesis, since lots of test use it
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("src/unit_tests/data/object_basics");
        let modules: Vec<_> = BuildConfig::new_for_testing()
            .build(path)
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

    fn with_child(&mut self, short_id: u32, owner: u32) {
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

    fn with_packages(&mut self, short_ids: &[u32]) {
        for short_id in short_ids {
            let object = Self::new_package();
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

    fn take_outputs(&mut self) -> Arc<TransactionOutputs> {
        let mut outputs = Self::new_outputs();
        std::mem::swap(&mut self.outputs, &mut outputs);
        Arc::new(outputs)
    }

    // Commit the current tx to the cache, return its digest, and reset the transaction
    // outputs to a new empty one.
    async fn do_tx(&mut self) -> TransactionDigest {
        // Resets outputs, but not objects, so that subsequent runs must respect
        // the state so far.
        let outputs = self.take_outputs();

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

    async fn clear_state_end_of_epoch(&self) {
        let execution_guard = tokio::sync::RwLock::new(1u64);
        let lock = execution_guard.write().await;
        self.cache().clear_state_end_of_epoch(&lock);
    }

    fn evict_caches(&self) {
        self.cache.clear_caches();
    }

    fn reset_cache(&mut self) {
        self.cache = Arc::new(WritebackCache::new(
            self.store.clone(),
            self.cache.metrics.clone(),
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

    fn assert_packages(&self, short_ids: &[u32]) {
        self.assert_live(short_ids);
        for short_id in short_ids {
            let id = self.id_map.get(short_id).expect("no such object");
            self.cache()
                .get_package_object(id)
                .expect("no such package");
        }
    }

    fn get_from_dirty_cache(&self, short_id: u32) -> Option<Object> {
        let id = self.id_map.get(&short_id).expect("no such object");
        let object = self.objects.get(id).expect("no such object");
        self.cache
            .dirty
            .objects
            .get(id)?
            .get(&object.version())
            .map(|e| e.unwrap_object().clone())
    }

    fn assert_dirty(&self, short_ids: &[u32]) {
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

    fn assert_not_dirty(&self, short_ids: &[u32]) {
        for short_id in short_ids {
            assert!(
                self.get_from_dirty_cache(*short_id).is_none(),
                "object exists in dirty cache"
            );
        }
    }

    fn assert_cached(&self, short_ids: &[u32]) {
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

    fn object(&self, short_id: u32) -> Object {
        self.objects
            .get(&self.obj_id(short_id))
            .expect("no such object")
            .clone()
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
        s.cache()
            .commit_transaction_outputs(1, &tx)
            .await
            .expect("commit failed");
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

#[tokio::test]
async fn test_write_transaction_outputs_is_sync() {
    telemetry_subscribers::init_for_testing();
    Scenario::iterate(|mut s| async move {
        s.with_created(&[1, 2]);
        let outputs = s.take_outputs();
        // assert that write_transaction_outputs is sync in non-simtest, which causes the
        // fail_point_async! macros above to be elided
        s.cache
            .write_transaction_outputs(1, outputs)
            .now_or_never()
            .unwrap()
            .unwrap();
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
        s.clear_state_end_of_epoch().await;
    })
    .await;
}

#[tokio::test]
#[should_panic(expected = "transaction must exist")]
async fn test_revert_commited_tx_panics() {
    telemetry_subscribers::init_for_testing();
    Scenario::iterate(|mut s| async move {
        s.with_created(&[1]);
        let tx1 = s.do_tx().await;
        s.commit(tx1).await.unwrap();
        s.cache().revert_state_update(&tx1).unwrap();
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

        s.cache().revert_state_update(&tx1).unwrap();
        s.clear_state_end_of_epoch().await;

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
            s.cache()
                .get_object(&s.obj_id(1))
                .unwrap()
                .unwrap()
                .version()
        };

        s.with_mutated(&[1]);
        let tx = s.do_tx().await;

        s.cache().revert_state_update(&tx).unwrap();
        s.clear_state_end_of_epoch().await;

        let version_after_revert = s
            .cache()
            .get_object(&s.obj_id(1))
            .unwrap()
            .unwrap()
            .version();
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

        s.cache().revert_state_update(&tx1).unwrap();
        s.clear_state_end_of_epoch().await;

        assert!(s
            .cache()
            .get_package_object(&s.obj_id(2))
            .unwrap()
            .is_none());
    })
    .await;
}

#[sim_test]
async fn test_concurrent_readers() {
    telemetry_subscribers::init_for_testing();

    register_fail_point_async("write_object_entry", || async {
        tokio::task::yield_now().await;
    });
    register_fail_point_async("write_marker_entry", || async {
        tokio::task::yield_now().await;
    });

    let store = init_authority_store().await;
    let cache = Arc::new(WritebackCache::new_for_tests(
        store.clone(),
        default_registry(),
    ));

    let mut s = Scenario::new_with_store_and_cache(store.clone(), cache.clone());
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
                cache.write_transaction_outputs(1, tx1).await.unwrap();

                barrier.wait().await;
                println!("writing tx2");
                cache.write_transaction_outputs(1, tx2).await.unwrap();
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
                    let parent = cache
                        .get_object_by_key(&parent_ref.0, parent_ref.1)
                        .unwrap();
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
