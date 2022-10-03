// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_core_types::{
    effects::Op, language_storage::StructTag, value::MoveTypeLayout, vm_status::StatusCode,
};
use move_vm_types::{
    loaded_data::runtime_types::Type,
    values::{GlobalValue, StructRef, Value},
};
use std::collections::BTreeMap;
use sui_types::{
    base_types::ObjectID,
    object::{Data, MoveObject},
    storage::ChildObjectResolver,
};

pub(super) struct ChildObject {
    pub(super) owner: ObjectID,
    pub(super) ty: Type,
    pub(super) tag: StructTag,
    pub(super) value: GlobalValue,
}

pub(super) struct ObjectStore<'a> {
    resolver: Box<dyn ChildObjectResolver + 'a>,
    cached_objects: BTreeMap<ObjectID, Option<MoveObject>>,
    store: BTreeMap<ObjectID, ChildObject>,
}

pub(crate) enum ObjectResult<V> {
    // object exists but type does not match. Should result in an abort
    MismatchedType,
    Loaded(V),
}

impl<'a> ObjectStore<'a> {
    pub(super) fn new(resolver: Box<dyn ChildObjectResolver + 'a>) -> Self {
        Self {
            resolver,
            cached_objects: BTreeMap::new(),
            store: BTreeMap::new(),
        }
    }

    fn get_or_fetch_object_from_store(
        &mut self,
        parent: ObjectID,
        child: ObjectID,
    ) -> PartialVMResult<Option<&MoveObject>> {
        if !self.cached_objects.contains_key(&child) {
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
            self.cached_objects.insert(child, obj_opt);
        }
        Ok(self.cached_objects.get(&child).unwrap().as_ref())
    }

    fn fetch_object_impl(
        &mut self,
        parent: ObjectID,
        child: ObjectID,
        child_ty: &Type,
        child_ty_layout: MoveTypeLayout,
        child_tag: StructTag,
    ) -> PartialVMResult<ObjectResult<(Type, StructTag, GlobalValue)>> {
        let obj = match self.get_or_fetch_object_from_store(parent, child)? {
            None => {
                return Ok(ObjectResult::Loaded((
                    child_ty.clone(),
                    child_tag,
                    GlobalValue::none(),
                )))
            }
            Some(obj) => obj,
        };
        // object exists, but the type does not match
        if obj.type_ != child_tag {
            return Ok(ObjectResult::MismatchedType);
        }
        let v = match Value::simple_deserialize(obj.contents(), &child_ty_layout) {
            Some(v) => v,
            None => {
                return Err(
                    PartialVMError::new(StatusCode::FAILED_TO_DESERIALIZE_RESOURCE).with_message(
                        format!("Failed to deserialize object {child} with type {child_tag}",),
                    ),
                )
            }
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
            child_tag.clone(),
            global_value,
        )))
    }

    pub(super) fn object_exists(
        &mut self,
        parent: ObjectID,
        child: ObjectID,
    ) -> PartialVMResult<bool> {
        if self.store.get(&child).is_some() {
            return Ok(true);
        }
        Ok(self
            .get_or_fetch_object_from_store(parent, child)?
            .is_some())
    }

    pub(super) fn object_exists_and_has_type(
        &mut self,
        parent: ObjectID,
        child: ObjectID,
        child_tag: StructTag,
    ) -> PartialVMResult<bool> {
        if let Some(child_object) = self.store.get(&child) {
            return Ok(&child_object.tag == &child_tag);
        }
        Ok(self
            .get_or_fetch_object_from_store(parent, child)?
            .map(|move_obj| move_obj.type_ == child_tag)
            .unwrap_or(false))
    }

    pub(super) fn get_or_fetch_object(
        &mut self,
        parent: ObjectID,
        child: ObjectID,
        child_ty: &Type,
        child_layout: MoveTypeLayout,
        child_tag: StructTag,
    ) -> PartialVMResult<ObjectResult<&mut ChildObject>> {
        if !self.store.contains_key(&child) {
            let (ty, tag, value) =
                match self.fetch_object_impl(parent, child, child_ty, child_layout, child_tag)? {
                    ObjectResult::MismatchedType => return Ok(ObjectResult::MismatchedType),
                    ObjectResult::Loaded(res) => res,
                };
            let prev = self.store.insert(
                child,
                ChildObject {
                    owner: parent,
                    ty,
                    tag,
                    value,
                },
            );
            assert!(prev.is_none())
        }
        Ok(ObjectResult::Loaded(self.store.get_mut(&child).unwrap()))
    }

    // retrieve the `Op` effects for the child objects
    pub(super) fn take_effects(
        &mut self,
    ) -> BTreeMap<ObjectID, (/* Owner */ ObjectID, Type, StructTag, Op<Value>)> {
        std::mem::take(&mut self.store)
            .into_iter()
            .filter_map(|(id, child_object)| {
                let ChildObject {
                    owner,
                    ty,
                    tag,
                    value,
                } = child_object;
                let effect = value.into_effect()?;
                Some((id, (owner, ty, tag, effect)))
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
