// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This analysis flags uses of the sui::coin::Coin struct in fields of other structs. In most cases
//! it's preferable to use sui::balance::Balance instead to save space.

use move_command_line_common::{address::NumericalAddress, parser::NumberFormat};
use move_compiler::{
    diag,
    diagnostics::codes::{custom, DiagnosticInfo, Severity},
    expansion::ast as E,
    naming::ast as N,
    shared::{CompilationEnv, Identifier},
    typing::{ast as T, core::ProgramInfo, visitor::TypingVisitor},
};
use move_core_types::account_address::AccountAddress;
use move_ir_types::location::Loc;
use move_symbol_pool::Symbol;

use super::{
    LinterDiagCategory, COIN_MOD_NAME, COIN_STRUCT_NAME, LINTER_DEFAULT_DIAG_CODE,
    LINT_WARNING_PREFIX, SUI_PKG_NAME,
};

const COIN_FIELD_DIAG: DiagnosticInfo = custom(
    LINT_WARNING_PREFIX,
    Severity::Warning,
    LinterDiagCategory::CoinField as u8,
    LINTER_DEFAULT_DIAG_CODE,
    "sub-optimal 'sui::coin::Coin' field type",
);

pub struct CoinFieldVisitor;

impl TypingVisitor for CoinFieldVisitor {
    fn visit(
        &mut self,
        env: &mut CompilationEnv,
        _program_info: &ProgramInfo,
        program: &mut T::Program,
    ) {
        for (_, _, mdef) in program.modules.iter() {
            env.add_warning_filter_scope(mdef.warning_filter.clone());
            mdef.structs
                .iter()
                .for_each(|(sloc, sname, sdef)| struct_def(env, *sname, sdef, sloc));
            env.pop_warning_filter_scope();
        }
    }
}

fn struct_def(env: &mut CompilationEnv, sname: Symbol, sdef: &N::StructDefinition, sloc: Loc) {
    env.add_warning_filter_scope(sdef.warning_filter.clone());

    if let N::StructFields::Defined(sfields) = &sdef.fields {
        for (floc, fname, (_, ftype)) in sfields.iter() {
            if is_field_coin_type(ftype) {
                let msg = format!("The field '{fname}' of '{sname}' has type 'sui::coin::Coin'");
                let uid_msg = "Storing 'sui::balance::Balance' in this field will typically be more space-efficient";
                let d = diag!(COIN_FIELD_DIAG, (sloc, msg), (floc, uid_msg));
                env.add_diag(d);
            }
        }
    }

    env.pop_warning_filter_scope();
}

fn is_field_coin_type(sp!(_, t): &N::Type) -> bool {
    use N::Type_ as T;
    match t {
        T::Ref(_, inner_t) => is_field_coin_type(inner_t),
        T::Apply(_, tname, _) => {
            let sp!(_, tname) = tname;
            if let N::TypeName_::ModuleType(mident, sname) = tname {
                return is_mident_sui_coin(mident) || sname.value() == COIN_STRUCT_NAME.into();
            }
            false
        }
        T::Unit | T::Param(_) | T::Var(_) | T::Anything | T::UnresolvedError => false,
    }
}

fn is_mident_sui_coin(sp!(_, mident): &E::ModuleIdent) -> bool {
    use E::Address as A;
    if mident.module.value() != COIN_MOD_NAME.into() {
        return false;
    }
    let sui_addr = NumericalAddress::new(AccountAddress::TWO.into_bytes(), NumberFormat::Hex);
    match mident.address {
        A::Numerical(_, addr) => addr.value == sui_addr,
        A::NamedUnassigned(n) => n.value == SUI_PKG_NAME.into(),
    }
}
