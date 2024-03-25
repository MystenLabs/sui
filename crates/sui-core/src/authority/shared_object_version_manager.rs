// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority::epoch_start_configuration::EpochStartConfigTrait;
use crate::authority::AuthorityPerEpochStore;
use crate::execution_cache::ExecutionCacheRead;
use std::collections::HashMap;
use sui_types::crypto::RandomnessRound;
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};
use sui_types::executable_transaction::VerifiedExecutableTransaction;
use sui_types::storage::{
    transaction_input_object_keys, transaction_receiving_object_keys, ObjectKey,
};
use sui_types::transaction::{
    SenderSignedData, SharedInputObject, TransactionDataAPI, TransactionKey,
};
use sui_types::{
    base_types::{ObjectID, SequenceNumber},
    error::SuiResult,
    SUI_RANDOMNESS_STATE_OBJECT_ID,
};
use tracing::{debug, trace};

pub struct SharedObjVerManager {}

pub type AssignedTxAndVersions = Vec<(TransactionKey, Vec<(ObjectID, SequenceNumber)>)>;

#[must_use]
#[derive(Default)]
pub struct ConsensusSharedObjVerAssignment {
    pub shared_input_next_versions: HashMap<ObjectID, SequenceNumber>,
    pub assigned_versions: AssignedTxAndVersions,
}

impl SharedObjVerManager {
    pub async fn assign_versions_from_consensus(
        epoch_store: &AuthorityPerEpochStore,
        cache_reader: &dyn ExecutionCacheRead,
        certificates: &[VerifiedExecutableTransaction],
        randomness_round: Option<RandomnessRound>,
    ) -> SuiResult<ConsensusSharedObjVerAssignment> {
        let mut shared_input_next_versions = get_or_init_versions(
            certificates.iter().map(|cert| cert.data()),
            epoch_store,
            cache_reader,
            randomness_round.is_some(),
        )
        .await?;
        let mut assigned_versions = Vec::new();
        // We must update randomness object version first before processing any transaction,
        // so that all reads are using the next version.
        // TODO: Add a test that actually check this, i.e. if we change the order, some test should fail.
        if let Some(round) = randomness_round {
            // If we're generating randomness, update the randomness state object version.
            let version = shared_input_next_versions
                .get_mut(&SUI_RANDOMNESS_STATE_OBJECT_ID)
                .expect("randomness state object must have been added in get_or_init_versions()");
            debug!("assigning shared object versions for randomness: epoch {}, round {round:?} -> version {version:?}", epoch_store.epoch());
            assigned_versions.push((
                TransactionKey::RandomnessRound(epoch_store.epoch(), round),
                vec![(SUI_RANDOMNESS_STATE_OBJECT_ID, *version)],
            ));
            version.increment();
        }
        for cert in certificates {
            if !cert.contains_shared_object() {
                continue;
            }
            let cert_assigned_versions =
                assign_versions_for_certificate(cert, &mut shared_input_next_versions);
            assigned_versions.push((cert.key(), cert_assigned_versions));
        }

        Ok(ConsensusSharedObjVerAssignment {
            shared_input_next_versions,
            assigned_versions,
        })
    }

    pub async fn assign_versions_from_effects(
        certs_and_effects: &[(&VerifiedExecutableTransaction, &TransactionEffects)],
        epoch_store: &AuthorityPerEpochStore,
        cache_reader: &dyn ExecutionCacheRead,
    ) -> SuiResult<AssignedTxAndVersions> {
        // We don't care about the results since we can use effects to assign versions.
        // But we must call it to make sure whenever a shared object is touched the first time
        // during an epoch, either through consensus or through checkpoint executor,
        // its next version must be initialized. This is because we initialize the next version
        // of a shared object in an epoch by reading the current version from the object store.
        // This must be done before we mutate it the first time, otherwise we would be initializing
        // it with the wrong version.
        let _ = get_or_init_versions(
            certs_and_effects.iter().map(|(cert, _)| cert.data()),
            epoch_store,
            cache_reader,
            false,
        )
        .await?;
        let mut assigned_versions = Vec::new();
        for (cert, effects) in certs_and_effects {
            let cert_assigned_versions: Vec<_> = effects
                .input_shared_objects()
                .into_iter()
                .map(|iso| iso.id_and_version())
                .collect();
            let tx_key = cert.key();
            trace!(
                ?tx_key,
                ?cert_assigned_versions,
                "locking shared objects from effects"
            );
            assigned_versions.push((tx_key, cert_assigned_versions));
        }
        Ok(assigned_versions)
    }
}

async fn get_or_init_versions(
    transactions: impl Iterator<Item = &SenderSignedData>,
    epoch_store: &AuthorityPerEpochStore,
    cache_reader: &dyn ExecutionCacheRead,
    generate_randomness: bool,
) -> SuiResult<HashMap<ObjectID, SequenceNumber>> {
    let mut shared_input_objects: Vec<_> = transactions
        .flat_map(|tx| {
            tx.transaction_data()
                .shared_input_objects()
                .into_iter()
                .map(|so| so.into_id_and_version())
        })
        .collect();

    if generate_randomness {
        shared_input_objects.push((
            SUI_RANDOMNESS_STATE_OBJECT_ID,
            epoch_store
                .epoch_start_config()
                .randomness_obj_initial_shared_version()
                .expect(
                    "randomness_obj_initial_shared_version should exist if randomness is enabled",
                ),
        ));
    }

    shared_input_objects.sort();
    shared_input_objects.dedup();

    epoch_store
        .get_or_init_next_object_versions(&shared_input_objects, cache_reader)
        .await
}

fn assign_versions_for_certificate(
    cert: &VerifiedExecutableTransaction,
    shared_input_next_versions: &mut HashMap<ObjectID, SequenceNumber>,
) -> Vec<(ObjectID, SequenceNumber)> {
    let tx_digest = cert.digest();

    // Make an iterator to update the locks of the transaction's shared objects.
    let shared_input_objects: Vec<_> = cert.shared_input_objects().collect();

    let mut input_object_keys =
        transaction_input_object_keys(cert).expect("Transaction input should have been verified");
    let mut assigned_versions = Vec::with_capacity(shared_input_objects.len());
    let mut is_mutable_input = Vec::with_capacity(shared_input_objects.len());
    // Record receiving object versions towards the shared version computation.
    let receiving_object_keys = transaction_receiving_object_keys(cert);
    input_object_keys.extend(receiving_object_keys);

    for (SharedInputObject { id, mutable, .. }, version) in shared_input_objects
        .iter()
        .map(|obj| (obj, *shared_input_next_versions.get(&obj.id()).unwrap()))
    {
        assigned_versions.push((*id, version));
        input_object_keys.push(ObjectKey(*id, version));
        is_mutable_input.push(*mutable);
    }

    let next_version = SequenceNumber::lamport_increment(input_object_keys.iter().map(|obj| obj.1));

    // Update the next version for the shared objects.
    assigned_versions
        .iter()
        .zip(is_mutable_input)
        .filter_map(|((id, _), mutable)| {
            if mutable {
                Some((*id, next_version))
            } else {
                None
            }
        })
        .for_each(|(id, version)| {
            shared_input_next_versions.insert(id, version);
        });

    trace!(
        ?tx_digest,
        ?assigned_versions,
        ?next_version,
        "locking shared objects"
    );

    assigned_versions
}
