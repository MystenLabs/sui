use crate::{
    file_format::{Bytecode, Visibility},
    normalized::*,
    unit_tests::compatibility_tests::mk_module_plus_code,
};

use std::rc::Rc;

#[test]
fn test_module_tables_ignored_in_equivalence() {
    let mut pool = RcPool::new();

    // Create base module
    let base_module = mk_module_plus_code(
        &mut pool,
        Visibility::Public as u8,
        vec![Bytecode::LdU8(0), Bytecode::Ret],
    );

    // Make an identical module and alter `tables` to ensure these changes are not observable.
    let mut modified_module = mk_module_plus_code(
        &mut pool,
        Visibility::Public as u8,
        vec![Bytecode::LdU8(0), Bytecode::Ret],
    );
    let new_signatures = vec![Rc::new(vec![Rc::new(Type::U128)])];
    modified_module.extend_table_signatures(new_signatures);

    assert!(
        base_module.equivalent(&modified_module),
        "Equivalence failed due to differing `tables`, but these should be ignored"
    );
}

#[test]
fn test_function_locals_ignored_in_equivalence() {
    let mut pool = RcPool::new();

    // Base module with minimal locals
    let base_module = mk_module_plus_code(
        &mut pool,
        Visibility::Public as u8,
        vec![Bytecode::LdU8(0), Bytecode::Ret],
    );

    // Modified module identical to base, but with extra locals
    let mut modified_module = mk_module_plus_code(
        &mut pool,
        Visibility::Public as u8,
        vec![Bytecode::LdU8(0), Bytecode::Ret],
    );
    for func in modified_module.functions.values_mut() {
        let mut_func = Rc::make_mut(func);
        mut_func.locals = Rc::new(
            [
                mut_func.locals.as_ref().clone(),
                vec![Rc::new(Type::U64), Rc::new(Type::Bool)],
            ]
            .concat(),
        );
    }

    assert!(
        base_module.equivalent(&modified_module),
        "Equivalence failed due to differing `locals` fields, but these should be ignored"
    );
}
