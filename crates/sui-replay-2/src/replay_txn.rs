// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This module contains the logic to use transaction data and effects for loading
//! all the objects and data needed to replay a transaction.
//!
//! The core of this module is in `ReplayTransaction::load()` which calls into the
//! different functions that operate over data and effects: `get_packages`, `get_input_ids`,
//! `get_effects_ids`.
//! `get_input_objects_for_replay` is used by the `execution.rs` module but could be moved
//! in this module and saved in the `ReplayTransaction` instance.

use crate::{
    execution::ReplayExecutor,
    replay_interface::{EpochStore, ObjectKey, ObjectStore, TransactionStore, VersionQuery},
};
use anyhow::Context;
use std::collections::{btree_map::Entry, BTreeMap, BTreeSet};
use sui_types::transaction::{InputObjectKind, ObjectReadResult, ObjectReadResultKind};
use sui_types::{base_types::SequenceNumber, TypeTag};
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    digests::TransactionDigest,
    effects::{InputSharedObject, TransactionEffects, TransactionEffectsAPI, UnchangedSharedKind},
    object::Object,
    transaction::{
        CallArg, Command, GasData, InputObjects, ObjectArg, TransactionData, TransactionDataAPI,
        TransactionKind,
    },
};
use tracing::{debug, trace};

pub type ObjectVersion = u64;
pub type PackageVersion = u64;

// `ReplayTransaction` contains all the data needed to replay a transaction.
// The `object_cache` will contain all the objects and packages touched by the transaction.
pub struct ReplayTransaction {
    pub digest: TransactionDigest,
    pub checkpoint: u64, // used for object queries
    pub txn_data: TransactionData,
    pub effects: TransactionEffects,
    pub executor: ReplayExecutor,
    // Objects and packages used by the transaction
    pub object_cache: BTreeMap<ObjectID, BTreeMap<ObjectVersion, Object>>,
}

impl ReplayTransaction {
    // Load a transaction and builds a `ReplayTransaction` instance.
    pub fn load(
        tx_digest: &str,
        txn_store: &dyn TransactionStore,
        epoch_store: &dyn EpochStore,
        object_store: &dyn ObjectStore,
    ) -> Result<Self, anyhow::Error> {
        debug!("Start load transaction");

        let digest = tx_digest
            .parse()
            .context(format!("Transaction digest malformed: {}", tx_digest))?;

        //
        // load transaction data and effects
        let (txn_data, effects, checkpoint) = txn_store.transaction_data_and_effects(tx_digest)?;

        //
        // load all objects and packages used by the transaction
        let object_cache = load_transaction_objects(&txn_data, &effects, checkpoint, object_store)?;

        //
        // instantiate the executor
        let epoch = effects.executed_epoch();
        let protocol_config = epoch_store
            .protocol_config(epoch)
            .unwrap_or_else(|e| panic!("Failed to get protocol config: {:?}", e));
        let executor =
            ReplayExecutor::new(protocol_config, None).unwrap_or_else(|e| panic!("{:?}", e));

        debug!("End load transaction");

        Ok(Self {
            digest,
            checkpoint,
            txn_data,
            effects,
            executor,
            object_cache,
        })
    }

    pub fn kind(&self) -> &TransactionKind {
        self.txn_data.kind()
    }

    pub fn sender(&self) -> SuiAddress {
        self.txn_data.sender()
    }

    pub fn gas_data(&self) -> &GasData {
        self.txn_data.gas_data()
    }

    pub fn epoch(&self) -> u64 {
        self.effects.executed_epoch()
    }

    pub fn checkpoint(&self) -> u64 {
        self.checkpoint
    }
}

// Load the objects and packages used by the transaction.
// Use data and effects to retrieve the objects and packages used.
// This is the tricky part of replay.
fn load_transaction_objects(
    txn_data: &TransactionData,
    effects: &TransactionEffects,
    checkpoint: u64,
    object_store: &dyn ObjectStore,
) -> Result<BTreeMap<ObjectID, BTreeMap<ObjectVersion, Object>>, anyhow::Error> {
    // collect all package ids required by the transaction
    let mut packages = get_packages(txn_data)?;

    // get the ids and versions of the input objects to load
    // load the objects and collect the package ids of the type parameters
    let object_keys = get_txn_object_keys(txn_data, effects)?;
    let (mut object_cache, tp_pkgs) = load_objects(&object_keys, object_store)?;
    packages.extend(&tp_pkgs);

    // load the packages and add them to the object cache
    let pkg_objects = load_packages(&packages, checkpoint, object_store)?;
    for (object_id, versions) in pkg_objects {
        match object_cache.entry(object_id) {
            Entry::Occupied(mut entry) => {
                // If the package already exists, extend its versions
                entry.get_mut().extend(versions);
            }
            Entry::Vacant(entry) => {
                // If the package does not exist, insert it
                entry.insert(versions);
            }
        }
    }

    Ok(object_cache)
}

// Collect all packages in input.
// For move calls is the package of the call.
// For vector commands the packages of the type parameter.
// For publish and upgrade commands, the packages of the dependencies.
fn get_packages(txn_data: &TransactionData) -> Result<BTreeSet<ObjectID>, anyhow::Error> {
    let mut packages = BTreeSet::new();
    if let TransactionKind::ProgrammableTransaction(ptb) = txn_data.kind() {
        for cmd in &ptb.commands {
            match cmd {
                Command::MoveCall(move_call) => {
                    packages.insert(move_call.package);
                    for type_input in move_call.type_arguments.iter() {
                        let typ = type_input
                            .to_type_tag()
                            .context(format!("Failed to read TypeTag: {:?}", type_input))?;
                        packages_from_type_tag(&typ, &mut packages);
                    }
                }
                Command::MakeMoveVec(type_input, _) => {
                    if let Some(t) = type_input {
                        let typ = t
                            .to_type_tag()
                            .context(format!("Failed to read TypeTag: {:?}", type_input))?;
                        packages_from_type_tag(&typ, &mut packages);
                    }
                }
                Command::Publish(_, deps) => {
                    packages.extend(deps);
                }
                Command::Upgrade(_, deps, pkg_id, _) => {
                    packages.extend(deps);
                    packages.insert(*pkg_id);
                }
                Command::TransferObjects(_, _)
                | Command::SplitCoins(_, _)
                | Command::MergeCoins(_, _) => (),
            }
        }
    }
    Ok(packages)
}

// Load the given objects. Objects are loaded and returned as a map from ObjectID to a map of
// version to Object.
// Returns also the packages of the type parameters instantiated (e.g. `SUI` in `Coin<SUI>`).
#[allow(clippy::type_complexity)]
fn load_objects(
    object_keys: &[ObjectKey],
    object_store: &dyn ObjectStore,
) -> Result<
    (
        BTreeMap<ObjectID, BTreeMap<ObjectVersion, Object>>, // objets loaded
        BTreeSet<ObjectID>,                                  // packages referenced
    ),
    anyhow::Error,
> {
    let mut packages = BTreeSet::new();
    let mut object_cache: BTreeMap<ObjectID, BTreeMap<ObjectVersion, Object>> = BTreeMap::new();
    let objects = object_store.get_objects(object_keys)?;
    for object in objects.into_iter() {
        if object.is_none() {
            // REVIEW: a `None` is simply ignored, is that correct?
            continue;
        }
        let object = object.unwrap();
        let object_id = object.id();
        if let Some(tag) = object.as_inner().struct_tag() {
            packages_from_type_tag(&tag.into(), &mut packages);
        }
        let version = object.version().value();
        object_cache
            .entry(object_id)
            .or_default()
            .insert(version, object);
    }
    Ok((object_cache, packages))
}

// Load packages and dependencies.
// This is a 2 steps process. We first load the packages and then collect all the
// dependencies and query for all of them.
// REVIEW: depending on what we do for system packages, we may not need to
// query by checkpoint.
// Non system package are immutable and may be queried with no version info.
fn load_packages(
    packages: &BTreeSet<ObjectID>,
    checkpoint: u64,
    object_store: &dyn ObjectStore,
) -> Result<BTreeMap<ObjectID, BTreeMap<ObjectVersion, Object>>, anyhow::Error> {
    let pkg_object_keys = packages
        .iter()
        .map(|pkg_id| ObjectKey {
            object_id: *pkg_id,
            version_query: VersionQuery::AtCheckpoint(checkpoint),
        })
        .collect::<Vec<_>>();
    debug!("Start load_packages");
    let (objects, packages) = load_objects(&pkg_object_keys, object_store)?;
    debug!("End load_packages");
    debug_assert!(
        packages.is_empty(),
        "Packages should be empty from packages load, there is no type parameter in packages"
    );
    Ok(objects)
}

// Return the list of objects to load in terms of `ObjectKey` (query to execute)
// Package objects are not included in the list and handled in `get_packages`.
fn get_txn_object_keys(
    txn_data: &TransactionData,
    effects: &TransactionEffects,
) -> Result<Vec<ObjectKey>, anyhow::Error> {
    let input_object_ids = get_input_ids(txn_data)?;
    trace!("Input Object IDs: {:#?}", input_object_ids);
    let effects_object_ids = get_effects_ids(effects)?;
    trace!("Effects Object IDs: {:#?}", effects_object_ids);
    // merge input and effects object ids; add the input ids to the effects ids if not present.
    let mut effect_ids = effects_object_ids
        .into_iter()
        .map(|input| (input.object_id, input.version_query))
        .collect::<BTreeMap<_, _>>();
    for input_object in input_object_ids.into_iter() {
        effect_ids
            .entry(input_object.object_id)
            .or_insert(input_object.version_query);
    }
    Ok(effect_ids
        .into_iter()
        .map(|(object_id, version_query)| ObjectKey {
            object_id,
            version_query,
        })
        .collect::<BTreeSet<ObjectKey>>()
        .into_iter()
        .collect::<Vec<_>>())
}

// Find all the object ids and versions that are defined in the transaction data.
// That includes:
// - the gas coins
// -- all `CallArg::Object` to PTBs
fn get_input_ids(txn_data: &TransactionData) -> Result<BTreeSet<ObjectKey>, anyhow::Error> {
    // grab all coins
    let mut object_keys: BTreeSet<ObjectKey> = txn_data
        .gas_data()
        .payment
        .iter()
        .map(|(id, seq_num, _)| ObjectKey {
            object_id: *id,
            version_query: VersionQuery::Version(seq_num.value()),
        })
        .collect();
    // grab all input objects whose version is defined in transaction data (e.g. owned, not shared)
    if let TransactionKind::ProgrammableTransaction(ptb) = txn_data.kind() {
        let input_object_keys = ptb
            .inputs
            .iter()
            .filter_map(|input| {
                if let CallArg::Object(call_arg) = input {
                    match call_arg {
                        ObjectArg::ImmOrOwnedObject((id, seq_num, _digest)) => Some(ObjectKey {
                            object_id: *id,
                            version_query: VersionQuery::Version(seq_num.value()),
                        }),
                        ObjectArg::SharedObject {
                            id: _,
                            initial_shared_version: _,
                            mutable: _,
                        } => {
                            None // will be in transaction effects
                        }
                        ObjectArg::Receiving((id, seq_num, _digest)) => Some(ObjectKey {
                            object_id: *id,
                            version_query: VersionQuery::Version(seq_num.value()),
                        }),
                    }
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        object_keys.extend(input_object_keys);
    }
    Ok(object_keys)
}

// Get the input shared objects and unchanged shared objects from the transaction effects
fn get_effects_ids(effects: &TransactionEffects) -> Result<BTreeSet<ObjectKey>, anyhow::Error> {
    let mut object_keys = effects
        .input_shared_objects()
        .iter()
        .map(|input_shared_object| match input_shared_object {
            InputSharedObject::MutateConsensusStreamEnded(object_id, version)
            | InputSharedObject::ReadConsensusStreamEnded(object_id, version)
            | InputSharedObject::Cancelled(object_id, version) => ObjectKey {
                object_id: *object_id,
                version_query: VersionQuery::Version(version.value()),
            },
            InputSharedObject::Mutate((object_id, version, _digest))
            | InputSharedObject::ReadOnly((object_id, version, _digest)) => ObjectKey {
                object_id: *object_id,
                version_query: VersionQuery::Version(version.value()),
            },
        })
        .collect::<BTreeSet<_>>();
    effects
        .unchanged_shared_objects()
        .iter()
        .for_each(|(obj_id, kind)| match kind {
            UnchangedSharedKind::ReadOnlyRoot((ver, _digest)) => {
                object_keys.insert(ObjectKey {
                    object_id: *obj_id,
                    version_query: VersionQuery::Version(ver.value()),
                });
            }
            UnchangedSharedKind::MutateConsensusStreamEnded(_)
            | UnchangedSharedKind::ReadConsensusStreamEnded(_)
            | UnchangedSharedKind::Cancelled(_)
            | UnchangedSharedKind::PerEpochConfigDEPRECATED
            | UnchangedSharedKind::PerEpochConfigWithSeqno(_) => {
                trace!("Ignored `UnchangedSharedKind`: {:?}", kind);
            }
        });
    Ok(object_keys)
}

//
// `InputObjects` for `execute_transaction_to_effects`
//

// Get `InputObjects` from a set of (ObjectId, version) pairs, where version is a u64.
// This is currently called from `execute_transaction_to_effects` but it could
// be computed for a `ReplayTransactoin` and cached.
pub fn get_input_objects_for_replay(
    txn: &TransactionData,
    tx_digest: &TransactionDigest,
    object_cache: &BTreeMap<ObjectID, BTreeMap<u64, Object>>, // objects used by the transaction
) -> Result<InputObjects, anyhow::Error> {
    let _deleted_shared_info_map: BTreeMap<ObjectID, (TransactionDigest, SequenceNumber)> =
        BTreeMap::new();
    let mut resolved_input_objs = vec![];
    let input_objects_kind = txn.input_objects().context(format!(
        "Failed to get input objects from transaction {}",
        tx_digest
    ))?;
    for kind in input_objects_kind.iter() {
        match kind {
            InputObjectKind::MovePackage(pkg_id) => {
                object_cache
                    .get(pkg_id)
                    .map(|pkgs| {
                        debug_assert!(
                            pkgs.len() == 1,
                            "Expected only one version for package {}",
                            pkg_id
                        );
                        let (_version, pkg) = pkgs.iter().next().unwrap();
                        resolved_input_objs.push(ObjectReadResult {
                            input_object_kind: *kind,
                            object: ObjectReadResultKind::Object(pkg.clone()),
                        })
                    })
                    .ok_or_else(|| anyhow::anyhow!(
                        format!(
                            "Package {} not found in transaction cache. Should have been loaded already",
                            pkg_id,
                        )
                    ))?;
            }
            InputObjectKind::ImmOrOwnedMoveObject((obj_id, version, _digest)) => {
                let object = object_cache
                    .get(obj_id)
                    .ok_or_else(|| anyhow::anyhow!(
                        format!(
                            "Object id {}[{}] not found in transaction cache. Should have been loaded already",
                            obj_id, version,
                        )
                    ))?
                    .get(&version.value())
                    .ok_or_else(|| anyhow::anyhow!(
                        format!(
                            "Object version {}[{}] not found in transaction cache. Should have been loaded already",
                            obj_id, version,
                        )
                    ))?;
                let input_object_kind =
                    InputObjectKind::ImmOrOwnedMoveObject(object.compute_object_reference());
                resolved_input_objs.push(ObjectReadResult {
                    input_object_kind,
                    object: ObjectReadResultKind::Object(object.clone()),
                });
            }
            InputObjectKind::SharedMoveObject {
                id,
                initial_shared_version,
                mutable,
            } => {
                let input_object_kind = InputObjectKind::SharedMoveObject {
                    id: *id,
                    initial_shared_version: *initial_shared_version,
                    mutable: *mutable,
                };
                let versions =
                    object_cache
                        .get(id)
                        .ok_or_else(|| anyhow::anyhow!(
                            format!(
                                "Shared Object id {} not found in transaction cache. Should have been loaded already",
                                id,
                            )
                        ))?;
                debug_assert!(
                    versions.len() == 1,
                    "Expected only one version for shared object {}",
                    id
                );
                let (_version, obj) = versions.iter().next().unwrap();
                resolved_input_objs.push(ObjectReadResult {
                    input_object_kind,
                    object: ObjectReadResultKind::Object(obj.clone()),
                });
            }
        }
    }
    trace!("resolved input objects: {:#?}", resolved_input_objs);
    Ok(InputObjects::new(resolved_input_objs))
}

// get the package info from the type tag and insert the packages of the type tags (if any)
// in `packages`
fn packages_from_type_tag(typ: &TypeTag, packages: &mut BTreeSet<ObjectID>) {
    match typ {
        TypeTag::Struct(struct_tag) => {
            packages.insert(struct_tag.address.into());
            for ty in struct_tag.type_params.iter() {
                packages_from_type_tag(ty, packages);
            }
        }
        TypeTag::Vector(type_tag) => {
            packages_from_type_tag(type_tag, packages);
        }
        TypeTag::Bool
        | TypeTag::U8
        | TypeTag::U64
        | TypeTag::U128
        | TypeTag::Address
        | TypeTag::Signer
        | TypeTag::U16
        | TypeTag::U32
        | TypeTag::U256 => (),
    }
}
