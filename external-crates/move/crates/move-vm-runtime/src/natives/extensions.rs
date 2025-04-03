// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use better_any::{Tid, TidExt};
use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_core_types::vm_status::StatusCode;
use std::{any::TypeId, collections::HashMap, rc::Rc};

/// A helper macro around a `Tid`able type that encapsulates interior mutability in a single-threaded
/// manner.
///
/// Note that this is _not_ threadsafe. If you need threadsafe access to the `T` you will need to
/// handle that within `T'`s type (just like in the previous implementation of the
/// `NativeContextExtensions`).
///
/// This is a macro as opposed to a generic struct type due to Rust restricions that don't allow
/// trait implementations on foreign types, coupled with the requirement that elements in the
/// `NativeContextExtensions` must have `NativeExtensionMarker` implemented on them.
#[macro_export]
macro_rules! derive_mutable_native_extension {
    ($name:ident, $mutable_name:ident) => {
        #[derive(Tid)]
        pub struct $mutable_name<'a>(pub std::cell::RefCell<$name<'a>>);

        impl<'a> $mutable_name<'a> {
            /// Create a new `NativeContextMut` value with the given value.
            pub fn new(t: $name<'a>) -> Self {
                $mutable_name(std::cell::RefCell::new(t))
            }

            pub fn into_inner(self) -> $name<'a> {
                self.0.into_inner()
            }
        }

        impl<'a> std::ops::Deref for $mutable_name<'a> {
            type Target = std::cell::RefCell<$name<'a>>;
            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }
    };
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

/// A marker trait that is used to identify a native extension. We use this as opposed to `TidAble`
/// since TidAble has auto implementations for various wrappers around a `TidAbles` which we don't
/// want. This must be implemented on the _exact_ type that is being added to the extensions
/// otherwise it will fail statically.
pub trait NativeExtensionMarker<'a>: Tid<'a> {}

impl<'a> NativeContextExtensions<'a> {
    pub fn add<T: NativeExtensionMarker<'a>>(&mut self, ext: T) {
        assert!(
            self.map.insert(T::id(), Rc::new(ext)).is_none(),
            "multiple extensions of the same type not allowed"
        )
    }

    pub fn get<T: NativeExtensionMarker<'a>>(&self) -> PartialVMResult<&T> {
        self.map
            .get(&T::id())
            .ok_or_else(|| {
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("native extension not found".to_string())
            })
            .and_then(|t| {
                t.as_ref().downcast_ref::<T>().ok_or_else(|| {
                    PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                        .with_message("downcast error".to_string())
                })
            })
    }

    pub fn remove<T: NativeExtensionMarker<'a>>(&mut self) -> PartialVMResult<Rc<T>> {
        // can't use expect below because it requires `T: Debug`.
        self.map
            .remove(&T::id())
            .ok_or_else(|| {
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("native extension not found".to_string())
            })
            .and_then(|t| {
                t.downcast_rc::<T>().map_err(|_| {
                    PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                        .with_message("downcast error".to_string())
                })
            })
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

    impl<'a> NativeExtensionMarker<'a> for NativeContextMut<'a, Ext<'a>> {}

    #[test]
    fn non_static_ext() {
        let mut v: u64 = 23;
        let e = Ext { a: &mut v };
        let mut exts = NativeContextExtensions::default();
        exts.add(NativeContextMut::new(e));
        *exts.get::<NativeContextMut<Ext>>().unwrap().borrow_mut().a += 1;
        assert_eq!(
            *exts.get::<NativeContextMut<Ext>>().unwrap().borrow_mut().a,
            24
        );
        *exts.get::<NativeContextMut<Ext>>().unwrap().borrow_mut().a += 1;
        let e1 = exts.get::<NativeContextMut<Ext>>().unwrap();
        assert_eq!(*e1.borrow().a, 25);
    }
}
