// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cfgir::cfg::MutForwardCFG,
    diag,
    diagnostics::DiagnosticReporter,
    expansion::ast::{ModuleIdent, Mutability},
    hlir::ast::{
        BaseType, BaseType_, Command, Command_, Exp, FunctionSignature, SingleType, TypeName,
        TypeName_, UnannotatedExp_, Value, Value_, Var,
    },
    ice,
    naming::ast::{BuiltinTypeName, BuiltinTypeName_},
    parser::ast::{BinOp, BinOp_, ConstantName, UnaryOp, UnaryOp_},
    shared::unique_map::UniqueMap,
};
use move_ir_types::location::*;
use move_proc_macros::growing_stack;
use std::{borrow::Cow, collections::BTreeMap, convert::TryFrom};

/// returns true if anything changed
pub fn optimize(
    reporter: &DiagnosticReporter,
    _signature: &FunctionSignature,
    _locals: &UniqueMap<Var, (Mutability, SingleType)>,
    constants: &BTreeMap<(ModuleIdent, ConstantName), Value>,
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
    reporter: &'a DiagnosticReporter<'a>,
    constants: &'a BTreeMap<(ModuleIdent, ConstantName), Value>,
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

        e_ @ E::Constant(_, _) => {
            let E::Constant(module, name) = e_ else {
                unreachable!()
            };
            if let Some(value) = context.constants.get(&(*module, *name)) {
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
                    vs.push(sp(eloc, v.clone()));
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
        | N::I256
        | N::Vector
        | N::Bool => true,
        N::Signer => false,
    }
}

//**************************************************************************************************
// Folding
//**************************************************************************************************

fn fold_unary_op(loc: Loc, op: &UnaryOp, v: &Value_) -> Option<UnannotatedExp_> {
    Some(evalue_(loc, fold_unary_op_(op, v)?))
}

// `None` means the operation errors at runtime, e.g. negating `MIN`.
fn fold_unary_op_(sp!(_, op_): &UnaryOp, v: &Value_) -> Option<Value_> {
    use UnaryOp_ as U;
    use Value_ as V;
    Some(match (op_, v) {
        (U::Not, V::Bool(b)) => V::Bool(!*b),
        (U::Neg, V::I8(v)) => V::I8(v.checked_neg()?),
        (U::Neg, V::I16(v)) => V::I16(v.checked_neg()?),
        (U::Neg, V::I32(v)) => V::I32(v.checked_neg()?),
        (U::Neg, V::I64(v)) => V::I64(v.checked_neg()?),
        (U::Neg, V::I128(v)) => V::I128(v.checked_neg()?),
        (U::Neg, V::I256(v)) => V::I256(v.checked_neg()?),
        (op_, v) => panic!("ICE unknown unary op. combo while folding: {} {:?}", op_, v),
    })
}

macro_rules! checked_int_binop {
    ($v1:expr, $v2:expr, $method:ident) => {{
        use Value_ as V;
        match ($v1, $v2) {
            (V::U8(a), V::U8(b)) => a.$method(*b).map(V::U8),
            (V::U16(a), V::U16(b)) => a.$method(*b).map(V::U16),
            (V::U32(a), V::U32(b)) => a.$method(*b).map(V::U32),
            (V::U64(a), V::U64(b)) => a.$method(*b).map(V::U64),
            (V::U128(a), V::U128(b)) => a.$method(*b).map(V::U128),
            (V::U256(a), V::U256(b)) => a.$method(*b).map(V::U256),
            (V::I8(a), V::I8(b)) => a.$method(*b).map(V::I8),
            (V::I16(a), V::I16(b)) => a.$method(*b).map(V::I16),
            (V::I32(a), V::I32(b)) => a.$method(*b).map(V::I32),
            (V::I64(a), V::I64(b)) => a.$method(*b).map(V::I64),
            (V::I128(a), V::I128(b)) => a.$method(*b).map(V::I128),
            (V::I256(a), V::I256(b)) => a.$method(*b).map(V::I256),
            _ => None,
        }
    }};
}

macro_rules! bitwise_int_binop {
    ($v1:expr, $v2:expr, $op:tt) => {{
        use Value_ as V;
        match ($v1, $v2) {
            (V::U8(a), V::U8(b)) => Some(V::U8(*a $op *b)),
            (V::U16(a), V::U16(b)) => Some(V::U16(*a $op *b)),
            (V::U32(a), V::U32(b)) => Some(V::U32(*a $op *b)),
            (V::U64(a), V::U64(b)) => Some(V::U64(*a $op *b)),
            (V::U128(a), V::U128(b)) => Some(V::U128(*a $op *b)),
            (V::U256(a), V::U256(b)) => Some(V::U256(*a $op *b)),
            (V::I8(a), V::I8(b)) => Some(V::I8(*a $op *b)),
            (V::I16(a), V::I16(b)) => Some(V::I16(*a $op *b)),
            (V::I32(a), V::I32(b)) => Some(V::I32(*a $op *b)),
            (V::I64(a), V::I64(b)) => Some(V::I64(*a $op *b)),
            (V::I128(a), V::I128(b)) => Some(V::I128(*a $op *b)),
            (V::I256(a), V::I256(b)) => Some(V::I256(*a $op *b)),
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
            (V::I256(a), V::I256(b)) => Some(V::Bool(a $op b)),
            _ => None,
        }
    }};
}

fn fold_shl(v1: &Value_, v2: &Value_) -> Option<Value_> {
    use Value_ as V;
    let V::U8(rhs) = v2 else { return None };
    let rhs = *rhs as u32;
    match v1 {
        V::U8(a) => a.checked_shl(rhs).map(V::U8),
        V::U16(a) => a.checked_shl(rhs).map(V::U16),
        V::U32(a) => a.checked_shl(rhs).map(V::U32),
        V::U64(a) => a.checked_shl(rhs).map(V::U64),
        V::U128(a) => a.checked_shl(rhs).map(V::U128),
        V::U256(a) => a.checked_shl(rhs).map(V::U256),
        // checked_shl does not detect signed overflow, so reject shifts whose bits do not round-trip.
        V::I8(a) => a.checked_shl(rhs).filter(|r| r >> rhs == *a).map(V::I8),
        V::I16(a) => a.checked_shl(rhs).filter(|r| r >> rhs == *a).map(V::I16),
        V::I32(a) => a.checked_shl(rhs).filter(|r| r >> rhs == *a).map(V::I32),
        V::I64(a) => a.checked_shl(rhs).filter(|r| r >> rhs == *a).map(V::I64),
        V::I128(a) => a.checked_shl(rhs).filter(|r| r >> rhs == *a).map(V::I128),
        V::I256(a) => a
            .checked_shl(rhs)
            .filter(|r| r.checked_shr(rhs) == Some(*a))
            .map(V::I256),
        _ => None,
    }
}

// Rust's `>>` on signed types is arithmetic (sign-extending).
fn fold_shr(v1: &Value_, v2: &Value_) -> Option<Value_> {
    use Value_ as V;
    let V::U8(rhs) = v2 else { return None };
    let rhs = *rhs as u32;
    match v1 {
        V::U8(a) => a.checked_shr(rhs).map(V::U8),
        V::U16(a) => a.checked_shr(rhs).map(V::U16),
        V::U32(a) => a.checked_shr(rhs).map(V::U32),
        V::U64(a) => a.checked_shr(rhs).map(V::U64),
        V::U128(a) => a.checked_shr(rhs).map(V::U128),
        V::U256(a) => a.checked_shr(rhs).map(V::U256),
        V::I8(a) => a.checked_shr(rhs).map(V::I8),
        V::I16(a) => a.checked_shr(rhs).map(V::I16),
        V::I32(a) => a.checked_shr(rhs).map(V::I32),
        V::I64(a) => a.checked_shr(rhs).map(V::I64),
        V::I128(a) => a.checked_shr(rhs).map(V::I128),
        V::I256(a) => a.checked_shr(rhs).map(V::I256),
        _ => None,
    }
}

fn fold_binary_op(loc: Loc, op: &BinOp, v1: &Value_, v2: &Value_) -> Option<UnannotatedExp_> {
    Some(evalue_(loc, fold_binary_op_(op, v1, v2)?))
}

// `None` means the operation is not folded, because it errors at runtime or is a signed left
// shift below the bit width that wraps instead. See `report_binop_always_errors`.
fn fold_binary_op_(sp!(_, op_): &BinOp, v1: &Value_, v2: &Value_) -> Option<Value_> {
    use BinOp_ as B;
    use Value_ as V;
    match op_ {
        B::Add => checked_int_binop!(v1, v2, checked_add),
        B::Sub => checked_int_binop!(v1, v2, checked_sub),
        B::Mul => checked_int_binop!(v1, v2, checked_mul),
        B::Div => checked_int_binop!(v1, v2, checked_div),
        B::Mod => checked_int_binop!(v1, v2, checked_rem),

        B::Shl => fold_shl(v1, v2),
        B::Shr => fold_shr(v1, v2),

        B::BitOr => bitwise_int_binop!(v1, v2, |),
        B::BitAnd => bitwise_int_binop!(v1, v2, &),
        B::Xor => bitwise_int_binop!(v1, v2, ^),

        B::And => match (v1, v2) {
            (V::Bool(a), V::Bool(b)) => Some(V::Bool(*a && *b)),
            _ => None,
        },
        B::Or => match (v1, v2) {
            (V::Bool(a), V::Bool(b)) => Some(V::Bool(*a || *b)),
            _ => None,
        },

        B::Lt => comparison_int_binop!(v1, v2, <),
        B::Gt => comparison_int_binop!(v1, v2, >),
        B::Le => comparison_int_binop!(v1, v2, <=),
        B::Ge => comparison_int_binop!(v1, v2, >=),

        B::Eq => Some(V::Bool(v1 == v2)),
        B::Neq => Some(V::Bool(v1 != v2)),
    }
}

// Casts mirror the VM's value-preserving semantics. A value that fits the target is preserved,
// otherwise the cast errors at runtime.

// Casts to an unsigned target (`u8`..`u128`). Out-of-range magnitudes and negative sources fail.
macro_rules! cast_u {
    ($v:expr, $target_v:ident, $target_ty:ty) => {
        match $v {
            V::U8(u) => V::$target_v(<$target_ty>::try_from(*u).ok()?),
            V::U16(u) => V::$target_v(<$target_ty>::try_from(*u).ok()?),
            V::U32(u) => V::$target_v(<$target_ty>::try_from(*u).ok()?),
            V::U64(u) => V::$target_v(<$target_ty>::try_from(*u).ok()?),
            V::U128(u) => V::$target_v(<$target_ty>::try_from(*u).ok()?),
            V::U256(u) => V::$target_v(<$target_ty>::try_from(*u).ok()?),
            V::I8(i) => V::$target_v(<$target_ty>::try_from(*i).ok()?),
            V::I16(i) => V::$target_v(<$target_ty>::try_from(*i).ok()?),
            V::I32(i) => V::$target_v(<$target_ty>::try_from(*i).ok()?),
            V::I64(i) => V::$target_v(<$target_ty>::try_from(*i).ok()?),
            V::I128(i) => V::$target_v(<$target_ty>::try_from(*i).ok()?),
            V::I256(i) => {
                // A negative I256 never fits an unsigned target, and a non-negative I256 shares its U256 bit view.
                if *i < I256::zero() {
                    return None;
                }
                V::$target_v(<$target_ty>::try_from(i.to_u256_bits()).ok()?)
            }
            _ => return None,
        }
    };
}

// Casts to a signed target (`i8`..`i128`). Unsigned sources fail when the value exceeds the target's `MAX`.
macro_rules! cast_i {
    ($v:expr, $target_v:ident, $target_ty:ty) => {
        match $v {
            V::U8(u) => V::$target_v(<$target_ty>::try_from(*u).ok()?),
            V::U16(u) => V::$target_v(<$target_ty>::try_from(*u).ok()?),
            V::U32(u) => V::$target_v(<$target_ty>::try_from(*u).ok()?),
            V::U64(u) => V::$target_v(<$target_ty>::try_from(*u).ok()?),
            V::U128(u) => V::$target_v(<$target_ty>::try_from(*u).ok()?),
            V::U256(u) => {
                // Signed targets are at most 128 bits, so narrowing through u128 first loses nothing.
                V::$target_v(
                    u128::try_from(*u)
                        .ok()
                        .and_then(|v| <$target_ty>::try_from(v).ok())?,
                )
            }
            V::I8(i) => V::$target_v(<$target_ty>::try_from(*i).ok()?),
            V::I16(i) => V::$target_v(<$target_ty>::try_from(*i).ok()?),
            V::I32(i) => V::$target_v(<$target_ty>::try_from(*i).ok()?),
            V::I64(i) => V::$target_v(<$target_ty>::try_from(*i).ok()?),
            V::I128(i) => V::$target_v(<$target_ty>::try_from(*i).ok()?),
            V::I256(i) => V::$target_v(<$target_ty>::try_from(*i).ok()?),
            _ => return None,
        }
    };
}

fn fold_cast(loc: Loc, bt: &BuiltinTypeName, v: &Value_) -> Option<UnannotatedExp_> {
    Some(evalue_(loc, fold_cast_(bt, v)?))
}

// `None` means the value does not fit in the target type, so the cast errors at runtime.
fn fold_cast_(sp!(_, bt_): &BuiltinTypeName, v: &Value_) -> Option<Value_> {
    use BuiltinTypeName_ as BT;
    use Value_ as V;
    use move_core_types::{i256::I256, u256::U256};
    // A negative value never fits u256, and a non-negative I256 shares its U256 bit view.
    fn signed_to_u256(x: I256) -> Option<U256> {
        if x < I256::zero() {
            None
        } else {
            Some(x.to_u256_bits())
        }
    }
    let cast = match bt_ {
        BT::U8 => cast_u!(v, U8, u8),
        BT::U16 => cast_u!(v, U16, u16),
        BT::U32 => cast_u!(v, U32, u32),
        BT::U64 => cast_u!(v, U64, u64),
        BT::U128 => cast_u!(v, U128, u128),
        BT::U256 => match v {
            V::U8(u) => V::U256((*u).into()),
            V::U16(u) => V::U256((*u).into()),
            V::U32(u) => V::U256((*u).into()),
            V::U64(u) => V::U256((*u).into()),
            V::U128(u) => V::U256((*u).into()),
            V::U256(u) => V::U256(*u),
            V::I8(i) => V::U256(signed_to_u256(I256::from(*i))?),
            V::I16(i) => V::U256(signed_to_u256(I256::from(*i))?),
            V::I32(i) => V::U256(signed_to_u256(I256::from(*i))?),
            V::I64(i) => V::U256(signed_to_u256(I256::from(*i))?),
            V::I128(i) => V::U256(signed_to_u256(I256::from(*i))?),
            V::I256(i) => V::U256(signed_to_u256(*i)?),
            _ => return None,
        },
        BT::I8 => cast_i!(v, I8, i8),
        BT::I16 => cast_i!(v, I16, i16),
        BT::I32 => cast_i!(v, I32, i32),
        BT::I64 => cast_i!(v, I64, i64),
        BT::I128 => cast_i!(v, I128, i128),
        BT::I256 => match v {
            V::I8(i) => V::I256(I256::from(*i)),
            V::I16(i) => V::I256(I256::from(*i)),
            V::I32(i) => V::I256(I256::from(*i)),
            V::I64(i) => V::I256(I256::from(*i)),
            V::I128(i) => V::I256(I256::from(*i)),
            V::I256(i) => V::I256(*i),
            // Every u8..u128 value is below 2^255, so its U256 bit view is non-negative as an I256.
            V::U8(u) => V::I256(I256::from_u256_bits(U256::from(*u))),
            V::U16(u) => V::I256(I256::from_u256_bits(U256::from(*u))),
            V::U32(u) => V::I256(I256::from_u256_bits(U256::from(*u))),
            V::U64(u) => V::I256(I256::from_u256_bits(U256::from(*u))),
            V::U128(u) => V::I256(I256::from_u256_bits(U256::from(*u))),
            V::U256(u) => {
                // A u256 above `I256::MAX` has its top bit set, so reject it instead of bit-casting to a negative.
                let result = I256::from_u256_bits(*u);
                if result < I256::zero() {
                    return None;
                }
                V::I256(result)
            }
            _ => return None,
        },
        _ => panic!("ICE unexpected cast target while folding: {:?}", bt_),
    };
    Some(cast)
}

const fn evalue_(loc: Loc, v: Value_) -> UnannotatedExp_ {
    use UnannotatedExp_ as E;
    E::Value(sp(loc, v))
}

//**************************************************************************************************
// Foldable Value
//**************************************************************************************************

fn foldable_exp(e: &Exp) -> Option<&Value_> {
    use UnannotatedExp_ as E;
    match &e.exp.value {
        E::Value(sp!(_, v_)) => Some(v_),
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

//**************************************************************************************************
// Always erroring operations
//**************************************************************************************************

/// Reports any operation over values that will always error at runtime, e.g. `1 / 0` or
/// `0xFFFFu16 as u8`.
pub fn report_always_erroring_operations(
    reporter: &DiagnosticReporter,
    constants: &BTreeMap<(ModuleIdent, ConstantName), Value>,
    cfg: &MutForwardCFG,
) {
    let context = Context {
        reporter,
        constants,
    };
    for block in cfg.blocks().values() {
        for cmd in block {
            check_cmd(&context, cmd);
        }
    }
}

#[growing_stack]
fn check_cmd(context: &Context, sp!(_, cmd_): &Command) {
    use Command_ as C;
    match cmd_ {
        C::Assign(_, _, e)
        | C::Return { exp: e, .. }
        | C::Abort(_, e)
        | C::JumpIf { cond: e, .. }
        | C::VariantSwitch { subject: e, .. }
        | C::IgnoreAndPop { exp: e, .. } => {
            check_exp(context, e);
        }
        C::Mutate(el, er) => {
            check_exp(context, el);
            check_exp(context, er);
        }
        C::Jump { .. } => (),
        C::Break(_) | C::Continue(_) => panic!("ICE break/continue not translated to jumps"),
    }
}

/// Returns the value of `e` if it can be evaluated statically. Any expression that always
/// errors is reported. If the expression cannot be evaluated statically (from an error or
/// otherwise), returns `None`.
#[growing_stack]
fn check_exp<'a>(context: &Context, e: &'a Exp) -> Option<Cow<'a, Value_>> {
    use UnannotatedExp_ as E;
    match &e.exp.value {
        E::Value(_) | E::Constant(_, _) => foldable_exp(e).map(Cow::Borrowed),

        E::Unit { .. }
        | E::UnresolvedError
        | E::BorrowLocal(_, _)
        | E::Move { .. }
        | E::Copy { .. }
        | E::ErrorConstant { .. }
        | E::Unreachable => None,

        E::ModuleCall(mcall) => {
            check_exps(context, &mcall.arguments);
            None
        }

        E::Freeze(er) | E::Dereference(er) | E::Borrow(_, er, _, _) => {
            check_exp(context, er);
            None
        }

        E::Pack(_, _, fields) => {
            for (_, _, er) in fields {
                check_exp(context, er);
            }
            None
        }

        E::PackVariant(_, _, _, fields) => {
            for (_, _, er) in fields {
                check_exp(context, er);
            }
            None
        }

        E::Multiple(es) | E::Vector(_, _, _, es) => {
            check_exps(context, es);
            None
        }

        E::UnaryExp(op, er) => {
            let v = check_exp(context, er)?;
            match fold_unary_op_(op, &v) {
                Some(folded) => Some(Cow::Owned(folded)),
                None => {
                    report_unop_always_errors(context, e.exp.loc, op, &v);
                    None
                }
            }
        }

        E::BinopExp(e1, op, e2) => {
            let v1_opt = check_exp(context, e1);
            let v2_opt = check_exp(context, e2);
            let (v1, v2) = (v1_opt?, v2_opt?);
            match fold_binary_op_(op, &v1, &v2) {
                Some(folded) => Some(Cow::Owned(folded)),
                None => {
                    report_binop_always_errors(context, e.exp.loc, op, &v1, &v2);
                    None
                }
            }
        }

        E::Cast(er, bt) => {
            let v = check_exp(context, er)?;
            match fold_cast_(bt, &v) {
                Some(folded) => Some(Cow::Owned(folded)),
                None => {
                    report_cast_always_errors(context, e.exp.loc, bt, &v);
                    None
                }
            }
        }
    }
}

fn check_exps(context: &Context, es: &[Exp]) {
    for e in es {
        check_exp(context, e);
    }
}

fn report_unop_always_errors(context: &Context, loc: Loc, sp!(_, op_): &UnaryOp, v: &Value_) {
    use UnaryOp_ as U;
    let reason = match op_ {
        U::Neg => {
            let Some((n, ty)) = numeric_value(v) else {
                context
                    .reporter
                    .add_diag(ice!((loc, "Only numeric negation can error at runtime")));
                return;
            };
            format!("Negating '{n}' is outside the range of '{ty}'")
        }
        U::Not => {
            context.reporter.add_diag(ice!((
                loc,
                "'!' cannot error at runtime, but it could not be folded"
            )));
            return;
        }
    };
    report_always_errors(context, loc, reason)
}

fn report_binop_always_errors(
    context: &Context,
    loc: Loc,
    sp!(_, op_): &BinOp,
    v1: &Value_,
    v2: &Value_,
) {
    use BinOp_ as B;
    let (Some((n1, ty)), Some((n2, _))) = (numeric_value(v1), numeric_value(v2)) else {
        context
            .reporter
            .add_diag(ice!((loc, "Only numeric operations can error at runtime")));
        return;
    };
    let signed = is_signed(v1);
    let reason = match op_ {
        B::Add => format!("The sum of '{n1}' and '{n2}' is outside the range of '{ty}'"),
        B::Sub if signed => {
            format!("The difference of '{n1}' and '{n2}' is outside the range of '{ty}'")
        }
        B::Sub => format!("Subtracting '{n2}' from '{n1}' is less than zero"),
        B::Mul => format!("The product of '{n1}' and '{n2}' is outside the range of '{ty}'"),
        B::Div if signed && !is_zero(v2) => {
            format!("Dividing '{n1}' by '{n2}' is outside the range of '{ty}'")
        }
        B::Div => "Cannot divide by zero".to_owned(),
        B::Mod if signed && !is_zero(v2) => {
            format!("Taking '{n1}' modulo '{n2}' is outside the range of '{ty}'")
        }
        B::Mod => "Cannot take the remainder modulo zero".to_owned(),
        // Signed left shifts below the bit width wrap at runtime instead of erroring, so nothing is reported.
        B::Shl if signed && shift_amount_in_range(v1, v2) => return,
        B::Shl | B::Shr => format!(
            "The shift amount '{n2}' is greater than or equal to the number of bits in '{ty}'"
        ),
        // no other operation can error
        B::BitOr
        | B::BitAnd
        | B::Xor
        | B::And
        | B::Or
        | B::Eq
        | B::Neq
        | B::Lt
        | B::Gt
        | B::Le
        | B::Ge => {
            context.reporter.add_diag(ice!((
                loc,
                format!("'{op_}' cannot error at runtime, but it could not be folded")
            )));
            return;
        }
    };
    report_always_errors(context, loc, reason)
}

fn report_cast_always_errors(
    context: &Context,
    loc: Loc,
    sp!(_, bt_): &BuiltinTypeName,
    v: &Value_,
) {
    let Some((n, ty)) = numeric_value(v) else {
        context
            .reporter
            .add_diag(ice!((loc, "Only numeric casts can error at runtime")));
        return;
    };
    let reason = format!("The '{ty}' value '{n}' is outside the range of '{bt_}'");
    report_always_errors(context, loc, reason)
}

fn report_always_errors(context: &Context, loc: Loc, reason: String) {
    let msg = format!("{reason}. This operation always errors at runtime");
    context
        .reporter
        .add_diag(diag!(CodeGeneration::AlwaysErrors, (loc, msg)));
}

fn numeric_value(v: &Value_) -> Option<(String, &'static str)> {
    use Value_ as V;
    Some(match v {
        V::U8(u) => (u.to_string(), "u8"),
        V::U16(u) => (u.to_string(), "u16"),
        V::U32(u) => (u.to_string(), "u32"),
        V::U64(u) => (u.to_string(), "u64"),
        V::U128(u) => (u.to_string(), "u128"),
        V::U256(u) => (u.to_string(), "u256"),
        V::I8(i) => (i.to_string(), "i8"),
        V::I16(i) => (i.to_string(), "i16"),
        V::I32(i) => (i.to_string(), "i32"),
        V::I64(i) => (i.to_string(), "i64"),
        V::I128(i) => (i.to_string(), "i128"),
        V::I256(i) => (i.to_string(), "i256"),
        V::Address(_) | V::Bool(_) | V::Vector(_, _) => return None,
    })
}

fn is_signed(v: &Value_) -> bool {
    use Value_ as V;
    matches!(
        v,
        V::I8(_) | V::I16(_) | V::I32(_) | V::I64(_) | V::I128(_) | V::I256(_)
    )
}

fn is_zero(v: &Value_) -> bool {
    use Value_ as V;
    match v {
        V::U8(u) => *u == 0,
        V::U16(u) => *u == 0,
        V::U32(u) => *u == 0,
        V::U64(u) => *u == 0,
        V::U128(u) => *u == 0,
        V::U256(u) => *u == move_core_types::u256::U256::zero(),
        V::I8(i) => *i == 0,
        V::I16(i) => *i == 0,
        V::I32(i) => *i == 0,
        V::I64(i) => *i == 0,
        V::I128(i) => *i == 0,
        V::I256(i) => *i == move_core_types::i256::I256::zero(),
        V::Address(_) | V::Bool(_) | V::Vector(_, _) => false,
    }
}

// True when the shift amount is below the bit width of the shifted value, where a signed left
// shift wraps at runtime rather than erroring.
fn shift_amount_in_range(v1: &Value_, v2: &Value_) -> bool {
    use Value_ as V;
    let V::U8(rhs) = v2 else { return false };
    let Some(bits) = bit_width(v1) else {
        return false;
    };
    (*rhs as u32) < bits
}

fn bit_width(v: &Value_) -> Option<u32> {
    use Value_ as V;
    Some(match v {
        V::U8(_) | V::I8(_) => 8,
        V::U16(_) | V::I16(_) => 16,
        V::U32(_) | V::I32(_) => 32,
        V::U64(_) | V::I64(_) => 64,
        V::U128(_) | V::I128(_) => 128,
        V::U256(_) | V::I256(_) => 256,
        V::Address(_) | V::Bool(_) | V::Vector(_, _) => return None,
    })
}
