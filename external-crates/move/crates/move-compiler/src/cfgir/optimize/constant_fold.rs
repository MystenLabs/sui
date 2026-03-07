// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cfgir::cfg::MutForwardCFG,
    diagnostics::DiagnosticReporter,
    expansion::ast::Mutability,
    hlir::ast::{
        BaseType, BaseType_, Command, Command_, Exp, FunctionSignature, SingleType, TypeName,
        TypeName_, UnannotatedExp_, Value, Value_, Var,
    },
    naming::ast::{BuiltinTypeName, BuiltinTypeName_},
    parser::ast::{BinOp, BinOp_, ConstantName, UnaryOp, UnaryOp_},
    shared::unique_map::UniqueMap,
};
use move_ir_types::location::*;
use move_proc_macros::growing_stack;

/// returns true if anything changed
pub fn optimize(
    reporter: &DiagnosticReporter,
    _signature: &FunctionSignature,
    _locals: &UniqueMap<Var, (Mutability, SingleType)>,
    constants: &UniqueMap<ConstantName, Value>,
    cfg: &mut MutForwardCFG,
) -> bool {
    let context = Context {
        reporter,
        constants,
    };
    let mut changed = false;
    for block_ref in cfg.blocks_mut().values_mut() {
        let block = std::mem::take(block_ref);
        *block_ref = block
            .into_iter()
            .filter_map(|mut cmd| match optimize_cmd(&context, &mut cmd) {
                None => {
                    changed = true;
                    None
                }
                Some(cmd_changed) => {
                    changed = cmd_changed || changed;
                    Some(cmd)
                }
            })
            .collect();
    }
    changed
}

struct Context<'a> {
    #[allow(dead_code)]
    reporter: &'a DiagnosticReporter<'a>,
    constants: &'a UniqueMap<ConstantName, Value>,
}

//**************************************************************************************************
// Scaffolding
//**************************************************************************************************

// Some(changed) to keep
// None to remove the cmd
#[growing_stack]
fn optimize_cmd(context: &Context, sp!(_, cmd_): &mut Command) -> Option<bool> {
    use Command_ as C;
    Some(match cmd_ {
        C::Assign(_, _ls, e) => optimize_exp(context, e),
        C::Mutate(el, er) => {
            let c1 = optimize_exp(context, er);
            let c2 = optimize_exp(context, el);
            c1 || c2
        }
        C::Return { exp: e, .. }
        | C::Abort(_, e)
        | C::JumpIf { cond: e, .. }
        | C::VariantSwitch { subject: e, .. } => optimize_exp(context, e),
        C::IgnoreAndPop { exp: e, .. } => {
            let c = optimize_exp(context, e);
            if ignorable_exp(e) {
                // value(s), so the command can be removed
                return None;
            } else {
                c
            }
        }

        C::Jump { .. } => false,
        C::Break(_) | C::Continue(_) => panic!("ICE break/continue not translated to jumps"),
    })
}

#[growing_stack]
fn optimize_exp(context: &Context, e: &mut Exp) -> bool {
    use UnannotatedExp_ as E;
    let optimize_exp = |e| optimize_exp(context, e);
    match &mut e.exp.value {
        //************************************
        // Pass through cases
        //************************************
        E::Unit { .. }
        | E::Value(_)
        | E::UnresolvedError
        | E::BorrowLocal(_, _)
        | E::Move { .. }
        | E::Copy { .. }
        | E::ErrorConstant { .. }
        | E::Unreachable => false,

        e_ @ E::Constant(_) => {
            let E::Constant(name) = e_ else {
                unreachable!()
            };
            if let Some(value) = context.constants.get(name) {
                *e_ = E::Value(value.clone());
                true
            } else {
                false
            }
        }

        E::ModuleCall(mcall) => mcall.arguments.iter_mut().any(optimize_exp),

        E::Freeze(e) | E::Dereference(e) | E::Borrow(_, e, _, _) => optimize_exp(e),

        E::Pack(_, _, fields) => fields.iter_mut().any(|(_, _, e)| optimize_exp(e)),

        E::PackVariant(_, _, _, fields) => fields.iter_mut().any(|(_, _, e)| optimize_exp(e)),

        E::Multiple(es) => es.iter_mut().any(optimize_exp),

        //************************************
        // Foldable cases
        //************************************
        e_ @ E::UnaryExp(_, _) => {
            let (op, er) = match e_ {
                E::UnaryExp(op, er) => (op, er),
                _ => unreachable!(),
            };
            let changed = optimize_exp(er);
            let v = match foldable_exp(er) {
                Some(v) => v,
                None => return changed,
            };
            match fold_unary_op(e.exp.loc, op, v) {
                Some(folded) => {
                    *e_ = folded;
                    true
                }
                None => changed,
            }
        }

        e_ @ E::BinopExp(_, _, _) => {
            let (e1, op, e2) = match e_ {
                E::BinopExp(e1, op, e2) => (e1, op, e2),
                _ => unreachable!(),
            };
            let changed1 = optimize_exp(e1);
            let changed2 = optimize_exp(e2);
            let changed = changed1 || changed2;
            let v1_opt = foldable_exp(e1);
            let v2_opt = foldable_exp(e2);
            // TODO warn on operations that always fail
            if let (Some(v1), Some(v2)) = (v1_opt, v2_opt) {
                if let Some(folded) = fold_binary_op(e.exp.loc, op, v1, v2) {
                    *e_ = folded;
                    true
                } else {
                    changed
                }
            } else {
                changed
            }
        }

        e_ @ E::Cast(_, _) => {
            let (e, bt) = match e_ {
                E::Cast(e, bt) => (e, bt),
                _ => unreachable!(),
            };
            let changed = optimize_exp(e);
            // TODO warn on operations that always fail
            let v = match foldable_exp(e) {
                Some(v) => v,
                None => return changed,
            };
            match fold_cast(e.exp.loc, bt, v) {
                Some(folded) => {
                    *e_ = folded;
                    true
                }
                None => changed,
            }
        }

        e_ @ E::Vector(_, _, _, _) => {
            let (n, ty, eargs) = match e_ {
                E::Vector(_, n, ty, eargs) => (*n, ty, eargs),
                _ => unreachable!(),
            };
            let changed = eargs.iter_mut().any(optimize_exp);
            if !is_valid_const_type(ty) {
                return changed;
            }
            let mut vs = vec![];
            for earg in eargs {
                let eloc = earg.exp.loc;
                if let Some(v) = foldable_exp(earg) {
                    vs.push(sp(eloc, v));
                } else {
                    return changed;
                }
            }
            debug_assert!(n == vs.len());
            *e_ = evalue_(e.exp.loc, Value_::Vector(ty.clone(), vs));
            true
        }
    }
}

fn is_valid_const_type(sp!(_, ty_): &BaseType) -> bool {
    use BaseType_ as T;
    match ty_ {
        T::Apply(_, tn, ty_args) if is_valid_const_type_name(tn) => {
            ty_args.iter().all(is_valid_const_type)
        }
        T::Apply(_, _, _) | T::Param(_) | T::Unreachable | T::UnresolvedError => false,
    }
}

fn is_valid_const_type_name(sp!(_, tn_): &TypeName) -> bool {
    use TypeName_ as T;
    match tn_ {
        T::Builtin(bt) => is_valid_const_builtin_type(bt),
        T::ModuleType(_, _) => false,
    }
}

fn is_valid_const_builtin_type(sp!(_, bt_): &BuiltinTypeName) -> bool {
    use BuiltinTypeName_ as N;
    match bt_ {
        N::Address
        | N::U8
        | N::U16
        | N::U32
        | N::U64
        | N::U128
        | N::U256
        | N::I8
        | N::I16
        | N::I32
        | N::I64
        | N::I128
        | N::Vector
        | N::Bool => true,
        N::Signer => false,
    }
}

//**************************************************************************************************
// Folding
//**************************************************************************************************

fn fold_unary_op(loc: Loc, sp!(_, op_): &UnaryOp, v: Value_) -> Option<UnannotatedExp_> {
    use UnaryOp_ as U;
    use Value_ as V;
    let folded = match (op_, v) {
        (U::Not, V::Bool(b)) => V::Bool(!b),
        (U::Neg, V::I8(v)) => V::I8(v.checked_neg()?),
        (U::Neg, V::I16(v)) => V::I16(v.checked_neg()?),
        (U::Neg, V::I32(v)) => V::I32(v.checked_neg()?),
        (U::Neg, V::I64(v)) => V::I64(v.checked_neg()?),
        (U::Neg, V::I128(v)) => V::I128(v.checked_neg()?),
        (op, v) => panic!("ICE unexpected unary op while folding: {op:?} {v:?}"),
    };
    Some(evalue_(loc, folded))
}

macro_rules! checked_int_binop {
    ($v1:expr, $v2:expr, $method:ident) => {{
        use Value_ as V;
        match ($v1, $v2) {
            (V::U8(a), V::U8(b)) => a.$method(b).map(V::U8),
            (V::U16(a), V::U16(b)) => a.$method(b).map(V::U16),
            (V::U32(a), V::U32(b)) => a.$method(b).map(V::U32),
            (V::U64(a), V::U64(b)) => a.$method(b).map(V::U64),
            (V::U128(a), V::U128(b)) => a.$method(b).map(V::U128),
            (V::U256(a), V::U256(b)) => a.$method(b).map(V::U256),
            (V::I8(a), V::I8(b)) => a.$method(b).map(V::I8),
            (V::I16(a), V::I16(b)) => a.$method(b).map(V::I16),
            (V::I32(a), V::I32(b)) => a.$method(b).map(V::I32),
            (V::I64(a), V::I64(b)) => a.$method(b).map(V::I64),
            (V::I128(a), V::I128(b)) => a.$method(b).map(V::I128),
            _ => None,
        }
    }};
}

macro_rules! bitwise_int_binop {
    ($v1:expr, $v2:expr, $op:tt) => {{
        use Value_ as V;
        match ($v1, $v2) {
            (V::U8(a), V::U8(b)) => Some(V::U8(a $op b)),
            (V::U16(a), V::U16(b)) => Some(V::U16(a $op b)),
            (V::U32(a), V::U32(b)) => Some(V::U32(a $op b)),
            (V::U64(a), V::U64(b)) => Some(V::U64(a $op b)),
            (V::U128(a), V::U128(b)) => Some(V::U128(a $op b)),
            (V::U256(a), V::U256(b)) => Some(V::U256(a $op b)),
            (V::I8(a), V::I8(b)) => Some(V::I8(a $op b)),
            (V::I16(a), V::I16(b)) => Some(V::I16(a $op b)),
            (V::I32(a), V::I32(b)) => Some(V::I32(a $op b)),
            (V::I64(a), V::I64(b)) => Some(V::I64(a $op b)),
            (V::I128(a), V::I128(b)) => Some(V::I128(a $op b)),
            _ => None,
        }
    }};
}

macro_rules! comparison_int_binop {
    ($v1:expr, $v2:expr, $op:tt) => {{
        use Value_ as V;
        match ($v1, $v2) {
            (V::U8(a), V::U8(b)) => Some(V::Bool(a $op b)),
            (V::U16(a), V::U16(b)) => Some(V::Bool(a $op b)),
            (V::U32(a), V::U32(b)) => Some(V::Bool(a $op b)),
            (V::U64(a), V::U64(b)) => Some(V::Bool(a $op b)),
            (V::U128(a), V::U128(b)) => Some(V::Bool(a $op b)),
            (V::U256(a), V::U256(b)) => Some(V::Bool(a $op b)),
            (V::I8(a), V::I8(b)) => Some(V::Bool(a $op b)),
            (V::I16(a), V::I16(b)) => Some(V::Bool(a $op b)),
            (V::I32(a), V::I32(b)) => Some(V::Bool(a $op b)),
            (V::I64(a), V::I64(b)) => Some(V::Bool(a $op b)),
            (V::I128(a), V::I128(b)) => Some(V::Bool(a $op b)),
            _ => None,
        }
    }};
}

macro_rules! shift_int_binop {
    ($v1:expr, $v2:expr, $method:ident) => {{
        use Value_ as V;
        let V::U8(rhs) = $v2 else { return None };
        match $v1 {
            V::U8(a) => a.$method(rhs as u32).map(V::U8),
            V::U16(a) => a.$method(rhs as u32).map(V::U16),
            V::U32(a) => a.$method(rhs as u32).map(V::U32),
            V::U64(a) => a.$method(rhs as u32).map(V::U64),
            V::U128(a) => a.$method(rhs as u32).map(V::U128),
            V::U256(a) => a.$method(rhs as u32).map(V::U256),
            V::I8(a) => a.$method(rhs as u32).map(V::I8),
            V::I16(a) => a.$method(rhs as u32).map(V::I16),
            V::I32(a) => a.$method(rhs as u32).map(V::I32),
            V::I64(a) => a.$method(rhs as u32).map(V::I64),
            V::I128(a) => a.$method(rhs as u32).map(V::I128),
            _ => None,
        }
    }};
}

fn fold_binary_op(
    loc: Loc,
    sp!(_, op_): &BinOp,
    v1: Value_,
    v2: Value_,
) -> Option<UnannotatedExp_> {
    use BinOp_ as B;
    use Value_ as V;
    let v = match op_ {
        B::Add => checked_int_binop!(v1, v2, checked_add),
        B::Sub => checked_int_binop!(v1, v2, checked_sub),
        B::Mul => checked_int_binop!(v1, v2, checked_mul),
        B::Div => checked_int_binop!(v1, v2, checked_div),
        B::Mod => checked_int_binop!(v1, v2, checked_rem),

        B::Shl => shift_int_binop!(v1, v2, checked_shl),
        B::Shr => shift_int_binop!(v1, v2, checked_shr),

        B::BitOr => bitwise_int_binop!(v1, v2, |),
        B::BitAnd => bitwise_int_binop!(v1, v2, &),
        B::Xor => bitwise_int_binop!(v1, v2, ^),

        B::And => match (v1, v2) {
            (V::Bool(a), V::Bool(b)) => Some(V::Bool(a && b)),
            _ => None,
        },
        B::Or => match (v1, v2) {
            (V::Bool(a), V::Bool(b)) => Some(V::Bool(a || b)),
            _ => None,
        },

        B::Lt => comparison_int_binop!(v1, v2, <),
        B::Gt => comparison_int_binop!(v1, v2, >),
        B::Le => comparison_int_binop!(v1, v2, <=),
        B::Ge => comparison_int_binop!(v1, v2, >=),

        B::Eq => Some(V::Bool(v1 == v2)),
        B::Neq => Some(V::Bool(v1 != v2)),

        B::Range | B::Implies | B::Iff => None,
    }?;
    Some(evalue_(loc, v))
}

fn fold_cast(loc: Loc, sp!(_, bt_): &BuiltinTypeName, v: Value_) -> Option<UnannotatedExp_> {
    use BuiltinTypeName_ as BT;
    use Value_ as V;
    let cast = match (bt_, v) {
        (BT::U8, V::U8(u)) => V::U8(u),
        (BT::U8, V::U16(u)) => V::U8(u8::try_from(u).ok()?),
        (BT::U8, V::U32(u)) => V::U8(u8::try_from(u).ok()?),
        (BT::U8, V::U64(u)) => V::U8(u8::try_from(u).ok()?),
        (BT::U8, V::U128(u)) => V::U8(u8::try_from(u).ok()?),
        (BT::U8, V::U256(u)) => V::U8(u8::try_from(u).ok()?),

        (BT::U16, V::U8(u)) => V::U16(u as u16),
        (BT::U16, V::U16(u)) => V::U16(u),
        (BT::U16, V::U32(u)) => V::U16(u16::try_from(u).ok()?),
        (BT::U16, V::U64(u)) => V::U16(u16::try_from(u).ok()?),
        (BT::U16, V::U128(u)) => V::U16(u16::try_from(u).ok()?),
        (BT::U16, V::U256(u)) => V::U16(u16::try_from(u).ok()?),

        (BT::U32, V::U8(u)) => V::U32(u as u32),
        (BT::U32, V::U16(u)) => V::U32(u as u32),
        (BT::U32, V::U32(u)) => V::U32(u),
        (BT::U32, V::U64(u)) => V::U32(u32::try_from(u).ok()?),
        (BT::U32, V::U128(u)) => V::U32(u32::try_from(u).ok()?),
        (BT::U32, V::U256(u)) => V::U32(u32::try_from(u).ok()?),

        (BT::U64, V::U8(u)) => V::U64(u as u64),
        (BT::U64, V::U16(u)) => V::U64(u as u64),
        (BT::U64, V::U32(u)) => V::U64(u as u64),
        (BT::U64, V::U64(u)) => V::U64(u),
        (BT::U64, V::U128(u)) => V::U64(u64::try_from(u).ok()?),
        (BT::U64, V::U256(u)) => V::U64(u64::try_from(u).ok()?),

        (BT::U128, V::U8(u)) => V::U128(u as u128),
        (BT::U128, V::U16(u)) => V::U128(u as u128),
        (BT::U128, V::U32(u)) => V::U128(u as u128),
        (BT::U128, V::U64(u)) => V::U128(u as u128),
        (BT::U128, V::U128(u)) => V::U128(u),
        (BT::U128, V::U256(u)) => V::U128(u128::try_from(u).ok()?),

        (BT::U256, V::U8(u)) => V::U256(u.into()),
        (BT::U256, V::U16(u)) => V::U256(u.into()),
        (BT::U256, V::U32(u)) => V::U256(u.into()),
        (BT::U256, V::U64(u)) => V::U256(u.into()),
        (BT::U256, V::U128(u)) => V::U256(u.into()),
        (BT::U256, V::U256(u)) => V::U256(u),

        (BT::I8, V::I8(v)) => V::I8(v),
        (BT::I8, V::I16(v)) => V::I8(i8::try_from(v).ok()?),
        (BT::I8, V::I32(v)) => V::I8(i8::try_from(v).ok()?),
        (BT::I8, V::I64(v)) => V::I8(i8::try_from(v).ok()?),
        (BT::I8, V::I128(v)) => V::I8(i8::try_from(v).ok()?),

        (BT::I16, V::I8(v)) => V::I16(v as i16),
        (BT::I16, V::I16(v)) => V::I16(v),
        (BT::I16, V::I32(v)) => V::I16(i16::try_from(v).ok()?),
        (BT::I16, V::I64(v)) => V::I16(i16::try_from(v).ok()?),
        (BT::I16, V::I128(v)) => V::I16(i16::try_from(v).ok()?),

        (BT::I32, V::I8(v)) => V::I32(v as i32),
        (BT::I32, V::I16(v)) => V::I32(v as i32),
        (BT::I32, V::I32(v)) => V::I32(v),
        (BT::I32, V::I64(v)) => V::I32(i32::try_from(v).ok()?),
        (BT::I32, V::I128(v)) => V::I32(i32::try_from(v).ok()?),

        (BT::I64, V::I8(v)) => V::I64(v as i64),
        (BT::I64, V::I16(v)) => V::I64(v as i64),
        (BT::I64, V::I32(v)) => V::I64(v as i64),
        (BT::I64, V::I64(v)) => V::I64(v),
        (BT::I64, V::I128(v)) => V::I64(i64::try_from(v).ok()?),

        (BT::I128, V::I8(v)) => V::I128(v as i128),
        (BT::I128, V::I16(v)) => V::I128(v as i128),
        (BT::I128, V::I32(v)) => V::I128(v as i128),
        (BT::I128, V::I64(v)) => V::I128(v as i128),
        (BT::I128, V::I128(v)) => V::I128(v),

        // Cross signed/unsigned casts: don't fold, let bytecode handle
        (BT::I8, _)
        | (BT::I16, _)
        | (BT::I32, _)
        | (BT::I64, _)
        | (BT::I128, _)
        | (_, V::I8(_))
        | (_, V::I16(_))
        | (_, V::I32(_))
        | (_, V::I64(_))
        | (_, V::I128(_)) => return None,
        (_, v) => panic!("ICE unexpected cast while folding: {:?} as {:?}", v, bt_),
    };
    Some(evalue_(loc, cast))
}

const fn evalue_(loc: Loc, v: Value_) -> UnannotatedExp_ {
    use UnannotatedExp_ as E;
    E::Value(sp(loc, v))
}

//**************************************************************************************************
// Foldable Value
//**************************************************************************************************

fn foldable_exp(e: &Exp) -> Option<Value_> {
    use UnannotatedExp_ as E;
    match &e.exp.value {
        E::Value(sp!(_, v_)) => Some(v_.clone()),
        _ => None,
    }
}

fn ignorable_exp(e: &Exp) -> bool {
    use UnannotatedExp_ as E;
    match &e.exp.value {
        E::Unit { .. } => true,
        E::Value(_) => true,
        E::Multiple(es) => es.iter().all(ignorable_exp),
        _ => false,
    }
}
