// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use sui_json_rpc_types::ObjectChange;
use sui_types::base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress};
use sui_types::effects::ObjectRemoveKind;
use sui_types::effects::{TransactionEffects, TransactionEffectsAPI};
use sui_types::object::Owner;
use sui_types::storage::WriteKind;
use tracing::instrument;

use crate::ObjectProvider;

#[instrument(skip_all, fields(transaction_digest = %effects.transaction_digest()))]
pub async fn get_object_changes<P: ObjectProvider<Error = E>, E>(
    object_provider: &P,
    effects: &TransactionEffects,
    sender: SuiAddress,
    modified_at_versions: Vec<(ObjectID, SequenceNumber)>,
    all_changed_objects: Vec<(ObjectRef, Owner, WriteKind)>,
    all_removed_objects: Vec<(ObjectRef, ObjectRemoveKind)>,
) -> Result<Vec<ObjectChange>, E> {
    let mut object_changes = vec![];

    let modify_at_version = modified_at_versions.into_iter().collect::<BTreeMap<_, _>>();

    for ((object_id, version, digest), owner, kind) in all_changed_objects {
        let o = object_provider.get_object(&object_id, &version).await?;
        if let Some(type_) = o.type_() {
            let object_type = type_.clone().into();

            match kind {
                WriteKind::Mutate => object_changes.push(ObjectChange::Mutated {
                    sender,
                    owner,
                    object_type,
                    object_id,
                    version,
                    // modify_at_version should always be available for mutated object
                    previous_version: modify_at_version
                        .get(&object_id)
                        .cloned()
                        .unwrap_or_default(),
                    digest,
                }),
                WriteKind::Create => object_changes.push(ObjectChange::Created {
                    sender,
                    owner,
                    object_type,
                    object_id,
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

    for ((id, version, _), kind) in all_removed_objects {
        let o = object_provider
            .find_object_lt_or_eq_version(&id, &version)
            .await?;
        if let Some(o) = o {
            if let Some(type_) = o.type_() {
                let object_type = type_.clone().into();
                match kind {
                    ObjectRemoveKind::Delete => object_changes.push(ObjectChange::Deleted {
                        sender,
                        object_type,
                        object_id: id,
                        version,
                    }),
                    ObjectRemoveKind::Wrap => object_changes.push(ObjectChange::Wrapped {
                        sender,
                        object_type,
                        object_id: id,
                        version,
                    }),
                }
            }
        };
    }

    Ok(object_changes)
}
