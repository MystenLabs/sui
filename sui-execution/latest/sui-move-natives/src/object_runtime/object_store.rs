// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::object_runtime::get_all_uids;
use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_core_types::{
    annotated_value as A, effects::Op, runtime_value as R, vm_status::StatusCode,
};
use move_vm_types::{
    loaded_data::runtime_types::Type,
    values::{GlobalValue, StructRef, Value},
};
use std::{
    collections::{btree_map, BTreeMap},
    sync::Arc,
};
use sui_protocol_config::{check_limit_by_meter, LimitThresholdCrossed, ProtocolConfig};
use sui_types::{
    base_types::{MoveObjectType, ObjectID, SequenceNumber},
    committee::EpochId,
    error::VMMemoryLimitExceededSubStatusCode,
    execution::DynamicallyLoadedObjectMetadata,
    metrics::LimitsMetrics,
    object::{Data, MoveObject, Object, Owner},
    storage::ChildObjectResolver,
};

pub(super) struct ChildObject {
    pub(super) owner: ObjectID,
    pub(super) ty: Type,
    pub(super) move_type: MoveObjectType,
    pub(super) value: GlobalValue,
}

pub(crate) struct ActiveChildObject<'a> {
    pub(crate) id: &'a ObjectID,
    pub(crate) owner: &'a ObjectID,
    pub(crate) ty: &'a Type,
    pub(crate) move_type: &'a MoveObjectType,
    pub(crate) copied_value: Option<Value>,
}

#[derive(Debug)]
struct ConfigSetting {
    config: ObjectID,
    ty: MoveObjectType,
    value: Value,
}

#[derive(Debug)]
pub(crate) struct ChildObjectEffectV0 {
    pub(super) owner: ObjectID,
    pub(super) ty: Type,
    pub(super) effect: Op<Value>,
}

pub(crate) enum ChildObjectEffects {
    // In this version, we accurately track mutations via WriteRef to the child object, or
    // references rooted in the child object.
    V0(BTreeMap<ObjectID, ChildObjectEffectV0>),
}

struct Inner<'a> {
    // used for loading child objects
    resolver: &'a dyn ChildObjectResolver,
    // The version of the root object in ownership at the beginning of the transaction.
    // If it was a child object, it resolves to the root parent's sequence number.
    // Otherwise, it is just the sequence number at the beginning of the transaction.
    root_version: BTreeMap<ObjectID, SequenceNumber>,
    // A map from a wrapped object to the object it was contained in at the
    // beginning of the transaction.
    wrapped_object_containers: BTreeMap<ObjectID, ObjectID>,
    // cached objects from the resolver. An object might be in this map but not in the store
    // if it's existence was queried, but the value was not used.
    cached_objects: BTreeMap<ObjectID, Option<Object>>,
    // whether or not this TX is gas metered
    is_metered: bool,
    // Protocol config used to enforce limits
    protocol_config: &'a ProtocolConfig,
    // Metrics for reporting exceeded limits
    metrics: Arc<LimitsMetrics>,
    // Epoch ID for the current transaction. Used for receiving objects.
    current_epoch_id: EpochId,
}

// maintains the runtime GlobalValues for child objects and manages the fetching of objects
// from storage, through the `ChildObjectResolver`
pub(super) struct ChildObjectStore<'a> {
    // contains object resolver and object cache
    // kept as a separate struct to deal with lifetime issues where the `store` is accessed
    // at the same time as the `cached_objects` is populated
    inner: Inner<'a>,
    // Maps of populated GlobalValues, meaning the child object has been accessed in this
    // transaction
    store: BTreeMap<ObjectID, ChildObject>,
    config_setting_cache: BTreeMap<ObjectID, ConfigSetting>,
    // whether or not this TX is gas metered
    is_metered: bool,
}

pub(crate) enum ObjectResult<V> {
    // object exists but type does not match. Should result in an abort
    MismatchedType,
    Loaded(V),
}

type LoadedWithMetadataResult<V> = Option<(V, DynamicallyLoadedObjectMetadata)>;

macro_rules! fetch_child_object_unbounded {
    ($inner:ident, $parent:ident, $child:ident, $parents_root_version:expr, $had_parent_root_version:expr) => {{
        let child_opt = $inner
            .resolver
            .read_child_object(&$parent, &$child, $parents_root_version)
            .map_err(|msg| {
                PartialVMError::new(StatusCode::STORAGE_ERROR).with_message(format!("{msg}"))
            })?;
        if let Some(object) = child_opt {
            // if there was no root version, guard against reading a child object. A newly
            // created parent should not have a child in storage
            if !$had_parent_root_version {
                return Err(
                    PartialVMError::new(StatusCode::STORAGE_ERROR).with_message(format!(
                        "A new parent {} should not have a child object {}.",
                        $parent, $child
                    )),
                );
            }
            // guard against bugs in `read_child_object`: if it returns a child object such that
            // C.parent != parent, we raise an invariant violation
            match &object.owner {
                Owner::ObjectOwner(id) => {
                    if ObjectID::from(*id) != $parent {
                        return Err(PartialVMError::new(StatusCode::STORAGE_ERROR).with_message(
                            format!(
                                "Bad owner for {}. Expected owner {} but found owner {}",
                                $child, $parent, id
                            ),
                        ));
                    }
                }
                Owner::AddressOwner(_)
                | Owner::Immutable
                | Owner::Shared { .. }
                | Owner::ConsensusV2 { .. } => {
                    return Err(PartialVMError::new(StatusCode::STORAGE_ERROR).with_message(
                        format!(
                            "Bad owner for {}. \
                            Expected an id owner {} but found an address, \
                            immutable, or shared owner",
                            $child, $parent
                        ),
                    ))
                }
            };
            match object.data {
                Data::Package(_) => {
                    return Err(PartialVMError::new(StatusCode::STORAGE_ERROR).with_message(
                        format!(
                            "Mismatched object type for {}. \
                            Expected a Move object but found a Move package",
                            $child
                        ),
                    ))
                }
                Data::Move(_) => Some(object),
            }
        } else {
            None
        }
    }};
}

impl<'a> Inner<'a> {
    fn receive_object_from_store(
        &self,
        owner: ObjectID,
        child: ObjectID,
        version: SequenceNumber,
    ) -> PartialVMResult<LoadedWithMetadataResult<MoveObject>> {
        let child_opt = self
            .resolver
            .get_object_received_at_version(
                &owner,
                &child,
                version,
                self.current_epoch_id,
                self.protocol_config
                    .use_object_per_epoch_marker_table_v2_as_option()
                    .unwrap_or(false),
            )
            .map_err(|msg| {
                PartialVMError::new(StatusCode::STORAGE_ERROR).with_message(format!("{msg}"))
            })?;
        let obj_opt = if let Some(object) = child_opt {
            // guard against bugs in `receive_object_at_version`: if it returns a child object such that
            // C.parent != parent, we raise an invariant violation since that should be checked by
            // `receive_object_at_version`.
            if object.owner != Owner::AddressOwner(owner.into()) {
                return Err(
                    PartialVMError::new(StatusCode::STORAGE_ERROR).with_message(format!(
                        "Bad owner for {child}. \
                        Expected owner {owner} but found owner {}",
                        object.owner
                    )),
                );
            }
            let loaded_metadata = DynamicallyLoadedObjectMetadata {
                version,
                digest: object.digest(),
                storage_rebate: object.storage_rebate,
                owner: object.owner.clone(),
                previous_transaction: object.previous_transaction,
            };

            // `ChildObjectResolver::receive_object_at_version` should return the object at the
            // version or nothing at all. If it returns an object with a different version, we
            // should raise an invariant violation since it should be checked by
            // `receive_object_at_version`.
            if object.version() != version {
                return Err(
                    PartialVMError::new(StatusCode::STORAGE_ERROR).with_message(format!(
                        "Bad version for {child}. \
                        Expected version {version} but found version {}",
                        object.version()
                    )),
                );
            }
            match object.into_inner().data {
                Data::Package(_) => {
                    return Err(PartialVMError::new(StatusCode::STORAGE_ERROR).with_message(
                        format!(
                            "Mismatched object type for {child}. \
                                Expected a Move object but found a Move package"
                        ),
                    ))
                }
                Data::Move(mo @ MoveObject { .. }) => Some((mo, loaded_metadata)),
            }
        } else {
            None
        };
        Ok(obj_opt)
    }

    #[allow(clippy::map_entry)]
    fn get_or_fetch_object_from_store(
        &mut self,
        parent: ObjectID,
        child: ObjectID,
    ) -> PartialVMResult<Option<&MoveObject>> {
        let cached_objects_count = self.cached_objects.len() as u64;
        let parents_root_version = self.root_version.get(&parent).copied();
        let had_parent_root_version = parents_root_version.is_some();
        // if not found, it must be new so it won't have any child objects, thus
        // we can return SequenceNumber(0) as no child object will be found
        let parents_root_version = parents_root_version.unwrap_or(SequenceNumber::new());
        if let btree_map::Entry::Vacant(e) = self.cached_objects.entry(child) {
            let obj_opt = fetch_child_object_unbounded!(
                self,
                parent,
                child,
                parents_root_version,
                had_parent_root_version
            );

            if let LimitThresholdCrossed::Hard(_, lim) = check_limit_by_meter!(
                self.is_metered,
                cached_objects_count,
                self.protocol_config.object_runtime_max_num_cached_objects(),
                self.protocol_config
                    .object_runtime_max_num_cached_objects_system_tx(),
                self.metrics.excessive_object_runtime_cached_objects
            ) {
                return Err(PartialVMError::new(StatusCode::MEMORY_LIMIT_EXCEEDED)
                    .with_message(format!(
                        "Object runtime cached objects limit ({} entries) reached",
                        lim
                    ))
                    .with_sub_status(
                        VMMemoryLimitExceededSubStatusCode::OBJECT_RUNTIME_CACHE_LIMIT_EXCEEDED
                            as u64,
                    ));
            };

            e.insert(obj_opt);
        }
        Ok(self
            .cached_objects
            .get(&child)
            .unwrap()
            .as_ref()
            .map(|obj| {
                obj.data
                    .try_as_move()
                    // unwrap safe because we only insert Move objects
                    .unwrap()
            }))
    }

    fn fetch_object_impl(
        &mut self,
        parent: ObjectID,
        child: ObjectID,
        child_ty: &Type,
        child_ty_layout: &R::MoveTypeLayout,
        child_ty_fully_annotated_layout: &A::MoveTypeLayout,
        child_move_type: &MoveObjectType,
    ) -> PartialVMResult<ObjectResult<(Type, GlobalValue)>> {
        let obj = match self.get_or_fetch_object_from_store(parent, child)? {
            None => {
                return Ok(ObjectResult::Loaded((
                    child_ty.clone(),
                    GlobalValue::none(),
                )))
            }
            Some(obj) => obj,
        };
        // object exists, but the type does not match
        if obj.type_() != child_move_type {
            return Ok(ObjectResult::MismatchedType);
        }
        // generate a GlobalValue
        let obj_contents = obj.contents();
        let v = match Value::simple_deserialize(obj_contents, child_ty_layout) {
            Some(v) => v,
            None => return Err(
                PartialVMError::new(StatusCode::FAILED_TO_DESERIALIZE_RESOURCE).with_message(
                    format!("Failed to deserialize object {child} with type {child_move_type}",),
                ),
            ),
        };
        let global_value =
            match GlobalValue::cached(v) {
                Ok(gv) => gv,
                Err(e) => {
                    return Err(PartialVMError::new(StatusCode::STORAGE_ERROR).with_message(
                        format!("Object {child} did not deserialize to a struct Value. Error: {e}"),
                    ))
                }
            };
        // Find all UIDs inside of the value and update the object parent maps
        let contained_uids =
            get_all_uids(child_ty_fully_annotated_layout, obj_contents).map_err(|e| {
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message(format!("Failed to find UIDs. ERROR: {e}"))
            })?;
        let parents_root_version = self.root_version.get(&parent).copied();
        if let Some(v) = parents_root_version {
            debug_assert!(contained_uids.contains(&child));
            for id in contained_uids {
                self.root_version.insert(id, v);
                if id != child {
                    let prev = self.wrapped_object_containers.insert(id, child);
                    debug_assert!(prev.is_none())
                }
            }
        }
        Ok(ObjectResult::Loaded((child_ty.clone(), global_value)))
    }
}

fn deserialize_move_object(
    obj: &MoveObject,
    child_ty: &Type,
    child_ty_layout: &R::MoveTypeLayout,
    child_move_type: MoveObjectType,
) -> PartialVMResult<ObjectResult<(Type, MoveObjectType, Value)>> {
    let child_id = obj.id();
    // object exists, but the type does not match
    if obj.type_() != &child_move_type {
        return Ok(ObjectResult::MismatchedType);
    }
    let value = match Value::simple_deserialize(obj.contents(), child_ty_layout) {
        Some(v) => v,
        None => {
            return Err(
                PartialVMError::new(StatusCode::FAILED_TO_DESERIALIZE_RESOURCE).with_message(
                    format!("Failed to deserialize object {child_id} with type {child_move_type}",),
                ),
            )
        }
    };
    Ok(ObjectResult::Loaded((
        child_ty.clone(),
        child_move_type,
        value,
    )))
}

impl<'a> ChildObjectStore<'a> {
    pub(super) fn new(
        resolver: &'a dyn ChildObjectResolver,
        root_version: BTreeMap<ObjectID, SequenceNumber>,
        wrapped_object_containers: BTreeMap<ObjectID, ObjectID>,
        is_metered: bool,
        protocol_config: &'a ProtocolConfig,
        metrics: Arc<LimitsMetrics>,
        current_epoch_id: EpochId,
    ) -> Self {
        Self {
            inner: Inner {
                resolver,
                root_version,
                wrapped_object_containers,
                cached_objects: BTreeMap::new(),
                is_metered,
                protocol_config,
                metrics,
                current_epoch_id,
            },
            store: BTreeMap::new(),
            config_setting_cache: BTreeMap::new(),
            is_metered,
        }
    }

    pub(super) fn receive_object(
        &mut self,
        parent: ObjectID,
        child: ObjectID,
        child_version: SequenceNumber,
        child_ty: &Type,
        child_layout: &R::MoveTypeLayout,
        child_fully_annotated_layout: &A::MoveTypeLayout,
        child_move_type: MoveObjectType,
    ) -> PartialVMResult<LoadedWithMetadataResult<ObjectResult<Value>>> {
        let Some((obj, obj_meta)) =
            self.inner
                .receive_object_from_store(parent, child, child_version)?
        else {
            return Ok(None);
        };

        Ok(Some(
            match deserialize_move_object(&obj, child_ty, child_layout, child_move_type)? {
                ObjectResult::MismatchedType => (ObjectResult::MismatchedType, obj_meta),
                ObjectResult::Loaded((_, _, v)) => {
                    // Find all UIDs inside of the value and update the object parent maps with the contained
                    // UIDs in the received value. They should all have an upper bound version as the receiving object.
                    // Only do this if we successfully load the object though.
                    let contained_uids = get_all_uids(child_fully_annotated_layout, obj.contents())
                        .map_err(|e| {
                            PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                                .with_message(format!(
                                    "Failed to find UIDs for receiving object. ERROR: {e}"
                                ))
                        })?;
                    for id in contained_uids {
                        self.inner.root_version.insert(id, child_version);
                        if id != child {
                            let prev = self.inner.wrapped_object_containers.insert(id, child);
                            debug_assert!(prev.is_none())
                        }
                    }
                    (ObjectResult::Loaded(v), obj_meta)
                }
            },
        ))
    }

    pub(super) fn object_exists(
        &mut self,
        parent: ObjectID,
        child: ObjectID,
    ) -> PartialVMResult<bool> {
        if let Some(child_object) = self.store.get(&child) {
            return child_object.value.exists();
        }
        Ok(self
            .inner
            .get_or_fetch_object_from_store(parent, child)?
            .is_some())
    }

    pub(super) fn object_exists_and_has_type(
        &mut self,
        parent: ObjectID,
        child: ObjectID,
        child_move_type: &MoveObjectType,
    ) -> PartialVMResult<bool> {
        if let Some(child_object) = self.store.get(&child) {
            // exists and has same type
            return Ok(child_object.value.exists()? && &child_object.move_type == child_move_type);
        }
        Ok(self
            .inner
            .get_or_fetch_object_from_store(parent, child)?
            .map(|move_obj| move_obj.type_() == child_move_type)
            .unwrap_or(false))
    }

    pub(super) fn get_or_fetch_object(
        &mut self,
        parent: ObjectID,
        child: ObjectID,
        child_ty: &Type,
        child_layout: &R::MoveTypeLayout,
        child_fully_annotated_layout: &A::MoveTypeLayout,
        child_move_type: MoveObjectType,
    ) -> PartialVMResult<ObjectResult<&mut ChildObject>> {
        let store_entries_count = self.store.len() as u64;
        let child_object = match self.store.entry(child) {
            btree_map::Entry::Vacant(e) => {
                let (ty, value) = match self.inner.fetch_object_impl(
                    parent,
                    child,
                    child_ty,
                    child_layout,
                    child_fully_annotated_layout,
                    &child_move_type,
                )? {
                    ObjectResult::MismatchedType => return Ok(ObjectResult::MismatchedType),
                    ObjectResult::Loaded(res) => res,
                };

                if let LimitThresholdCrossed::Hard(_, lim) = check_limit_by_meter!(
                    self.is_metered,
                    store_entries_count,
                    self.inner
                        .protocol_config
                        .object_runtime_max_num_store_entries(),
                    self.inner
                        .protocol_config
                        .object_runtime_max_num_store_entries_system_tx(),
                    self.inner.metrics.excessive_object_runtime_store_entries
                ) {
                    return Err(PartialVMError::new(StatusCode::MEMORY_LIMIT_EXCEEDED)
                        .with_message(format!(
                            "Object runtime store limit ({} entries) reached",
                            lim
                        ))
                        .with_sub_status(
                            VMMemoryLimitExceededSubStatusCode::OBJECT_RUNTIME_STORE_LIMIT_EXCEEDED
                                as u64,
                        ));
                };

                e.insert(ChildObject {
                    owner: parent,
                    ty,
                    move_type: child_move_type,
                    value,
                })
            }
            btree_map::Entry::Occupied(e) => {
                let child_object = e.into_mut();
                if child_object.move_type != child_move_type {
                    return Ok(ObjectResult::MismatchedType);
                }
                child_object
            }
        };
        Ok(ObjectResult::Loaded(child_object))
    }

    pub(super) fn add_object(
        &mut self,
        parent: ObjectID,
        child: ObjectID,
        child_ty: &Type,
        child_move_type: MoveObjectType,
        child_value: Value,
    ) -> PartialVMResult<()> {
        if let LimitThresholdCrossed::Hard(_, lim) = check_limit_by_meter!(
            self.is_metered,
            self.store.len(),
            self.inner
                .protocol_config
                .object_runtime_max_num_store_entries(),
            self.inner
                .protocol_config
                .object_runtime_max_num_store_entries_system_tx(),
            self.inner.metrics.excessive_object_runtime_store_entries
        ) {
            return Err(PartialVMError::new(StatusCode::MEMORY_LIMIT_EXCEEDED)
                .with_message(format!(
                    "Object runtime store limit ({} entries) reached",
                    lim
                ))
                .with_sub_status(
                    VMMemoryLimitExceededSubStatusCode::OBJECT_RUNTIME_STORE_LIMIT_EXCEEDED as u64,
                ));
        };

        let mut value = if let Some(ChildObject { value, .. }) = self.store.remove(&child) {
            if value.exists()? {
                return Err(
                    PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                        .with_message(
                            "Duplicate addition of a child object. \
                            The previous value cannot be dropped. Indicates possible duplication \
                            of objects as an object was fetched more than once from two different \
                            parents, yet was not removed from one first"
                                .to_string(),
                        ),
                );
            }
            value
        } else {
            GlobalValue::none()
        };
        if let Err((e, _)) = value.move_to(child_value) {
            return Err(
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR).with_message(
                    format!("Unable to set value for child {child}, with error {e}",),
                ),
            );
        }
        let child_object = ChildObject {
            owner: parent,
            ty: child_ty.clone(),
            move_type: child_move_type,
            value,
        };
        self.store.insert(child, child_object);
        Ok(())
    }

    pub(super) fn config_setting_unsequenced_read(
        &mut self,
        config_id: ObjectID,
        name_df_id: ObjectID,
        _field_setting_ty: &Type,
        field_setting_layout: &R::MoveTypeLayout,
        field_setting_object_type: &MoveObjectType,
    ) -> PartialVMResult<ObjectResult<Option<Value>>> {
        let parent = config_id;
        let child = name_df_id;

        let setting = match self.config_setting_cache.entry(child) {
            btree_map::Entry::Vacant(e) => {
                let child_move_type = field_setting_object_type;
                let inner = &self.inner;
                let obj_opt =
                    fetch_child_object_unbounded!(inner, parent, child, SequenceNumber::MAX, true);
                let Some(move_obj) = obj_opt.as_ref().map(|obj| obj.data.try_as_move().unwrap())
                else {
                    return Ok(ObjectResult::Loaded(None));
                };
                let Some(value) =
                    Value::simple_deserialize(move_obj.contents(), field_setting_layout)
                else {
                    return Err(
                        PartialVMError::new(StatusCode::FAILED_TO_DESERIALIZE_RESOURCE)
                            .with_message(format!(
                            "Failed to deserialize object {child} with type {field_setting_layout}",
                        )),
                    );
                };
                e.insert(ConfigSetting {
                    config: parent,
                    ty: child_move_type.clone(),
                    value,
                })
            }
            btree_map::Entry::Occupied(e) => {
                let setting = e.into_mut();
                if setting.ty != *field_setting_object_type {
                    return Ok(ObjectResult::MismatchedType);
                }
                if setting.config != parent {
                    return Err(
                        PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                            .with_message(format!(
                                "Parent for config setting changed. Potential hash collision?
                                parent: {parent},
                                child: {child},
                                setting_value_object_type: {field_setting_object_type},
                                setting: {setting:#?}"
                            )),
                    );
                }
                setting
            }
        };
        let value = setting.value.copy_value().map_err(|e| {
            PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR).with_message(
                format!("Failed to copy value for config setting {child}, with error {e}",),
            )
        })?;
        Ok(ObjectResult::Loaded(Some(value)))
    }

    /// Used by test scenario to insert a config setting into the cache, which replicates the
    /// behavior of a config already being in the object store.
    pub(super) fn config_setting_cache_update(
        &mut self,
        config_id: ObjectID,
        name_df_id: ObjectID,
        setting_value_object_type: MoveObjectType,
        value: Option<Value>,
    ) {
        let child_move_type = setting_value_object_type;
        match value {
            Some(value) => {
                let setting = ConfigSetting {
                    config: config_id,
                    ty: child_move_type,
                    value,
                };
                self.config_setting_cache.insert(name_df_id, setting);
            }
            None => {
                self.config_setting_cache.remove(&name_df_id);
            }
        }
    }

    pub(super) fn cached_objects(&self) -> &BTreeMap<ObjectID, Option<Object>> {
        &self.inner.cached_objects
    }

    pub(super) fn wrapped_object_containers(&self) -> &BTreeMap<ObjectID, ObjectID> {
        &self.inner.wrapped_object_containers
    }

    // retrieve the `Op` effects for the child objects
    pub(super) fn take_effects(&mut self) -> ChildObjectEffects {
        let v0_effects = std::mem::take(&mut self.store)
            .into_iter()
            .filter_map(|(id, child_object)| {
                let ChildObject {
                    owner,
                    ty,
                    move_type: _,
                    value,
                } = child_object;
                let effect = value.into_effect()?;
                let child_effect = ChildObjectEffectV0 { owner, ty, effect };
                Some((id, child_effect))
            })
            .collect();
        ChildObjectEffects::V0(v0_effects)
    }

    pub(super) fn all_active_objects(&self) -> impl Iterator<Item = ActiveChildObject<'_>> {
        self.store.iter().map(|(id, child_object)| {
            let copied_child_value = if child_object.value.exists().unwrap() {
                Some(
                    child_object
                        .value
                        .borrow_global()
                        .unwrap()
                        .value_as::<StructRef>()
                        .unwrap()
                        .read_ref()
                        .unwrap(),
                )
            } else {
                None
            };
            ActiveChildObject {
                id,
                owner: &child_object.owner,
                ty: &child_object.ty,
                move_type: &child_object.move_type,
                copied_value: copied_child_value,
            }
        })
    }
}

impl ChildObjectEffects {
    pub(crate) fn empty() -> Self {
        ChildObjectEffects::V0(BTreeMap::new())
    }
}
