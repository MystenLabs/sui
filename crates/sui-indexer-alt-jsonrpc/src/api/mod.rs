// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_json_rpc_types::ObjectChange as SuiObjectChange;
use sui_types::base_types::ObjectID;
use sui_types::base_types::SequenceNumber;
use sui_types::base_types::SuiAddress;
use sui_types::digests::ObjectDigest;
use sui_types::effects::IDOperation;
use sui_types::object::Object;

pub(crate) mod checkpoints;
pub(crate) mod coin;
pub(crate) mod dynamic_fields;
pub(crate) mod governance;
pub(crate) mod move_utils;
pub(crate) mod name_service;
pub(crate) mod objects;
pub(crate) mod rpc_module;
pub(crate) mod transactions;
pub mod write;

/// Map an object change from the effects to a `SuiObjectChange` response type. Returns `None` for
/// changes that are not visible in the response (created-and-wrapped, unwrapped, unwrapped-then-
/// deleted, system package upgrades).
pub(crate) fn to_sui_object_change(
    sender: SuiAddress,
    object_id: ObjectID,
    id_operation: IDOperation,
    input: Option<(Object, ObjectDigest)>,
    output: Option<(Object, ObjectDigest)>,
    lamport_version: SequenceNumber,
) -> anyhow::Result<Option<SuiObjectChange>> {
    use IDOperation as ID;

    let change = match (id_operation, &input, &output) {
        (ID::Created, Some((i, _)), _) => anyhow::bail!(
            "Unexpected input version {} for object {object_id} created by transaction",
            i.version().value(),
        ),

        (ID::Deleted, _, Some((o, _))) => anyhow::bail!(
            "Unexpected output version {} for object {object_id} deleted by transaction",
            o.version().value(),
        ),

        // Created but no output object: created and immediately wrapped.
        (ID::Created, _, None) => return Ok(None),
        // No ID change and no input: unwrapped object (or unwrapped-then-deleted).
        (ID::None, None, _) => return Ok(None),
        // System package upgrade (happens in-place, not a user-visible change).
        (ID::None, _, Some((o, _))) if o.is_package() => return Ok(None),
        // Deleted but no input: unwrapped-then-deleted.
        (ID::Deleted, None, _) => return Ok(None),

        (ID::Created, _, Some((o, d))) if o.is_package() => SuiObjectChange::Published {
            package_id: object_id,
            version: o.version(),
            digest: *d,
            modules: o
                .data
                .try_as_package()
                .unwrap() // SAFETY: Match guard checks that the object is a package.
                .serialized_module_map()
                .keys()
                .cloned()
                .collect(),
        },

        (ID::Created, _, Some((o, d))) => SuiObjectChange::Created {
            sender,
            owner: o.owner().clone(),
            object_type: o
                .struct_tag()
                .ok_or_else(|| anyhow::anyhow!("No type for object {object_id}"))?,
            object_id,
            version: o.version(),
            digest: *d,
        },

        (ID::None, Some((i, _)), Some((o, od))) if i.owner() != o.owner() => {
            SuiObjectChange::Transferred {
                sender,
                recipient: o.owner().clone(),
                object_type: o
                    .struct_tag()
                    .ok_or_else(|| anyhow::anyhow!("No type for object {object_id}"))?,
                object_id,
                version: o.version(),
                digest: *od,
            }
        }

        (ID::None, Some((i, _)), Some((o, od))) => SuiObjectChange::Mutated {
            sender,
            owner: o.owner().clone(),
            object_type: o
                .struct_tag()
                .ok_or_else(|| anyhow::anyhow!("No type for object {object_id}"))?,
            object_id,
            version: o.version(),
            previous_version: i.version(),
            digest: *od,
        },

        (ID::None, Some((i, _)), None) => SuiObjectChange::Wrapped {
            sender,
            object_type: i
                .struct_tag()
                .ok_or_else(|| anyhow::anyhow!("No type for object {object_id}"))?,
            object_id,
            version: lamport_version,
        },

        (ID::Deleted, Some((i, _)), None) => SuiObjectChange::Deleted {
            sender,
            object_type: i
                .struct_tag()
                .ok_or_else(|| anyhow::anyhow!("No type for object {object_id}"))?,
            object_id,
            version: lamport_version,
        },
    };

    Ok(Some(change))
}
