// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::ObjectProvider;
use sui_json_rpc_types::ObjectChange;
use sui_types::base_types::{MoveObjectType, SuiAddress};
use sui_types::governance::StakedSui;
use sui_types::messages::TransactionEffectsAPI;
use sui_types::storage::{DeleteKind, WriteKind};

pub async fn get_object_change_from_effect<P: ObjectProvider<Error = E>, E>(
    object_provider: &P,
    sender: SuiAddress,
    effects: &impl TransactionEffectsAPI,
) -> Result<Vec<ObjectChange>, E> {
    let mut object_changes = vec![];

    for ((id, version, digest), owner, kind) in effects.all_mutated() {
        let o = object_provider.get_object(id, version).await?;
        if let Some(type_) = o.type_() {
            let type_ = match type_ {
                MoveObjectType::Other(type_) => Some(type_.clone()),
                MoveObjectType::StakedSui => Some(StakedSui::type_()),
                _ => None,
            };

            if let Some(object_type) = type_ {
                match kind {
                    WriteKind::Mutate => object_changes.push(ObjectChange::Mutated {
                        sender,
                        owner: *owner,
                        object_type,
                        object_id: *id,
                        version: *version,
                        digest: *digest,
                    }),
                    WriteKind::Create => object_changes.push(ObjectChange::Created {
                        sender,
                        owner: *owner,
                        object_type,
                        object_id: *id,
                        version: *version,
                        digest: *digest,
                    }),
                    _ => {}
                }
            }
        } else if let Some(p) = o.data.try_as_package() {
            if kind == WriteKind::Create {
                object_changes.push(ObjectChange::Published {
                    package_id: p.id(),
                    version: p.version(),
                    digest: *digest,
                    modules: p.serialized_module_map().keys().cloned().collect(),
                })
            }
        };
    }

    for ((id, version, _), kind) in effects.all_deleted() {
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
