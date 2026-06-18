// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// -------------------------------------------------------------------------------------------------
// Structural equality on `Exp`
// -------------------------------------------------------------------------------------------------
//
// Used by `compress_dispatch_cascade` (comparing cloned arm bodies) and by
// `acyclic::recognize_skip_diamond`'s body-equivalence guard (comparing the two stale arms
// of an abs_diff diamond). Conservative: for foreign-typed fields that don't implement
// `PartialEq` (`DataOp` from `move-stackless-bytecode-2`, `Constant<Symbol>` from
// `move-binary-format`, `TypeRef`/`ModuleRef`/`Value`/`UnstructuredNode`) we fall back to
// comparing their `Debug` representation — deterministic for the shapes any current caller
// sees. Everything else recurses structurally so renaming or reordering of fields can't
// pretend two arms are equal when they aren't.
//
// NB: this is purely *structural* equivalence on the lowered `Exp` shape. It does NOT prove
// semantic equivalence — two arms with the same Exp shape ARE observationally equivalent
// (modulo the foreign-typed Debug fallback's correctness), but two arms with different shapes
// COULD still be observationally equivalent (e.g., `x + 0` vs `x`). A semantic-prover-grade
// guard would need alias analysis or symbolic execution; out of scope.

use crate::ast::Exp;

pub(crate) fn exp_struct_eq(a: &Exp, b: &Exp) -> bool {
    use Exp::*;
    fn dbg<T: std::fmt::Debug>(a: &T, b: &T) -> bool {
        format!("{a:?}") == format!("{b:?}")
    }
    fn seq_eq(a: &[Exp], b: &[Exp]) -> bool {
        a.len() == b.len() && a.iter().zip(b).all(|(x, y)| exp_struct_eq(x, y))
    }
    match (a, b) {
        (Break(la), Break(lb)) | (Continue(la), Continue(lb)) => la == lb,
        (Loop(la, ba), Loop(lb, bb)) => la == lb && exp_struct_eq(ba, bb),
        (Seq(va), Seq(vb)) | (Return(va), Return(vb)) => seq_eq(va, vb),
        (While(la, ca, ba), While(lb, cb, bb)) => {
            la == lb && exp_struct_eq(ca, cb) && exp_struct_eq(ba, bb)
        }
        (IfElse(ca, ta, ea), IfElse(cb, tb, eb)) => {
            exp_struct_eq(ca, cb)
                && exp_struct_eq(ta, tb)
                && match (ea.as_ref().as_ref(), eb.as_ref().as_ref()) {
                    (Some(a), Some(b)) => exp_struct_eq(a, b),
                    (None, None) => true,
                    _ => false,
                }
        }
        (Switch(ca, ea, arms_a), Switch(cb, eb, arms_b)) => {
            exp_struct_eq(ca, cb)
                && dbg(ea, eb)
                && arms_a.len() == arms_b.len()
                && arms_a
                    .iter()
                    .zip(arms_b)
                    .all(|((va, ba), (vb, bb))| va == vb && exp_struct_eq(ba, bb))
        }
        (Match(ca, ea, arms_a), Match(cb, eb, arms_b)) => {
            exp_struct_eq(ca, cb)
                && dbg(ea, eb)
                && arms_a.len() == arms_b.len()
                && arms_a
                    .iter()
                    .zip(arms_b)
                    .all(|((va, fa, ba), (vb, fb, bb))| {
                        va == vb && fa == fb && exp_struct_eq(ba, bb)
                    })
        }
        (MatchLit(sa, arms_a), MatchLit(sb, arms_b)) => {
            exp_struct_eq(sa, sb)
                && arms_a.len() == arms_b.len()
                && arms_a
                    .iter()
                    .zip(arms_b)
                    .all(|((ka, ba), (kb, bb))| ka == kb && exp_struct_eq(ba, bb))
        }
        (Assign(na, ea), Assign(nb, eb)) | (LetBind(na, ea), LetBind(nb, eb)) => {
            na == nb && exp_struct_eq(ea, eb)
        }
        (Declare(na), Declare(nb)) => na == nb,
        (Call((ma, fa), va), Call((mb, fb), vb)) => dbg(ma, mb) && fa == fb && seq_eq(va, vb),
        (Abort(ea), Abort(eb)) => exp_struct_eq(ea, eb),
        (Primitive { op: oa, args: aa }, Primitive { op: ob, args: ab }) => {
            oa == ob && seq_eq(aa, ab)
        }
        // `DataOp` is a foreign enum with non-PartialEq fields — fall back to Debug string.
        (Data { op: oa, args: aa }, Data { op: ob, args: ab }) => dbg(oa, ob) && seq_eq(aa, ab),
        (Unpack(ta, fa, ea), Unpack(tb, fb, eb)) => {
            dbg(ta, tb) && fa == fb && exp_struct_eq(ea, eb)
        }
        (UnpackVariant(ka, (ta, va), fa, ea), UnpackVariant(kb, (tb, vb), fb, eb)) => {
            ka == kb && dbg(ta, tb) && va == vb && fa == fb && exp_struct_eq(ea, eb)
        }
        (VecUnpack(na, ea), VecUnpack(nb, eb)) => na == nb && exp_struct_eq(ea, eb),
        (Borrow(ma, ea), Borrow(mb, eb)) => ma == mb && exp_struct_eq(ea, eb),
        // `Value` (MoveValue) and `Constant<Symbol>` are foreign — Debug-string fallback.
        (Value(a), Value(b)) => dbg(a, b),
        (Constant(a), Constant(b)) => dbg(a, b),
        (Variable(a), Variable(b)) => a == b,
        // `UnstructuredNode` is local but contains `Box<Exp>` — recurse via Debug for now;
        // this branch is rare (only set by `generate_output` for unhandled CFG shapes) and
        // the arm-comparison context doesn't hit it.
        (Unstructured(a), Unstructured(b)) => dbg(a, b),
        (Block(la, ba), Block(lb, bb)) => la == lb && exp_struct_eq(ba, bb),
        _ => false,
    }
}
