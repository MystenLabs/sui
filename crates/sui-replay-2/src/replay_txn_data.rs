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
use tracing::{debug, info};

// #[derive(Debug)]
pub struct ReplayTransaction {
    pub executor: ReplayExecutor,
    pub digest: TransactionDigest,
    pub kind: TransactionKind,
    pub epoch: u64,
    pub epoch_start_timestamp: u64,
    pub sender: SuiAddress,
    pub input_objects: InputObjects,
    pub gas_data: GasData,
    pub reference_gas_price: u64,
}

impl ReplayTransaction {
    pub async fn load(env: &mut ReplayEnvironment, tx_digest: &str) -> Result<Self, ReplayError> {
        // load transaction data and effects
        let txn_data = env.data_store.transaction_data(tx_digest).await?;
        info!("Transaction data: {:#?}", txn_data);
        let effects = env.data_store.transaction_effects(tx_digest).await?;
        info!("Transaction effects: {:#?}", effects);

        let mut packages = get_packages(&txn_data)?;
        let input_object_ids = get_input_ids(&txn_data)?;
        info!("Input Object IDs: {:#?}", input_object_ids);
        let effects_object_ids = get_effects_ids(&effects)?;
        info!("Effects Object IDs: {:#?}", effects_object_ids);
        let mut input_versions = effects_object_ids
            .into_iter()
            .map(|input| (input.object_id, input.version.unwrap()))
            .collect::<BTreeMap<_, _>>();
        for input_object in input_object_ids.iter() {
            input_versions
                .entry(input_object.object_id)
                .or_insert(input_object.version.unwrap());
        }
        let object_versions = input_versions
            .into_iter()
            .map(|(object_id, version)| InputObject {
                object_id,
                version: Some(version),
            })
            .collect::<BTreeSet<InputObject>>();

        let obj_pkgs = env.load_objects(&object_versions).await?;
        packages.extend(&obj_pkgs);
        env.load_packages(&packages).await?;

        let epoch = effects.executed_epoch();
        let epoch_start_timestamp = env.epoch_info.epoch_timestamp(epoch)?;
        let reference_gas_price = env.epoch_info.rgp(epoch)?;

        info!("Object Versions: {:#?}", object_versions);
        let input_objects =
            get_input_objects_for_replay(env, &txn_data, tx_digest, &object_versions)?;

        let digest = tx_digest.parse().map_err(|e| {
            let err = format!("{:?}", e);
            let digest = tx_digest.to_string();
            ReplayError::FailedToParseDigest { digest, err }
        })?;

        let gas_data = txn_data.gas_data().clone();
        let sender = txn_data.sender();
        let kind = txn_data.into_kind();

        let protocol_config = env
            .epoch_info
            .protocol_config(epoch, env.data_store.chain())
            .unwrap_or_else(|e| panic!("Failed to get protocl config: {:?}", e));
        let executor =
            ReplayExecutor::new(protocol_config, None).unwrap_or_else(|e| panic!("{:?}", e));

        Ok(Self {
            executor,
            digest,
            kind,
            epoch,
            epoch_start_timestamp,
            sender,
            input_objects,
            gas_data,
            reference_gas_price,
        })
    }
}

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
                                .as_type_tag()
                                .map_err(|err| ReplayError::GenericError {
                                    err: format!("{:?}", err),
                                })?;
                        packages_from_type_tag(&typ, &mut packages);
                    }
                }
                Command::MakeMoveVec(type_input, _) => {
                    if let Some(t) = type_input {
                        let typ = t.as_type_tag().map_err(|err| ReplayError::GenericError {
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

fn get_input_ids(txn_data: &TransactionData) -> Result<BTreeSet<InputObject>, ReplayError> {
    let mut all_objects = txn_data
        .gas_data()
        .payment
        .iter()
        .map(|(id, seq_num, _)| InputObject {
            object_id: *id,
            version: Some(seq_num.value()),
        })
        .collect::<BTreeSet<_>>();
    if let TransactionKind::ProgrammableTransaction(ptb) = txn_data.kind() {
        let input_objects = ptb
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
                            // Some(InputObject { object_id: *id, version: Some(initial_shared_version.value()) })
                            None
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
        all_objects.extend(input_objects);
        Ok(all_objects)
    } else {
        Ok(all_objects)
    }
}

fn get_effects_ids(effects: &TransactionEffects) -> Result<BTreeSet<InputObject>, ReplayError> {
    // let object_ids = effects
    //     .object_changes()
    //     .iter()
    //     .filter_map(|object_change| {
    //         object_change.input_version
    //         .and_then(|seq_num| {
    //             info!("Object changes: {:?}[{}]", object_change.id, seq_num.value());
    //             Some(InputObject {
    //                 object_id: object_change.id,
    //                 version: Some(seq_num.value()),
    //             })
    //         })
    //     })
    //     .collect::<BTreeSet<_>>();
    let mut object_ids = effects
        .modified_at_versions()
        .iter()
        .map(|(obj_id, seq_num)| {
            debug!("Modified at version: {:?}[{}]", obj_id, seq_num.value());
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
            sui_types::effects::UnchangedSharedKind::MutateDeleted(_)
            | sui_types::effects::UnchangedSharedKind::ReadDeleted(_)
            | sui_types::effects::UnchangedSharedKind::Cancelled(_)
            | sui_types::effects::UnchangedSharedKind::PerEpochConfig => {
                debug!("Unchanged shared kind: {:?}", kind);
            }
        });
    Ok(object_ids)
}

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
            InputObjectKind::MovePackage(_) => (),
            InputObjectKind::ImmOrOwnedMoveObject((obj_id, _version, _digest)) => {
                let version = *object_versions.get(obj_id).ok_or_else(|| {
                    ReplayError::ObjectVersionNotFound {
                        address: obj_id.to_string(),
                        version: None,
                    }
                })?;
                // let version = version.value();
                let object =
                    env.objects
                        .get(obj_id)
                        .ok_or_else(|| ReplayError::ObjectNotFound {
                            address: obj_id.to_string(),
                            version: Some(version),
                        })
                        .and_then(|versions| {
                            versions.get(&version).cloned().ok_or_else(|| {
                                ReplayError::ObjectNotFound {
                                    address: obj_id.to_string(),
                                    version: Some(version),
                                }
                            })
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
                        env.objects
                            .get(id)
                            .ok_or_else(|| ReplayError::ObjectNotFound {
                                address: id.to_string(),
                                version: Some(version),
                            })
                            .and_then(|versions| {
                                versions.get(&version).cloned().ok_or_else(|| {
                                    ReplayError::ObjectNotFound {
                                        address: id.to_string(),
                                        version: Some(version),
                                    }
                                })
                            })?,
                    );
                    resolved_input_objs.push(ObjectReadResult {
                        input_object_kind,
                        object,
                    });
                } else {
                    let (digest, version) = deleted_shared_info_map.get(id).unwrap();
                    let object = ObjectReadResultKind::DeletedSharedObject(*version, *digest);
                    resolved_input_objs.push(ObjectReadResult {
                        input_object_kind,
                        object,
                    });
                }
            }
        }
    }
    info!("resolved input objects: {:#?}", resolved_input_objs);
    Ok(InputObjects::new(resolved_input_objs))
}
