// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! ProgramInfo extension for Sui Flavor
//! Contains information that may be expensive to compute and is needed only for Sui

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    diagnostics::warning_filters::WarningFilters,
    expansion::ast::{Fields, ModuleIdent},
    naming::ast as N,
    parser::ast::{Ability_, DatatypeName, DocComment, Field},
    shared::{
        program_info::{DatatypeKind, TypingProgramInfo},
        unique_map::UniqueMap,
    },
    sui_mode::{
        OBJECT_MODULE_NAME, SUI_ADDR_VALUE, TRANSFER_FUNCTION_NAME, TRANSFER_MODULE_NAME,
        UID_TYPE_NAME,
    },
    typing::{ast as T, visitor::TypingVisitorContext},
};
use move_ir_types::location::Loc;
use move_proc_macros::growing_stack;

#[derive(Debug, Clone, Copy)]
pub enum UIDHolder {
    /// is `sui::object::UID``
    IsUID,
    /// holds UID directly as one of the fields
    Direct { field: Field, ty: Loc },
    /// holds a type which in turn `Direct`ly or `Indirect`ly holds UID
    Indirect { field: Field, ty: Loc, uid: Loc },
}

#[derive(Debug, Clone, Copy)]
pub enum TransferKind {
    /// The object has store
    PublicTransfer(Loc),
    /// transferred within the module to an address vis `sui::transfer::transfer`
    PrivateTransfer(Loc),
}

/// Sui info for the entire program (mostly used during translation ).
#[derive(Debug, Clone)]
pub struct SuiInfo {
    /// All types that contain a UID, directly or indirectly
    /// This requires a DFS traversal of type declarations
    pub uid_holders: BTreeMap<ModuleIdent, BTreeMap<DatatypeName, UIDHolder>>,
    /// All types that either have store or are transferred privately
    pub transferred: BTreeMap<ModuleIdent, BTreeMap<DatatypeName, TransferKind>>,
}

/// Same as `SuiInfo` but for a given module (store per-module in `ProgramInfo`).
#[derive(Debug, Clone)]
pub struct SuiModInfo {
    pub uid_holders: BTreeMap<DatatypeName, UIDHolder>,
    pub transferred: BTreeMap<DatatypeName, TransferKind>,
}

impl SuiInfo {
    pub fn new(
        modules: &UniqueMap<ModuleIdent, T::ModuleDefinition>,
        info: &TypingProgramInfo,
    ) -> Self {
        let uid_holders = all_uid_holders(info);
        let transferred = all_transferred(modules, info);
        Self {
            uid_holders,
            transferred,
        }
    }
}

/// DFS traversal to find all UID holders
fn all_uid_holders(
    info: &TypingProgramInfo,
) -> BTreeMap<ModuleIdent, BTreeMap<DatatypeName, UIDHolder>> {
    fn merge_uid_holder(u1: UIDHolder, u2: UIDHolder) -> UIDHolder {
        match (u1, u2) {
            (u @ UIDHolder::IsUID, _) | (_, u @ UIDHolder::IsUID) => u,
            (d @ UIDHolder::Direct { .. }, _) | (_, d @ UIDHolder::Direct { .. }) => d,
            (u1, _) => u1,
        }
    }

    fn merge_uid_holder_opt(
        u1_opt: Option<UIDHolder>,
        u2_opt: Option<UIDHolder>,
    ) -> Option<UIDHolder> {
        match (u1_opt, u2_opt) {
            (Some(u1), Some(u2)) => Some(merge_uid_holder(u1, u2)),
            (o1, o2) => o1.or(o2),
        }
    }

    // returns true if the type at the given position is a phantom type
    fn phantom_positions(
        info: &TypingProgramInfo,
        sp!(_, tn_): &N::TypeName,
    ) -> Vec</* is_phantom */ bool> {
        match tn_ {
            N::TypeName_::Multiple(n) => vec![false; *n],
            N::TypeName_::Builtin(sp!(_, b_)) => b_
                .tparam_constraints(Loc::invalid())
                .into_iter()
                .map(|_| false)
                .collect(),
            N::TypeName_::ModuleType(m, n) => {
                let ty_params = match info.datatype_kind(m, n) {
                    DatatypeKind::Struct => &info.struct_definition(m, n).type_parameters,
                    DatatypeKind::Enum => &info.enum_definition(m, n).type_parameters,
                };
                ty_params.iter().map(|tp| tp.is_phantom).collect()
            }
        }
    }

    #[growing_stack]
    fn visit_ty(
        info: &TypingProgramInfo,
        visited: &mut BTreeSet<(ModuleIdent, DatatypeName)>,
        uid_holders: &mut BTreeMap<ModuleIdent, BTreeMap<DatatypeName, UIDHolder>>,
        sp!(_, ty_): &N::Type,
    ) -> Option<UIDHolder> {
        use N::TypeInner as TI;

        match ty_.inner() {
            TI::Unit
            | TI::Param(_)
            | TI::Var(_)
            | TI::Fun(_, _)
            | TI::Anything
            | TI::UnresolvedError
            | TI::Void => None,

            TI::Ref(_, inner) => visit_ty(info, visited, uid_holders, inner),

            TI::Apply(_, sp!(_, tn_), _)
                if tn_.is(&SUI_ADDR_VALUE, OBJECT_MODULE_NAME, UID_TYPE_NAME) =>
            {
                Some(UIDHolder::IsUID)
            }

            TI::Apply(_, tn, tys) => {
                let phantom_positions = phantom_positions(info, tn);
                let ty_args_holder = tys
                    .iter()
                    .zip(phantom_positions)
                    .filter(|(_t, is_phantom)| *is_phantom)
                    .map(|(t, _is_phantom)| visit_ty(info, visited, uid_holders, t))
                    .fold(None, merge_uid_holder_opt);
                let tn_holder = if let N::TypeName_::ModuleType(m, n) = tn.value {
                    visit_decl(info, visited, uid_holders, m, n);
                    uid_holders
                        .get(&m)
                        .and_then(|m_uid_holders| m_uid_holders.get(&n).copied())
                } else {
                    None
                };
                merge_uid_holder_opt(ty_args_holder, tn_holder)
            }
        }
    }

    #[growing_stack]
    fn visit_fields(
        info: &TypingProgramInfo,
        visited: &mut BTreeSet<(ModuleIdent, DatatypeName)>,
        uid_holders: &mut BTreeMap<ModuleIdent, BTreeMap<DatatypeName, UIDHolder>>,
        fields: &Fields<(DocComment, N::Type)>,
    ) -> Option<UIDHolder> {
        fields
            .key_cloned_iter()
            .map(|(field, (_, (_, ty)))| {
                Some(match visit_ty(info, visited, uid_holders, ty)? {
                    UIDHolder::IsUID => UIDHolder::Direct { field, ty: ty.loc },
                    UIDHolder::Direct { field, ty: uid }
                    | UIDHolder::Indirect { field, uid, ty: _ } => UIDHolder::Indirect {
                        field,
                        ty: ty.loc,
                        uid,
                    },
                })
            })
            .fold(None, merge_uid_holder_opt)
    }

    #[growing_stack]
    fn visit_decl(
        info: &TypingProgramInfo,
        visited: &mut BTreeSet<(ModuleIdent, DatatypeName)>,
        uid_holders: &mut BTreeMap<ModuleIdent, BTreeMap<DatatypeName, UIDHolder>>,
        mident: ModuleIdent,
        tn: DatatypeName,
    ) {
        if visited.contains(&(mident, tn)) {
            return;
        }
        visited.insert((mident, tn));

        let uid_holder_opt = match info.datatype_kind(&mident, &tn) {
            DatatypeKind::Struct => match &info.struct_definition(&mident, &tn).fields {
                N::StructFields::Defined(_, fields) => {
                    visit_fields(info, visited, uid_holders, fields)
                }
                N::StructFields::Native(_) => None,
            },
            DatatypeKind::Enum => info
                .enum_definition(&mident, &tn)
                .variants
                .iter()
                .filter_map(|(_, _, v)| match &v.fields {
                    N::VariantFields::Defined(_, fields) => Some(fields),
                    N::VariantFields::Empty => None,
                })
                .map(|fields| visit_fields(info, visited, uid_holders, fields))
                .fold(None, merge_uid_holder_opt),
        };
        if let Some(uid_holder) = uid_holder_opt {
            uid_holders
                .entry(mident)
                .or_default()
                .insert(tn, uid_holder);
        }
    }

    // iterate over all struct/enum declarations
    let visited = &mut BTreeSet::new();
    let mut uid_holders = BTreeMap::new();
    for (mident, mdef) in info.modules.key_cloned_iter() {
        let datatypes = mdef
            .structs
            .key_cloned_iter()
            .map(|(n, _)| n)
            .chain(mdef.enums.key_cloned_iter().map(|(n, _)| n));
        for tn in datatypes {
            visit_decl(info, visited, &mut uid_holders, mident, tn)
        }
    }
    uid_holders
}

fn all_transferred(
    modules: &UniqueMap<ModuleIdent, T::ModuleDefinition>,
    info: &TypingProgramInfo,
) -> BTreeMap<ModuleIdent, BTreeMap<DatatypeName, TransferKind>> {
    let mut transferred: BTreeMap<ModuleIdent, BTreeMap<DatatypeName, TransferKind>> =
        BTreeMap::new();
    for (mident, minfo) in info.modules.key_cloned_iter() {
        for (s, sdef) in minfo.structs.key_cloned_iter() {
            if !sdef.abilities.has_ability_(Ability_::Key) {
                continue;
            }
            let Some(store_loc) = sdef.abilities.ability_loc_(Ability_::Store) else {
                continue;
            };
            transferred
                .entry(mident)
                .or_default()
                .insert(s, TransferKind::PublicTransfer(store_loc));
        }

        if let Some(mdef) = modules.get(&mident) {
            for (_, _, fdef) in &mdef.functions {
                add_private_transfers(&mut transferred, fdef);
            }
        }
    }
    transferred
}

fn add_private_transfers(
    transferred: &mut BTreeMap<ModuleIdent, BTreeMap<DatatypeName, TransferKind>>,
    fdef: &T::Function,
) {
    struct TransferVisitor<'a> {
        transferred: &'a mut BTreeMap<ModuleIdent, BTreeMap<DatatypeName, TransferKind>>,
    }
    impl TypingVisitorContext for TransferVisitor<'_> {
        fn push_warning_filter_scope(&mut self, _: WarningFilters) {
            unreachable!("no warning filters in function bodies")
        }

        fn pop_warning_filter_scope(&mut self) {
            unreachable!("no warning filters in function bodies")
        }

        fn visit_exp_custom(&mut self, e: &T::Exp) -> bool {
            use T::UnannotatedExp_ as E;
            let E::ModuleCall(call) = &e.exp.value else {
                return false;
            };
            if !call.is(
                &SUI_ADDR_VALUE,
                TRANSFER_MODULE_NAME,
                TRANSFER_FUNCTION_NAME,
            ) {
                return false;
            }
            let [sp!(_, ty)] = call.type_arguments.as_slice() else {
                return false;
            };
            let Some((mident, n)) = ty.type_name().and_then(|t| t.value.datatype_name()) else {
                return false;
            };
            self.transferred
                .entry(mident)
                .or_default()
                .insert(n, TransferKind::PrivateTransfer(e.exp.loc));
            false
        }
    }

    let mut visitor = TransferVisitor { transferred };
    match &fdef.body.value {
        T::FunctionBody_::Native | &T::FunctionBody_::Macro => (),
        T::FunctionBody_::Defined(seq) => visitor.visit_seq(fdef.body.loc, seq),
    }
}
