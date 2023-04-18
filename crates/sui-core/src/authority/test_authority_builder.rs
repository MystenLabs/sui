// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::authority_per_epoch_store::AuthorityPerEpochStore;
use crate::authority::epoch_start_configuration::EpochStartConfiguration;
use crate::authority::{AuthorityState, AuthorityStore};
use crate::checkpoints::CheckpointStore;
use crate::epoch::committee_store::CommitteeStore;
use crate::epoch::epoch_metrics::EpochMetrics;
use crate::module_cache_metrics::ResolverMetrics;
use crate::signature_verifier::SignatureVerifierMetrics;
use fastcrypto::traits::KeyPair;
use prometheus::Registry;
use std::path::PathBuf;
use std::sync::Arc;
use sui_config::genesis::Genesis;
use sui_config::node::{
    AuthorityStorePruningConfig, DBCheckpointConfig, ExpensiveSafetyCheckConfig,
};
use sui_macros::nondeterministic;
use sui_protocol_config::SupportedProtocolVersions;
use sui_storage::IndexStore;
use sui_types::base_types::{AuthorityName, ObjectID};
use sui_types::committee::Committee;
use sui_types::crypto::AuthorityKeyPair;
use sui_types::messages::{VerifiedExecutableTransaction, VerifiedTransaction};
use sui_types::object::Object;

pub struct TestAuthorityBuilder {
    // TODO: Add more configurable fields.
    store_base_path: PathBuf,
}

impl Default for TestAuthorityBuilder {
    fn default() -> Self {
        let dir = std::env::temp_dir();
        let store_base_path = dir.join(format!("DB_{:?}", nondeterministic!(ObjectID::random())));
        std::fs::create_dir(&store_base_path).unwrap();
        Self { store_base_path }
    }
}

impl TestAuthorityBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_store_base_path(mut self, path: PathBuf) -> Self {
        self.store_base_path = path;
        self
    }

    // TODO: Figure out why we could not derive genesis_committee from genesis.
    pub async fn build(
        self,
        genesis_committee: Committee,
        key: &AuthorityKeyPair,
        genesis: &Genesis,
    ) -> Arc<AuthorityState> {
        // unwrap ok - for testing only.
        let store = AuthorityStore::open_with_committee_for_testing(
            &self.store_base_path.join("store"),
            None,
            &genesis_committee,
            genesis,
            0,
        )
        .await
        .unwrap();
        let state = Self::build_with_store(self, genesis_committee, key, store, &[]).await;
        // For any type of local testing that does not actually spawn a node, the checkpoint executor
        // won't be started, which means we won't actually execute the genesis transaction. In that case,
        // the genesis objects (e.g. all the genesis test coins) won't be accessible. Executing it
        // explicitly makes sure all genesis objects are ready for use.
        state
            .try_execute_immediately(
                &VerifiedExecutableTransaction::new_from_checkpoint(
                    VerifiedTransaction::new_unchecked(genesis.transaction().clone()),
                    genesis.epoch(),
                    genesis.checkpoint().sequence_number,
                ),
                None,
                &state.epoch_store_for_testing(),
            )
            .await
            .unwrap();
        state
    }

    pub async fn build_with_store(
        self,
        genesis_committee: Committee,
        key: &AuthorityKeyPair,
        authority_store: Arc<AuthorityStore>,
        genesis_objects: &[Object],
    ) -> Arc<AuthorityState> {
        let secret = Arc::pin(key.copy());
        let name: AuthorityName = secret.public().into();
        let path = self.store_base_path;
        let registry = Registry::new();
        let cache_metrics = Arc::new(ResolverMetrics::new(&registry));
        let signature_verifier_metrics = SignatureVerifierMetrics::new(&registry);
        let epoch_store = AuthorityPerEpochStore::new(
            name,
            Arc::new(genesis_committee.clone()),
            &path.join("store"),
            None,
            EpochMetrics::new(&registry),
            EpochStartConfiguration::new_for_testing(),
            authority_store.clone(),
            cache_metrics,
            signature_verifier_metrics,
            &ExpensiveSafetyCheckConfig::default(),
        );

        let committee_store = Arc::new(CommitteeStore::new(
            path.join("epochs"),
            &genesis_committee,
            None,
        ));

        let checkpoint_store = CheckpointStore::new(&path.join("checkpoints"));
        let index_store = Some(Arc::new(IndexStore::new(path.join("indexes"), &registry)));
        AuthorityState::new(
            name,
            secret,
            SupportedProtocolVersions::SYSTEM_DEFAULT,
            authority_store,
            epoch_store,
            committee_store,
            index_store,
            checkpoint_store,
            &registry,
            AuthorityStorePruningConfig::default(),
            genesis_objects,
            &DBCheckpointConfig::default(),
            ExpensiveSafetyCheckConfig::new_enable_all(),
        )
        .await
    }
}
