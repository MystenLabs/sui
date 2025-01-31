// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use better_any::{Tid, TidAble, TidExt};
use std::{
    any::TypeId,
    cell::{Ref, RefCell, RefMut},
    collections::HashMap,
    ops::{Deref, DerefMut},
    rc::Rc,
};

/// A helper wrapper around a `Tid`able type that encapsulates interior mutability in a single-threaded
/// manner.
///
/// Note that this is _not_ threadsafe. If you need threadsafe access to the `T` you will need to
/// handle that within `T'`s type (just like in the previous implementation of the
/// `NativeContextExtensions`).
#[derive(Tid)]
pub struct NativeContextMut<'a, T: Tid<'a>>(pub RefCell<T>, std::marker::PhantomData<&'a ()>);

impl<'a, T: Tid<'a>> NativeContextMut<'a, T> {
    /// Create a new `NativeContextMut` value with the given value.
    pub fn new(t: T) -> Self {
        NativeContextMut(RefCell::new(t), std::marker::PhantomData)
    }

    /// Get the inner value by `&mut`.
    pub fn get_mut(&self) -> RefMut<T> {
        self.0.borrow_mut()
    }

    /// Get the inner value by `&`.
    pub fn get(&self) -> Ref<T> {
        self.0.borrow()
    }

    pub fn into_inner(self) -> T {
        self.0.into_inner()
    }
}

impl<'a, T: Tid<'a>> Deref for NativeContextMut<'a, T> {
    type Target = RefCell<T>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a, T: Tid<'a>> DerefMut for NativeContextMut<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// A data type to represent a heterogeneous collection of extensions which are available to
/// native functions. A value to this is passed into the session function execution.
///
/// The implementation uses the crate `better_any` which implements a version of the `Any`
/// type, called `Tid<`a>`, which allows for up to one lifetime parameter. This
/// avoids that extensions need to have `'static` lifetime, which `Any` requires. In order to make a
/// struct suitable to be a 'Tid', use `#[derive(Tid)]` in the struct declaration. (See also
/// tests at the end of this module.)
#[derive(Default, Clone)]
pub struct NativeContextExtensions<'a> {
    map: HashMap<TypeId, Rc<dyn Tid<'a>>>,
}

impl<'a> NativeContextExtensions<'a> {
    pub fn add<T: TidAble<'a>>(&mut self, ext: T) {
        assert!(
            self.map.insert(T::id(), Rc::new(ext)).is_none(),
            "multiple extensions of the same type not allowed"
        )
    }

    pub fn get<T: TidAble<'a>>(&self) -> &T {
        self.map
            .get(&T::id())
            .expect("extension unknown")
            .as_ref()
            .downcast_ref::<T>()
            .unwrap()
    }

    pub fn remove<T: TidAble<'a>>(&mut self) -> Rc<T> {
        // can't use expect below because it requires `T: Debug`.
        match self
            .map
            .remove(&T::id())
            .expect("extension unknown")
            .downcast_rc::<T>()
        {
            Ok(val) => val,
            Err(_) => panic!("downcast error"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use better_any::{Tid, TidAble};

    #[derive(Tid)]
    struct Ext<'a> {
        a: &'a mut u64,
    }

    #[test]
    fn non_static_ext() {
        let mut v: u64 = 23;
        let e = Ext { a: &mut v };
        let mut exts = NativeContextExtensions::default();
        exts.add(NativeContextMut::new(e));
        *exts.get::<NativeContextMut<Ext>>().get_mut().a += 1;
        assert_eq!(*exts.get::<NativeContextMut<Ext>>().get_mut().a, 24);
        *exts.get::<NativeContextMut<Ext>>().get_mut().a += 1;
        let e1 = exts.get::<NativeContextMut<Ext>>();
        assert_eq!(*e1.get().a, 25);
    }
}
