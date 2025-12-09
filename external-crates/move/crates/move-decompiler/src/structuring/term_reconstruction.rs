// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::ast as Out;

use move_stackless_bytecode_2::ast::{DataOp, RValue, RegId, Trivial};

use std::collections::{BTreeMap, HashSet};

pub fn exp(
    block: move_stackless_bytecode_2::ast::BasicBlock,
    let_binds: &mut HashSet<RegId>,
) -> Out::Exp {
    use move_stackless_bytecode_2::ast::Instruction as SI;
    let mut map: BTreeMap<RegId, Out::Exp> = BTreeMap::new();
    let mut seq = Vec::new();

    for instr in block.instructions {
        match instr {
            SI::Return(trivs) => {
                let exps = trivials(&mut map, trivs);
                seq.push(Out::Exp::Return(exps));
            }
            SI::AssignReg {
                lhs,
                rhs: RValue::Call { target, args },
            } => {
                let args = trivials(&mut map, args);
                let call = Out::Exp::Call(target, args);
                match &lhs[..] {
                    [] => {
                        seq.push(call);
                    }
                    [reg] => {
                        map.insert(reg.name, call);
                    }
                    _ => {
                        let tmps = lhs
                            .into_iter()
                            .map(|reg| {
                                let tmp = reg.name();
                                map.insert(reg.name, Out::Exp::Variable(tmp.clone()));
                                tmp
                            })
                            .collect();
                        seq.push(Out::Exp::Assign(tmps, Box::new(call)));
                    }
                }
            }
            SI::AssignReg {
                lhs,
                rhs: RValue::Data { op, args },
            } => match op {
                // Multi-arity LHSs
                DataOp::Unpack(ty) => {
                    let fields = &ty.struct_.fields.0;
                    debug_assert!(fields.len() == lhs.len());
                    assert!(args.len() == 1);
                    let unpack_fields = fields
                        .iter()
                        .zip(lhs.iter())
                        .map(|(f, r)| (f.1.name, r.name()))
                        .collect::<Vec<_>>();
                    let module_id = ty.struct_.defining_module;
                    let name = ty.struct_.name;
                    let rhs = Box::new(trivials(&mut map, args.clone()).remove(0));
                    seq.push(Out::Exp::Unpack((module_id, name), unpack_fields, rhs));
                }
                DataOp::UnpackVariant(_)
                | DataOp::UnpackVariantImmRef(_)
                | DataOp::UnpackVariantMutRef(_) => {
                    let (ty, unpack_kind) = match op {
                        DataOp::UnpackVariant(ty) => (ty, Out::UnpackKind::Value),
                        DataOp::UnpackVariantImmRef(ty) => (ty, Out::UnpackKind::ImmRef),
                        DataOp::UnpackVariantMutRef(ty) => (ty, Out::UnpackKind::MutRef),
                        _ => unreachable!(),
                    };
                    let fields = &ty.variant.fields.0;
                    debug_assert!(fields.len() == lhs.len());
                    assert!(args.len() == 1);
                    let unpack_fields = fields
                        .iter()
                        .zip(lhs.iter())
                        .map(|(f, r)| (*f.0, r.name()))
                        .collect::<Vec<_>>();
                    let module_id = ty.enum_.defining_module;
                    let enum_ = ty.enum_.name;
                    let variant = ty.variant.name;
                    let rhs = Box::new(trivials(&mut map, args.clone()).remove(0));
                    seq.push(Out::Exp::UnpackVariant(
                        unpack_kind,
                        (module_id, enum_, variant),
                        unpack_fields,
                        rhs,
                    ));
                }
                DataOp::VecUnpack(_ty) => {
                    assert!(args.len() == 1, "VecUnpack expects a single argument");
                    let unpack_fields = lhs.iter().map(|r| r.name()).collect::<Vec<_>>();
                    let rhs = Box::new(trivials(&mut map, args.clone()).remove(0));
                    seq.push(Out::Exp::VecUnpack(unpack_fields, rhs));
                }
                // Zero-arity LHSs
                DataOp::VecSwap(ty) => seq.push(Out::Exp::Data {
                    op: DataOp::VecSwap(ty),
                    args: trivials(&mut map, args),
                }),
                DataOp::WriteRef => seq.push(Out::Exp::Data {
                    op: DataOp::WriteRef,
                    args: trivials(&mut map, args),
                }),
                DataOp::VecPushBack(ty) => seq.push(Out::Exp::Data {
                    op: DataOp::VecPushBack(ty),
                    args: trivials(&mut map, args),
                }),
                // Single-arity LHSs
                DataOp::ReadRef
                | DataOp::FreezeRef
                | DataOp::MutBorrowField(_)
                | DataOp::ImmBorrowField(_)
                | DataOp::VecPack(_)
                | DataOp::VecLen(_)
                | DataOp::VecImmBorrow(_)
                | DataOp::VecMutBorrow(_)
                | DataOp::VecPopBack(_)
                | DataOp::PackVariant(_)
                | DataOp::Pack(_) => {
                    let [reg] = &lhs[..] else {
                        panic!("Register assignment with invalid lhs {:?}", lhs);
                    };
                    let rvalue = rvalue(&mut map, RValue::Data { op, args });
                    let res = map.insert(reg.name, rvalue);
                    assert!(res.is_none());
                }
            },
            SI::AssignReg { lhs, rhs } => {
                let [reg] = &lhs[..] else {
                    panic!("Register assignment with invalid lhs {:?}", lhs);
                };
                let rvalue = rvalue(&mut map, rhs);
                let res = map.insert(reg.name, rvalue);
                assert!(res.is_none());
            }
            SI::StoreLoc { loc, value } => {
                let triv = trivial(&mut map, value);
                if let_binds.insert(loc) {
                    seq.push(Out::Exp::LetBind(vec![local_name(loc)], Box::new(triv)));
                } else {
                    seq.push(Out::Exp::Assign(vec![local_name(loc)], Box::new(triv)));
                }
            }

            SI::Abort(triv) => seq.push(Out::Exp::Abort(Box::new(trivial(&mut map, triv)))),

            SI::Jump(_) => continue,
            SI::JumpIf { condition, .. } => seq.push(trivial(&mut map, condition)),
            SI::VariantSwitch { condition, .. } => seq.push(trivial(&mut map, condition)),
            SI::Nop | SI::Drop(_) | SI::NotImplemented(_) => continue,
        }
    }

    Out::Exp::Seq(seq)
}

fn rvalue(map: &mut BTreeMap<RegId, Out::Exp>, rvalue: RValue) -> Out::Exp {
    use move_stackless_bytecode_2::ast as S;
    match rvalue {
        RValue::Call { .. } => unreachable!(),
        RValue::Primitive { op, args } => Out::Exp::Primitive {
            op,
            args: trivials(map, args),
        },
        RValue::Data { op, args } => {
            // TODO: more structuring based on `op` here -- generate vector stdlib calls, etc.
            Out::Exp::Data {
                op,
                args: trivials(map, args),
            }
        }
        RValue::Local { op, arg } => match op {
            S::LocalOp::Move | S::LocalOp::Copy => local(arg),
            S::LocalOp::Borrow(mutability) => {
                let mut_ = match mutability {
                    S::Mutability::Mutable => true,
                    S::Mutability::Immutable => false,
                };
                Out::Exp::Borrow(mut_, Box::new(local(arg)))
            }
        },
        RValue::Trivial(triv) => trivial(map, triv),
        RValue::Constant(constant) => Out::Exp::Constant(constant),
    }
}

fn trivials(map: &mut BTreeMap<RegId, Out::Exp>, trivs: Vec<Trivial>) -> Vec<Out::Exp> {
    trivs.into_iter().map(|t| trivial(map, t)).collect()
}

fn trivial(map: &mut BTreeMap<RegId, Out::Exp>, triv: Trivial) -> Out::Exp {
    match triv {
        // If it is not there, just use the register as-is: it probably came from an unpack or a
        // call with multiple return values.
        Trivial::Register(reg_id) => map
            .remove(&reg_id.name)
            .unwrap_or_else(|| Out::Exp::Variable(reg_id.to_string())),
        Trivial::Immediate(value) => Out::Exp::Value(value),
    }
}

fn local_name(id: usize) -> String {
    format!("l{}", id)
}

fn local(id: usize) -> Out::Exp {
    Out::Exp::Variable(local_name(id))
}
