// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::convert::TryFrom;

use crate::{
    compatibility::{Compatibility, InclusionCheck},
    file_format::*,
    normalized,
};
use move_core_types::{account_address::AccountAddress, identifier::Identifier};

// A way to permute pools, and index into them still.
pub struct Permutation {
    permutation: Vec<u16>,
    inverse: Vec<u16>,
}

impl Permutation {
    pub fn new(permutation: Vec<u16>) -> Self {
        let inverse: Vec<_> = (0..permutation.len() as u16)
            .map(|i| permutation.iter().position(|p| *p == i).unwrap() as u16)
            .collect();
        Self {
            permutation,
            inverse,
        }
    }

    pub fn pool<T: Clone>(&self, pool: Vec<T>) -> Vec<T> {
        (0..pool.len() as u16)
            .map(|i| pool[*self.permutation.get(i as usize).unwrap_or(&i) as usize].clone())
            .collect()
    }

    pub fn permute(&self, i: u16) -> u16 {
        // If we don't have it in the permutation default to the identity
        *self.inverse.get(i as usize).unwrap_or(&i)
    }
}

fn mk_module(vis: u8) -> normalized::Module {
    let (visibility, is_entry) = if vis == Visibility::DEPRECATED_SCRIPT {
        (Visibility::Public, true)
    } else {
        (Visibility::try_from(vis).unwrap(), false)
    };
    let m = CompiledModule {
        version: crate::file_format_common::VERSION_4,
        module_handles: vec![
            // only self module
            ModuleHandle {
                address: AddressIdentifierIndex(0),
                name: IdentifierIndex(0),
            },
        ],
        self_module_handle_idx: ModuleHandleIndex(0),
        identifiers: vec![
            Identifier::new("M").unwrap(),  // Module name
            Identifier::new("fn").unwrap(), // Function name
        ],
        address_identifiers: vec![
            AccountAddress::ZERO, // Module address
        ],
        function_handles: vec![
            // fun fn()
            FunctionHandle {
                module: ModuleHandleIndex(0),
                name: IdentifierIndex(1),
                parameters: SignatureIndex(0),
                return_: SignatureIndex(0),
                type_parameters: vec![],
            },
        ],
        function_defs: vec![
            // public(script) fun fn() { return; }
            FunctionDefinition {
                function: FunctionHandleIndex(0),
                visibility,
                is_entry,
                acquires_global_resources: vec![],
                code: Some(CodeUnit {
                    locals: SignatureIndex(0),
                    code: vec![
                        Bytecode::LdConst(ConstantPoolIndex(0)),
                        Bytecode::LdConst(ConstantPoolIndex(1)),
                        Bytecode::LdConst(ConstantPoolIndex(2)),
                        Bytecode::Ret,
                    ],
                }),
            },
        ],
        signatures: vec![
            Signature(vec![]), // void
        ],
        struct_defs: vec![],
        struct_handles: vec![],
        constant_pool: vec![
            Constant {
                type_: SignatureToken::U8,
                data: vec![0],
            },
            Constant {
                type_: SignatureToken::U8,
                data: vec![1],
            },
            Constant {
                type_: SignatureToken::Bool,
                data: vec![1],
            },
        ],
        metadata: vec![],
        field_handles: vec![],
        friend_decls: vec![],
        struct_def_instantiations: vec![],
        function_instantiations: vec![],
        field_instantiations: vec![],
    };
    normalized::Module::new(&m)
}

fn mk_module_plus_code(vis: u8, code: Vec<Bytecode>) -> normalized::Module {
    mk_module_plus_code_perm(vis, code, Permutation::new(vec![]))
}

fn mk_module_plus_code_perm(vis: u8, code: Vec<Bytecode>, p: Permutation) -> normalized::Module {
    let (visibility, is_entry) = if vis == Visibility::DEPRECATED_SCRIPT {
        (Visibility::Public, true)
    } else {
        (Visibility::try_from(vis).unwrap(), false)
    };
    let m = CompiledModule {
        version: crate::file_format_common::VERSION_4,
        module_handles: vec![
            // only self module
            ModuleHandle {
                address: AddressIdentifierIndex(0),
                name: IdentifierIndex(p.permute(0)),
            },
        ],
        self_module_handle_idx: ModuleHandleIndex(0),
        identifiers: p.pool(vec![
            Identifier::new("M").unwrap(),   // Module name
            Identifier::new("fn").unwrap(),  // Function name
            Identifier::new("fn1").unwrap(), // Function name
        ]),
        address_identifiers: vec![
            AccountAddress::ZERO, // Module address
        ],
        function_handles: p.pool(vec![
            // fun fn()
            FunctionHandle {
                module: ModuleHandleIndex(0),
                name: IdentifierIndex(p.permute(1)),
                parameters: SignatureIndex(p.permute(0)),
                return_: SignatureIndex(p.permute(0)),
                type_parameters: vec![],
            },
            FunctionHandle {
                module: ModuleHandleIndex(0),
                name: IdentifierIndex(p.permute(2)),
                parameters: SignatureIndex(p.permute(1)),
                return_: SignatureIndex(p.permute(0)),
                type_parameters: vec![],
            },
        ]),
        function_defs: p.pool(vec![
            // public(script) fun fn1(u64) { return; }
            FunctionDefinition {
                function: FunctionHandleIndex(p.permute(1)),
                visibility,
                is_entry,
                acquires_global_resources: vec![],
                code: Some(CodeUnit {
                    locals: SignatureIndex(p.permute(0)),
                    code,
                }),
            },
            // public(script) fun fn() { return; }
            FunctionDefinition {
                function: FunctionHandleIndex(p.permute(0)),
                visibility,
                is_entry,
                acquires_global_resources: vec![],
                code: Some(CodeUnit {
                    locals: SignatureIndex(p.permute(0)),
                    code: vec![
                        Bytecode::LdConst(ConstantPoolIndex(p.permute(0))),
                        Bytecode::LdConst(ConstantPoolIndex(p.permute(1))),
                        Bytecode::LdConst(ConstantPoolIndex(p.permute(2))),
                        Bytecode::Ret,
                    ],
                }),
            },
        ]),
        signatures: p.pool(vec![
            Signature(vec![]),                    // void
            Signature(vec![SignatureToken::U64]), // u64
        ]),
        struct_defs: vec![],
        struct_handles: vec![],
        constant_pool: p.pool(vec![
            Constant {
                type_: SignatureToken::U8,
                data: vec![0],
            },
            Constant {
                type_: SignatureToken::U8,
                data: vec![1],
            },
            Constant {
                type_: SignatureToken::Bool,
                data: vec![1],
            },
        ]),
        metadata: vec![],
        field_handles: vec![],
        friend_decls: vec![],
        struct_def_instantiations: vec![],
        function_instantiations: vec![],
        field_instantiations: vec![],
    };
    normalized::Module::new(&m)
}

fn mk_module_plus(vis: u8) -> normalized::Module {
    mk_module_plus_code(vis, vec![Bytecode::Ret])
}

fn mk_module_plus_perm(vis: u8, permutation: Permutation) -> normalized::Module {
    mk_module_plus_code_perm(vis, vec![Bytecode::Ret], permutation)
}

fn make_complex_module_perm(p: Permutation) -> normalized::Module {
    let m = CompiledModule {
        version: crate::file_format_common::VERSION_MAX,
        module_handles: vec![
            // only self module
            ModuleHandle {
                address: AddressIdentifierIndex(0),
                name: IdentifierIndex(p.permute(0)),
            },
        ],
        self_module_handle_idx: ModuleHandleIndex(0),
        identifiers: p.pool(vec![
            Identifier::new("M").unwrap(),       // Module name
            Identifier::new("S").unwrap(),       // Struct name
            Identifier::new("GS").unwrap(),      // Generic struct name
            Identifier::new("R").unwrap(),       // Resource name
            Identifier::new("GR").unwrap(),      // Generic resource name
            Identifier::new("f").unwrap(),       // Field name
            Identifier::new("fn").unwrap(),      // Function name
            Identifier::new("g_fn").unwrap(),    // Generic function name
            Identifier::new("test_fn").unwrap(), // Test function name
        ]),
        address_identifiers: vec![
            AccountAddress::ZERO, // Module address
        ],
        struct_handles: p.pool(vec![
            StructHandle {
                module: ModuleHandleIndex(0),
                name: IdentifierIndex(p.permute(1)),
                abilities: AbilitySet::PRIMITIVES,
                type_parameters: vec![],
            },
            StructHandle {
                module: ModuleHandleIndex(0),
                name: IdentifierIndex(p.permute(2)),
                abilities: AbilitySet::PRIMITIVES,
                type_parameters: vec![StructTypeParameter {
                    constraints: AbilitySet::PRIMITIVES,
                    is_phantom: false,
                }],
            },
            StructHandle {
                module: ModuleHandleIndex(0),
                name: IdentifierIndex(p.permute(3)),
                abilities: AbilitySet::EMPTY | Ability::Key,
                type_parameters: vec![],
            },
            StructHandle {
                module: ModuleHandleIndex(0),
                name: IdentifierIndex(p.permute(4)),
                abilities: AbilitySet::EMPTY | Ability::Key,
                type_parameters: vec![StructTypeParameter {
                    constraints: AbilitySet::PRIMITIVES,
                    is_phantom: false,
                }],
            },
        ]),
        struct_defs: p.pool(vec![
            // struct S { f: u64 }
            StructDefinition {
                struct_handle: StructHandleIndex(p.permute(0)),
                field_information: StructFieldInformation::Declared(vec![FieldDefinition {
                    name: IdentifierIndex(p.permute(5)),
                    signature: TypeSignature(SignatureToken::U64),
                }]),
            },
            // struct GS<T> { f: T }
            StructDefinition {
                struct_handle: StructHandleIndex(p.permute(1)),
                field_information: StructFieldInformation::Declared(vec![FieldDefinition {
                    name: IdentifierIndex(p.permute(5)),
                    signature: TypeSignature(SignatureToken::TypeParameter(0)),
                }]),
            },
            // struct R has key { f: u64 }
            StructDefinition {
                struct_handle: StructHandleIndex(p.permute(2)),
                field_information: StructFieldInformation::Declared(vec![FieldDefinition {
                    name: IdentifierIndex(p.permute(5)),
                    signature: TypeSignature(SignatureToken::U64),
                }]),
            },
            // struct GR<T> has key { f: T }
            StructDefinition {
                struct_handle: StructHandleIndex(p.permute(3)),
                field_information: StructFieldInformation::Declared(vec![FieldDefinition {
                    name: IdentifierIndex(p.permute(5)),
                    signature: TypeSignature(SignatureToken::TypeParameter(0)),
                }]),
            },
        ]),
        function_handles: p.pool(vec![
            // fun fn()
            FunctionHandle {
                module: ModuleHandleIndex(0),
                name: IdentifierIndex(p.permute(6)),
                parameters: SignatureIndex(p.permute(0)),
                return_: SignatureIndex(p.permute(0)),
                type_parameters: vec![],
            },
            // fun g_fn<T: key>(): u64
            FunctionHandle {
                module: ModuleHandleIndex(0),
                name: IdentifierIndex(p.permute(7)),
                parameters: SignatureIndex(p.permute(0)),
                return_: SignatureIndex(p.permute(2)),
                type_parameters: vec![AbilitySet::EMPTY | Ability::Key],
            },
            // fun test_fn(Sender)
            FunctionHandle {
                module: ModuleHandleIndex(0),
                name: IdentifierIndex(p.permute(8)),
                parameters: SignatureIndex(p.permute(1)),
                return_: SignatureIndex(p.permute(0)),
                type_parameters: vec![],
            },
        ]),
        function_defs: p.pool(vec![
            // public fun fn() { return; }
            FunctionDefinition {
                function: FunctionHandleIndex(p.permute(0)),
                visibility: Visibility::Public,
                is_entry: false,
                acquires_global_resources: vec![],
                code: Some(CodeUnit {
                    locals: SignatureIndex(p.permute(0)),
                    code: vec![Bytecode::Ret],
                }),
            },
            // fun g_fn<T>() { return; }
            FunctionDefinition {
                function: FunctionHandleIndex(p.permute(1)),
                visibility: Visibility::Private,
                is_entry: false,
                acquires_global_resources: vec![],
                code: Some(CodeUnit {
                    locals: SignatureIndex(p.permute(0)),
                    code: vec![Bytecode::Ret],
                }),
            },
            FunctionDefinition {
                function: FunctionHandleIndex(p.permute(2)),
                visibility: Visibility::Private,
                is_entry: false,
                acquires_global_resources: vec![],
                code: Some(CodeUnit {
                    locals: SignatureIndex(p.permute(0)),
                    code: vec![],
                }),
            },
        ]),
        signatures: p.pool(vec![
            Signature(vec![]),                        // void
            Signature(vec![SignatureToken::Address]), // address
            Signature(vec![SignatureToken::U64]),     // u64
        ]),
        constant_pool: p.pool(vec![
            Constant {
                type_: SignatureToken::U8,
                data: vec![0],
            },
            Constant {
                type_: SignatureToken::U8,
                data: vec![1],
            },
            Constant {
                type_: SignatureToken::Bool,
                data: vec![1],
            },
            // an address
            Constant {
                type_: SignatureToken::Address,
                data: AccountAddress::random().to_vec(),
            },
        ]),
        metadata: vec![],
        field_handles: vec![],
        friend_decls: vec![],
        struct_def_instantiations: vec![],
        function_instantiations: vec![],
        field_instantiations: vec![],
    };
    normalized::Module::new(&m)
}

#[test]
fn deprecated_unchanged_script_visibility() {
    let script_module = mk_module(Visibility::DEPRECATED_SCRIPT);
    assert!(Compatibility::full_check()
        .check(&script_module, &script_module)
        .is_ok(),);
}

#[test]
fn deprecated_remove_script_visibility() {
    let script_module = mk_module(Visibility::DEPRECATED_SCRIPT);
    // script -> private, not allowed
    let private_module = mk_module(Visibility::Private as u8);
    assert!(Compatibility::full_check()
        .check(&script_module, &private_module)
        .is_err());
    // script -> public, not allowed
    let public_module = mk_module(Visibility::Public as u8);
    assert!(Compatibility::full_check()
        .check(&script_module, &public_module)
        .is_err());
    // script -> friend, not allowed
    let friend_module = mk_module(Visibility::Friend as u8);
    assert!(Compatibility::full_check()
        .check(&script_module, &friend_module)
        .is_err());
}

#[test]
fn deprecated_add_script_visibility() {
    let script_module = mk_module(Visibility::DEPRECATED_SCRIPT);
    // private -> script, allowed
    let private_module = mk_module(Visibility::Private as u8);
    assert!(Compatibility::full_check()
        .check(&private_module, &script_module)
        .is_ok());
    // public -> script, not allowed
    let public_module = mk_module(Visibility::Public as u8);
    assert!(Compatibility::full_check()
        .check(&public_module, &script_module)
        .is_err());
    // friend -> script, not allowed
    let friend_module = mk_module(Visibility::Friend as u8);
    assert!(Compatibility::full_check()
        .check(&friend_module, &script_module)
        .is_err());
}

#[test]
fn check_exact_and_unchange_same_module() {
    let m1 = mk_module(Visibility::Private as u8);
    assert!(InclusionCheck::Subset.check(&m1, &m1).is_ok());
    assert!(InclusionCheck::Equal.check(&m1, &m1).is_ok());

    // m1 + an extra function
    let m2 = mk_module_plus(Visibility::Private as u8);
    assert!(InclusionCheck::Subset.check(&m1, &m2).is_ok());
    assert!(InclusionCheck::Subset.check(&m2, &m1).is_err());
    assert!(InclusionCheck::Equal.check(&m1, &m2).is_err());
    assert!(InclusionCheck::Equal.check(&m2, &m1).is_err());

    // m1 + a change in the bytecode of fn
    let m3 = mk_module_plus_code(
        Visibility::Private as u8,
        vec![Bytecode::LdU8(0), Bytecode::Ret],
    );
    assert!(InclusionCheck::Subset.check(&m2, &m3).is_err());
    // fn1 is not in m1 so the changed bytecode doesn't matter.
    assert!(InclusionCheck::Subset.check(&m1, &m3).is_ok());
    assert!(InclusionCheck::Subset.check(&m3, &m2).is_err());
    assert!(InclusionCheck::Subset.check(&m3, &m1).is_err());
    assert!(InclusionCheck::Equal.check(&m1, &m3).is_err());
    assert!(InclusionCheck::Equal.check(&m2, &m3).is_err());
}

#[test]
fn check_exact_and_unchange_same_module_permutations() {
    let m1 = mk_module(Visibility::Private as u8);
    let m2 = mk_module_plus(Visibility::Private as u8);
    let m3 = mk_module_plus_perm(Visibility::Private as u8, Permutation::new(vec![1, 0]));
    assert_ne!(m2, m3);
    assert!(InclusionCheck::Equal.check(&m2, &m3).is_ok());
    assert!(InclusionCheck::Equal.check(&m3, &m2).is_ok());
    // double inclusion
    assert!(InclusionCheck::Subset.check(&m3, &m2).is_ok());
    assert!(InclusionCheck::Subset.check(&m2, &m3).is_ok());

    assert!(InclusionCheck::Subset.check(&m1, &m3).is_ok());
    assert!(InclusionCheck::Subset.check(&m3, &m1).is_err());
    assert!(InclusionCheck::Equal.check(&m1, &m3).is_err());
    assert!(InclusionCheck::Equal.check(&m3, &m1).is_err());
}

#[test]
fn check_exact_and_unchange_same_complex_module_permutations() {
    let perms = vec![
        vec![0, 1, 2],
        vec![0, 2, 1],
        vec![1, 0, 2],
        vec![1, 2, 0],
        vec![2, 0, 1],
        vec![2, 1, 0],
    ];
    let modules: Vec<_> = perms
        .into_iter()
        .map(|permutation| make_complex_module_perm(Permutation::new(permutation)))
        .collect();

    for m0 in &modules {
        for m1 in &modules {
            assert!(InclusionCheck::Equal.check(m0, m1).is_ok());
            assert!(InclusionCheck::Equal.check(m1, m0).is_ok());
            assert!(InclusionCheck::Subset.check(m0, m1).is_ok());
            assert!(InclusionCheck::Subset.check(m1, m0).is_ok());
        }
    }
}
