// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use sui_json_rpc_types::{ObjectChange, OwnedObjectRef, SuiObjectRef};
use sui_types::base_types::{MoveObjectType, ObjectID, SequenceNumber, SuiAddress};
use sui_types::coin::Coin;
use sui_types::gas_coin::GasCoin;
use sui_types::governance::StakedSui;
use sui_types::storage::{DeleteKind, WriteKind};

use crate::balance_changes::ObjectProvider;

pub async fn get_object_changes<P: ObjectProvider<Error = E>, E>(
    object_provider: &P,
    sender: SuiAddress,
    modified_at_versions: &[(ObjectID, SequenceNumber)],
    all_changed_objects: Vec<(&OwnedObjectRef, WriteKind)>,
    all_deleted: Vec<(&SuiObjectRef, DeleteKind)>,
) -> Result<Vec<ObjectChange>, E> {
    let mut object_changes = vec![];
    let modify_at_version = modified_at_versions
        .iter()
        .cloned()
        .collect::<BTreeMap<_, _>>();

    for (owner_object_ref, kind) in all_changed_objects {
        let id = owner_object_ref.reference.object_id;
        let version = owner_object_ref.reference.version;
        let digest = owner_object_ref.reference.digest;
        let owner = owner_object_ref.owner;

        let o = object_provider.get_object(&id, &version).await?;
        if let Some(type_) = o.type_() {
            let object_type = match type_ {
                MoveObjectType::Other(type_) => type_.clone(),
                MoveObjectType::StakedSui => StakedSui::type_(),
                MoveObjectType::GasCoin => GasCoin::type_(),
                MoveObjectType::Coin(t) => Coin::type_(t.clone()),
            };

            match kind {
                WriteKind::Mutate => object_changes.push(ObjectChange::Mutated {
                    sender,
                    owner,
                    object_type,
                    object_id: id,
                    version,
                    // modify_at_version should always be available for mutated object
                    previous_version: modify_at_version.get(&id).cloned().unwrap_or_default(),
                    digest,
                }),
                WriteKind::Create => object_changes.push(ObjectChange::Created {
                    sender,
                    owner,
                    object_type,
                    object_id: id,
                    version,
                    digest,
                }),
                _ => {}
            }
        } else if let Some(p) = o.data.try_as_package() {
            if kind == WriteKind::Create {
                object_changes.push(ObjectChange::Published {
                    package_id: p.id(),
                    version: p.version(),
                    digest,
                    modules: p.serialized_module_map().keys().cloned().collect(),
                })
            }
        };
    }

    for (object_ref, kind) in all_deleted {
        let id = &object_ref.object_id;
        let version = &object_ref.version;
        let o = object_provider
            .find_object_lt_or_eq_version(id, version)
            .await?;
        if let Some(o) = o {
            if let Some(type_) = o.type_() {
                let type_ = match type_ {
                    MoveObjectType::Other(type_) => Some(type_.clone()),
                    MoveObjectType::StakedSui => Some(StakedSui::type_()),
                    _ => None,
                };
                if let Some(object_type) = type_ {
                    match kind {
                        DeleteKind::Normal => object_changes.push(ObjectChange::Deleted {
                            sender,
                            object_type,
                            object_id: *id,
                            version: *version,
                        }),
                        DeleteKind::Wrap => object_changes.push(ObjectChange::Wrapped {
                            sender,
                            object_type,
                            object_id: *id,
                            version: *version,
                        }),
                        _ => {}
                    }
                }
            }
        };
    }

    Ok(object_changes)
}
