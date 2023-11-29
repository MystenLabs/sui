// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use move_binary_format::{
    access::ModuleAccess,
    file_format::{
        Bytecode, CodeUnit, DatatypeHandleIndex, FunctionDefinition, FunctionDefinitionIndex,
        FunctionHandleIndex, IdentifierIndex, ModuleHandleIndex, Signature, SignatureToken,
        StructDefinition, StructDefinitionIndex, StructFieldInformation, TableIndex,
    },
    internals::ModuleIndex,
    CompiledModule,
};
use move_core_types::account_address::AccountAddress;
use move_symbol_pool::Symbol;

/// Pass to order handles in compiled modules stably and canonically.  Performs the
/// following canonicalizations:
///
/// - Identifiers are sorted in lexicographic order.
///
/// - Module Handles are sorted so the self-module comes first, followed by modules with named
///   addresses in lexical order (by address name and module name), followed by unnamed addresses in
///   their original order.
///
/// - Struct and Function Handles are sorted so that definitions in the module come first, in
///   definition order, and remaining handles follow, in lexicographical order by fully-qualified
///   name.
///
/// - Friend Declarations are sorted in lexical order (by address name and module name), followed by
///   unnamed addresses in their original order.

/// Key for ordering module handles, distinguishing the module's self handle, handles with names,
/// and handles without names.
#[derive(Eq, PartialEq, Ord, PartialOrd)]
enum ModuleKey {
    SelfModule,
    Named {
        address: Symbol,
        name: IdentifierIndex,
    },
    Unnamed,
}

/// Key for ordering function and struct handles, distinguishing handles for definitions in
/// the module and handles for externally defined functions and structs.
#[derive(Eq, PartialEq, Ord, PartialOrd)]
enum ReferenceKey {
    Internal(TableIndex),
    External {
        module: ModuleHandleIndex,
        name: IdentifierIndex,
    },
}

/// Forward the index at `$ix`, of type `$Ix` to its new location according to the `$perm`utation
/// array.
macro_rules! remap {
    ($Ix:ty, $ix:expr, $perm:expr) => {
        $ix = <$Ix>::new($perm[$ix.into_index()])
    };
}

/// Apply canonicalization to a compiled module.
pub fn in_module(
    module: &mut CompiledModule,
    address_names: &HashMap<(AccountAddress, &str), Symbol>,
) {
    // 1 (a). Choose ordering for identifiers.
    let identifiers = permutation(&module.identifiers, |_ix, ident| ident);

    // 1 (b). Update references to identifiers.
    for module in &mut module.module_handles {
        remap!(IdentifierIndex, module.name, identifiers);
    }

    for module in &mut module.friend_decls {
        remap!(IdentifierIndex, module.name, identifiers);
    }

    for fun in &mut module.function_handles {
        remap!(IdentifierIndex, fun.name, identifiers);
    }

    for struct_ in &mut module.datatype_handles {
        remap!(IdentifierIndex, struct_.name, identifiers);
    }

    for def in &mut module.struct_defs {
        if let StructFieldInformation::Declared(fields) = &mut def.field_information {
            for field in fields {
                remap!(IdentifierIndex, field.name, identifiers);
            }
        };
    }

    // 1 (c). Update ordering for identifiers.  Note that updates need to happen before other
    //        handles are re-ordered, so that they can continue referencing identifiers in their own
    //        comparators.
    apply_permutation(&mut module.identifiers, identifiers);

    // 2 (a). Choose ordering for module handles.
    let modules = permutation(&module.module_handles, |_ix, handle| {
        // Order the self module first
        if handle == module.self_handle() {
            return ModuleKey::SelfModule;
        }

        // Preserve order between modules without a named address, pushing them to the end of the
        // pool.
        let Some(address_name) = address_names.get(&(
            module.address_identifiers[handle.address.0 as usize],
            module.identifiers[handle.name.0 as usize].as_str(),
        )) else {
            return ModuleKey::Unnamed;
        };

        // Layout remaining modules in lexicographical order of named address and module name.
        ModuleKey::Named {
            address: *address_name,
            name: handle.name,
        }
    });

    // 2 (b). Update references to module handles.
    remap!(ModuleHandleIndex, module.self_module_handle_idx, modules);

    for fun in &mut module.function_handles {
        remap!(ModuleHandleIndex, fun.module, modules);
    }

    for struct_ in &mut module.datatype_handles {
        remap!(ModuleHandleIndex, struct_.module, modules);
    }

    // 2 (c). Update ordering for module handles.
    apply_permutation(&mut module.module_handles, modules);

    // 3 (a). Choose ordering for struct handles.
    let struct_defs = struct_definition_order(&module.struct_defs);
    let structs = permutation(&module.datatype_handles, |ix, handle| {
        if handle.module == module.self_handle_idx() {
            // Order structs from this module first, and in definition order
            let Some(def_position) = struct_defs.get(&DatatypeHandleIndex(ix)) else {
                panic!("ICE struct handle from module without definition: {handle:?}");
            };
            ReferenceKey::Internal(def_position.0)
        } else {
            // Order the remaining handles afterwards, in lexicographical order of module, then
            // struct name.
            ReferenceKey::External {
                module: handle.module,
                name: handle.name,
            }
        }
    });

    // 3 (b). Update references to struct handles.
    for def in &mut module.struct_defs {
        remap!(DatatypeHandleIndex, def.struct_handle, structs);
        if let StructFieldInformation::Declared(fields) = &mut def.field_information {
            for field in fields {
                remap_signature_token(&mut field.signature.0, &structs);
            }
        };
    }

    for Signature(tokens) in &mut module.signatures {
        for token in tokens {
            remap_signature_token(token, &structs);
        }
    }

    // 3 (c). Update ordering for struct handles.
    apply_permutation(&mut module.datatype_handles, structs);

    // 4 (a). Choose ordering for function handles.
    let function_defs = function_definition_order(&module.function_defs);
    let functions = permutation(&module.function_handles, |ix, handle| {
        if handle.module == module.self_handle_idx() {
            // Order functions from this module first, and in definition order
            let Some(def_position) = function_defs.get(&FunctionHandleIndex(ix)) else {
                panic!("ICE function handle from module without definition: {handle:?}");
            };
            ReferenceKey::Internal(def_position.0)
        } else {
            // Order the remaining handles afterwards, in lexicographical order of module, then
            // function name.
            ReferenceKey::External {
                module: handle.module,
                name: handle.name,
            }
        }
    });

    // 4 (b). Update references to function handles.
    for inst in &mut module.function_instantiations {
        remap!(FunctionHandleIndex, inst.handle, functions);
    }

    for def in &mut module.function_defs {
        remap!(FunctionHandleIndex, def.function, functions);
        if let Some(code) = &mut def.code {
            remap_code(code, &functions);
        }
    }

    // 4 (c). Update ordering for function handles.
    apply_permutation(&mut module.function_handles, functions);

    // 5. Update ordering for friend decls, (it has no internal references pointing to it).
    module.friend_decls.sort_by_key(|handle| {
        // Preserve order between modules without a named address, pushing them to the end of the
        // pool.
        let Some(address_name) = address_names.get(&(
            module.address_identifiers[handle.address.0 as usize],
            module.identifiers[handle.name.0 as usize].as_str(),
        )) else {
            return ModuleKey::Unnamed;
        };

        // Layout remaining modules in lexicographical order of named address and module name.
        ModuleKey::Named {
            address: *address_name,
            name: handle.name,
        }
    });
}

/// Reverses mapping from `StructDefinition(Index)` to `StructHandle`, so that handles for structs
/// defined in a module can be arranged in definition order.
fn struct_definition_order(
    defs: &[StructDefinition],
) -> HashMap<DatatypeHandleIndex, StructDefinitionIndex> {
    defs.iter()
        .enumerate()
        .map(|(ix, def)| (def.struct_handle, StructDefinitionIndex(ix as TableIndex)))
        .collect()
}

/// Reverses mapping from `FunctionDefinition(Index)` to `FunctionHandle`, so that handles for
/// structs defined in a module can be arranged in definition order.
fn function_definition_order(
    defs: &[FunctionDefinition],
) -> HashMap<FunctionHandleIndex, FunctionDefinitionIndex> {
    defs.iter()
        .enumerate()
        .map(|(ix, def)| (def.function, FunctionDefinitionIndex(ix as TableIndex)))
        .collect()
}

/// Update references to `DatatypeHandle`s within signatures according to the permutation defined by
/// `structs`.
fn remap_signature_token(token: &mut SignatureToken, structs: &[TableIndex]) {
    use SignatureToken as T;
    match token {
        T::Bool
        | T::U8
        | T::U16
        | T::U32
        | T::U64
        | T::U128
        | T::U256
        | T::Address
        | T::Signer
        | T::TypeParameter(_) => (),

        T::Vector(token) | T::Reference(token) | T::MutableReference(token) => {
            remap_signature_token(token, structs)
        }

        T::Datatype(handle) => remap!(DatatypeHandleIndex, *handle, structs),

        T::DatatypeInstantiation(handle, tokens) => {
            remap!(DatatypeHandleIndex, *handle, structs);
            for token in tokens {
                remap_signature_token(token, structs)
            }
        }
    }
}

/// Update references to function handles within code according to the permutation defined by
/// `functions`.
fn remap_code(code: &mut CodeUnit, functions: &[TableIndex]) {
    for instr in &mut code.code {
        if let Bytecode::Call(function) = instr {
            remap!(FunctionHandleIndex, *function, functions);
        }
    }
}

/// Calculates the permutation of indices in `pool` that sorts it according to the key function
/// `key`:  The resulting `permutation` array is such that, new `pool'` defined by:
///
///   pool'[permutation[i]] = pool[i]
///
/// is sorted according to `key`.
fn permutation<'p, T, K: Ord>(
    pool: &'p Vec<T>,
    key: impl Fn(TableIndex, &'p T) -> K + 'p,
) -> Vec<TableIndex> {
    let mut inverse: Vec<_> = (0..pool.len() as TableIndex).collect();
    inverse.sort_by_key(move |ix| key(*ix, &pool[*ix as usize]));

    let mut permutation = vec![0 as TableIndex; pool.len()];
    for (ix, jx) in inverse.into_iter().enumerate() {
        permutation[jx as usize] = ix as TableIndex;
    }

    permutation
}

/// Re-order `pool` according to the `permutation` array.  `permutation[i]` is the new location of
/// `pool[i]`.
fn apply_permutation<T>(pool: &mut Vec<T>, mut permutation: Vec<TableIndex>) {
    assert_eq!(pool.len(), permutation.len());

    // At every iteration we confirm that one more value is in its final position in the pool,
    // either because we discover it is already in the correct place, or we move it to its correct
    // place.
    for ix in 0..pool.len() {
        loop {
            let jx = permutation[ix] as usize;
            if ix == jx {
                break;
            }
            pool.swap(ix, jx);
            permutation.swap(ix, jx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permutation_reverse() {
        let mut orig = vec![0i32, 1, 2, 3];

        let perm = permutation(&orig, |_, i| -i);
        assert_eq!(perm, vec![3, 2, 1, 0]);

        apply_permutation(&mut orig, perm);
        assert_eq!(orig, vec![3, 2, 1, 0]);
    }

    #[test]
    fn permutation_stability() {
        let orig = vec![5, 3, 6, 2, 1, 4];

        // Generating the permutation
        let perm = permutation(&orig, |_, i| i % 2 == 1);
        assert_eq!(perm, vec![3, 4, 0, 1, 5, 2]);

        // Applying the permutation
        let mut sort = orig.clone();
        apply_permutation(&mut sort, perm.clone());
        assert_eq!(sort, vec![6, 2, 4, 5, 3, 1]);

        // Confirm the definition of the permutation array
        for (ix, i) in orig.iter().enumerate() {
            assert_eq!(sort[perm[ix] as usize], *i, "{ix}: {i}");
        }
    }
}
