// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto::traits::KeyPair as KeypairTraits;
use rand::{rngs::StdRng, SeedableRng};
use std::{
    collections::{btree_map::Entry as BTreeMapEntry, BTreeMap},
    fs,
    sync::Arc,
};
use sui_test_transaction_builder::TestTransactionBuilder;
use sui_types::{
    base_types::random_object_ref,
    crypto::{deterministic_random_account_key, get_key_pair_from_rng},
    object::{MoveObject, Owner, OBJECT_START_VERSION},
    transaction::{SenderSignedData, TransactionData},
};
use tempfile::tempdir;

use super::*;
use crate::{
    authority::{authority_store_tables::AuthorityPerpetualTables, AuthorityStore},
    test_utils::init_state_parameters_from_rng,
};

async fn init_authority_store() -> Arc<AuthorityStore> {
    let seed = [1u8; 32];
    let (genesis, _) = init_state_parameters_from_rng(&mut StdRng::from_seed(seed));
    let committee = genesis.committee().unwrap();

    // Create a random directory to store the DB
    let dir = tempdir().unwrap();
    let db_path = dir.path();
    fs::create_dir(&db_path).unwrap();

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

struct TransactionOutputBuilder {
    objects: BTreeMap<u32, Object>,

    outputs: TransactionOutputs,
    /*
    created: BTreeSet<u32>,
    deleted: BTreeSet<u32>,
    locks_to_init: BTreeSet<ObjectRef>,
    locks_to_delete: BTreeSet<ObjectRef>,
    */
}

impl TransactionOutputBuilder {
    fn new() -> Self {
        let mut rng = StdRng::from_seed([0; 32]);
        let (sender, keypair) = get_key_pair_from_rng(&mut rng);
        let (receiver, _) = get_key_pair_from_rng(&mut rng);

        let tx = TestTransactionBuilder::new(sender, random_object_ref(), 100)
            .transfer(random_object_ref(), receiver)
            .build_and_sign(keypair);

        let tx = VerifiedTransaction::new_unchecked(tx);

        Self {
            objects: BTreeMap::new(),
            outputs: Self::new_outputs(),
        }
    }

    fn new_outputs() -> TransactionOutputs {
        let mut rng = StdRng::from_seed([0; 32]);
        let (sender, keypair) = get_key_pair_from_rng(&mut rng);
        let (receiver, _) = get_key_pair_from_rng(&mut rng);

        let tx = TestTransactionBuilder::new(sender, random_object_ref(), 100)
            .transfer(random_object_ref(), receiver)
            .build_and_sign(keypair);

        let tx = VerifiedTransaction::new_unchecked(tx);

        TransactionOutputs {
            transaction: Arc::new(tx),
            effects: TransactionEffects::default(),
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

    fn with_created(&mut self, short_ids: &[u32]) -> Self {
        // for every id in short_ids, create an object with that id if it doesn't exist
        for id in short_ids {
            let object = Self::new_object();
            self.outputs
                .new_locks_to_init
                .push(object.compute_object_reference());
            self.outputs.written.insert(object.id(), object.clone());
            self.objects.insert(*id, object).assert_inserted();
        }
        self
    }

    fn with_mutated(&mut self, short_ids: &[u32]) -> Self {
        // for every id in short_ids, create an object with that id if it doesn't exist
        for id in short_ids {
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

        self
    }

    fn with_deleted(&mut self, short_ids: &[u32]) -> Self {
        for id in short_ids {
            let object = self.objects.remove(id).expect("object not found");
            let object_ref = object.compute_object_reference();
            self.outputs.locks_to_delete.push(object_ref);
            self.outputs.deleted.push(object_ref.into());
        }
        self
    }

    fn with_wrapped(&mut self, short_ids: &[u32]) -> Self {
        for id in short_ids {
            let object = self.objects.get(id).cloned().expect("object not found");
            let object_ref = object.compute_object_reference();
            self.outputs.locks_to_delete.push(object_ref);
            self.outputs
                .wrapped
                .push(object.compute_object_reference().into());
        }
        self
    }

    fn with_received(&mut self, short_ids: &[u32]) -> Self {
        for id in short_ids {
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
        self
    }

    // Resets outputs, but not objects, so that subsequent outputs must respect
    // the state so far.
    fn build(&mut self) -> Arc<TransactionOutputs> {
        let outputs = Self::new_outputs();
        std::mem::swap(&mut self.outputs, &mut outputs);
        outputs.into()
    }
}

#[tokio::test]
async fn test_object_methods() {
    let authority_store = init_authority_store().await;
}
