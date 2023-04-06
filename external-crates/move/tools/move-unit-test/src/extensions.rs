// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! This module manages native extensions supported by the unit testing framework.
//! Such extensions are enabled by cfg features and must be compiled into the test
//! to be usable.

use move_vm_runtime::native_extensions::NativeContextExtensions;
use once_cell::sync::Lazy;
use std::{fmt::Write, sync::Mutex};

#[cfg(feature = "table-extension")]
use itertools::Itertools;
#[cfg(feature = "table-extension")]
use move_table_extension::NativeTableContext;
#[cfg(feature = "table-extension")]
use move_vm_test_utils::BlankStorage;

static EXTENSION_HOOK: Lazy<
    Mutex<Option<Box<dyn Fn(&mut NativeContextExtensions<'_>) + Send + Sync>>>,
> = Lazy::new(|| Mutex::new(None));

/// Sets a hook which is called to populate additional native extensions. This can be used to
/// get extensions living outside of the Move repo into the unit testing environment.
///
/// This need to be called with the extensions of the custom Move environment at two places:
///
/// (a) At start of a custom Move CLI, to enable unit testing with the additional
/// extensions;
/// (b) Before `cli::run_move_unit_tests` if unit tests are called programmatically from Rust.
/// You may want to define a new function `my_cli::run_move_unit_tests` which does this.
///
/// Note that the table extension is handled already internally, and does not need to added via
/// this hook.
pub fn set_extension_hook(p: Box<dyn Fn(&mut NativeContextExtensions<'_>) + Send + Sync>) {
    *EXTENSION_HOOK.lock().unwrap() = Some(p)
}

/// Create all available native context extensions.
#[allow(unused_mut, clippy::let_and_return)]
pub(crate) fn new_extensions<'a>() -> NativeContextExtensions<'a> {
    let mut e = NativeContextExtensions::default();
    if let Some(h) = &*EXTENSION_HOOK.lock().unwrap() {
        (*h)(&mut e)
    }
    #[cfg(feature = "table-extension")]
    create_table_extension(&mut e);
    e
}

/// Print the change sets for available native context extensions.
#[allow(unused)]
pub(crate) fn print_change_sets<W: Write>(_w: &mut W, mut extensions: NativeContextExtensions) {
    #[cfg(feature = "table-extension")]
    print_table_extension(_w, &mut extensions);
}

// =============================================================================================
// Table Extensions

#[cfg(feature = "table-extension")]
fn create_table_extension(extensions: &mut NativeContextExtensions) {
    extensions.add(NativeTableContext::new([0u8; 32], &*DUMMY_RESOLVER));
}

#[cfg(feature = "table-extension")]
fn print_table_extension<W: Write>(w: &mut W, extensions: &mut NativeContextExtensions) {
    let cs = extensions.remove::<NativeTableContext>().into_change_set();
    if let Ok(cs) = cs {
        if !cs.new_tables.is_empty() {
            writeln!(
                w,
                "new tables {}",
                cs.new_tables
                    .iter()
                    .map(|(k, v)| format!("{}<{},{}>", k, v.key_type, v.value_type))
                    .join(", ")
            )
            .unwrap();
        }
        if !cs.removed_tables.is_empty() {
            writeln!(
                w,
                "removed tables {}",
                cs.removed_tables.iter().map(|h| h.to_string()).join(", ")
            )
            .unwrap();
        }
        for (h, c) in cs.changes {
            writeln!(w, "for {}", h).unwrap();
            for (k, v) in c.entries {
                writeln!(w, "  {:X?} := {:X?}", k, v).unwrap();
            }
        }
    }
}

#[cfg(feature = "table-extension")]
static DUMMY_RESOLVER: Lazy<BlankStorage> = Lazy::new(|| BlankStorage);

#[cfg(test)]
mod tests {
    use crate::extensions::{new_extensions, set_extension_hook};
    use better_any::{Tid, TidAble};
    use move_vm_runtime::native_extensions::NativeContextExtensions;

    /// A test that extension hooks work as expected.
    #[test]
    fn test_extension_hook() {
        set_extension_hook(Box::new(my_hook));
        let ext = new_extensions();
        let _e = ext.get::<TestExtension>();
    }

    #[derive(Tid)]
    struct TestExtension();

    fn my_hook(ext: &mut NativeContextExtensions) {
        ext.add(TestExtension())
    }
}
