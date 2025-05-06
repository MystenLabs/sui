// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::packages_from_type_tag;
use crate::{
    errors::ReplayError,
    execution::ReplayExecutor,
    replay_interface::{EpochStore, ObjectKey, ObjectStore, TransactionStore, VersionQuery},
};
use std::collections::{BTreeMap, BTreeSet};
use sui_types::{
    base_types::{ObjectID, SuiAddress},
    digests::TransactionDigest,
    effects::{TransactionEffects, TransactionEffectsAPI},
    object::Object,
    transaction::{
        CallArg, Command, GasData, InputObjects, ObjectArg, TransactionData, TransactionDataAPI,
        TransactionKind,
    },
};
use tracing::trace;

pub type ObjectVersion = u64;
pub type PackageVersion = u64;

// #[derive(Debug)]
pub struct ReplayTransaction {
    pub digest: TransactionDigest,
    pub txn_data: TransactionData,
    pub effects: TransactionEffects,
    pub executor: ReplayExecutor,
    pub input_objects: InputObjects,
    pub objects: BTreeMap<ObjectID, BTreeMap<ObjectVersion, Object>>,
}

impl ReplayTransaction {
    pub fn load(
        tx_digest: &str,
        txn_store: &dyn TransactionStore,
        epoch_store: &dyn EpochStore,
        object_store: &dyn ObjectStore,
    ) -> Result<Self, ReplayError> {
        let epoch_data = epoch_store.epoch_info(770)?;
        println!("DARIO: {:?}", epoch_data);

        //
        // load transaction data and effects
        let (txn_data, effects) = txn_store.transaction_data_and_effects(tx_digest)?;

        //
        // find all objects and packages used by the transaction
        let input_objects: Vec<ObjectKey> = vec![];
            // load_objects_and_packages(&txn_data, &effects, object_store).await?;

        // make the `InputObjects` for `execute_transaction_to_effects`
        let input_objects =
            get_input_objects_for_replay(object_store, &txn_data, tx_digest, &input_objects)?;
        let epoch = effects.executed_epoch();
        let protocol_config = epoch_store
            .protocol_config(epoch)
            .unwrap_or_else(|e| panic!("Failed to get protocl config: {:?}", e));
        let executor =
            ReplayExecutor::new(protocol_config, None).unwrap_or_else(|e| panic!("{:?}", e));

        let digest = tx_digest.parse().map_err(|e| {
            let digest = tx_digest.to_string();
            let err = format!("Cannot parse digest {}. Error {:?}", digest, e);
            ReplayError::DataConversionError { err }
        })?;

        Ok(Self {
            executor,
            digest,
            txn_data,
            effects,
            input_objects,
            objects: BTreeMap::new(),
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
}

//
// Transaction input objects
//

// Load the objects and packages used by the transaction.
async fn load_objects_and_packages(
    txn_data: &TransactionData,
    effects: &TransactionEffects,
    object_store: &dyn ObjectStore,
) -> Result<Vec<ObjectKey>, ReplayError> {
    // get the ids and versions of the input objects to load
    let input_objects = get_input_objects(txn_data, effects)?;
    // load the objects and collect the package ids of the type parameters
    let type_param_pkgs = load_objects(&input_objects, object_store)?;
    // collect all package ids required by the transaction
    let mut packages = get_packages(txn_data)?;
    packages.extend(&type_param_pkgs);
    // TODO: load the packages
    // env.load_packages(&packages).await?;
    Ok(input_objects.into_iter().collect())
}

// Load the given objects.
// Return the packages of the type parameters instantiated
// (e.g. `SUI` in `Coin<SUI>`).
fn load_objects(
    object_ids: &[ObjectKey],
    object_store: &dyn ObjectStore,
) -> Result<Vec<ObjectID>, ReplayError> {
    let mut packages = BTreeSet::new();
    let objects = object_store.get_objects(object_ids)?;
    for object in objects.into_iter() {
        let _object_id = object.id();
        if let Some(tag) = object.as_inner().struct_tag() {
            packages_from_type_tag(&tag.into(), &mut packages);
        }
        let _version = object.version().value();
        // TODO: return object no self
        // self.objects
        //     .entry(object_id)
        //     .or_default()
        //     .insert(version, object);
    }
    Ok(packages.into_iter().collect())
}

// Return the list of objects to load.
// Package objects are not included in the list and handled in `get_packages`.
fn get_input_objects(
    txn_data: &TransactionData,
    effects: &TransactionEffects,
) -> Result<Vec<ObjectKey>, ReplayError> {
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

// Find all the object ids and versions that are defined in the transaction data
fn get_input_ids(txn_data: &TransactionData) -> Result<BTreeSet<ObjectKey>, ReplayError> {
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

// Get the modified objects and unchanged shared objects from the transaction effects
fn get_effects_ids(effects: &TransactionEffects) -> Result<BTreeSet<ObjectKey>, ReplayError> {
    let mut object_keys = effects
        .modified_at_versions()
        .iter()
        .map(|(obj_id, seq_num)| {
            trace!("Modified at version: {:?}[{}]", obj_id, seq_num.value());
            ObjectKey {
                object_id: *obj_id,
                version_query: VersionQuery::Version(seq_num.value()),
            }
        })
        .collect::<BTreeSet<_>>();
    effects
        .unchanged_shared_objects()
        .iter()
        .for_each(|(obj_id, kind)| match kind {
            sui_types::effects::UnchangedSharedKind::ReadOnlyRoot((ver, _digest)) => {
                object_keys.insert(ObjectKey {
                    object_id: *obj_id,
                    version_query: VersionQuery::Version(ver.value()),
                });
            }
            sui_types::effects::UnchangedSharedKind::MutateConsensusStreamEnded(_)
            | sui_types::effects::UnchangedSharedKind::ReadConsensusStreamEnded(_)
            | sui_types::effects::UnchangedSharedKind::Cancelled(_)
            | sui_types::effects::UnchangedSharedKind::PerEpochConfig => {
                trace!("Ignored `UnchangedSharedKind`: {:?}", kind);
            }
        });
    Ok(object_keys)
}

//
// Transaction input packages
//

fn get_packages(txn_data: &TransactionData) -> Result<BTreeSet<ObjectID>, ReplayError> {
    let mut packages = BTreeSet::new();
    if let TransactionKind::ProgrammableTransaction(ptb) = txn_data.kind() {
        for cmd in &ptb.commands {
            match cmd {
                Command::MoveCall(move_call) => {
                    packages.insert(move_call.package);
                    for type_input in move_call.type_arguments.iter() {
                        let typ =
                            type_input
                                .to_type_tag()
                                .map_err(|err| ReplayError::GenericError {
                                    err: format!("{:?}", err),
                                })?;
                        packages_from_type_tag(&typ, &mut packages);
                    }
                }
                Command::MakeMoveVec(type_input, _) => {
                    if let Some(t) = type_input {
                        let typ = t.to_type_tag().map_err(|err| ReplayError::GenericError {
                            err: format!("{:?}", err),
                        })?;
                        packages_from_type_tag(&typ, &mut packages);
                    }
                }
                Command::Publish(_, deps) | Command::Upgrade(_, deps, _, _) => {
                    packages.extend(deps);
                }
                Command::TransferObjects(_, _)
                | Command::SplitCoins(_, _)
                | Command::MergeCoins(_, _) => (),
            }
        }
    }
    Ok(packages)
}

//
// `InputObjects` for `execute_transaction_to_effects`
//

// Get `InputObjects` from a set of (ObjectId, version) pairs, where version is a u64.
fn get_input_objects_for_replay(
    _object_store: &dyn ObjectStore,
    _txn: &TransactionData,
    _tx_digest: &str,
    _object_versions: &[ObjectKey],
) -> Result<InputObjects, ReplayError> {
    // let object_versions = object_versions
    //     .iter()
    //     .map(|input| (input.object_id, input.version))
    //     .collect::<BTreeMap<_, _>>();
    // let deleted_shared_info_map: BTreeMap<ObjectID, (TransactionDigest, SequenceNumber)> =
    //     BTreeMap::new();
    // let mut resolved_input_objs = vec![];
    // let input_objects_kind = txn
    //     .input_objects()
    //     .map_err(|e| ReplayError::InputObjectsError {
    //         digest: format!("{:?}", tx_digest),
    //         err: format!("{:?}", e),
    //     })?;
    // for kind in input_objects_kind.iter() {
    //     match kind {
    //         InputObjectKind::MovePackage(pkg_id) => {
    //             env.get_package_object(pkg_id).map(|pkg| {
    //                 resolved_input_objs.push(ObjectReadResult {
    //                     input_object_kind: *kind,
    //                     object: ObjectReadResultKind::Object(pkg),
    //                 })
    //             })?;
    //         }
    //         InputObjectKind::ImmOrOwnedMoveObject((obj_id, _version, _digest)) => {
    //             let version = *object_versions.get(obj_id).ok_or_else(|| {
    //                 ReplayError::ObjectVersionNotFound {
    //                     address: obj_id.to_string(),
    //                     version: None,
    //                 }
    //             })?;
    //             let object = env.get_object_at_version(obj_id, version).ok_or_else(|| {
    //                 ReplayError::ObjectNotFound {
    //                     address: obj_id.to_string(),
    //                     version: Some(version),
    //                 }
    //             })?;
    //             let input_object_kind =
    //                 InputObjectKind::ImmOrOwnedMoveObject(object.compute_object_reference());
    //             resolved_input_objs.push(ObjectReadResult {
    //                 input_object_kind,
    //                 object: ObjectReadResultKind::Object(object),
    //             });
    //         }
    //         InputObjectKind::SharedMoveObject {
    //             id,
    //             initial_shared_version,
    //             mutable,
    //         } => {
    //             let input_object_kind = InputObjectKind::SharedMoveObject {
    //                 id: *id,
    //                 initial_shared_version: *initial_shared_version,
    //                 mutable: *mutable,
    //             };
    //             let version =
    //                 *object_versions
    //                     .get(id)
    //                     .ok_or_else(|| ReplayError::ObjectVersionNotFound {
    //                         address: id.to_string(),
    //                         version: None,
    //                     })?;
    //             let is_deleted = deleted_shared_info_map.contains_key(id);
    //             if !is_deleted {
    //                 let object = ObjectReadResultKind::Object(
    //                     env.get_object_at_version(id, version).ok_or_else(|| {
    //                         ReplayError::ObjectNotFound {
    //                             address: id.to_string(),
    //                             version: Some(version),
    //                         }
    //                     })?,
    //                 );
    //                 resolved_input_objs.push(ObjectReadResult {
    //                     input_object_kind,
    //                     object,
    //                 });
    //             } else {
    //                 let (digest, version) = deleted_shared_info_map.get(id).unwrap();
    //                 let object =
    //                     ObjectReadResultKind::ObjectConsensusStreamEnded(*version, *digest);
    //                 resolved_input_objs.push(ObjectReadResult {
    //                     input_object_kind,
    //                     object,
    //                 });
    //             }
    //         }
    //     }
    // }
    // trace!("resolved input objects: {:#?}", resolved_input_objs);
    // Ok(InputObjects::new(resolved_input_objs))
    Ok(InputObjects::new(vec![]))
}
