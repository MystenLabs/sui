// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_core_types::{effects::Op, value::MoveTypeLayout, vm_status::StatusCode};
use move_vm_types::{
    loaded_data::runtime_types::Type,
    values::{GlobalValue, StructRef, Value},
};
use std::collections::{btree_map, BTreeMap};
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    base_types::{MoveObjectType, ObjectID, SequenceNumber},
    error::VMMemoryLimitExceededSubStatusCode,
    object::{Data, MoveObject},
    storage::ChildObjectResolver,
};

pub(super) struct ChildObject {
    pub(super) owner: ObjectID,
    pub(super) ty: Type,
    pub(super) move_type: MoveObjectType,
    pub(super) value: GlobalValue,
}

pub(crate) struct ChildObjectEffect {
    pub(super) owner: ObjectID,
    // none if it was an input object
    pub(super) loaded_version: Option<SequenceNumber>,
    pub(super) ty: Type,
    pub(super) move_type: MoveObjectType,
    pub(super) effect: Op<Value>,
}

struct Inner<'a> {
    // used for loading child objects
    resolver: Box<dyn ChildObjectResolver + 'a>,
    // cached objects from the resolver. An object might be in this map but not in the store
    // if it's existence was queried, but the value was not used.
    cached_objects: BTreeMap<ObjectID, Option<MoveObject>>,
    // This sets a limit on the number of cached objects
    max_num_cached_objects: u64,
    // whether or not this TX is gas metered
    is_metered: bool,
}

// maintains the runtime GlobalValues for child objects and manages the fetching of objects
// from storage, through the `ChildObjectResolver`
pub(super) struct ObjectStore<'a> {
    // contains object resolver and object cache
    // kept as a separate struct to deal with lifetime issues where the `store` is accessed
    // at the same time as the `cached_objects` is populated
    inner: Inner<'a>,
    // Maps of populated GlobalValues, meaning the child object has been accessed in this
    // transaction
    store: BTreeMap<ObjectID, ChildObject>,
    // This sets a limit on the number of entries in the `store`
    max_num_store_entries: u64,
    // whether or not this TX is gas metered
    is_metered: bool,
}

pub(crate) enum ObjectResult<V> {
    // object exists but type does not match. Should result in an abort
    MismatchedType,
    Loaded(V),
}

impl<'a> Inner<'a> {
    fn get_or_fetch_object_from_store(
        &mut self,
        parent: ObjectID,
        child: ObjectID,
    ) -> PartialVMResult<Option<&MoveObject>> {
        let cached_objects_count = self.cached_objects.len() as u64;
        if let btree_map::Entry::Vacant(e) = self.cached_objects.entry(child) {
            let child_opt = self
                .resolver
                .read_child_object(&parent, &child)
                .map_err(|msg| {
                    PartialVMError::new(StatusCode::STORAGE_ERROR).with_message(format!("{msg}"))
                })?;
            let obj_opt = if let Some(object) = child_opt {
                match object.data {
                    Data::Package(_) => {
                        return Err(PartialVMError::new(StatusCode::STORAGE_ERROR).with_message(
                            format!(
                                "Mismatched object type for {child}. \
                                Expected a Move object but found a Move package"
                            ),
                        ))
                    }
                    Data::Move(mo @ MoveObject { .. }) => Some(mo),
                }
            } else {
                None
            };

            if self.is_metered && cached_objects_count >= self.max_num_cached_objects {
                return Err(PartialVMError::new(StatusCode::MEMORY_LIMIT_EXCEEDED)
                    .with_message(format!(
                        "Object runtime cached objects limit ({} entries) reached",
                        self.max_num_cached_objects
                    ))
                    .with_sub_status(
                        VMMemoryLimitExceededSubStatusCode::OBJECT_RUNTIME_CACHE_LIMIT_EXCEEDED
                            as u64,
                    ));
            }

            e.insert(obj_opt);
        }
        Ok(self.cached_objects.get(&child).unwrap().as_ref())
    }

    fn fetch_object_impl(
        &mut self,
        parent: ObjectID,
        child: ObjectID,
        child_ty: &Type,
        child_ty_layout: MoveTypeLayout,
        child_move_type: MoveObjectType,
    ) -> PartialVMResult<ObjectResult<(Type, MoveObjectType, GlobalValue)>> {
        let obj = match self.get_or_fetch_object_from_store(parent, child)? {
            None => {
                return Ok(ObjectResult::Loaded((
                    child_ty.clone(),
                    child_move_type,
                    GlobalValue::none(),
                )))
            }
            Some(obj) => obj,
        };
        // object exists, but the type does not match
        if obj.type_() != &child_move_type {
            return Ok(ObjectResult::MismatchedType);
        }
        let v = match Value::simple_deserialize(obj.contents(), &child_ty_layout) {
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
        Ok(ObjectResult::Loaded((
            child_ty.clone(),
            child_move_type,
            global_value,
        )))
    }
}

impl<'a> ObjectStore<'a> {
    pub(super) fn new(
        resolver: Box<dyn ChildObjectResolver + 'a>,
        protocol_config: &ProtocolConfig,
        is_metered: bool,
    ) -> Self {
        Self {
            inner: Inner {
                resolver,
                cached_objects: BTreeMap::new(),
                max_num_cached_objects: protocol_config.object_runtime_max_num_cached_objects(),
                is_metered,
            },
            store: BTreeMap::new(),
            max_num_store_entries: protocol_config.object_runtime_max_num_store_entries(),
            is_metered,
        }
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
        child_layout: MoveTypeLayout,
        child_move_type: MoveObjectType,
    ) -> PartialVMResult<ObjectResult<&mut ChildObject>> {
        let store_entries_count = self.store.len() as u64;
        let child_object = match self.store.entry(child) {
            btree_map::Entry::Vacant(e) => {
                let (ty, move_type, value) = match self.inner.fetch_object_impl(
                    parent,
                    child,
                    child_ty,
                    child_layout,
                    child_move_type,
                )? {
                    ObjectResult::MismatchedType => return Ok(ObjectResult::MismatchedType),
                    ObjectResult::Loaded(res) => res,
                };

                if self.is_metered && store_entries_count >= self.max_num_store_entries {
                    return Err(PartialVMError::new(StatusCode::MEMORY_LIMIT_EXCEEDED)
                        .with_message(format!(
                            "Object runtime store limit ({} entries) reached",
                            self.max_num_store_entries
                        ))
                        .with_sub_status(
                            VMMemoryLimitExceededSubStatusCode::OBJECT_RUNTIME_STORE_LIMIT_EXCEEDED
                                as u64,
                        ));
                }

                e.insert(ChildObject {
                    owner: parent,
                    ty,
                    move_type,
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
        let mut child_object = ChildObject {
            owner: parent,
            ty: child_ty.clone(),
            move_type: child_move_type,
            value: GlobalValue::none(),
        };
        child_object.value.move_to(child_value).unwrap();
        if self.is_metered && self.store.len() >= self.max_num_store_entries as usize {
            return Err(PartialVMError::new(StatusCode::MEMORY_LIMIT_EXCEEDED)
                .with_message(format!(
                    "Object runtime store limit ({} entries) reached",
                    self.max_num_store_entries
                ))
                .with_sub_status(
                    VMMemoryLimitExceededSubStatusCode::OBJECT_RUNTIME_STORE_LIMIT_EXCEEDED as u64,
                ));
        }

        if let Some(prev) = self.store.insert(child, child_object) {
            if prev.value.exists()? {
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
        }
        Ok(())
    }

    // retrieve the `Op` effects for the child objects
    pub(super) fn take_effects(&mut self) -> BTreeMap<ObjectID, ChildObjectEffect> {
        std::mem::take(&mut self.store)
            .into_iter()
            .filter_map(|(id, child_object)| {
                let ChildObject {
                    owner,
                    ty,
                    move_type,
                    value,
                } = child_object;
                let loaded_version = self
                    .inner
                    .cached_objects
                    .get(&id)
                    .and_then(|obj_opt| Some(obj_opt.as_ref()?.version()));
                let effect = value.into_effect()?;
                let child_effect = ChildObjectEffect {
                    owner,
                    loaded_version,
                    ty,
                    move_type,
                    effect,
                };
                Some((id, child_effect))
            })
            .collect()
    }

    pub(super) fn all_active_objects(&self) -> impl Iterator<Item = (&ObjectID, &Type, Value)> {
        self.store.iter().filter_map(|(id, child_object)| {
            let child_exists = child_object.value.exists().unwrap();
            if !child_exists {
                None
            } else {
                let copied_child_value = child_object
                    .value
                    .borrow_global()
                    .unwrap()
                    .value_as::<StructRef>()
                    .unwrap()
                    .read_ref()
                    .unwrap();
                Some((id, &child_object.ty, copied_child_value))
            }
        })
    }
}
