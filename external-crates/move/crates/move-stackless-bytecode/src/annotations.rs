// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::{
    any::{Any, TypeId},
    collections::BTreeMap,
    fmt::{Debug, Formatter, Result},
    rc::Rc,
};

use itertools::Itertools;

/// A container for an extensible, dynamically typed set of annotations, that may or may not have
/// reached a fixedpoint state
#[derive(Default, Clone)]
pub struct Annotations {
    map: BTreeMap<TypeId, (Data, bool)>,
}

/// An internal struct to represent annotation data. This carries in addition to the
/// dynamically typed value a function for cloning this value. This works
/// around the restriction that we cannot use a trait to call into an Any type, so we need
/// to maintain the "vtable" by ourselves.
struct Data {
    value: Box<dyn Any>,
    clone_fun: Rc<dyn Fn(&Box<dyn Any>) -> Box<dyn Any>>,
}

impl Data {
    fn new<T: Any + Clone>(x: T) -> Self {
        let clone_fun = Rc::new(|x: &Box<dyn Any>| -> Box<dyn Any> {
            Box::new(x.downcast_ref::<T>().unwrap().clone())
        });
        Self {
            value: Box::new(x),
            clone_fun,
        }
    }
}

impl Clone for Data {
    fn clone(&self) -> Self {
        Self {
            value: (self.clone_fun)(&self.value),
            clone_fun: self.clone_fun.clone(),
        }
    }
}

impl Debug for Annotations {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(
            f,
            "annotations{{{}}}",
            self.map.keys().map(|t| format!("{:?}", t)).join(", ")
        )
    }
}

impl Annotations {
    /// Tests whether annotation of type T is present.
    pub fn has<T: Any>(&self) -> bool {
        let id = TypeId::of::<T>();
        self.map.contains_key(&id)
    }

    /// Gets annotation of type T.
    pub fn get<T: Any>(&self) -> Option<&T> {
        let id = TypeId::of::<T>();
        self.map
            .get(&id)
            .and_then(|(d, _)| d.value.downcast_ref::<T>())
    }

    /// Gets annotation of type T or creates one from default.
    pub fn get_or_default_mut<T: Any + Default + Clone>(&mut self, fixedpoint: bool) -> &mut T {
        let id = TypeId::of::<T>();
        let (data, _) = self
            .map
            .entry(id)
            .or_insert_with(|| (Data::new(T::default()), fixedpoint));
        data.value.downcast_mut::<T>().expect("cast successful")
    }

    /// Sets annotation of type T.
    pub fn set<T: Any + Clone>(&mut self, x: T, fixedpoint: bool) {
        let id = TypeId::of::<T>();
        self.map.insert(id, (Data::new(x), fixedpoint));
    }

    /// Removes annotation of type T.
    pub fn remove<T: Any>(&mut self) -> Option<Box<T>> {
        let id = TypeId::of::<T>();
        self.map
            .remove(&id)
            .and_then(|(d, _)| d.value.downcast::<T>().ok())
    }

    /// Mark whether the annotations all reach a fixedpoint status
    pub fn reached_fixedpoint(&self) -> bool {
        self.map.values().all(|(_, fixedpoint)| *fixedpoint)
    }
}
