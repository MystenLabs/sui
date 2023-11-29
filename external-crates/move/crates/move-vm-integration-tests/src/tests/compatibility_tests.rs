// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::{
    compatibility::{Compatibility, InclusionCheck},
    file_format::AbilitySet,
    normalized,
};
use move_ir_to_bytecode::{compiler::compile_module, parser::parse_module};

fn compile(prog: &str) -> normalized::Module {
    let prog = parse_module(prog).unwrap();
    let (compiled_module, _) = compile_module(prog, vec![]).unwrap();
    normalized::Module::new(&compiled_module)
}

// Things to test for enum upgrades
// * [x] Variant removal (never)
// * [x] Variant rename (never)
// * [x] Variant reordering (never)
// * [x] Additional field in existing variant (never)
// * [x] Remove field from existing variant (never)
// * [x] Rename field in existing variant (never)
// * [x] Change type of existing field in variant (never)
// * [x] Add new variant at beginning (w/out disallow_new_variants) (never)
// * [x] Add new variant at end (w/out disallow_new_variants, equal and subset inclusions)
//   - Allowed if `disallow_new_variants = false` or `InclusionCheck::Subset`
// * [x] Change abilities on type

#[test]
fn test_enum_upgrade_variant_removal() {
    let old = compile(
        "
        module 0x1.M {
            enum E { V { }, L { } }
        }
        ",
    );
    // Enum variant removal is not allowed
    let new = compile(
        "
        module 0x1.M {
            enum E { V { } }
        }
        ",
    );
    assert!(Compatibility::default().check(&old, &new).is_err());
    assert!(InclusionCheck::Equal.check(&old, &new).is_err());
    assert!(InclusionCheck::Subset.check(&old, &new).is_err());
}

#[test]
fn test_enum_upgrade_variant_rename() {
    let old = compile(
        "
        module 0x1.M {
            enum E { V { } }
        }
        ",
    );
    // Enum variant renaming is not allowed
    let new = compile(
        "
        module 0x1.M {
            enum E { L { } }
        }
        ",
    );
    assert!(Compatibility::default().check(&old, &new).is_err());
    assert!(InclusionCheck::Equal.check(&old, &new).is_err());
    assert!(InclusionCheck::Subset.check(&old, &new).is_err());
}

#[test]
fn test_enum_upgrade_variant_reorder() {
    let old = compile(
        "
        module 0x1.M {
            enum E { V { }, L { } }
        }
        ",
    );
    // Enum variant reordering is not allowed.
    let new = compile(
        "
        module 0x1.M {
            enum E { L { }, V { } }
        }
        ",
    );
    assert!(Compatibility::default().check(&old, &new).is_err());
    assert!(InclusionCheck::Equal.check(&old, &new).is_err());
    assert!(InclusionCheck::Subset.check(&old, &new).is_err());
}

#[test]
fn test_enum_upgrade_variant_add_field() {
    let old = compile(
        "
        module 0x1.M {
            enum E { V { } }
        }
        ",
    );
    // Adding a new field to an existing enum variant is not allowed
    let new = compile(
        "
        module 0x1.M {
            enum E { V { x: u64 } }
        }
        ",
    );
    assert!(Compatibility::default().check(&old, &new).is_err());
    assert!(InclusionCheck::Equal.check(&old, &new).is_err());
    assert!(InclusionCheck::Subset.check(&old, &new).is_err());
}

#[test]
fn test_enum_upgrade_variant_remove_field() {
    let old = compile(
        "
        module 0x1.M {
            enum E { V { x: u64 } }
        }
        ",
    );
    // Adding a new field to an existing enum variant is not allowed
    let new = compile(
        "
        module 0x1.M {
            enum E { V { } }
        }
        ",
    );
    assert!(Compatibility::default().check(&old, &new).is_err());
    assert!(InclusionCheck::Equal.check(&old, &new).is_err());
    assert!(InclusionCheck::Subset.check(&old, &new).is_err());
}

#[test]
fn test_enum_upgrade_variant_rename_field() {
    let old = compile(
        "
        module 0x1.M {
            enum E { V { x: u64 } }
        }
        ",
    );
    // Renaming a field in an existing enum variant is not allowed
    let new = compile(
        "
        module 0x1.M {
            enum E { V { y: u64 } }
        }
        ",
    );
    assert!(Compatibility::default().check(&old, &new).is_err());
    assert!(InclusionCheck::Equal.check(&old, &new).is_err());
    assert!(InclusionCheck::Subset.check(&old, &new).is_err());
}

#[test]
fn test_enum_upgrade_variant_change_field_type() {
    let old = compile(
        "
        module 0x1.M {
            enum E { V { x: u64 } }
        }
        ",
    );
    // Changing the type of an existing field in an enum variant is not allowed
    let new = compile(
        "
        module 0x1.M {
            enum E { V { x: bool } }
        }
        ",
    );
    assert!(Compatibility::default().check(&old, &new).is_err());
    assert!(InclusionCheck::Equal.check(&old, &new).is_err());
    assert!(InclusionCheck::Subset.check(&old, &new).is_err());
}

#[test]
fn test_enum_upgrade_add_variant_at_front() {
    let old = compile(
        "
        module 0x1.M {
            enum E { V { x: u64 } }
        }
        ",
    );
    // Adding a new variant at the front of the enum is not allowed (ever)
    let new = compile(
        "
        module 0x1.M {
            enum E { L { x: u64 }, V { x: u64 } }
        }
        ",
    );
    let mut compat = Compatibility::default();
    assert!(compat.disallow_new_variants);
    assert!(compat.check(&old, &new).is_err());
    compat.disallow_new_variants = false;
    assert!(compat.check(&old, &new).is_err());
    assert!(InclusionCheck::Equal.check(&old, &new).is_err());
    assert!(InclusionCheck::Subset.check(&old, &new).is_err());
}

#[test]
fn test_enum_upgrade_add_variant_at_end() {
    let old = compile(
        "
        module 0x1.M {
            enum E { V { x: u64 } }
        }
        ",
    );
    // Adding a new variant at the end of the enum is not allowed unless the
    // `disallow_new_variants` flag is set to false.
    let new = compile(
        "
        module 0x1.M {
            enum E { V { x: u64 }, L { x: u64 }}
        }
        ",
    );
    let mut compat = Compatibility::default();
    assert!(compat.disallow_new_variants);
    assert!(compat.check(&old, &new).is_err());
    // Allow adding new variants at the end of the enum
    compat.disallow_new_variants = false;
    assert!(compat.check(&old, &new).is_ok());
    assert!(InclusionCheck::Equal.check(&old, &new).is_err());
    assert!(InclusionCheck::Subset.check(&old, &new).is_ok());
}

#[test]
fn test_enum_upgrade_add_store_ability() {
    let old = compile(
        "
        module 0x1.M {
            enum E { V { x: u64 } }
        }
        ",
    );
    // Adding a new variant at the front of the enum is not allowed (ever)
    let new = compile(
        "
        module 0x1.M {
            enum E has store { V { x: u64 } }
        }
        ",
    );
    let mut compat = Compatibility::default();
    assert!(compat.check(&old, &new).is_ok());
    compat.disallowed_new_abilities = AbilitySet::ALL;
    assert!(compat.check(&old, &new).is_err());
    assert!(InclusionCheck::Equal.check(&old, &new).is_err());
    assert!(InclusionCheck::Subset.check(&old, &new).is_err());
}
