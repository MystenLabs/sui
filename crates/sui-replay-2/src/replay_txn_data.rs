// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    data_store::InputObject, 
    environment::ReplayEnvironment, 
    errors::ReplayError, 
    execution::ReplayExecutor,
};
use std::collections::{BTreeMap, BTreeSet};
use sui_types::{
    base_types::{ObjectID, SequenceNumber, SuiAddress}, digests::{ObjectDigest, TransactionDigest}, effects::{TransactionEffects, TransactionEffectsAPI}, transaction::{
        CallArg, Command, InputObjectKind, InputObjects, ObjectArg, ObjectReadResult, ObjectReadResultKind, TransactionData, TransactionDataAPI, TransactionKind
    }
};
use tracing::info;

// #[derive(Debug)]
pub struct ReplayTransaction {
    pub executor: ReplayExecutor,
    pub digest: TransactionDigest,
    pub kind: TransactionKind,
    pub epoch: u64,
    pub epoch_start_timestamp: u64,
    pub sender: SuiAddress,
    pub input_objects: InputObjects,
    pub gas: Vec<(ObjectID, SequenceNumber, ObjectDigest)>,
    pub gas_budget: u64,
    pub gas_price: u64,
    pub reference_gas_price: u64,
}

impl ReplayTransaction {
    pub async fn load(
        env: &mut ReplayEnvironment,
        tx_digest: &str,
    ) -> Result<Self, ReplayError> {
        // load transaction data and effects
        let txn_data = env
            .data_store
            .transaction_data(tx_digest)
            .await?;
        info!("Transaction data: {:#?}", txn_data); 
        let effects = env
            .data_store
            .transaction_effects(tx_digest)
            .await?;
        info!("Transaction effects: {:#?}", effects);

        let (input_object_ids, packages) = get_input_ids(&txn_data)?;
        info!("Input Object IDs: {:#?}", input_object_ids);
        let effects_object_ids = get_effects_ids(&effects)?;
        info!("Effects Object IDs: {:#?}", effects_object_ids);
        let mut input_versions = effects_object_ids.into_iter().map(|input| {
            (input.object_id, input.version.unwrap())
        })
        .collect::<BTreeMap<_, _>>();
        for input_object in input_object_ids.iter() {
            input_versions.entry(input_object.object_id).or_insert(input_object.version.unwrap());
        }
        let object_versions = input_versions
            .into_iter()
            .map(|(object_id, version)| InputObject{ object_id, version: Some(version) })
            .collect::<BTreeSet<InputObject>>();

        env.load_objects(&object_versions).await?;
        env.load_packages(&packages).await?;

        let epoch = effects.executed_epoch();
        let epoch_start_timestamp = env.epoch_info.epoch_timestamp(epoch)?;
        let reference_gas_price = env.epoch_info.rgp(epoch)?;

        info!("Object Versions: {:#?}", object_versions);
        let input_objects = get_input_objects_for_replay(env, &txn_data, tx_digest, &object_versions)?;

        let digest = tx_digest.parse().map_err(|e| {
            let err = format!("{:?}", e);
            let digest = tx_digest.to_string();
            ReplayError::FailedToParseDigest {digest, err}
        })?;

        let gas = txn_data.gas_data().payment.clone();
        let sender = txn_data.sender();
        let gas_price = txn_data.gas_price();
        let gas_budget = txn_data.gas_budget();
        let kind = txn_data.into_kind();

        let protocol_config = env.epoch_info.protocol_config(epoch, env.data_store.chain())
            .unwrap_or_else(|e| panic!("Failed to get protocl config: {:?}", e));
        let executor = ReplayExecutor::new(protocol_config, None)
            .unwrap_or_else(|e| panic!("{:?}", e));

        Ok(
            Self {
                executor,
                digest,
                kind,
                epoch,
                epoch_start_timestamp,
                sender,
                input_objects,
                gas,
                gas_budget,
                gas_price,
                reference_gas_price,
            },
        )
    }
}

fn get_input_ids(
    txn_data: &TransactionData,
) -> Result<(BTreeSet<InputObject>, BTreeSet<ObjectID>), ReplayError> {
    let mut all_objects = txn_data
        .gas_data()
        .payment
        .iter()
        .map(|(id, seq_num, _)| {
            InputObject {
                object_id: *id,
                version: Some(seq_num.value()),
            }
        })
        .collect::<BTreeSet<_>>();
    if let TransactionKind::ProgrammableTransaction(ptb) = txn_data.kind() {
        let input_objects = ptb
            .inputs
            .iter()
            .filter_map(|input| {
                if let CallArg::Object(call_arg) = input {
                    match call_arg {
                        ObjectArg::ImmOrOwnedObject((id, seq_num, _digest)) => {
                            Some(InputObject { object_id: *id, version: Some(seq_num.value()) })
                        }
                        ObjectArg::SharedObject {
                            id: _,
                            initial_shared_version: _,
                            mutable: _,
                        } => {
                            // Some(InputObject { object_id: *id, version: Some(initial_shared_version.value()) })
                            None
                        }
                        ObjectArg::Receiving((id, seq_num, _digest)) => {
                            Some(InputObject { object_id: *id, version: Some(seq_num.value()) })
                        }
                    }
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        all_objects.extend(input_objects);

        let packages = ptb
            .commands
            .iter()
            .filter_map(|cmd| {
                if let Command::MoveCall(move_call) = cmd {
                    Some(move_call.package)
                } else {
                    None
                }
            })
            .collect::<BTreeSet<_>>();
        Ok((all_objects, packages))
    } else {
        Ok((all_objects, BTreeSet::new()))
    }
}

fn get_effects_ids(effects: &TransactionEffects) -> Result<BTreeSet<InputObject>, ReplayError>  {
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
    let object_ids = effects
        .modified_at_versions()
        .iter()
        .filter_map(|(obj_id, seq_num)| {
            info!("Modified at version: {:?}[{}]", obj_id, seq_num.value());
            Some(InputObject {
                object_id: *obj_id,
                version: Some(seq_num.value()),
            })
        })
        .collect::<BTreeSet<_>>();
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
        .map(|input| {
            (input.object_id, input.version.unwrap())
        })
        .collect::<BTreeMap<_, _>>(); 
    let deleted_shared_info_map: BTreeMap<ObjectID, (TransactionDigest, SequenceNumber)> = BTreeMap::new();
    let mut resolved_input_objs = vec![];
    let input_objects_kind = txn
        .input_objects()
        .map_err(|e| ReplayError::InputObjectsError {
            digest: format!("{:?}", tx_digest),
            err: format!("{:?}", e),
        })?;
    for kind in input_objects_kind.iter() {
        match kind {
            InputObjectKind::MovePackage(object_id) => {
                let package = env
                    .package_objects
                    .get(object_id)
                    .ok_or_else(|| {
                        ReplayError::PackageNotFound {
                            pkg: object_id.to_string(),
                        }
                    })?;
                resolved_input_objs.push(ObjectReadResult {
                    input_object_kind: InputObjectKind::MovePackage(*object_id),
                    object: ObjectReadResultKind::Object(package.clone()),
                });
            }
            InputObjectKind::ImmOrOwnedMoveObject((obj_id, _version, _digest)) => {
                let version = *object_versions
                    .get(obj_id)
                    .ok_or_else(|| {
                        ReplayError::ObjectVersionNotFound {
                            address: obj_id.to_string(),
                            version: None,
                        }
                    })?;                        
                // let version = version.value();
                let object = env
                    .objects
                    .get(obj_id)
                    .ok_or_else(|| {
                        ReplayError::ObjectNotFound {
                            address: obj_id.to_string(),
                            version: Some(version),
                        }
                    })
                    .and_then(|versions| {
                        versions
                            .get(&version)
                            .cloned()
                            .ok_or_else(|| {
                                ReplayError::ObjectNotFound {
                                    address: obj_id.to_string(),
                                    version: Some(version),
                                }
                            })
                    })?;                        
                let input_object_kind = InputObjectKind::ImmOrOwnedMoveObject(
                    object.compute_object_reference(),
                );
                resolved_input_objs.push(ObjectReadResult {
                    input_object_kind,
                    object: ObjectReadResultKind::Object(object),
                });
            }
            InputObjectKind::SharedMoveObject{ id, initial_shared_version, mutable} => {
                let input_object_kind = InputObjectKind::SharedMoveObject {
                    id: *id,
                    initial_shared_version: *initial_shared_version,
                    mutable: *mutable,
                };
                let version = *object_versions
                    .get(id)
                    .ok_or_else(|| {
                        ReplayError::ObjectVersionNotFound {
                            address: id.to_string(),
                            version: None,
                        }
                    })?;                        
                // let version = initial_shared_version.value();
                let is_deleted = deleted_shared_info_map.contains_key(id);
                if !is_deleted {
                    let object = ObjectReadResultKind::Object(env
                        .objects
                        .get(id)
                        .ok_or_else(|| {
                            ReplayError::ObjectNotFound {
                                address: id.to_string(),
                                version: Some(version),
                            }
                        })
                        .and_then(|versions| {
                            versions
                                .get(&version)
                                .cloned()
                                .ok_or_else(|| {
                                    ReplayError::ObjectNotFound {
                                        address: id.to_string(),
                                        version: Some(version),
                                    }
                                })
                        })?);                        
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
