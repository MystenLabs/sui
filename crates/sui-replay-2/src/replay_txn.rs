// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    data_store::InputObject, environment::ReplayEnvironment, errors::ReplayError,
    execution::ReplayExecutor,
};
use std::collections::{BTreeMap, BTreeSet};
use sui_types::{
    base_types::{ObjectID, SequenceNumber, SuiAddress},
    digests::TransactionDigest,
    effects::{TransactionEffects, TransactionEffectsAPI},
    transaction::{
        CallArg, Command, GasData, InputObjectKind, InputObjects, ObjectArg, ObjectReadResult,
        ObjectReadResultKind, TransactionData, TransactionDataAPI, TransactionKind,
    },
    TypeTag,
};
use tracing::trace;

// #[derive(Debug)]
pub struct ReplayTransaction {
    pub digest: TransactionDigest,
    pub txn_data: TransactionData,
    pub effects: TransactionEffects,
    pub executor: ReplayExecutor,
    pub input_objects: InputObjects,
}

impl ReplayTransaction {
    pub async fn load(env: &mut ReplayEnvironment, tx_digest: &str) -> Result<Self, ReplayError> {
        let digest = tx_digest.parse().map_err(|e| {
            let err = format!("{:?}", e);
            let digest = tx_digest.to_string();
            ReplayError::FailedToParseDigest { digest, err }
        })?;

        // load transaction data and effects
        let (txn_data, effects) = env.load_txn_data(tx_digest).await?;
        // load objects and packages used by the transaction
        // get the ids and versions of the input objects to load
        let input_objects = get_input_objects(&txn_data, &effects)?;
        // load the objects and collect the package ids of the type parameters
        let type_param_pkgs = env.load_objects(&input_objects).await?;
        // collect all package ids required by the transaction
        let mut packages = get_packages(&txn_data)?;
        packages.extend(&type_param_pkgs);
        // load the packages
        env.load_packages(&packages).await?;

        // make the `InputObjects` for `execute_transaction_to_effects`
        let input_objects =
            get_input_objects_for_replay(env, &txn_data, tx_digest, &input_objects)?;
        let epoch = effects.executed_epoch();
        let protocol_config = env
            .protocol_config(epoch, env.chain())
            .unwrap_or_else(|e| panic!("Failed to get protocl config: {:?}", e));
        let executor =
            ReplayExecutor::new(protocol_config, None).unwrap_or_else(|e| panic!("{:?}", e));

        Ok(Self {
            executor,
            digest,
            txn_data,
            effects,
            input_objects,
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

// Return the list of objects to load as "object id" and "version".
// Package objects are not included in the list and handled in `get_packages`.
fn get_input_objects(
    txn_data: &TransactionData,
    effects: &TransactionEffects,
) -> Result<BTreeSet<InputObject>, ReplayError> {
    let input_object_ids = get_input_ids(txn_data)?;
    trace!("Input Object IDs: {:#?}", input_object_ids);
    let effects_object_ids = get_effects_ids(effects)?;
    trace!("Effects Object IDs: {:#?}", effects_object_ids);
    // merge input and effects object ids; add the input ids to the effects ids if not present
    let mut effect_ids = effects_object_ids
        .into_iter()
        .map(|input| (input.object_id, input.version.unwrap()))
        .collect::<BTreeMap<_, _>>();
    for input_object in input_object_ids.into_iter() {
        effect_ids
            .entry(input_object.object_id)
            .or_insert(input_object.version.unwrap());
    }
    Ok(effect_ids
        .into_iter()
        .map(|(object_id, version)| InputObject {
            object_id,
            version: Some(version),
        })
        .collect::<BTreeSet<InputObject>>())
}

// Find all the object ids and versions that are defined in the transaction data
fn get_input_ids(txn_data: &TransactionData) -> Result<BTreeSet<InputObject>, ReplayError> {
    // grab all coins
    let mut object_ids = txn_data
        .gas_data()
        .payment
        .iter()
        .map(|(id, seq_num, _)| InputObject {
            object_id: *id,
            version: Some(seq_num.value()),
        })
        .collect::<BTreeSet<_>>();
    // grab all input objects whose version is defined in transaction data (e.g. owned, not shared)
    if let TransactionKind::ProgrammableTransaction(ptb) = txn_data.kind() {
        let input_object_ids = ptb
            .inputs
            .iter()
            .filter_map(|input| {
                if let CallArg::Object(call_arg) = input {
                    match call_arg {
                        ObjectArg::ImmOrOwnedObject((id, seq_num, _digest)) => Some(InputObject {
                            object_id: *id,
                            version: Some(seq_num.value()),
                        }),
                        ObjectArg::SharedObject {
                            id: _,
                            initial_shared_version: _,
                            mutable: _,
                        } => {
                            None // will be in transaction effects
                        }
                        ObjectArg::Receiving((id, seq_num, _digest)) => Some(InputObject {
                            object_id: *id,
                            version: Some(seq_num.value()),
                        }),
                    }
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        object_ids.extend(input_object_ids);
        Ok(object_ids)
    } else {
        Ok(object_ids)
    }
}

// Get the modified objects and unchanged shared objects from the transaction effects
fn get_effects_ids(effects: &TransactionEffects) -> Result<BTreeSet<InputObject>, ReplayError> {
    let mut object_ids = effects
        .modified_at_versions()
        .iter()
        .map(|(obj_id, seq_num)| {
            trace!("Modified at version: {:?}[{}]", obj_id, seq_num.value());
            InputObject {
                object_id: *obj_id,
                version: Some(seq_num.value()),
            }
        })
        .collect::<BTreeSet<_>>();
    effects
        .unchanged_shared_objects()
        .iter()
        .for_each(|(obj_id, kind)| match kind {
            sui_types::effects::UnchangedSharedKind::ReadOnlyRoot((ver, _digest)) => {
                object_ids.insert(InputObject {
                    object_id: *obj_id,
                    version: Some(ver.value()),
                });
            }
            sui_types::effects::UnchangedSharedKind::MutateConsensusStreamEnded(_)
            | sui_types::effects::UnchangedSharedKind::ReadConsensusStreamEnded(_)
            | sui_types::effects::UnchangedSharedKind::Cancelled(_)
            | sui_types::effects::UnchangedSharedKind::PerEpochConfig => {
                trace!("Ignored `UnchangedSharedKind`: {:?}", kind);
            }
        });
    Ok(object_ids)
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

// get the package info from the type tag and insert the packages of the type tags (if any)
// in `packages`
pub fn packages_from_type_tag(typ: &TypeTag, packages: &mut BTreeSet<ObjectID>) {
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

//
// `InputObjects` for `execute_transaction_to_effects`
//

// Get `InputObjects` from a set of (ObjectId, version) pairs, where version is a u64.
fn get_input_objects_for_replay(
    env: &mut ReplayEnvironment,
    txn: &TransactionData,
    tx_digest: &str,
    object_versions: &BTreeSet<InputObject>,
) -> Result<InputObjects, ReplayError> {
    let object_versions = object_versions
        .iter()
        .map(|input| (input.object_id, input.version.unwrap()))
        .collect::<BTreeMap<_, _>>();
    let deleted_shared_info_map: BTreeMap<ObjectID, (TransactionDigest, SequenceNumber)> =
        BTreeMap::new();
    let mut resolved_input_objs = vec![];
    let input_objects_kind = txn
        .input_objects()
        .map_err(|e| ReplayError::InputObjectsError {
            digest: format!("{:?}", tx_digest),
            err: format!("{:?}", e),
        })?;
    for kind in input_objects_kind.iter() {
        match kind {
            InputObjectKind::MovePackage(pkg_id) => {
                env.get_package_object(pkg_id).map(|pkg| {
                    resolved_input_objs.push(ObjectReadResult {
                        input_object_kind: *kind,
                        object: ObjectReadResultKind::Object(pkg),
                    })
                })?;
            }
            InputObjectKind::ImmOrOwnedMoveObject((obj_id, _version, _digest)) => {
                let version = *object_versions.get(obj_id).ok_or_else(|| {
                    ReplayError::ObjectVersionNotFound {
                        address: obj_id.to_string(),
                        version: None,
                    }
                })?;
                let object = env.get_object_at_version(obj_id, version).ok_or_else(|| {
                    ReplayError::ObjectNotFound {
                        address: obj_id.to_string(),
                        version: Some(version),
                    }
                })?;
                let input_object_kind =
                    InputObjectKind::ImmOrOwnedMoveObject(object.compute_object_reference());
                resolved_input_objs.push(ObjectReadResult {
                    input_object_kind,
                    object: ObjectReadResultKind::Object(object),
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
                let version =
                    *object_versions
                        .get(id)
                        .ok_or_else(|| ReplayError::ObjectVersionNotFound {
                            address: id.to_string(),
                            version: None,
                        })?;
                // let version = initial_shared_version.value();
                let is_deleted = deleted_shared_info_map.contains_key(id);
                if !is_deleted {
                    let object = ObjectReadResultKind::Object(
                        env.get_object_at_version(id, version).ok_or_else(|| {
                            ReplayError::ObjectNotFound {
                                address: id.to_string(),
                                version: Some(version),
                            }
                        })?,
                    );
                    resolved_input_objs.push(ObjectReadResult {
                        input_object_kind,
                        object,
                    });
                } else {
                    let (digest, version) = deleted_shared_info_map.get(id).unwrap();
                    let object =
                        ObjectReadResultKind::ObjectConsensusStreamEnded(*version, *digest);
                    resolved_input_objs.push(ObjectReadResult {
                        input_object_kind,
                        object,
                    });
                }
            }
        }
    }
    trace!("resolved input objects: {:#?}", resolved_input_objs);
    Ok(InputObjects::new(resolved_input_objs))
}
