// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use better_any::{Tid, TidExt};
use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_core_types::vm_status::StatusCode;
use std::{any::TypeId, collections::HashMap};

/// A data type to represent a heterogeneous collection of extensions which are available to
/// native functions. A value to this is passed into the session function execution.
///
/// The implementation uses the crate `better_any` which implements a version of the `Any`
/// type, called `Tid<`a>`, which allows for up to one lifetime parameter. This
/// avoids that extensions need to have `'static` lifetime, which `Any` requires. In order to make a
/// struct suitable to be a 'Tid', use `#[derive(Tid)]` in the struct declaration. (See also
/// tests at the end of this module.)
#[derive(Default)]
pub struct NativeContextExtensions<'a> {
    map: HashMap<TypeId, Box<dyn Tid<'a>>>,
}

/// A marker trait that is used to identify a native extension. We use this as opposed to `TidAble`
/// since TidAble has auto implementations for various wrappers around a `TidAbles` which we don't
/// want. This must be implemented on the _exact_ type that is being added to the extensions
/// otherwise it will fail statically.
pub trait NativeExtensionMarker<'a>: Tid<'a> {}

impl<'a> NativeContextExtensions<'a> {
    pub fn add<T: NativeExtensionMarker<'a>>(&mut self, ext: T) {
        assert!(
            self.map.insert(T::id(), Box::new(ext)).is_none(),
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

    pub fn get_mut<T: NativeExtensionMarker<'a>>(&mut self) -> PartialVMResult<&mut T> {
        self.map
            .get_mut(&T::id())
            .ok_or_else(|| {
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("native extension not found".to_string())
            })
            .and_then(|t| {
                t.as_mut().downcast_mut::<T>().ok_or_else(|| {
                    PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                        .with_message("downcast error".to_string())
                })
            })
    }

    pub fn remove<T: NativeExtensionMarker<'a>>(&mut self) -> PartialVMResult<T> {
        // can't use expect below because it requires `T: Debug`.
        self.map
            .remove(&T::id())
            .ok_or_else(|| {
                PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                    .with_message("native extension not found".to_string())
            })
            .and_then(|t| {
                t.downcast_box::<T>().map(|t| *t).map_err(|_| {
                    PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                        .with_message("downcast error".to_string())
                })
            })
    }
}

#[cfg(test)]
mod tests {
    use crate::native_extensions::{NativeContextExtensions, NativeExtensionMarker};
    use better_any::{Tid, TidAble};

    #[derive(Tid)]
    struct Ext<'a> {
        a: &'a mut u64,
    }

    impl<'a> NativeExtensionMarker<'a> for Ext<'a> {}

    #[test]
    fn non_static_ext() {
        let mut v: u64 = 23;
        let e = Ext { a: &mut v };
        let mut exts = NativeContextExtensions::default();
        exts.add(e);
        *exts.get_mut::<Ext>().unwrap().a += 1;
        assert_eq!(*exts.get_mut::<Ext>().unwrap().a, 24);
        *exts.get_mut::<Ext>().unwrap().a += 1;
        let e1 = exts.remove::<Ext>().unwrap();
        assert_eq!(*e1.a, 25)
    }
}
