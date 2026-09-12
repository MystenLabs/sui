// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use mysten_common::ZipDebugEqIteratorExt;
use mysten_common::debug_fatal;

use crate::authority::AuthorityPerEpochStore;
use crate::authority::authority_per_epoch_store::CancelConsensusCertificateReason;
use crate::execution_cache::ObjectCacheRead;
use either::Either;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;
use sui_types::base_types::ConsensusObjectSequenceKey;
use sui_types::base_types::ConsensusObjectVersion;
use sui_types::base_types::SystemObjectVersions;
use sui_types::base_types::TransactionDigest;
use sui_types::committee::EpochId;
use sui_types::crypto::RandomnessRound;
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};
use sui_types::executable_transaction::VerifiedExecutableTransaction;
use sui_types::executable_transaction::VerifiedExecutableTransactionWithAliases;
use sui_types::storage::{
    ObjectKey, transaction_non_shared_input_object_keys, transaction_receiving_object_keys,
};
use sui_types::transaction::SharedObjectMutability;
use sui_types::transaction::{SharedInputObject, TransactionDataAPI, TransactionKey};
use sui_types::{
    IMPLICITLY_READ_SYSTEM_OBJECTS, SUI_RANDOMNESS_STATE_OBJECT_ID, base_types::SequenceNumber,
    error::SuiResult,
};
use sui_types::{
    SUI_ACCUMULATOR_ROOT_OBJECT_ID, SUI_CLOCK_OBJECT_ID, SUI_CLOCK_OBJECT_SHARED_VERSION,
    SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID,
};
use tracing::trace;

pub struct SharedObjVerManager {}

/// Version assignments for a single transaction
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssignedVersions {
    pub shared_object_versions: Vec<(ConsensusObjectSequenceKey, SequenceNumber)>,
    /// Versions of system objects, keyed by object ID, that this transaction may read during
    /// execution. Each version is assigned deterministically during consensus sequencing, so that
    /// every validator reads the same version of the object.
    ///
    /// The accumulator root is always present while accumulator settlement is enabled. The
    /// forwarding-address registry records the same effective input as
    /// `shared_object_versions` for eligible transactions, whether it was implicit or declared.
    /// Settlements and cancellations do not read it.
    pub system_object_versions: SystemObjectVersions,
}

impl AssignedVersions {
    pub fn new(
        shared_object_versions: Vec<(ConsensusObjectSequenceKey, SequenceNumber)>,
        system_object_versions: SystemObjectVersions,
    ) -> Self {
        if let Some(registry_version) =
            system_object_versions.get(&SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID)
        {
            let registry_assignment = shared_object_versions
                .iter()
                .find(|((id, _), _)| *id == SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID);
            if registry_assignment
                != Some(&(
                    (
                        SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID,
                        registry_version.initial_shared_version,
                    ),
                    registry_version.version,
                ))
            {
                debug_fatal!(
                    "forwarding registry assignment {registry_assignment:?} differs from system version {registry_version:?}"
                );
            }
        }
        Self {
            shared_object_versions,
            system_object_versions,
        }
    }

    pub fn empty() -> Self {
        Self::new(vec![], SystemObjectVersions::empty())
    }

    /// Construct the system-object versions used by tests running at the latest protocol version.
    #[cfg(test)]
    pub fn new_for_testing(
        shared_object_versions: Vec<(ConsensusObjectSequenceKey, SequenceNumber)>,
        accumulator_version: Option<SequenceNumber>,
    ) -> Self {
        let forwarding_address_registry_version =
            shared_object_versions
                .iter()
                .find_map(|((id, initial_shared_version), version)| {
                    (*id == SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID).then_some(
                        ConsensusObjectVersion {
                            initial_shared_version: *initial_shared_version,
                            version: *version,
                        },
                    )
                });
        Self::new(
            shared_object_versions,
            SystemObjectVersions::new(
                accumulator_version.map(|v| ConsensusObjectVersion {
                    initial_shared_version: sui_types::object::OBJECT_START_VERSION,
                    version: v,
                }),
                forwarding_address_registry_version,
            ),
        )
    }

    /// The accumulator root version this transaction reads, if any.
    pub fn accumulator_version(&self) -> Option<SequenceNumber> {
        self.system_object_versions
            .get(&SUI_ACCUMULATOR_ROOT_OBJECT_ID)
            .map(|v| v.version)
    }

    pub fn iter(&self) -> impl Iterator<Item = &(ConsensusObjectSequenceKey, SequenceNumber)> {
        self.shared_object_versions.iter()
    }

    pub fn as_slice(&self) -> &[(ConsensusObjectSequenceKey, SequenceNumber)] {
        &self.shared_object_versions
    }
}

#[derive(Default, Debug, PartialEq, Eq)]
pub struct AssignedTxAndVersions(pub Vec<(TransactionKey, AssignedVersions)>);

impl AssignedTxAndVersions {
    pub fn new(assigned_versions: Vec<(TransactionKey, AssignedVersions)>) -> Self {
        Self(assigned_versions)
    }

    pub fn into_map(self) -> HashMap<TransactionKey, AssignedVersions> {
        self.0.into_iter().collect()
    }
}

/// A wrapper around things that can be scheduled for execution by the assigning of
/// shared object versions.
#[derive(Clone)]
pub enum Schedulable<T = VerifiedExecutableTransaction> {
    Transaction(T),
    RandomnessStateUpdate(EpochId, RandomnessRound),
    AccumulatorSettlement(EpochId, u64 /* checkpoint height */),
    ConsensusCommitPrologue(EpochId, u64 /* round */, u32 /* sub_dag_index */),
}

impl From<VerifiedExecutableTransaction> for Schedulable<VerifiedExecutableTransaction> {
    fn from(tx: VerifiedExecutableTransaction) -> Self {
        Schedulable::Transaction(tx)
    }
}

impl From<Schedulable<VerifiedExecutableTransactionWithAliases>>
    for Schedulable<VerifiedExecutableTransaction>
{
    fn from(schedulable: Schedulable<VerifiedExecutableTransactionWithAliases>) -> Self {
        match schedulable {
            Schedulable::Transaction(tx) => Schedulable::Transaction(tx.into_tx()),
            Schedulable::RandomnessStateUpdate(epoch, round) => {
                Schedulable::RandomnessStateUpdate(epoch, round)
            }
            Schedulable::AccumulatorSettlement(epoch, checkpoint_height) => {
                Schedulable::AccumulatorSettlement(epoch, checkpoint_height)
            }
            Schedulable::ConsensusCommitPrologue(epoch, round, sub_dag_index) => {
                Schedulable::ConsensusCommitPrologue(epoch, round, sub_dag_index)
            }
        }
    }
}

// AsTx is like Deref, in that it allows us to use either refs or values in Schedulable.
// Deref does not work because it conflicts with the impl of Deref for VerifiedExecutableTransaction.
pub trait AsTx {
    fn as_tx(&self) -> &VerifiedExecutableTransaction;
}

impl AsTx for VerifiedExecutableTransaction {
    fn as_tx(&self) -> &VerifiedExecutableTransaction {
        self
    }
}

impl AsTx for &'_ VerifiedExecutableTransaction {
    fn as_tx(&self) -> &VerifiedExecutableTransaction {
        self
    }
}

impl AsTx for VerifiedExecutableTransactionWithAliases {
    fn as_tx(&self) -> &VerifiedExecutableTransaction {
        self.tx()
    }
}

impl AsTx for &'_ VerifiedExecutableTransactionWithAliases {
    fn as_tx(&self) -> &VerifiedExecutableTransaction {
        self.tx()
    }
}

impl Schedulable<&'_ VerifiedExecutableTransaction> {
    // Cannot use the blanket ToOwned trait impl because it just calls clone.
    pub fn to_owned_schedulable(&self) -> Schedulable<VerifiedExecutableTransaction> {
        match self {
            Schedulable::Transaction(tx) => Schedulable::Transaction((*tx).clone()),
            Schedulable::RandomnessStateUpdate(epoch, round) => {
                Schedulable::RandomnessStateUpdate(*epoch, *round)
            }
            Schedulable::AccumulatorSettlement(epoch, checkpoint_height) => {
                Schedulable::AccumulatorSettlement(*epoch, *checkpoint_height)
            }
            Schedulable::ConsensusCommitPrologue(epoch, round, sub_dag_index) => {
                Schedulable::ConsensusCommitPrologue(*epoch, *round, *sub_dag_index)
            }
        }
    }
}

impl<T> Schedulable<T> {
    pub fn as_tx(&self) -> Option<&VerifiedExecutableTransaction>
    where
        T: AsTx,
    {
        match self {
            Schedulable::Transaction(tx) => Some(tx.as_tx()),
            Schedulable::RandomnessStateUpdate(_, _) => None,
            Schedulable::AccumulatorSettlement(_, _) => None,
            Schedulable::ConsensusCommitPrologue(_, _, _) => None,
        }
    }

    pub fn shared_input_objects(
        &self,
        epoch_store: &AuthorityPerEpochStore,
    ) -> impl Iterator<Item = SharedInputObject> + '_
    where
        T: AsTx,
    {
        match self {
            Schedulable::Transaction(tx) => Either::Left(tx.as_tx().shared_input_objects()),
            Schedulable::RandomnessStateUpdate(_, _) => {
                Either::Right(std::iter::once(SharedInputObject {
                    id: SUI_RANDOMNESS_STATE_OBJECT_ID,
                    initial_shared_version: epoch_store
                        .epoch_start_config()
                        .randomness_obj_initial_shared_version()
                        .expect("randomness obj initial shared version should be set"),
                    mutability: SharedObjectMutability::Mutable,
                }))
            }
            Schedulable::AccumulatorSettlement(_, _) => {
                Either::Right(std::iter::once(SharedInputObject {
                    id: SUI_ACCUMULATOR_ROOT_OBJECT_ID,
                    initial_shared_version: epoch_store
                        .epoch_start_config()
                        .accumulator_root_obj_initial_shared_version()
                        .expect("accumulator root obj initial shared version should be set"),
                    mutability: SharedObjectMutability::Mutable,
                }))
            }
            Schedulable::ConsensusCommitPrologue(_, _, _) => {
                Either::Right(std::iter::once(SharedInputObject {
                    id: SUI_CLOCK_OBJECT_ID,
                    initial_shared_version: SUI_CLOCK_OBJECT_SHARED_VERSION,
                    mutability: SharedObjectMutability::Mutable,
                }))
            }
        }
    }

    pub fn non_shared_input_object_keys(&self) -> Vec<ObjectKey>
    where
        T: AsTx,
    {
        match self {
            Schedulable::Transaction(tx) => transaction_non_shared_input_object_keys(tx.as_tx())
                .expect("Transaction input should have been verified"),
            Schedulable::RandomnessStateUpdate(_, _) => vec![],
            Schedulable::AccumulatorSettlement(_, _) => vec![],
            Schedulable::ConsensusCommitPrologue(_, _, _) => vec![],
        }
    }

    pub fn receiving_object_keys(&self) -> Vec<ObjectKey>
    where
        T: AsTx,
    {
        match self {
            Schedulable::Transaction(tx) => transaction_receiving_object_keys(tx.as_tx()),
            Schedulable::RandomnessStateUpdate(_, _) => vec![],
            Schedulable::AccumulatorSettlement(_, _) => vec![],
            Schedulable::ConsensusCommitPrologue(_, _, _) => vec![],
        }
    }

    pub fn key(&self) -> TransactionKey
    where
        T: AsTx,
    {
        match self {
            Schedulable::Transaction(tx) => tx.as_tx().key(),
            Schedulable::RandomnessStateUpdate(epoch, round) => {
                TransactionKey::RandomnessRound(*epoch, *round)
            }
            Schedulable::AccumulatorSettlement(epoch, checkpoint_height) => {
                TransactionKey::AccumulatorSettlement(*epoch, *checkpoint_height)
            }
            Schedulable::ConsensusCommitPrologue(epoch, round, sub_dag_index) => {
                TransactionKey::ConsensusCommitPrologue(*epoch, *round, *sub_dag_index)
            }
        }
    }
}

#[must_use]
#[derive(Default, Eq, PartialEq, Debug)]
pub struct ConsensusSharedObjVerAssignment {
    pub shared_input_next_versions: HashMap<ConsensusObjectSequenceKey, SequenceNumber>,
    pub assigned_versions: AssignedTxAndVersions,
}

impl SharedObjVerManager {
    pub fn assign_versions_from_consensus<'a, T>(
        epoch_store: &AuthorityPerEpochStore,
        cache_reader: &dyn ObjectCacheRead,
        assignables: impl Iterator<Item = &'a Schedulable<T>> + Clone,
        cancelled_txns: &BTreeMap<TransactionDigest, CancelConsensusCertificateReason>,
    ) -> SuiResult<ConsensusSharedObjVerAssignment>
    where
        T: AsTx + 'a,
    {
        let mut shared_input_next_versions = get_or_init_versions(
            assignables
                .clone()
                .flat_map(|a| a.shared_input_objects(epoch_store)),
            epoch_store,
            cache_reader,
        )?;
        let mut assigned_versions = Vec::new();
        for assignable in assignables {
            assert!(
                !matches!(assignable, Schedulable::AccumulatorSettlement(_, _))
                    || epoch_store.accumulators_enabled(),
                "AccumulatorSettlement should not be scheduled when accumulators are disabled"
            );

            let cert_assigned_versions = Self::assign_versions_for_certificate(
                epoch_store,
                assignable,
                &mut shared_input_next_versions,
                cancelled_txns,
            );
            assigned_versions.push((assignable.key(), cert_assigned_versions));
        }

        Ok(ConsensusSharedObjVerAssignment {
            shared_input_next_versions,
            assigned_versions: AssignedTxAndVersions::new(assigned_versions),
        })
    }

    pub fn assign_versions_from_effects(
        certs_and_effects: &[(
            &VerifiedExecutableTransaction,
            &TransactionEffects,
            // Accumulator version
            Option<SequenceNumber>,
        )],
        epoch_store: &AuthorityPerEpochStore,
        cache_reader: &dyn ObjectCacheRead,
    ) -> AssignedTxAndVersions {
        // We don't care about the results since we can use effects to assign versions.
        // But we must call it to make sure whenever a consensus object is touched the first time
        // during an epoch, either through consensus or through checkpoint executor,
        // its next version must be initialized. This is because we initialize the next version
        // of a consensus object in an epoch by reading the current version from the object store.
        // This must be done before we mutate it the first time, otherwise we would be initializing
        // it with the wrong version.
        let _ = get_or_init_versions(
            certs_and_effects.iter().flat_map(|(cert, _, _)| {
                cert.transaction_data().shared_input_objects().into_iter()
            }),
            epoch_store,
            cache_reader,
        );
        let mut assigned_versions = Vec::new();
        for (cert, effects, accumulator_version) in certs_and_effects {
            let declared_shared_inputs = cert.transaction_data().shared_input_objects();
            let declared_initial_versions: BTreeMap<_, _> = declared_shared_inputs
                .iter()
                .map(|input| input.id_and_version())
                .collect();
            let mut accessed_versions = BTreeMap::new();
            for accessed_object in effects.accessed_consensus_objects() {
                let (id, version) = accessed_object.id_and_version();
                let previous_version = accessed_versions.insert(id, version);
                if id == SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID && previous_version.is_some() {
                    debug_fatal!(
                        "forwarding address registry is assigned more than once in effects for tx {:?}",
                        cert.digest()
                    );
                }
            }
            let mut cert_assigned_versions: Vec<_> = declared_shared_inputs
                .iter()
                .filter_map(|input| {
                    accessed_versions
                        .get(&input.id)
                        .map(|version| (input.id_and_version(), *version))
                })
                .collect();
            let forwarding_address_registry_initial_version = epoch_store
                .protocol_config()
                .enable_forwarding_addresses()
                .then(|| {
                    epoch_store
                        .epoch_start_config()
                        .forwarding_address_registry_obj_initial_shared_version()
                })
                .flatten();
            if !declared_initial_versions.contains_key(&SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID)
                && let Some(version) =
                    accessed_versions.get(&SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID)
                && !version.is_cancelled()
            {
                let initial_shared_version = forwarding_address_registry_initial_version.expect(
                    "forwarding address registry initial version must be known when it is accessed",
                );
                cert_assigned_versions.push((
                    (
                        SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID,
                        initial_shared_version,
                    ),
                    *version,
                ));
            }
            for id in accessed_versions.keys() {
                if !declared_initial_versions.contains_key(id)
                    && *id != SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID
                {
                    debug_assert!(
                        IMPLICITLY_READ_SYSTEM_OBJECTS.contains(id),
                        "accessed consensus object is neither a declared input nor a known implicitly read system object: \
                         accessed={accessed_versions:?} declared={declared_initial_versions:?}"
                    );
                }
            }
            if let (Some(effects_version), Some(sequenced_version)) = (
                accessed_versions.get(&SUI_ACCUMULATOR_ROOT_OBJECT_ID),
                accumulator_version,
            ) && !effects_version.is_cancelled()
                && effects_version != sequenced_version
            {
                debug_fatal!(
                    "accumulator root version from effects {:?} disagrees \
                        with the reconstructed accumulator version {:?} for tx {:?}",
                    effects_version,
                    sequenced_version,
                    cert.digest()
                );
            }
            // The accumulator version is still supplied by the checkpoint executor for legacy
            // object-funds withdrawals. Forwarding registry metadata is derived from its single
            // effective assignment so replay matches consensus sequencing.
            let accumulator_version = accumulator_version.map(|version| {
                let initial_shared_version = epoch_store
                    .epoch_start_config()
                    .accumulator_root_obj_initial_shared_version()
                    .expect(
                        "initial shared version must be known for an implicitly read system object",
                    );
                ConsensusObjectVersion {
                    initial_shared_version,
                    version,
                }
            });
            let tx_key = cert.key();
            let system_object_versions = system_object_versions_from_assigned(
                accumulator_version,
                &cert_assigned_versions,
                forwarding_address_registry_initial_version,
                &tx_key,
            );
            trace!(
                ?tx_key,
                ?cert_assigned_versions,
                ?system_object_versions,
                "assigned consensus object versions from effects"
            );
            assigned_versions.push((
                tx_key,
                AssignedVersions::new(cert_assigned_versions, system_object_versions),
            ));
        }
        AssignedTxAndVersions::new(assigned_versions)
    }

    pub fn assign_versions_for_certificate(
        epoch_store: &AuthorityPerEpochStore,
        assignable: &Schedulable<impl AsTx>,
        shared_input_next_versions: &mut HashMap<ConsensusObjectSequenceKey, SequenceNumber>,
        cancelled_txns: &BTreeMap<TransactionDigest, CancelConsensusCertificateReason>,
    ) -> AssignedVersions {
        let mut shared_input_objects: Vec<_> =
            assignable.shared_input_objects(epoch_store).collect();

        let accumulator_version = if epoch_store.accumulators_enabled() {
            let accumulator_initial_version = epoch_store
                .epoch_start_config()
                .accumulator_root_obj_initial_shared_version()
                .expect("accumulator root obj initial shared version should be set when accumulators are enabled");

            let accumulator_version = *shared_input_next_versions
                .get(&(SUI_ACCUMULATOR_ROOT_OBJECT_ID, accumulator_initial_version))
                .expect("accumulator object must be in shared_input_next_versions when withdraws are enabled");

            Some(ConsensusObjectVersion {
                initial_shared_version: accumulator_initial_version,
                version: accumulator_version,
            })
        } else {
            None
        };

        let tx_key = assignable.key();
        // Check if the transaction is cancelled due to congestion.
        let cancellation_info = tx_key
            .as_digest()
            .and_then(|tx_digest| cancelled_txns.get(tx_digest));
        let congested_objects_info: Option<HashSet<_>> =
            if let Some(CancelConsensusCertificateReason::CongestionOnObjects(congested_objects)) =
                &cancellation_info
            {
                Some(congested_objects.iter().cloned().collect())
            } else {
                None
            };
        let txn_cancelled = cancellation_info.is_some();

        let forwarding_address_registry_initial_version = epoch_store
            .protocol_config()
            .enable_forwarding_addresses()
            .then(|| {
                epoch_store
                    .epoch_start_config()
                    .forwarding_address_registry_obj_initial_shared_version()
            })
            .flatten()
            // Settlement batches and barriers advance only the accumulator's clock. Inheriting
            // a higher registry version would violate their consecutive-version contract.
            .filter(|_| match assignable {
                Schedulable::AccumulatorSettlement(..) => false,
                Schedulable::Transaction(tx) => !tx
                    .as_tx()
                    .transaction_data()
                    .kind()
                    .is_accumulator_settle_tx(),
                Schedulable::RandomnessStateUpdate(..)
                | Schedulable::ConsensusCommitPrologue(..) => true,
            });
        if !txn_cancelled
            && let Some(initial_shared_version) = forwarding_address_registry_initial_version
            && !shared_input_objects
                .iter()
                .any(|input| input.id == SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID)
        {
            shared_input_objects.push(SharedInputObject {
                id: SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID,
                initial_shared_version,
                mutability: SharedObjectMutability::Immutable,
            });
        }

        if shared_input_objects.is_empty() {
            // No shared object used by this transaction. No need to assign versions.
            return AssignedVersions::new(
                vec![],
                system_object_versions_from_assigned(
                    accumulator_version,
                    &[],
                    forwarding_address_registry_initial_version,
                    &tx_key,
                ),
            );
        }

        let mut input_object_keys = assignable.non_shared_input_object_keys();
        let mut assigned_versions = Vec::with_capacity(shared_input_objects.len());
        let mut is_exclusively_accessed_input = Vec::with_capacity(shared_input_objects.len());
        // Record receiving object versions towards the shared version computation.
        let receiving_object_keys = assignable.receiving_object_keys();
        input_object_keys.extend(receiving_object_keys);

        if txn_cancelled {
            // For cancelled transaction due to congestion, assign special versions to all shared objects.
            // Note that new lamport version does not depend on any shared objects.
            for SharedInputObject {
                id,
                initial_shared_version,
                ..
            } in shared_input_objects.iter()
            {
                let assigned_version = match cancellation_info {
                    Some(CancelConsensusCertificateReason::CongestionOnObjects(_)) => {
                        if congested_objects_info
                            .as_ref()
                            .is_some_and(|info| info.contains(id))
                        {
                            SequenceNumber::CONGESTED
                        } else {
                            SequenceNumber::CANCELLED_READ
                        }
                    }
                    Some(CancelConsensusCertificateReason::DkgFailed) => {
                        if id == &SUI_RANDOMNESS_STATE_OBJECT_ID {
                            SequenceNumber::RANDOMNESS_UNAVAILABLE
                        } else {
                            SequenceNumber::CANCELLED_READ
                        }
                    }
                    None => unreachable!("cancelled transaction should have cancellation info"),
                };
                assigned_versions.push(((*id, *initial_shared_version), assigned_version));
                is_exclusively_accessed_input.push(false);
            }
        } else {
            for (
                SharedInputObject {
                    id,
                    initial_shared_version,
                    mutability,
                },
                assigned_version,
            ) in shared_input_objects.iter().map(|obj| {
                (
                    obj,
                    *shared_input_next_versions
                        .get(&obj.id_and_version())
                        .unwrap(),
                )
            }) {
                assigned_versions.push(((*id, *initial_shared_version), assigned_version));
                input_object_keys.push(ObjectKey(*id, assigned_version));
                is_exclusively_accessed_input.push(mutability.is_exclusive());
            }
        }

        let next_version =
            SequenceNumber::lamport_increment(input_object_keys.iter().map(|obj| obj.1));
        assert!(
            next_version.is_valid(),
            "Assigned version must be valid. Got {:?}",
            next_version
        );

        if !txn_cancelled {
            // Update the next version for the shared objects.
            assigned_versions
                .iter()
                .zip_debug_eq(is_exclusively_accessed_input)
                .filter_map(|((id, _), mutable)| {
                    if mutable {
                        Some((*id, next_version))
                    } else {
                        None
                    }
                })
                .for_each(|(id, version)| {
                    assert!(
                        version.is_valid(),
                        "Assigned version must be a valid version."
                    );
                    shared_input_next_versions
                        .insert(id, version)
                        .expect("Object must exist in shared_input_next_versions.");
                });
        }

        let system_object_versions = system_object_versions_from_assigned(
            accumulator_version,
            &assigned_versions,
            forwarding_address_registry_initial_version,
            &tx_key,
        );
        trace!(
            ?tx_key,
            ?assigned_versions,
            ?next_version,
            ?txn_cancelled,
            "locking shared objects"
        );

        AssignedVersions::new(assigned_versions, system_object_versions)
    }
}

fn system_object_versions_from_assigned(
    accumulator_version: Option<ConsensusObjectVersion>,
    assigned_versions: &[(ConsensusObjectSequenceKey, SequenceNumber)],
    forwarding_address_registry_initial_version: Option<SequenceNumber>,
    tx_key: &TransactionKey,
) -> SystemObjectVersions {
    let Some(expected_initial_version) = forwarding_address_registry_initial_version else {
        return SystemObjectVersions::new(accumulator_version, None);
    };
    let mut registry_assignments = assigned_versions
        .iter()
        .filter(|((object_id, _), _)| *object_id == SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID);
    let forwarding_address_registry_version = registry_assignments.next().and_then(
        |((_, initial_shared_version), version)| {
            if let Some(((_, duplicate_initial_shared_version), duplicate_version)) =
                registry_assignments.next()
            {
                debug_fatal!(
                    "forwarding address registry assigned more than once for tx {:?}: \
                     first=({:?}, {:?}), duplicate=({:?}, {:?})",
                    tx_key,
                    initial_shared_version,
                    version,
                    duplicate_initial_shared_version,
                    duplicate_version
                );
            }
            if version.is_cancelled() {
                return None;
            }
            if expected_initial_version != *initial_shared_version {
                debug_fatal!(
                    "forwarding address registry assignment has unexpected initial version for tx {:?}: \
                     assigned={:?}, expected={:?}",
                    tx_key,
                    initial_shared_version,
                    expected_initial_version
                );
            }
            Some(ConsensusObjectVersion {
                initial_shared_version: *initial_shared_version,
                version: *version,
            })
        },
    );
    SystemObjectVersions::new(accumulator_version, forwarding_address_registry_version)
}

fn get_or_init_versions<'a>(
    shared_input_objects: impl Iterator<Item = SharedInputObject> + 'a,
    epoch_store: &AuthorityPerEpochStore,
    cache_reader: &dyn ObjectCacheRead,
) -> SuiResult<HashMap<ConsensusObjectSequenceKey, SequenceNumber>> {
    let mut shared_input_objects: Vec<_> = shared_input_objects
        .map(|so| so.into_id_and_version())
        .collect();

    if epoch_store.accumulators_enabled() {
        shared_input_objects.push((
            SUI_ACCUMULATOR_ROOT_OBJECT_ID,
            epoch_store
                .epoch_start_config()
                .accumulator_root_obj_initial_shared_version()
                .expect("accumulator root obj initial shared version should be set"),
        ));
    }

    if epoch_store.protocol_config().enable_forwarding_addresses()
        && let Some(initial_shared_version) = epoch_store
            .epoch_start_config()
            .forwarding_address_registry_obj_initial_shared_version()
    {
        shared_input_objects.push((
            SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID,
            initial_shared_version,
        ));
    }

    shared_input_objects.sort();
    shared_input_objects.dedup();

    epoch_store.get_or_init_next_object_versions(&shared_input_objects, cache_reader)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::accumulators::build_accumulator_barrier_tx;
    use crate::authority::AuthorityState;
    use crate::authority::authority_test_utils::execute_from_consensus;
    use crate::authority::shared_object_version_manager::{
        ConsensusSharedObjVerAssignment, SharedObjVerManager,
    };
    use crate::authority::test_authority_builder::TestAuthorityBuilder;
    use crate::execution_scheduler::funds_withdraw_scheduler::FundsSettlement;
    use move_core_types::ident_str;
    use std::collections::{BTreeMap, HashMap};
    use std::sync::Arc;
    use sui_protocol_config::ProtocolConfig;
    use sui_test_transaction_builder::TestTransactionBuilder;
    use sui_types::base_types::{ObjectID, SequenceNumber, SuiAddress};
    use sui_types::crypto::{RandomnessRound, get_account_key_pair};
    use sui_types::digests::ObjectDigest;
    use sui_types::effects::TestEffectsBuilder;
    use sui_types::executable_transaction::{
        CertificateProof, ExecutableTransaction, VerifiedExecutableTransaction,
    };

    use sui_types::object::{Object, Owner};
    use sui_types::transaction::{ObjectArg, SenderSignedData, VerifiedTransaction};

    use sui_types::gas_coin::GAS;
    use sui_types::transaction::FundsWithdrawalArg;
    use sui_types::{
        SUI_ACCUMULATOR_ROOT_OBJECT_ID, SUI_FRAMEWORK_PACKAGE_ID, SUI_RANDOMNESS_STATE_OBJECT_ID,
    };

    #[tokio::test]
    async fn test_assign_versions_from_consensus_basic() {
        let shared_object = Object::shared_for_testing();
        let id = shared_object.id();
        let init_shared_version = shared_object.owner.start_version().unwrap();
        let authority = TestAuthorityBuilder::new()
            .with_starting_objects(std::slice::from_ref(&shared_object))
            .build()
            .await;
        let certs = [
            generate_shared_objs_tx_with_gas_version(&[(id, init_shared_version, true)], 3),
            generate_shared_objs_tx_with_gas_version(&[(id, init_shared_version, false)], 5),
            generate_shared_objs_tx_with_gas_version(&[(id, init_shared_version, true)], 9),
            generate_shared_objs_tx_with_gas_version(&[(id, init_shared_version, true)], 11),
        ];
        let epoch_store = authority.epoch_store_for_testing();
        let forwarding_address_registry_initial_version = epoch_store
            .epoch_start_config()
            .forwarding_address_registry_obj_initial_shared_version()
            .unwrap();
        let assignables = certs
            .iter()
            .map(Schedulable::Transaction)
            .collect::<Vec<_>>();
        let ConsensusSharedObjVerAssignment {
            shared_input_next_versions,
            assigned_versions,
        } = SharedObjVerManager::assign_versions_from_consensus(
            &epoch_store,
            authority.get_object_cache_reader().as_ref(),
            assignables.iter(),
            &BTreeMap::new(),
        )
        .unwrap();
        // Check that the shared object's next version is always initialized in the epoch store.
        assert_eq!(
            epoch_store
                .get_next_object_version(&id, init_shared_version)
                .unwrap(),
            init_shared_version
        );
        // Check that the final version of the shared object is the lamport version of the last
        // transaction.
        assert_eq!(
            *shared_input_next_versions
                .get(&(id, init_shared_version))
                .unwrap(),
            SequenceNumber::from_u64(12)
        );
        // Check that the version assignment for each transaction is correct.
        // For a transaction that uses the shared object with mutable=false, it won't update the version
        // using lamport version, hence the next transaction will use the same version number.
        // In the following case, certs[2] has the same assignment as certs[1] for this reason.
        let expected_accumulator_version = SequenceNumber::from_u64(1);
        assert_eq!(
            assigned_versions.0,
            vec![
                (
                    certs[0].key(),
                    AssignedVersions::new_for_testing(
                        vec![
                            ((id, init_shared_version), init_shared_version),
                            (
                                (
                                    SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID,
                                    forwarding_address_registry_initial_version,
                                ),
                                forwarding_address_registry_initial_version,
                            ),
                        ],
                        Some(expected_accumulator_version)
                    )
                ),
                (
                    certs[1].key(),
                    AssignedVersions::new_for_testing(
                        vec![
                            ((id, init_shared_version), SequenceNumber::from_u64(4)),
                            (
                                (
                                    SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID,
                                    forwarding_address_registry_initial_version,
                                ),
                                forwarding_address_registry_initial_version,
                            ),
                        ],
                        Some(expected_accumulator_version)
                    )
                ),
                (
                    certs[2].key(),
                    AssignedVersions::new_for_testing(
                        vec![
                            ((id, init_shared_version), SequenceNumber::from_u64(4)),
                            (
                                (
                                    SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID,
                                    forwarding_address_registry_initial_version,
                                ),
                                forwarding_address_registry_initial_version,
                            ),
                        ],
                        Some(expected_accumulator_version)
                    )
                ),
                (
                    certs[3].key(),
                    AssignedVersions::new_for_testing(
                        vec![
                            ((id, init_shared_version), SequenceNumber::from_u64(10)),
                            (
                                (
                                    SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID,
                                    forwarding_address_registry_initial_version,
                                ),
                                forwarding_address_registry_initial_version,
                            ),
                        ],
                        Some(expected_accumulator_version)
                    )
                ),
            ]
        );
    }

    #[tokio::test]
    async fn test_forwarding_registry_version_follows_consensus_order() {
        let mut config = ProtocolConfig::get_for_max_version_UNSAFE();
        let other_shared_object = ObjectID::random();
        config.set_create_forwarding_address_registry_for_testing(true);
        config.set_enable_forwarding_addresses_for_testing(true);
        let authority = TestAuthorityBuilder::new()
            .with_protocol_config(config)
            .build()
            .await;
        let epoch_store = authority.epoch_store_for_testing();
        let initial_shared_version = epoch_store
            .epoch_start_config()
            .forwarding_address_registry_obj_initial_shared_version()
            .unwrap();
        let certs = [
            generate_shared_objs_tx_with_gas_version(&[], 3),
            generate_shared_objs_tx_with_gas_version(
                &[(
                    SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID,
                    initial_shared_version,
                    true,
                )],
                5,
            ),
            generate_shared_objs_tx_with_gas_version(
                &[(other_shared_object, initial_shared_version, true)],
                1,
            ),
        ];
        let assignables = certs
            .iter()
            .map(Schedulable::Transaction)
            .collect::<Vec<_>>();

        let assignment = SharedObjVerManager::assign_versions_from_consensus(
            &epoch_store,
            authority.get_object_cache_reader().as_ref(),
            assignables.iter(),
            &BTreeMap::new(),
        )
        .unwrap();
        let assigned_registry_versions = assignment
            .assigned_versions
            .0
            .iter()
            .map(|(_, versions)| {
                let registry_assignments: Vec<_> = versions
                    .shared_object_versions
                    .iter()
                    .filter(|((id, _), _)| *id == SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID)
                    .collect();
                assert_eq!(registry_assignments.len(), 1);
                let ((_, assigned_initial_version), assigned_version) = registry_assignments[0];
                let metadata = versions
                    .system_object_versions
                    .get(&SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID)
                    .unwrap();
                assert_eq!(
                    metadata,
                    ConsensusObjectVersion {
                        initial_shared_version: *assigned_initial_version,
                        version: *assigned_version,
                    }
                );
                *assigned_version
            })
            .collect::<Vec<_>>();

        assert_eq!(
            assigned_registry_versions,
            vec![
                initial_shared_version,
                initial_shared_version,
                SequenceNumber::from_u64(6),
            ]
        );
        assert_eq!(
            assignment.shared_input_next_versions.get(&(
                SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID,
                initial_shared_version,
            )),
            Some(&SequenceNumber::from_u64(6))
        );
        assert_eq!(
            assignment
                .shared_input_next_versions
                .get(&(other_shared_object, initial_shared_version)),
            Some(&SequenceNumber::from_u64(7))
        );
    }

    #[tokio::test]
    async fn test_forwarding_registry_does_not_advance_settlement_clock() {
        let (sender, keypair) = get_account_key_pair();
        let gas = Object::with_id_owner_version_for_testing(
            ObjectID::random(),
            SequenceNumber::from_u64(10_000),
            Owner::AddressOwner(sender),
        );
        let mut config = ProtocolConfig::get_for_max_version_UNSAFE();
        config.set_create_forwarding_address_registry_for_testing(true);
        config.set_enable_forwarding_addresses_for_testing(true);
        config.set_forwarding_address_resolve_cost_base_for_testing(52);
        config.set_forwarding_address_resolve_cost_per_byte_for_testing(
            config.obj_access_cost_read_per_byte(),
        );
        let authority = TestAuthorityBuilder::new()
            .with_starting_objects(std::slice::from_ref(&gas))
            .with_protocol_config(config)
            .build()
            .await;
        let epoch_store = authority.epoch_store_for_testing();
        let epoch = epoch_store.epoch();
        let registry_initial_version = epoch_store
            .epoch_start_config()
            .forwarding_address_registry_obj_initial_shared_version()
            .unwrap();
        let accumulator = authority
            .get_object(&SUI_ACCUMULATOR_ROOT_OBJECT_ID)
            .unwrap();
        let accumulator_initial_version = accumulator.owner.start_version().unwrap();
        let mut expected_accumulator_version = accumulator.version();
        let mut builder = TestTransactionBuilder::new(
            sender,
            gas.compute_object_reference(),
            epoch_store.reference_gas_price(),
        );
        let ptb = builder.ptb_builder_mut();
        let registry = ptb
            .obj(ObjectArg::SharedObject {
                id: SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID,
                initial_shared_version: registry_initial_version,
                mutability: SharedObjectMutability::Mutable,
            })
            .unwrap();
        let master_id = ptb.pure(7u64).unwrap();
        ptb.programmable_move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
            ident_str!("forwarding_address").into(),
            ident_str!("register").into(),
            vec![],
            vec![registry, master_id],
        );
        let registration =
            VerifiedExecutableTransaction::new_for_testing(builder.build(), &keypair);
        let barriers = [1, 2].map(|height| {
            VerifiedExecutableTransaction::new_system(
                VerifiedTransaction::new_system_transaction(build_accumulator_barrier_tx(
                    epoch,
                    accumulator_initial_version,
                    height,
                    &[],
                )),
                epoch,
            )
        });
        let assignables = [
            Schedulable::Transaction(registration.clone()),
            Schedulable::AccumulatorSettlement(epoch, 1),
            Schedulable::Transaction(barriers[1].clone()),
        ];
        let assignment = SharedObjVerManager::assign_versions_from_consensus(
            &epoch_store,
            authority.get_object_cache_reader().as_ref(),
            assignables.iter(),
            &BTreeMap::new(),
        )
        .unwrap();
        let mut versions = assignment.assigned_versions.into_map();
        let registration_versions = versions.remove(&registration.key()).unwrap();
        let (effects, _) =
            execute_from_consensus(&authority, registration, registration_versions).await;
        assert!(effects.status().is_ok(), "{effects:?}");
        let registry_version = authority
            .get_object(&SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID)
            .unwrap()
            .version();
        assert!(registry_version > expected_accumulator_version);

        for (barrier, assignable) in barriers.into_iter().zip_debug_eq(&assignables[1..]) {
            let assigned = versions.remove(&assignable.key()).unwrap();
            let (effects, _) = execute_from_consensus(&authority, barrier, assigned).await;
            assert!(effects.status().is_ok(), "{effects:?}");
            let next_accumulator_version = effects
                .mutated()
                .into_iter()
                .find_map(|(object_ref, _)| {
                    (object_ref.0 == SUI_ACCUMULATOR_ROOT_OBJECT_ID).then_some(object_ref.1)
                })
                .unwrap();
            expected_accumulator_version = expected_accumulator_version.next();
            assert_eq!(next_accumulator_version, expected_accumulator_version);
            assert!(effects.accessed_consensus_objects().iter().all(|object| {
                object.id_and_version().0 != SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID
            }));
            authority
                .execution_scheduler
                .settle_address_funds(FundsSettlement {
                    next_accumulator_version,
                    funds_changes: BTreeMap::new(),
                });
        }
        assert_eq!(
            assignment
                .shared_input_next_versions
                .get(&(SUI_ACCUMULATOR_ROOT_OBJECT_ID, accumulator_initial_version)),
            Some(&expected_accumulator_version)
        );
    }

    #[tokio::test]
    async fn test_forwarding_registry_version_reconstructed_from_explicit_input_effects() {
        let mut config = ProtocolConfig::get_for_max_version_UNSAFE();
        config.set_create_forwarding_address_registry_for_testing(true);
        config.set_enable_forwarding_addresses_for_testing(true);
        let authority = TestAuthorityBuilder::new()
            .with_protocol_config(config)
            .build()
            .await;
        let epoch_store = authority.epoch_store_for_testing();
        let initial_shared_version = epoch_store
            .epoch_start_config()
            .forwarding_address_registry_obj_initial_shared_version()
            .unwrap();
        let cert = generate_shared_objs_tx_with_gas_version(
            &[(
                SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID,
                initial_shared_version,
                true,
            )],
            3,
        );
        let effects = TestEffectsBuilder::new(cert.data())
            .with_shared_input_versions(BTreeMap::from([(
                SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID,
                initial_shared_version,
            )]))
            .build();

        let assignment = SharedObjVerManager::assign_versions_from_effects(
            &[(&cert, &effects, None)],
            &epoch_store,
            authority.get_object_cache_reader().as_ref(),
        );
        let (_, assigned) = &assignment.0[0];
        assert_eq!(
            assigned.shared_object_versions,
            vec![(
                (
                    SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID,
                    initial_shared_version,
                ),
                initial_shared_version,
            )]
        );
        assert_eq!(
            assigned
                .system_object_versions
                .get(&SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID),
            Some(ConsensusObjectVersion {
                initial_shared_version,
                version: initial_shared_version,
            })
        );
    }
    #[tokio::test]
    async fn test_forwarding_registry_version_reconstructed_from_implicit_input_effects() {
        let mut config = ProtocolConfig::get_for_max_version_UNSAFE();
        config.set_create_forwarding_address_registry_for_testing(true);
        config.set_enable_forwarding_addresses_for_testing(true);
        let authority = TestAuthorityBuilder::new()
            .with_protocol_config(config)
            .build()
            .await;
        let epoch_store = authority.epoch_store_for_testing();
        let initial_shared_version = epoch_store
            .epoch_start_config()
            .forwarding_address_registry_obj_initial_shared_version()
            .unwrap();
        let cert = generate_shared_objs_tx_with_gas_version(&[], 3);
        let effects = TestEffectsBuilder::new(cert.data())
            .with_shared_input_versions(BTreeMap::from([(
                SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID,
                initial_shared_version,
            )]))
            .build();

        let assignment = SharedObjVerManager::assign_versions_from_effects(
            &[(&cert, &effects, None)],
            &epoch_store,
            authority.get_object_cache_reader().as_ref(),
        );
        let (_, assigned) = &assignment.0[0];
        let live_assignable = Schedulable::Transaction(&cert);
        let live_assignment = SharedObjVerManager::assign_versions_from_consensus(
            &epoch_store,
            authority.get_object_cache_reader().as_ref(),
            std::iter::once(&live_assignable),
            &BTreeMap::new(),
        )
        .unwrap();
        let (_, live_assigned) = &live_assignment.assigned_versions.0[0];
        assert_eq!(
            assigned.shared_object_versions,
            live_assigned.shared_object_versions
        );
        assert_eq!(
            assigned
                .system_object_versions
                .get(&SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID),
            live_assigned
                .system_object_versions
                .get(&SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID)
        );
        assert_eq!(
            assigned.shared_object_versions,
            vec![(
                (
                    SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID,
                    initial_shared_version,
                ),
                initial_shared_version,
            )]
        );
        assert_eq!(
            assigned
                .system_object_versions
                .get(&SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID),
            Some(ConsensusObjectVersion {
                initial_shared_version,
                version: initial_shared_version,
            })
        );
    }

    #[tokio::test]
    async fn test_assign_versions_from_consensus_with_randomness() {
        let authority = TestAuthorityBuilder::new().build().await;
        let epoch_store = authority.epoch_store_for_testing();
        let randomness_obj_version = epoch_store
            .epoch_start_config()
            .randomness_obj_initial_shared_version()
            .unwrap();
        let forwarding_address_registry_initial_version = epoch_store
            .epoch_start_config()
            .forwarding_address_registry_obj_initial_shared_version()
            .unwrap();
        let certs = [
            VerifiedExecutableTransaction::new_system(
                VerifiedTransaction::new_randomness_state_update(
                    epoch_store.epoch(),
                    RandomnessRound::new(1),
                    vec![],
                    randomness_obj_version,
                ),
                epoch_store.epoch(),
            ),
            generate_shared_objs_tx_with_gas_version(
                &[(
                    SUI_RANDOMNESS_STATE_OBJECT_ID,
                    randomness_obj_version,
                    // This can only be false since it's not allowed to use randomness object with mutable=true.
                    false,
                )],
                3,
            ),
            generate_shared_objs_tx_with_gas_version(
                &[(
                    SUI_RANDOMNESS_STATE_OBJECT_ID,
                    randomness_obj_version,
                    false,
                )],
                5,
            ),
        ];
        let assignables = certs
            .iter()
            .map(Schedulable::Transaction)
            .collect::<Vec<_>>();
        let ConsensusSharedObjVerAssignment {
            shared_input_next_versions,
            assigned_versions,
        } = SharedObjVerManager::assign_versions_from_consensus(
            &epoch_store,
            authority.get_object_cache_reader().as_ref(),
            assignables.iter(),
            &BTreeMap::new(),
        )
        .unwrap();
        // Check that the randomness object's next version is initialized.
        assert_eq!(
            epoch_store
                .get_next_object_version(&SUI_RANDOMNESS_STATE_OBJECT_ID, randomness_obj_version)
                .unwrap(),
            randomness_obj_version
        );
        let next_randomness_obj_version = randomness_obj_version.next();
        assert_eq!(
            *shared_input_next_versions
                .get(&(SUI_RANDOMNESS_STATE_OBJECT_ID, randomness_obj_version))
                .unwrap(),
            // Randomness object's version is only incremented by 1 regardless of lamport version.
            next_randomness_obj_version
        );
        let expected_accumulator_version = SequenceNumber::from_u64(1);
        assert_eq!(
            assigned_versions.0,
            vec![
                (
                    certs[0].key(),
                    AssignedVersions::new_for_testing(
                        vec![
                            (
                                (SUI_RANDOMNESS_STATE_OBJECT_ID, randomness_obj_version),
                                randomness_obj_version,
                            ),
                            (
                                (
                                    SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID,
                                    forwarding_address_registry_initial_version,
                                ),
                                forwarding_address_registry_initial_version,
                            ),
                        ],
                        Some(expected_accumulator_version)
                    )
                ),
                (
                    certs[1].key(),
                    // It is critical that the randomness object version is updated before the assignment.
                    AssignedVersions::new_for_testing(
                        vec![
                            (
                                (SUI_RANDOMNESS_STATE_OBJECT_ID, randomness_obj_version),
                                next_randomness_obj_version,
                            ),
                            (
                                (
                                    SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID,
                                    forwarding_address_registry_initial_version,
                                ),
                                forwarding_address_registry_initial_version,
                            ),
                        ],
                        Some(expected_accumulator_version)
                    )
                ),
                (
                    certs[2].key(),
                    // It is critical that the randomness object version is updated before the assignment.
                    AssignedVersions::new_for_testing(
                        vec![
                            (
                                (SUI_RANDOMNESS_STATE_OBJECT_ID, randomness_obj_version),
                                next_randomness_obj_version,
                            ),
                            (
                                (
                                    SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID,
                                    forwarding_address_registry_initial_version,
                                ),
                                forwarding_address_registry_initial_version,
                            ),
                        ],
                        Some(expected_accumulator_version)
                    )
                ),
            ]
        );
    }

    // Tests shared object version assignment for cancelled transaction.
    #[tokio::test]
    async fn test_assign_versions_from_consensus_with_cancellation() {
        let shared_object_1 = Object::shared_for_testing();
        let shared_object_2 = Object::shared_for_testing();
        let id1 = shared_object_1.id();
        let id2 = shared_object_2.id();
        let init_shared_version_1 = shared_object_1.owner.start_version().unwrap();
        let init_shared_version_2 = shared_object_2.owner.start_version().unwrap();
        let authority = TestAuthorityBuilder::new()
            .with_starting_objects(&[shared_object_1.clone(), shared_object_2.clone()])
            .build()
            .await;
        let randomness_obj_version = authority
            .epoch_store_for_testing()
            .epoch_start_config()
            .randomness_obj_initial_shared_version()
            .unwrap();

        // Generate 5 transactions for testing.
        //   tx1: shared_object_1, shared_object_2, owned_object_version = 3
        //   tx2: shared_object_1, shared_object_2, owned_object_version = 5
        //   tx3: shared_object_1, owned_object_version = 1
        //   tx4: shared_object_1, shared_object_2, owned_object_version = 9
        //   tx5: shared_object_1, shared_object_2, owned_object_version = 11
        //
        // Later, we cancel transaction 2 and 4 due to congestion, and 5 due to DKG failure.
        // Expected outcome:
        //   tx1: both shared objects assign version 1, lamport version = 4
        //   tx2: shared objects assign cancelled version, lamport version = 6 due to gas object version = 5
        //   tx3: shared object 1 assign version 4, lamport version = 5
        //   tx4: shared objects assign cancelled version, lamport version = 10 due to gas object version = 9
        //   tx5: shared objects assign cancelled version, lamport version = 12 due to gas object version = 11
        let certs = [
            generate_shared_objs_tx_with_gas_version(
                &[
                    (id1, init_shared_version_1, true),
                    (id2, init_shared_version_2, true),
                ],
                3,
            ),
            generate_shared_objs_tx_with_gas_version(
                &[
                    (id1, init_shared_version_1, true),
                    (id2, init_shared_version_2, true),
                ],
                5,
            ),
            generate_shared_objs_tx_with_gas_version(&[(id1, init_shared_version_1, true)], 1),
            generate_shared_objs_tx_with_gas_version(
                &[
                    (id1, init_shared_version_1, true),
                    (id2, init_shared_version_2, true),
                ],
                9,
            ),
            generate_shared_objs_tx_with_gas_version(
                &[
                    (
                        SUI_RANDOMNESS_STATE_OBJECT_ID,
                        randomness_obj_version,
                        false,
                    ),
                    (id2, init_shared_version_2, true),
                ],
                11,
            ),
        ];
        let epoch_store = authority.epoch_store_for_testing();
        let forwarding_address_registry_initial_version = epoch_store
            .epoch_start_config()
            .forwarding_address_registry_obj_initial_shared_version()
            .unwrap();

        // Cancel transactions 2 and 4 due to congestion.
        let cancelled_txns: BTreeMap<TransactionDigest, CancelConsensusCertificateReason> = [
            (
                *certs[1].digest(),
                CancelConsensusCertificateReason::CongestionOnObjects(vec![id1]),
            ),
            (
                *certs[3].digest(),
                CancelConsensusCertificateReason::CongestionOnObjects(vec![id2]),
            ),
            (
                *certs[4].digest(),
                CancelConsensusCertificateReason::DkgFailed,
            ),
        ]
        .into_iter()
        .collect();

        let assignables = certs
            .iter()
            .map(Schedulable::Transaction)
            .collect::<Vec<_>>();

        // Run version assignment logic.
        let ConsensusSharedObjVerAssignment {
            mut shared_input_next_versions,
            assigned_versions,
        } = SharedObjVerManager::assign_versions_from_consensus(
            &epoch_store,
            authority.get_object_cache_reader().as_ref(),
            assignables.iter(),
            &cancelled_txns,
        )
        .unwrap();

        // Check that the final version of the shared object is the lamport version of the last
        // transaction.
        shared_input_next_versions
            .remove(&(SUI_ACCUMULATOR_ROOT_OBJECT_ID, SequenceNumber::from_u64(1)));
        shared_input_next_versions.remove(&(
            SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID,
            SequenceNumber::from_u64(1),
        ));
        assert_eq!(
            shared_input_next_versions,
            HashMap::from([
                ((id1, init_shared_version_1), SequenceNumber::from_u64(5)), // determined by tx3
                ((id2, init_shared_version_2), SequenceNumber::from_u64(4)), // determined by tx1
                (
                    (SUI_RANDOMNESS_STATE_OBJECT_ID, randomness_obj_version),
                    SequenceNumber::from_u64(1)
                ), // not mutable
            ])
        );

        // Check that the version assignment for each transaction is correct.
        let expected_accumulator_version = SequenceNumber::from_u64(1);
        assert_eq!(
            assigned_versions.0,
            vec![
                (
                    certs[0].key(),
                    AssignedVersions::new_for_testing(
                        vec![
                            ((id1, init_shared_version_1), init_shared_version_1),
                            ((id2, init_shared_version_2), init_shared_version_2),
                            (
                                (
                                    SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID,
                                    forwarding_address_registry_initial_version,
                                ),
                                forwarding_address_registry_initial_version,
                            ),
                        ],
                        Some(expected_accumulator_version)
                    )
                ),
                (
                    certs[1].key(),
                    AssignedVersions::new_for_testing(
                        vec![
                            ((id1, init_shared_version_1), SequenceNumber::CONGESTED),
                            ((id2, init_shared_version_2), SequenceNumber::CANCELLED_READ),
                        ],
                        Some(expected_accumulator_version)
                    )
                ),
                (
                    certs[2].key(),
                    AssignedVersions::new_for_testing(
                        vec![
                            ((id1, init_shared_version_1), SequenceNumber::from_u64(4)),
                            (
                                (
                                    SUI_FORWARDING_ADDRESS_REGISTRY_OBJECT_ID,
                                    forwarding_address_registry_initial_version,
                                ),
                                forwarding_address_registry_initial_version,
                            ),
                        ],
                        Some(expected_accumulator_version)
                    )
                ),
                (
                    certs[3].key(),
                    AssignedVersions::new_for_testing(
                        vec![
                            ((id1, init_shared_version_1), SequenceNumber::CANCELLED_READ),
                            ((id2, init_shared_version_2), SequenceNumber::CONGESTED)
                        ],
                        Some(expected_accumulator_version)
                    )
                ),
                (
                    certs[4].key(),
                    AssignedVersions::new_for_testing(
                        vec![
                            (
                                (SUI_RANDOMNESS_STATE_OBJECT_ID, randomness_obj_version),
                                SequenceNumber::RANDOMNESS_UNAVAILABLE
                            ),
                            ((id2, init_shared_version_2), SequenceNumber::CANCELLED_READ)
                        ],
                        Some(expected_accumulator_version)
                    )
                ),
            ]
        );
    }

    #[tokio::test]
    async fn test_assign_versions_from_effects() {
        let shared_object = Object::shared_for_testing();
        let id = shared_object.id();
        let init_shared_version = shared_object.owner.start_version().unwrap();
        let authority = TestAuthorityBuilder::new()
            .with_starting_objects(std::slice::from_ref(&shared_object))
            .build()
            .await;
        let certs = [
            generate_shared_objs_tx_with_gas_version(&[(id, init_shared_version, true)], 3),
            generate_shared_objs_tx_with_gas_version(&[(id, init_shared_version, false)], 5),
            generate_shared_objs_tx_with_gas_version(&[(id, init_shared_version, true)], 9),
            generate_shared_objs_tx_with_gas_version(&[(id, init_shared_version, true)], 11),
        ];
        let effects = [
            TestEffectsBuilder::new(certs[0].data()).build(),
            TestEffectsBuilder::new(certs[1].data())
                .with_shared_input_versions(BTreeMap::from([(id, SequenceNumber::from_u64(4))]))
                .build(),
            TestEffectsBuilder::new(certs[2].data())
                .with_shared_input_versions(BTreeMap::from([(id, SequenceNumber::from_u64(4))]))
                .build(),
            TestEffectsBuilder::new(certs[3].data())
                .with_shared_input_versions(BTreeMap::from([(id, SequenceNumber::from_u64(10))]))
                .build(),
        ];
        let epoch_store = authority.epoch_store_for_testing();
        let assigned_versions = SharedObjVerManager::assign_versions_from_effects(
            certs
                .iter()
                .zip_debug_eq(effects.iter())
                .map(|(cert, effect)| (cert, effect, None))
                .collect::<Vec<_>>()
                .as_slice(),
            &epoch_store,
            authority.get_object_cache_reader().as_ref(),
        );
        // Check that the shared object's next version is always initialized in the epoch store.
        assert_eq!(
            epoch_store
                .get_next_object_version(&id, init_shared_version)
                .unwrap(),
            init_shared_version
        );
        assert_eq!(
            assigned_versions.0,
            vec![
                (
                    certs[0].key(),
                    AssignedVersions::new_for_testing(
                        vec![((id, init_shared_version), init_shared_version)],
                        None
                    )
                ),
                (
                    certs[1].key(),
                    AssignedVersions::new_for_testing(
                        vec![((id, init_shared_version), SequenceNumber::from_u64(4))],
                        None
                    )
                ),
                (
                    certs[2].key(),
                    AssignedVersions::new_for_testing(
                        vec![((id, init_shared_version), SequenceNumber::from_u64(4))],
                        None
                    )
                ),
                (
                    certs[3].key(),
                    AssignedVersions::new_for_testing(
                        vec![((id, init_shared_version), SequenceNumber::from_u64(10))],
                        None
                    )
                ),
            ]
        );
    }

    /// Generate a transaction that uses shared objects as specified in the parameters.
    /// Also uses a gas object with specified version.
    /// The version of the gas object is used to manipulate the lamport version of this transaction.
    fn generate_shared_objs_tx_with_gas_version(
        shared_objects: &[(ObjectID, SequenceNumber, bool)],
        gas_object_version: u64,
    ) -> VerifiedExecutableTransaction {
        let mut tx_builder = TestTransactionBuilder::new(
            SuiAddress::ZERO,
            (
                ObjectID::random(),
                SequenceNumber::from_u64(gas_object_version),
                ObjectDigest::random(),
            ),
            0,
        );
        let tx_data = {
            let builder = tx_builder.ptb_builder_mut();
            for (shared_object_id, shared_object_init_version, shared_object_mutable) in
                shared_objects
            {
                builder
                    .obj(ObjectArg::SharedObject {
                        id: *shared_object_id,
                        initial_shared_version: *shared_object_init_version,
                        mutability: if *shared_object_mutable {
                            SharedObjectMutability::Mutable
                        } else {
                            SharedObjectMutability::Immutable
                        },
                    })
                    .unwrap();
            }
            tx_builder.build()
        };
        let tx = SenderSignedData::new(tx_data, vec![]);
        VerifiedExecutableTransaction::new_unchecked(ExecutableTransaction::new_from_data_and_sig(
            tx,
            CertificateProof::new_system(0),
        ))
    }

    struct WithdrawTestContext {
        authority: Arc<AuthorityState>,
        assignables: Vec<Schedulable<VerifiedExecutableTransaction>>,
        shared_objects: Vec<Object>,
    }

    impl WithdrawTestContext {
        pub async fn new() -> Self {
            // Create a shared object for testing
            let shared_objects = vec![Object::shared_for_testing()];
            let mut config = ProtocolConfig::get_for_max_version_UNSAFE();
            config.set_enable_accumulators_for_testing(true);
            // These tests exercise accumulator sequencing in isolation.
            config.set_create_forwarding_address_registry_for_testing(false);
            config.set_enable_forwarding_addresses_for_testing(false);
            let authority = TestAuthorityBuilder::new()
                .with_starting_objects(&shared_objects)
                .with_protocol_config(config)
                .build()
                .await;
            Self {
                authority,
                assignables: vec![],
                shared_objects,
            }
        }

        pub fn add_withdraw_transaction(&mut self) -> TransactionKey {
            // Generate random sender and gas object for each transaction
            let (sender, keypair) = get_account_key_pair();
            let gas_object = Object::with_owner_for_testing(sender);
            let gas_object_ref = gas_object.compute_object_reference();
            // Generate a unique gas price to make the transaction unique.
            let gas_price = (self.assignables.len() + 1) as u64;
            let mut tx_builder = TestTransactionBuilder::new(sender, gas_object_ref, gas_price);
            let tx_data = {
                let ptb_builder = tx_builder.ptb_builder_mut();
                ptb_builder
                    .funds_withdrawal(FundsWithdrawalArg::balance_from_sender(
                        200,
                        GAS::type_tag(),
                    ))
                    .unwrap();
                tx_builder.build()
            };
            let cert = VerifiedExecutableTransaction::new_for_testing(tx_data, &keypair);
            let key = cert.key();
            self.assignables.push(Schedulable::Transaction(cert));
            key
        }

        pub fn add_settlement_transaction(&mut self) -> TransactionKey {
            let height = (self.assignables.len() + 1) as u64;
            let settlement = Schedulable::AccumulatorSettlement(0, height);
            let key = settlement.key();
            self.assignables.push(settlement);
            key
        }

        pub fn add_withdraw_with_shared_object_transaction(&mut self) -> TransactionKey {
            // Generate random sender and gas object for each transaction
            let (sender, keypair) = get_account_key_pair();
            let gas_object = Object::with_owner_for_testing(sender);
            let gas_object_ref = gas_object.compute_object_reference();
            // Generate a unique gas price to make the transaction unique.
            let gas_price = (self.assignables.len() + 1) as u64;
            let mut tx_builder = TestTransactionBuilder::new(sender, gas_object_ref, gas_price);
            let tx_data = {
                let ptb_builder = tx_builder.ptb_builder_mut();
                // Add shared object to the transaction
                if let Some(shared_obj) = self.shared_objects.first() {
                    let id = shared_obj.id();
                    let init_version = shared_obj.owner.start_version().unwrap();
                    ptb_builder
                        .obj(ObjectArg::SharedObject {
                            id,
                            initial_shared_version: init_version,
                            mutability: SharedObjectMutability::Mutable,
                        })
                        .unwrap();
                }
                // Add balance withdraw
                ptb_builder
                    .funds_withdrawal(FundsWithdrawalArg::balance_from_sender(
                        200,
                        GAS::type_tag(),
                    ))
                    .unwrap();
                tx_builder.build()
            };
            let cert = VerifiedExecutableTransaction::new_for_testing(tx_data, &keypair);
            let key = cert.key();
            self.assignables.push(Schedulable::Transaction(cert));
            key
        }

        pub fn assign_versions_from_consensus(&self) -> ConsensusSharedObjVerAssignment {
            let epoch_store = self.authority.epoch_store_for_testing();
            SharedObjVerManager::assign_versions_from_consensus(
                &epoch_store,
                self.authority.get_object_cache_reader().as_ref(),
                self.assignables.iter(),
                &BTreeMap::new(),
            )
            .unwrap()
        }
    }

    #[tokio::test]
    async fn test_assign_versions_from_consensus_with_withdraws_simple() {
        // Note that we don't need a shared object to trigger withdraw version assignment.
        // In fact it is important that this works without a shared object.
        let mut ctx = WithdrawTestContext::new().await;

        let acc_version = ctx
            .authority
            .get_object(&SUI_ACCUMULATOR_ROOT_OBJECT_ID)
            .unwrap()
            .version();

        let withdraw_key = ctx.add_withdraw_transaction();
        let settlement_key = ctx.add_settlement_transaction();

        let assigned_versions = ctx.assign_versions_from_consensus();
        assert_eq!(
            assigned_versions,
            ConsensusSharedObjVerAssignment {
                assigned_versions: AssignedTxAndVersions::new(vec![
                    (
                        withdraw_key,
                        AssignedVersions::new_for_testing(vec![], Some(acc_version))
                    ),
                    (
                        settlement_key,
                        AssignedVersions::new_for_testing(
                            vec![((SUI_ACCUMULATOR_ROOT_OBJECT_ID, acc_version), acc_version)],
                            Some(acc_version)
                        )
                    ),
                ]),
                shared_input_next_versions: HashMap::from([(
                    (SUI_ACCUMULATOR_ROOT_OBJECT_ID, acc_version),
                    acc_version.next()
                )]),
            }
        );
    }

    #[tokio::test]
    async fn test_assign_versions_from_consensus_with_multiple_withdraws_and_settlements() {
        // Test with multiple withdrawals and multiple settlements, with settlement as the last transaction
        let mut ctx = WithdrawTestContext::new().await;

        let acc_version = ctx
            .authority
            .get_object(&SUI_ACCUMULATOR_ROOT_OBJECT_ID)
            .unwrap()
            .version();

        // First withdrawal and settlement
        let withdraw_key1 = ctx.add_withdraw_transaction();
        let settlement_key1 = ctx.add_settlement_transaction();

        // Second withdrawal and settlement
        let withdraw_key2 = ctx.add_withdraw_transaction();
        let settlement_key2 = ctx.add_settlement_transaction();

        // Third withdrawal and final settlement
        let withdraw_key3 = ctx.add_withdraw_transaction();
        let settlement_key3 = ctx.add_settlement_transaction();

        let assigned_versions = ctx.assign_versions_from_consensus();
        assert_eq!(
            assigned_versions,
            ConsensusSharedObjVerAssignment {
                assigned_versions: AssignedTxAndVersions::new(vec![
                    (
                        withdraw_key1,
                        AssignedVersions::new_for_testing(vec![], Some(acc_version))
                    ),
                    (
                        settlement_key1,
                        AssignedVersions::new_for_testing(
                            vec![((SUI_ACCUMULATOR_ROOT_OBJECT_ID, acc_version), acc_version)],
                            Some(acc_version)
                        )
                    ),
                    (
                        withdraw_key2,
                        AssignedVersions::new_for_testing(vec![], Some(acc_version.next()))
                    ),
                    (
                        settlement_key2,
                        AssignedVersions::new_for_testing(
                            vec![(
                                (SUI_ACCUMULATOR_ROOT_OBJECT_ID, acc_version),
                                acc_version.next()
                            )],
                            Some(acc_version.next())
                        )
                    ),
                    (
                        withdraw_key3,
                        AssignedVersions::new_for_testing(vec![], Some(acc_version.next().next()))
                    ),
                    (
                        settlement_key3,
                        AssignedVersions::new_for_testing(
                            vec![(
                                (SUI_ACCUMULATOR_ROOT_OBJECT_ID, acc_version),
                                acc_version.next().next()
                            )],
                            Some(acc_version.next().next())
                        )
                    ),
                ]),
                shared_input_next_versions: HashMap::from([(
                    (SUI_ACCUMULATOR_ROOT_OBJECT_ID, acc_version),
                    acc_version.next().next().next()
                )]),
            }
        );
    }

    #[tokio::test]
    async fn test_assign_versions_from_consensus_with_withdraw_and_shared_object() {
        // Test that a transaction can have both a withdrawal and use a shared object
        let mut ctx = WithdrawTestContext::new().await;

        // Get the shared object info from the context
        let shared_obj_id = ctx.shared_objects[0].id();
        let shared_obj_version = ctx.shared_objects[0].owner.start_version().unwrap();

        let acc_version = ctx
            .authority
            .get_object(&SUI_ACCUMULATOR_ROOT_OBJECT_ID)
            .unwrap()
            .version();

        let withdraw_with_shared_key = ctx.add_withdraw_with_shared_object_transaction();
        let settlement_key = ctx.add_settlement_transaction();

        let assigned_versions = ctx.assign_versions_from_consensus();
        assert_eq!(
            assigned_versions,
            ConsensusSharedObjVerAssignment {
                assigned_versions: AssignedTxAndVersions::new(vec![
                    (
                        withdraw_with_shared_key,
                        AssignedVersions::new_for_testing(
                            vec![((shared_obj_id, shared_obj_version), shared_obj_version)],
                            Some(acc_version)
                        )
                    ),
                    (
                        settlement_key,
                        AssignedVersions::new_for_testing(
                            vec![((SUI_ACCUMULATOR_ROOT_OBJECT_ID, acc_version), acc_version)],
                            Some(acc_version)
                        )
                    ),
                ]),
                shared_input_next_versions: HashMap::from([
                    (
                        (SUI_ACCUMULATOR_ROOT_OBJECT_ID, acc_version),
                        acc_version.next()
                    ),
                    (
                        (shared_obj_id, shared_obj_version),
                        shared_obj_version.next()
                    ),
                ]),
            }
        );
    }
}
